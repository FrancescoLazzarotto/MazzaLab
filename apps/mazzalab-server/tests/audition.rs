//! Auditioning a cue: the request reaches the audio backend, the show is
//! left alone, and clients are told what is being auditioned and when it
//! ends. A stand-in backend takes the place of the sound card.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use mazzalab_core::{AudioCue, Cue, CueId, CueKind, Workspace};
use mazzalab_engine::Event;
use mazzalab_server::{AudioStatus, Output, ServerConfig, Update};
use mazzalab_show::Show;
use serde_json::{Value, json};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const TIMEOUT: Duration = Duration::from_secs(5);

/// Remembers what it was asked to audition, like headphones would play it.
struct Headphones(Arc<Mutex<Option<CueId>>>);

impl Output for Headphones {
    fn handle(&mut self, _: &Show, _: &[Event]) {}

    fn audition(&mut self, _: &Show, cue: Option<CueId>) {
        *self.0.lock().unwrap() = cue;
    }

    fn audio_status(&self) -> Option<AudioStatus> {
        Some(AudioStatus {
            running: true,
            audition: *self.0.lock().unwrap(),
            ..AudioStatus::default()
        })
    }
}

#[tokio::test]
async fn auditioning_plays_outside_the_show_and_clients_follow_it() {
    let cue = Cue::new(CueKind::Audio(AudioCue::default())).with_id(CueId::from_u128(1));
    let mut workspace = Workspace::new("Audition");
    workspace.cue_lists[0].cues = vec![cue];
    let playing = Arc::new(Mutex::new(None));
    let headphones = playing.clone();
    let config = ServerConfig {
        bind: LOCALHOST,
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        ..ServerConfig::default()
    };
    let server = mazzalab_server::start(config, workspace, move |_| {
        vec![Box::new(Headphones(headphones)) as Box<dyn Output>]
    })
    .await
    .unwrap();
    let mut updates = server.handle.subscribe();
    // Waits until clients are told that this is what is being auditioned.
    let mut told = async |expected: Option<CueId>| {
        timeout(TIMEOUT, async {
            loop {
                if let Update::Audio(audio) = updates.recv().await.unwrap()
                    && audio.audition == expected
                {
                    break;
                }
            }
        })
        .await
        .expect("clients were not told");
    };

    // Over the WebSocket, as the operator screen asks for it.
    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", server.http))
        .await
        .unwrap();
    socket
        .send(Message::Text(
            json!({ "type": "audition", "id": 3, "cue": CueId::from_u128(1) })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let result = timeout(TIMEOUT, async {
        loop {
            if let Message::Text(text) = socket.next().await.unwrap().unwrap() {
                let value: Value = serde_json::from_str(text.as_str()).unwrap();
                if value["type"] == "result" {
                    break value;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(result["id"], 3);
    assert!(result.get("error").is_none());

    assert_eq!(*playing.lock().unwrap(), Some(CueId::from_u128(1)));
    told(Some(CueId::from_u128(1))).await;
    // The show itself did nothing: no cue runs, nothing to undo or save.
    let status = server.handle.status().await.unwrap();
    assert!(status.running.is_empty());
    let snapshot = server.handle.snapshot().await.unwrap();
    assert!(!snapshot.dirty && !snapshot.can_undo);

    // The sound ends on its own: clients find out without asking.
    *playing.lock().unwrap() = None;
    told(None).await;

    // Stopping, and cues that do not exist.
    server
        .handle
        .audition(Some(CueId::from_u128(1)))
        .await
        .unwrap();
    told(Some(CueId::from_u128(1))).await;
    server.handle.audition(None).await.unwrap();
    assert_eq!(*playing.lock().unwrap(), None);
    assert!(
        server
            .handle
            .audition(Some(CueId::from_u128(99)))
            .await
            .is_err()
    );
    server.stop().await;
}
