//! Consolidating media, listing what is missing and relinking it, through
//! the server's HTTP API.

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use mazzalab_core::{AudioCue, Cue, CueId, CueKind, CueList, Workspace};
use mazzalab_server::{Output, RunningServer, ServerConfig};
use mazzalab_show::Command;
use serde_json::{Value, json};
use tower::ServiceExt;

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mazzalab-mm-http-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sound(file: &Path) -> Cue {
    Cue::new(CueKind::Audio(AudioCue {
        file: Some(file.to_path_buf()),
        ..AudioCue::default()
    }))
}

async fn start(show: &Path, cues: Vec<Cue>) -> RunningServer {
    let mut workspace = Workspace::new("Media");
    let mut list = CueList::new("Main");
    list.cues = cues;
    workspace.cue_lists = vec![list];
    let config = ServerConfig {
        show_path: Some(show.to_path_buf()),
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        announce: false,
        ..ServerConfig::default()
    };
    mazzalab_server::start(config, workspace, |_| Vec::<Box<dyn Output>>::new())
        .await
        .unwrap()
}

async fn call(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

fn post(uri: &str, body: &Value) -> Request<Body> {
    Request::post(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// The file each cue of the show names now.
async fn files(server: &RunningServer) -> Vec<String> {
    let snapshot = server.handle.snapshot().await.unwrap();
    snapshot
        .workspace
        .cues()
        .map(|cue| match &cue.kind {
            CueKind::Audio(audio) => audio
                .file
                .as_ref()
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/"),
            _ => String::new(),
        })
        .collect()
}

#[tokio::test]
async fn consolidating_copies_outside_files_under_free_names_and_one_undo_puts_them_back() {
    let base = dir("consolidate");
    let media = base.join("media");
    std::fs::create_dir_all(&media).unwrap();
    std::fs::write(media.join("in.wav"), b"inside").unwrap();
    // A different file already has the name the outside one would take.
    std::fs::write(media.join("out.wav"), b"other contents").unwrap();
    let elsewhere = dir("consolidate-elsewhere");
    let outside = elsewhere.join("out.wav");
    std::fs::write(&outside, b"outside").unwrap();

    let server = start(
        &base.join("show.mazzalab"),
        vec![
            sound(Path::new("media/in.wav")),
            sound(&outside),
            sound(&outside),
            sound(Path::new("media/gone.wav")),
        ],
    )
    .await;
    let app = mazzalab_server::router(server.handle.clone(), None);
    let (status, body) = call(&app, post("/api/media/consolidate", &json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    let copied = body["copied"].as_array().unwrap();
    // Two cues, one copy: the name was taken, so it is numbered.
    assert_eq!(copied.len(), 2);
    assert!(
        copied
            .iter()
            .all(|entry| entry["file"] == "media/out (2).wav")
    );
    assert_eq!(
        std::fs::read(media.join("out (2).wav")).unwrap(),
        b"outside"
    );
    // What was already in the folder is untouched.
    assert_eq!(
        std::fs::read(media.join("out.wav")).unwrap(),
        b"other contents"
    );
    assert_eq!(
        files(&server).await,
        vec![
            "media/in.wav",
            "media/out (2).wav",
            "media/out (2).wav",
            "media/gone.wav"
        ]
    );

    // Doing it again has nothing left to do.
    let (_, again) = call(&app, post("/api/media/consolidate", &json!({}))).await;
    assert_eq!(again["copied"].as_array().unwrap().len(), 0);

    // One step to undo.
    server.handle.execute(Command::Undo).await.unwrap();
    assert_eq!(
        files(&server).await[1],
        outside.to_string_lossy().replace('\\', "/")
    );

    server.shutdown();
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&elsewhere);
}

#[tokio::test]
async fn missing_files_are_listed_and_found_again_by_name() {
    let base = dir("relink");
    std::fs::create_dir_all(base.join("media")).unwrap();
    std::fs::write(base.join("media/here.wav"), b"x").unwrap();
    let backup = dir("relink-backup");
    std::fs::create_dir_all(backup.join("old/sounds")).unwrap();
    std::fs::write(backup.join("old/sounds/Rain.WAV"), b"rain").unwrap();

    let rain = sound(Path::new("media/rain.wav"));
    let rain_id: CueId = rain.id;
    let server = start(
        &base.join("show.mazzalab"),
        vec![
            sound(Path::new("media/here.wav")),
            rain,
            sound(Path::new("media/never.wav")),
        ],
    )
    .await;
    let app = mazzalab_server::router(server.handle.clone(), None);

    let (status, body) = call(
        &app,
        Request::get("/api/media/missing")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["missing"].as_array().unwrap().len(), 2);
    assert_eq!(body["missing"][0]["cue"], rain_id.to_string());

    let (status, body) = call(
        &app,
        post("/api/media/relink", &json!({ "search_dir": backup })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["relinked"].as_array().unwrap().len(), 1);
    assert_eq!(body["relinked"][0]["from"], "media/rain.wav");
    assert_eq!(body["not_found"].as_array().unwrap().len(), 1);
    assert_eq!(body["not_found"][0]["file"], "media/never.wav");
    let relinked = files(&server).await[1].clone();
    assert!(relinked.ends_with("old/sounds/Rain.WAV"), "{relinked}");
    assert!(Path::new(&relinked).is_file());

    // A folder that is not there is refused.
    let (status, _) = call(
        &app,
        post(
            "/api/media/relink",
            &json!({ "search_dir": backup.join("nope") }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    server.shutdown();
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&backup);
}

#[tokio::test]
async fn an_unsaved_show_has_no_media_folder_to_manage() {
    let mut workspace = Workspace::new("Unsaved");
    workspace.cue_lists = vec![CueList::new("Main")];
    let config = ServerConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        announce: false,
        ..ServerConfig::default()
    };
    let server = mazzalab_server::start(config, workspace, |_| Vec::<Box<dyn Output>>::new())
        .await
        .unwrap();
    let app = mazzalab_server::router(server.handle.clone(), None);
    let (status, _) = call(&app, post("/api/media/consolidate", &json!({}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    server.shutdown();
}
