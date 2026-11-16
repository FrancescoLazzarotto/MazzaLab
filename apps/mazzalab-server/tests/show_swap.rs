//! Opening another show, and saving under another name, without restarting
//! the server.

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use mazzalab_core::{AudioCue, Cue, CueId, CueKind, CueList, WaitCue, Workspace};
use mazzalab_server::{RunningServer, ServerConfig};
use mazzalab_show::Command;
use serde_json::{Value, json};
use tower::ServiceExt;

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const WAIT: CueId = CueId::from_u128(1);

/// A fresh folder for one test.
fn folder(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mazzalab-swap-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn show(name: &str, cues: Vec<Cue>) -> Workspace {
    let mut workspace = Workspace::new(name);
    workspace.cue_lists = vec![CueList::new("Main").with_cues(cues)];
    workspace
}

fn wait() -> Cue {
    Cue::new(CueKind::Wait(WaitCue {
        duration: Duration::from_secs(60),
    }))
    .with_id(WAIT)
    .with_number("1")
}

async fn start(path: &Path, workspace: Workspace) -> RunningServer {
    let config = ServerConfig {
        show_path: Some(path.to_path_buf()),
        bind: LOCALHOST,
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        autosave: None,
        ..ServerConfig::default()
    };
    mazzalab_server::start(config, workspace, |_| Vec::new())
        .await
        .unwrap()
}

async fn post(app: &Router, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::post(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn get(app: &Router, uri: &str) -> Value {
    let response = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "GET {uri}");
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

#[tokio::test]
async fn another_show_opens_on_the_same_server() {
    let dir = folder("open");
    let first = dir.join("First.mazzalab");
    let second = dir.join("Second.mazzalab");
    mazzalab_project::save(&first, &show("First", vec![wait()])).unwrap();
    let other = show("Second", vec![Cue::new(CueKind::Memo).with_number("9")]);
    mazzalab_project::save(&second, &other).unwrap();

    let server = start(&first, mazzalab_project::load(&first).unwrap()).await;
    let app = mazzalab_server::router(server.handle.clone(), None);
    let mut updates = server.handle.subscribe();
    server
        .handle
        .execute(Command::Go { list: None })
        .await
        .unwrap();
    assert!(!server.handle.status().await.unwrap().running.is_empty());
    let before = server.handle.workspace_id();

    let files = get(&app, "/api/show/files").await;
    assert_eq!(files["shows"], json!(["First.mazzalab", "Second.mazzalab"]));

    let (status, _) = post(&app, "/api/show/open", json!({ "path": second })).await;
    assert_eq!(status, StatusCode::OK);
    let snapshot = server.handle.snapshot().await.unwrap();
    assert_eq!(snapshot.workspace.name, "Second");
    assert_eq!(snapshot.workspace.id, other.id);
    // What ran in the old show stopped, and the server's identity followed.
    assert!(server.handle.status().await.unwrap().running.is_empty());
    assert_ne!(server.handle.workspace_id(), before);
    assert_eq!(&*server.handle.workspace_id(), other.id.to_string());
    assert_eq!(
        server.handle.path().as_deref(),
        Some(std::path::absolute(&second).unwrap().as_path())
    );
    assert_eq!(
        get(&app, "/api/show").await["path"],
        json!(std::path::absolute(&second).unwrap().display().to_string())
    );

    // Clients connected all along hear about the new show.
    let mut saw_stop = false;
    let mut saw_second = false;
    while let Ok(update) = updates.try_recv() {
        match update {
            mazzalab_server::Update::Events(events) => {
                saw_stop |= events
                    .iter()
                    .any(|event| serde_json::to_value(event).unwrap()["kind"] == json!("stopped"));
            }
            mazzalab_server::Update::Snapshot(snapshot) => {
                saw_second |= snapshot.workspace.name == "Second";
            }
            _ => {}
        }
    }
    // The snapshot goes out when the actor next syncs.
    tokio::time::sleep(Duration::from_millis(50)).await;
    while let Ok(update) = updates.try_recv() {
        if let mazzalab_server::Update::Snapshot(snapshot) = update {
            saw_second |= snapshot.workspace.name == "Second";
        }
    }
    assert!(saw_stop, "the running cue was not reported stopped");
    assert!(saw_second, "no snapshot of the new show was sent");

    // Saving now writes the new show's file.
    server
        .handle
        .execute(Command::RenameWorkspace {
            name: "Second, edited".into(),
        })
        .await
        .unwrap();
    server.handle.save().await.unwrap();
    assert_eq!(
        mazzalab_project::load(&second).unwrap().name,
        "Second, edited"
    );
    assert_eq!(mazzalab_project::load(&first).unwrap().name, "First");

    server.shutdown();
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn unsaved_changes_are_not_thrown_away_unless_asked() {
    let dir = folder("unsaved");
    let first = dir.join("First.mazzalab");
    let second = dir.join("Second.mazzalab");
    mazzalab_project::save(&second, &show("Second", Vec::new())).unwrap();
    let server = start(&first, show("First", vec![wait()])).await;
    let app = mazzalab_server::router(server.handle.clone(), None);
    server
        .handle
        .execute(Command::RenameWorkspace {
            name: "Edited".into(),
        })
        .await
        .unwrap();

    let (status, body) = post(&app, "/api/show/open", json!({ "path": second })).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["unsaved"], json!(true));
    assert_eq!(
        server.handle.snapshot().await.unwrap().workspace.name,
        "Edited"
    );

    let (status, _) = post(
        &app,
        "/api/show/open",
        json!({ "path": second, "discard": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let snapshot = server.handle.snapshot().await.unwrap();
    assert_eq!(snapshot.workspace.name, "Second");
    assert!(!snapshot.dirty);

    let (status, _) = post(
        &app,
        "/api/show/open",
        json!({ "path": dir.join("Missing.mazzalab") }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A new show is created empty, with the extension added.
    let (status, _) = post(&app, "/api/show/new", json!({ "path": dir.join("Third") })).await;
    assert_eq!(status, StatusCode::OK);
    assert!(dir.join("Third.mazzalab").is_file());
    assert_eq!(
        server.handle.snapshot().await.unwrap().workspace.name,
        "Third"
    );
    let (status, _) = post(&app, "/api/show/new", json!({ "path": second })).await;
    assert_eq!(status, StatusCode::CONFLICT);

    server.shutdown();
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn save_as_moves_the_show_without_stopping_it() {
    let dir = folder("save-as");
    let original = dir.join("show").join("Show.mazzalab");
    std::fs::create_dir_all(original.parent().unwrap()).unwrap();
    let sound = Cue::new(CueKind::Audio(AudioCue {
        file: Some(PathBuf::from("media/door.wav")),
        ..AudioCue::default()
    }))
    .with_number("2");
    let sound_id = sound.id;
    let server = start(&original, show("Show", vec![wait(), sound])).await;
    let app = mazzalab_server::router(server.handle.clone(), None);
    server
        .handle
        .execute(Command::Go { list: None })
        .await
        .unwrap();

    let copy = dir.join("tour").join("venue").join("Tour");
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    let (status, _) = post(&app, "/api/show/save-as", json!({ "path": copy })).await;
    assert_eq!(status, StatusCode::OK);
    let copy = copy.with_extension(mazzalab_project::FILE_EXTENSION);

    // Still running, now saved in the new file, with the sound found from there.
    assert!(!server.handle.status().await.unwrap().running.is_empty());
    let snapshot = server.handle.snapshot().await.unwrap();
    assert!(!snapshot.dirty);
    assert_eq!(
        server.handle.path().as_deref(),
        Some(std::path::absolute(&copy).unwrap().as_path())
    );
    let expected = std::path::absolute(dir.join("show").join("media").join("door.wav")).unwrap();
    let file_of = |workspace: &Workspace| match &workspace.find_cue(sound_id).unwrap().kind {
        CueKind::Audio(audio) => audio.file.clone().unwrap(),
        _ => unreachable!(),
    };
    let saved = mazzalab_project::load(&copy).unwrap();
    assert_eq!(
        std::path::absolute(copy.parent().unwrap().join(file_of(&saved))).unwrap(),
        expected
    );
    assert_eq!(
        server
            .handle
            .cue_file(sound_id)
            .await
            .map(|file| std::path::absolute(file).unwrap()),
        Some(expected)
    );
    assert!(
        !original.exists(),
        "the old file was never saved, and stays so"
    );

    server.shutdown();
    std::fs::remove_dir_all(dir).unwrap();
}
