//! A Network cue whose OSC value fades, and stops with the cue, through the
//! server's network backend.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use mazzalab_core::{Cue, CueKind, CueList, NetworkCue, NetworkPatch, NetworkProtocol, Workspace};
use mazzalab_server::{NetworkBackend, Output, ServerConfig};
use mazzalab_show::Command;
use rosc::OscType;
use tokio::net::UdpSocket;
use tokio::time::timeout;

const TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn a_value_fades_over_the_cues_duration_and_a_stop_ends_it() {
    let receiver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = receiver.local_addr().unwrap().port();
    let fade = Cue::new(CueKind::Network(NetworkCue {
        patch: Some("Desk".into()),
        message: "/fader/1 0.0>1.0".into(),
        duration: Duration::from_millis(300),
    }))
    .with_number("1");
    let fade_id = fade.id;
    let mut workspace = Workspace::new("Fade");
    workspace.cue_lists = vec![CueList::new("Main")];
    workspace.cue_lists[0].cues = vec![fade];
    workspace.settings.network_patches = vec![NetworkPatch {
        name: "Desk".into(),
        host: "127.0.0.1".into(),
        port,
        protocol: NetworkProtocol::OscUdp,
    }];
    let config = ServerConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        announce: false,
        ..ServerConfig::default()
    };
    let server = mazzalab_server::start(config, workspace, |completion| {
        vec![Box::new(NetworkBackend::new(completion)) as Box<dyn Output>]
    })
    .await
    .unwrap();

    server
        .handle
        .execute(Command::Go { list: None })
        .await
        .unwrap();
    let mut buffer = [0_u8; 256];
    let mut values = Vec::new();
    loop {
        let (length, _) = timeout(TIMEOUT, receiver.recv_from(&mut buffer))
            .await
            .expect("the fade stopped coming")
            .unwrap();
        let message = mazzalab_osc::decode(&buffer[..length]).unwrap().remove(0);
        let OscType::Float(value) = message.args[0] else {
            panic!("a float")
        };
        values.push(value);
        if value >= 1.0 {
            break;
        }
    }
    assert!(values.len() >= 3, "{values:?}");
    assert_eq!(values[0], 0.0);

    // A second GO starts it again; stopping the cue ends the fade short.
    server
        .handle
        .execute(Command::Go { list: None })
        .await
        .unwrap_or(());
    server
        .handle
        .execute(Command::Start { cue: fade_id })
        .await
        .unwrap();
    timeout(TIMEOUT, receiver.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    server
        .handle
        .execute(Command::Stop { cue: fade_id })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    while timeout(Duration::from_millis(100), receiver.recv_from(&mut buffer))
        .await
        .is_ok()
    {}
    assert!(
        timeout(Duration::from_millis(300), receiver.recv_from(&mut buffer))
            .await
            .is_err(),
        "the fade went on after the stop"
    );
    server.stop().await;
}
