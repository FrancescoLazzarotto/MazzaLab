//! Stages: what clients are told is up, the media they fetch, and a stage
//! saying a file played to its end.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures_util::{SinkExt, StreamExt};
use mazzalab_core::{Cue, CueId, CueKind, VideoCue, Workspace};
use mazzalab_server::{Output, ServerConfig, StageBackend, Update};
use mazzalab_show::Command;
use serde_json::{Value, json};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tower::ServiceExt;

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn stages_are_told_what_is_up_fetch_its_file_and_report_its_end() {
    let dir = std::env::temp_dir().join(format!("mazzalab-stage-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("clip.mp4");
    std::fs::write(&file, b"0123456789").unwrap();
    // No length known: the stage says when it ends.
    let video = Cue::new(CueKind::Video(VideoCue {
        file: Some(file.clone()),
        layer: 3,
        ..VideoCue::default()
    }))
    .with_id(CueId::from_u128(1));
    let mut workspace = Workspace::new("Stage");
    workspace.cue_lists[0].cues = vec![video];
    let config = ServerConfig {
        bind: LOCALHOST,
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        announce: false,
        ..ServerConfig::default()
    };
    let server = mazzalab_server::start(config, workspace, |_| {
        vec![Box::new(StageBackend::new()) as Box<dyn Output>]
    })
    .await
    .unwrap();

    let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", server.http))
        .await
        .unwrap();
    let (mut sink, mut socket) = socket.split();
    let mut next_stage = async || {
        timeout(TIMEOUT, async {
            loop {
                if let Message::Text(text) = socket.next().await.unwrap().unwrap() {
                    let value: Value = serde_json::from_str(text.as_str()).unwrap();
                    if value["type"] == "stage" {
                        break value;
                    }
                }
            }
        })
        .await
        .expect("no stage message")
    };
    // On connect: the stages, nothing up.
    let first = next_stage().await;
    assert_eq!(
        first["stages"],
        json!([{ "name": "Main", "width": 1920, "height": 1080 }])
    );
    assert_eq!(first["items"], json!([]));

    server
        .handle
        .execute(Command::Start {
            cue: CueId::from_u128(1),
        })
        .await
        .unwrap();
    let up = next_stage().await;
    let item = &up["items"][0];
    assert_eq!(item["cue"], json!(CueId::from_u128(1)));
    assert_eq!(
        (
            item["kind"].as_str(),
            item["stage"].as_str(),
            item["layer"].as_i64()
        ),
        (Some("video"), Some("Main"), Some(3))
    );
    assert!(item["started"].as_f64().unwrap() <= up["now"].as_f64().unwrap());

    // The file, a range at a time.
    let app = mazzalab_server::router(server.handle.clone(), None);
    let response = app
        .oneshot(
            Request::get(format!("/api/cue-media?cue={}", CueId::from_u128(1)))
                .header(header::RANGE, "bytes=2-5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    let body = axum::body::to_bytes(response.into_body(), 64)
        .await
        .unwrap();
    assert_eq!(&body[..], b"2345");

    // A stage says it played to the end: the cue completes and comes down.
    sink.send(Message::Text(
        json!({ "type": "stage_ended", "id": 4, "cue": CueId::from_u128(1) })
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    let down = next_stage().await;
    assert_eq!(down["items"], json!([]));
    let status = server.handle.status().await.unwrap();
    assert!(status.running.is_empty());
    let mut updates = server.handle.subscribe();
    assert!(!matches!(updates.try_recv(), Ok(Update::Stage(_))));
    server.stop().await;
}

/// Displays that only remember whether they were asked to show the stages.
struct FakeScreens(std::sync::atomic::AtomicBool);

impl mazzalab_server::Screens for FakeScreens {
    fn list(&self) -> Vec<mazzalab_server::Screen> {
        vec![mazzalab_server::Screen {
            name: "DISPLAY2".into(),
            width: 1280,
            height: 720,
            x: 1920,
            y: 0,
            main: false,
        }]
    }

    fn shown(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn set_shown(&self, shown: bool) {
        self.0.store(shown, std::sync::atomic::Ordering::SeqCst);
    }
}

#[tokio::test]
async fn the_machines_screens_are_offered_only_when_it_has_some() {
    let config = ServerConfig {
        bind: LOCALHOST,
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        announce: false,
        ..ServerConfig::default()
    };
    let server = mazzalab_server::start(config, Workspace::new("Screens"), |_| Vec::new())
        .await
        .unwrap();
    let request = |method: &str, body: Option<Value>| {
        let builder = Request::builder().method(method).uri("/api/screens");
        match body {
            Some(body) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        }
    };
    let json_of = async |response: axum::response::Response| -> Value {
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    };

    // A headless server has none, and cannot put stages on any.
    let headless = mazzalab_server::router(server.handle.clone(), None);
    let listed = json_of(
        headless
            .clone()
            .oneshot(request("GET", None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(listed["available"], false);
    let response = headless
        .oneshot(request("POST", Some(json!({ "shown": true }))))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let screens = mazzalab_server::ScreensHandle(std::sync::Arc::new(FakeScreens(
        std::sync::atomic::AtomicBool::new(false),
    )));
    let app = mazzalab_server::router_with_screens(server.handle.clone(), None, Some(screens));
    let listed = json_of(app.clone().oneshot(request("GET", None)).await.unwrap()).await;
    assert_eq!(
        (&listed["available"], &listed["shown"]),
        (&json!(true), &json!(false))
    );
    assert_eq!(listed["screens"][0]["name"], "DISPLAY2");
    assert_eq!(listed["screens"][0]["width"], 1280);
    let shown = json_of(
        app.clone()
            .oneshot(request("POST", Some(json!({ "shown": true }))))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(shown, json!({ "shown": true }));
    let listed = json_of(app.oneshot(request("GET", None)).await.unwrap()).await;
    assert_eq!(listed["shown"], true);
    server.stop().await;
}
