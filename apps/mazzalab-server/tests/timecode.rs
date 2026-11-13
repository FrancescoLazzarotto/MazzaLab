//! Incoming timecode: the cues it starts as it runs, the jumps that start
//! nothing, and what clients are told.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mazzalab_core::{Cue, CueId, CueKind, SmpteFormat, WaitCue, Workspace};
use mazzalab_server::{RunningServer, ServerConfig, Update};
use mazzalab_timecode::{Rate, Timecode};
use tokio::time::timeout;
use tower::ServiceExt;

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

fn id(n: u128) -> CueId {
    CueId::from_u128(n)
}

fn waiting_for(n: u128, timecode: &str) -> Cue {
    let mut cue = Cue::new(CueKind::Wait(WaitCue {
        duration: Duration::from_secs(60),
    }))
    .with_id(id(n));
    cue.triggers.timecode = Some(timecode.to_owned());
    cue
}

async fn server() -> RunningServer {
    let mut workspace = Workspace::new("Timecode");
    workspace.cue_lists[0].cues = vec![
        waiting_for(1, "01:00:00:10"),
        waiting_for(2, "01:00:05:00"),
        waiting_for(3, "nonsense"),
    ];
    let config = ServerConfig {
        bind: LOCALHOST,
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        ..ServerConfig::default()
    };
    mazzalab_server::start(config, workspace, |_| Vec::new())
        .await
        .unwrap()
}

async fn running(server: &RunningServer) -> Vec<CueId> {
    let status = server.handle.status().await.unwrap();
    status.running.iter().map(|run| run.cue).collect()
}

#[tokio::test]
async fn running_timecode_starts_the_cues_it_passes_and_a_jump_starts_none() {
    let server = server().await;
    let mut updates = server.handle.subscribe();
    let rate = Rate::FPS_25;
    let feed = async |timecode: Timecode| server.handle.timecode_in(timecode, rate).await;

    // Timecode comes in every two frames, as MTC does.
    for frame in (0..10).step_by(2) {
        feed(Timecode::new(1, 0, 0, frame)).await;
    }
    assert!(running(&server).await.is_empty());
    feed(Timecode::new(1, 0, 0, 10)).await;
    assert_eq!(running(&server).await, [id(1)]);
    // Clients see where it is.
    let shown = timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Update::Timecode(timecode)) = updates.recv().await
                && timecode.timecode == "01:00:00:10"
            {
                break timecode;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(shown.format, SmpteFormat::Fps25);

    // A locate past the second trigger starts nothing...
    feed(Timecode::new(1, 0, 6, 0)).await;
    // ...nor does running backwards over it.
    feed(Timecode::new(1, 0, 4, 20)).await;
    feed(Timecode::new(1, 0, 4, 22)).await;
    assert_eq!(running(&server).await, [id(1)]);
    // Running forward across it does, even skipping the very frame.
    feed(Timecode::new(1, 0, 4, 24)).await;
    feed(Timecode::new(1, 0, 5, 1)).await;
    assert_eq!(running(&server).await, [id(1), id(2)]);
    server.stop().await;
}

#[tokio::test]
async fn timecode_can_be_fed_in_over_http() {
    let server = server().await;
    let app = mazzalab_server::router(server.handle.clone(), None);
    let post = |body: &'static str| {
        app.clone().oneshot(
            Request::post("/api/timecode/in")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
    };
    let ok = post(r#"{"timecode":"01:00:00:09","format":"fps25"}"#)
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    post(r#"{"timecode":"01:00:00:11","format":"fps25"}"#)
        .await
        .unwrap();
    assert_eq!(running(&server).await, [id(1)]);
    let bad = post(r#"{"timecode":"tomorrow"}"#).await.unwrap();
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    server.stop().await;
}
