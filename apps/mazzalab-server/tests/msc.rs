//! MIDI Show Control from a MIDI input runs the show.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mazzalab_core::{Cue, CueKind, CueList, WaitCue, Workspace};
use mazzalab_server::{Completion, ServerConfig};

/// `F0 7F <device> 02 01 <command> <data> F7`: general lighting format.
fn msc(device: u8, command: u8, data: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0xF0, 0x7F, device, 0x02, 0x01, command];
    bytes.extend_from_slice(data);
    bytes.push(0xF7);
    bytes
}

async fn until(what: &str, mut done: impl AsyncFnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !done().await {
        assert!(tokio::time::Instant::now() < deadline, "{what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn msc_for_the_show_runs_it_and_for_others_does_not() {
    let wait = |number: &str| {
        Cue::new(CueKind::Wait(WaitCue {
            duration: Duration::from_secs(60),
        }))
        .with_number(number)
    };
    let mut workspace = Workspace::new("MSC");
    workspace.settings.msc_device_id = Some(5);
    workspace.cue_lists = vec![CueList::new("Main").with_cues(vec![wait("1"), wait("2")])];
    let second = workspace.cue_lists[0].cues[1].id;
    let config = ServerConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        announce: false,
        record_inputs: false,
        ..ServerConfig::default()
    };
    // The completion a MIDI backend would hold, to feed it by hand.
    let held: Arc<Mutex<Option<Completion>>> = Arc::default();
    let keep = Arc::clone(&held);
    let server = mazzalab_server::start(config, workspace, move |completion| {
        *keep.lock().unwrap() = Some(completion);
        Vec::new()
    })
    .await
    .unwrap();
    let completion = held.lock().unwrap().clone().unwrap();
    let running = async || server.handle.status().await.unwrap().running.len();

    // For device 9: not this show.
    completion.sysex_received(&msc(9, 0x01, &[]));
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(running().await, 0);

    // GO to device 5, then GO for cue 2 to every device.
    completion.sysex_received(&msc(5, 0x01, &[]));
    until("MSC GO started nothing", async || running().await == 1).await;
    completion.sysex_received(&msc(0x7F, 0x01, b"2"));
    until("MSC GO 2 did not start cue 2", async || {
        server
            .handle
            .status()
            .await
            .unwrap()
            .running
            .iter()
            .any(|run| run.cue == second)
    })
    .await;

    // ALL OFF ends everything.
    completion.sysex_received(&msc(5, 0x08, &[]));
    until("MSC ALL OFF left cues running", async || {
        running().await == 0
    })
    .await;
    server.shutdown();
}
