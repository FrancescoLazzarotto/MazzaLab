//! The pre-show check over HTTP.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use mazzalab_core::{AudioCue, Cue, CueKind, CueList, Workspace};
use mazzalab_server::{Output, ServerConfig};
use serde_json::Value;
use tower::ServiceExt;

#[tokio::test]
async fn a_missing_file_is_reported_and_a_present_one_is_not() {
    let dir = std::env::temp_dir().join(format!("mazzalab-preflight-http-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("media")).unwrap();
    std::fs::write(dir.join("media/here.wav"), b"x").unwrap();

    let sound = |file: &str| {
        Cue::new(CueKind::Audio(AudioCue {
            file: Some(PathBuf::from(file)),
            ..AudioCue::default()
        }))
    };
    let missing = sound("media/gone.wav");
    let missing_id = missing.id;
    let mut workspace = Workspace::new("Check");
    let mut list = CueList::new("Main");
    list.cues = vec![sound("media/here.wav"), missing];
    workspace.cue_lists = vec![list];

    let config = ServerConfig {
        show_path: Some(dir.join("show.mazzalab")),
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
    let response = app
        .oneshot(Request::get("/api/preflight").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    let issues = body["issues"].as_array().unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0]["key"], "missing_media");
    assert_eq!(issues[0]["severity"], "error");
    assert_eq!(issues[0]["cue"], missing_id.to_string());
    assert_eq!(issues[0]["detail"], "media/gone.wav");

    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}
