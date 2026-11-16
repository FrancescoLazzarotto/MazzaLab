//! The server records every input next to the show, and the record replays.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use mazzalab_core::{Cue, CueKind, CueList, WaitCue, Workspace};
use mazzalab_server::ServerConfig;
use mazzalab_show::{Command, Input};
use rosc::OscMessage;
use tokio::net::UdpSocket;

#[tokio::test]
async fn inputs_are_recorded_in_logs_and_replay() {
    let dir = std::env::temp_dir().join(format!("mazzalab-record-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Night.mazzalab");
    let mut workspace = Workspace::new("Night");
    workspace.cue_lists = vec![CueList::new("Main").with_cues(vec![
        Cue::new(CueKind::Wait(WaitCue {
            duration: Duration::from_secs(30),
        }))
        .with_number("1"),
    ])];
    let config = ServerConfig {
        show_path: Some(path.clone()),
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        autosave: None,
        announce: false,
        ..ServerConfig::default()
    };
    let server = mazzalab_server::start(config, workspace, |_| Vec::new())
        .await
        .unwrap();

    server
        .handle
        .execute(Command::RenameWorkspace {
            name: "Night, edited".into(),
        })
        .await
        .unwrap();
    let client = UdpSocket::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .unwrap();
    let go = mazzalab_osc::encode(&OscMessage {
        addr: "/go".into(),
        args: Vec::new(),
    })
    .unwrap();
    client.send_to(&go, server.osc_udp).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while server.handle.status().await.unwrap().running.is_empty() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the OSC GO never arrived"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    server.stop().await;

    let records: Vec<_> = std::fs::read_dir(dir.join("logs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(records.len(), 1, "{records:?}");
    let name = records[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(
        name.starts_with("Night ") && name.ends_with(".jsonl"),
        "{name}"
    );

    let mut inputs = Vec::new();
    let show = mazzalab_show::replay(
        std::io::BufReader::new(std::fs::File::open(&records[0]).unwrap()),
        |_, entry| inputs.push(entry.input.clone()),
    )
    .unwrap();
    assert!(
        matches!(
            &inputs[..],
            [Input::Command { command }, Input::Osc { .. }]
                if matches!(**command, Command::RenameWorkspace { .. })
        ),
        "{inputs:?}"
    );
    assert_eq!(show.workspace().name, "Night, edited");
    assert!(
        !show.status().running.is_empty(),
        "the replayed GO started the cue"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
