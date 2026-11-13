//! Light cues, end to end: from GO to Art-Net packets on the network.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use mazzalab_core::{
    Cue, CueKind, DmxOutput, DmxProtocol, Fixture, FixtureGroup, LightCue, Workspace,
};
use mazzalab_server::{LightingBackend, NoticeLevel, Output, ServerConfig, Update};
use mazzalab_show::Command;
use tokio::net::UdpSocket;
use tokio::time::timeout;

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const TIMEOUT: Duration = Duration::from_secs(5);

fn light(number: &str, commands: &[&str], seconds: f64) -> Cue {
    Cue::new(CueKind::Light(LightCue {
        commands: commands.iter().map(|line| (*line).to_owned()).collect(),
        duration: Duration::from_secs_f64(seconds),
    }))
    .with_number(number)
}

/// Reads Art-Net packets until one satisfies `done`; returns its DMX data.
async fn dmx_until(socket: &UdpSocket, done: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    let mut buffer = [0_u8; 1024];
    timeout(TIMEOUT, async {
        loop {
            let (length, _) = socket.recv_from(&mut buffer).await.unwrap();
            assert_eq!(&buffer[..8], b"Art-Net\0");
            let data = &buffer[18..length];
            if done(data) {
                return data.to_vec();
            }
        }
    })
    .await
    .expect("the expected DMX never came")
}

#[tokio::test]
async fn light_cues_fade_dmx_out_and_report_what_they_cannot_read() {
    let node = UdpSocket::bind((LOCALHOST, 0)).await.unwrap();
    let mut workspace = Workspace::new("Lights");
    workspace.settings.dmx_outputs = vec![DmxOutput {
        universe: 1,
        protocol: DmxProtocol::ArtNet,
        host: format!("127.0.0.1:{}", node.local_addr().unwrap().port()),
        net_universe: 0,
    }];
    workspace.cue_lists[0].cues = vec![
        light("1", &["1 thru 3 @ full", "", "10 @ 50"], 0.0),
        light("2", &["1 @ out", "front wash @ 75"], 0.4),
    ];

    let config = ServerConfig {
        bind: LOCALHOST,
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        ..ServerConfig::default()
    };
    let server = mazzalab_server::start(config, workspace, |completion| {
        vec![Box::new(LightingBackend::new(completion)) as Box<dyn Output>]
    })
    .await
    .unwrap();
    let mut updates = server.handle.subscribe();

    // Nothing is lit before the first cue, and the node is kept alive.
    let dark = dmx_until(&node, |_| true).await;
    assert!(dark.iter().all(|value| *value == 0));
    assert_eq!(server.handle.dmx_levels().await, Some(Vec::new()));

    server
        .handle
        .execute(Command::Go { list: None })
        .await
        .unwrap();
    let look = dmx_until(&node, |data| data[0] == 255).await;
    assert_eq!(look[..4], [255, 255, 255, 0]);
    assert_eq!(look[9], 128);

    // Clients are told which slots are lit.
    let levels = timeout(TIMEOUT, async {
        loop {
            if let Update::Dmx(levels) = updates.recv().await.unwrap()
                && !levels.is_empty()
            {
                break levels;
            }
        }
    })
    .await
    .expect("no DMX update");
    assert_eq!(
        *levels,
        vec![(1, 1, 255), (1, 2, 255), (1, 3, 255), (1, 10, 128)]
    );

    // The second cue fades slot 1 out; the line that makes no sense is
    // reported and the rest still runs.
    server
        .handle
        .execute(Command::Go { list: None })
        .await
        .unwrap();
    dmx_until(&node, |data| (40..220).contains(&data[0])).await;
    let end = dmx_until(&node, |data| data[0] == 0).await;
    assert_eq!(end[1..3], [255, 255], "untouched slots keep their level");
    let notice = timeout(TIMEOUT, async {
        loop {
            if let Update::Notice(notice) = updates.recv().await.unwrap() {
                break notice;
            }
        }
    })
    .await
    .expect("no notice");
    assert_eq!(notice.level, NoticeLevel::Warning);
    assert!(notice.message.contains("front wash"), "{}", notice.message);

    assert_eq!(
        server.handle.dmx_levels().await,
        Some(vec![(1, 2, 255), (1, 3, 255), (1, 10, 128)])
    );
    server.stop().await;
}

#[tokio::test]
async fn light_cues_call_fixtures_and_groups_of_the_show_patch() {
    let node = UdpSocket::bind((LOCALHOST, 0)).await.unwrap();
    let mut workspace = Workspace::new("Patch");
    workspace.settings.dmx_outputs = vec![DmxOutput {
        universe: 1,
        protocol: DmxProtocol::ArtNet,
        host: format!("127.0.0.1:{}", node.local_addr().unwrap().port()),
        net_universe: 0,
    }];
    workspace.settings.fixtures = vec![
        Fixture::dimmer("Front left", 1, 1),
        Fixture::dimmer("Front right", 1, 2),
        Fixture::dimmer("Cyc", 1, 20).with_parameters(["intensity", "red", "green", "blue"]),
    ];
    workspace.settings.fixture_groups = vec![FixtureGroup {
        name: "Front wash".into(),
        members: vec!["Front left".into(), "Front right".into()],
    }];
    workspace.cue_lists[0].cues = vec![
        light(
            "1",
            &["front wash @ 75", "cyc @ full", "cyc @ #ff8000"],
            0.0,
        ),
        light("2", &["front wash @ out"], 0.0),
    ];
    let config = ServerConfig {
        bind: LOCALHOST,
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        ..ServerConfig::default()
    };
    let server = mazzalab_server::start(config, workspace, |completion| {
        vec![Box::new(LightingBackend::new(completion)) as Box<dyn Output>]
    })
    .await
    .unwrap();

    let go = async || {
        server
            .handle
            .execute(Command::Go { list: None })
            .await
            .unwrap();
    };
    go().await;
    let look = dmx_until(&node, |data| data[0] == 191 && data[19] == 255).await;
    assert_eq!(look[..2], [191, 191]);
    assert_eq!(look[19..23], [255, 255, 128, 0]);

    // The patch is the show's: moved while the show runs, the group follows.
    let mut settings = server.handle.snapshot().await.unwrap().workspace.settings;
    settings.fixtures[1].address = 5;
    server
        .handle
        .execute(Command::UpdateSettings { settings })
        .await
        .unwrap();
    go().await;
    let look = dmx_until(&node, |data| data[0] == 0).await;
    assert_eq!(look[1], 191, "the old address is no longer the fixture");
    assert_eq!(look[4], 0);
    server.stop().await;
}
