//! Incoming MIDI: cue triggers, transport controls and what clients hear.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mazzalab_core::{
    Cue, CueId, CueKind, CueList, MidiControls, MidiTrigger, MidiTriggerKind, WaitCue, Workspace,
};
use mazzalab_midi::MidiEvent;
use mazzalab_server::{RunningServer, ServerConfig, Update};
use tokio::time::timeout;
use tower::ServiceExt;

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const LIST: CueId = CueId::from_u128(1000);

fn id(n: u128) -> CueId {
    CueId::from_u128(n)
}

fn wait(n: u128) -> Cue {
    Cue::new(CueKind::Wait(WaitCue {
        duration: Duration::from_secs(60),
    }))
    .with_id(id(n))
}

fn trigger(kind: MidiTriggerKind, channel: Option<u8>, number: u8) -> MidiTrigger {
    MidiTrigger {
        kind,
        channel,
        number,
        value: None,
    }
}

fn event(kind: MidiTriggerKind, channel: u8, number: u8, value: u8) -> MidiEvent {
    MidiEvent {
        kind,
        channel,
        number,
        value,
    }
}

async fn server() -> RunningServer {
    let mut pad = wait(2);
    pad.triggers.midi = Some(trigger(MidiTriggerKind::NoteOn, Some(10), 36));
    let mut same_pad = wait(3);
    same_pad.triggers.midi = Some(trigger(MidiTriggerKind::NoteOn, Some(10), 36));

    let mut workspace = Workspace::new("Triggers");
    workspace.cue_lists = vec![CueList::new("Main").with_id(LIST).with_cues(vec![
        wait(1),
        pad,
        same_pad,
    ])];
    workspace.settings.midi_controls = MidiControls {
        go: Some(trigger(MidiTriggerKind::ControlChange, None, 64)),
        panic: Some(trigger(MidiTriggerKind::ProgramChange, Some(1), 127)),
        ..MidiControls::default()
    };
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
async fn midi_starts_the_cues_that_wait_for_it() {
    let server = server().await;
    let mut updates = server.handle.subscribe();

    // The wrong note, then the right note on the wrong channel: nothing.
    server
        .handle
        .midi_in(event(MidiTriggerKind::NoteOn, 10, 37, 100))
        .await;
    server
        .handle
        .midi_in(event(MidiTriggerKind::NoteOn, 9, 36, 100))
        .await;
    assert!(running(&server).await.is_empty());

    // The pad: both cues that wait for it start, the playhead stays put.
    server
        .handle
        .midi_in(event(MidiTriggerKind::NoteOn, 10, 36, 100))
        .await;
    assert_eq!(running(&server).await, [id(2), id(3)]);
    let status = server.handle.status().await.unwrap();
    assert_eq!(status.playheads[0].cue, Some(id(1)));

    // Letting go of the pad is a different message.
    server
        .handle
        .midi_in(event(MidiTriggerKind::NoteOff, 10, 36, 0))
        .await;
    assert_eq!(running(&server).await.len(), 2);

    // Clients hear every message, so they can learn triggers.
    let heard = timeout(Duration::from_secs(5), async {
        let mut heard = Vec::new();
        while heard.len() < 4 {
            if let Update::MidiIn(event) = updates.recv().await.unwrap() {
                heard.push((event.kind, event.channel, event.number));
            }
        }
        heard
    })
    .await
    .expect("MIDI was not passed on");
    assert_eq!(heard[2], (MidiTriggerKind::NoteOn, 10, 36));
    assert_eq!(heard[3], (MidiTriggerKind::NoteOff, 10, 36));
    server.stop().await;
}

#[tokio::test]
async fn midi_works_the_transport() {
    let server = server().await;
    // A sustain pedal on any channel is GO.
    server
        .handle
        .midi_in(event(MidiTriggerKind::ControlChange, 5, 64, 127))
        .await;
    assert_eq!(running(&server).await, [id(1)]);
    let status = server.handle.status().await.unwrap();
    assert_eq!(status.playheads[0].cue, Some(id(2)));

    // Panic is tied to channel 1: channel 2 does nothing.
    server
        .handle
        .midi_in(event(MidiTriggerKind::ProgramChange, 2, 127, 0))
        .await;
    assert!(!server.handle.status().await.unwrap().panicking);
    server
        .handle
        .midi_in(event(MidiTriggerKind::ProgramChange, 1, 127, 0))
        .await;
    assert!(server.handle.status().await.unwrap().panicking);
    server.stop().await;
}

#[tokio::test]
async fn a_message_can_be_fed_in_over_http() {
    let server = server().await;
    let app = mazzalab_server::router(server.handle.clone(), None);
    let response = app
        .oneshot(
            Request::post("/api/midi/in")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"kind":"note_on","channel":10,"number":36,"value":90}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(running(&server).await, [id(2), id(3)]);
    server.stop().await;
}
