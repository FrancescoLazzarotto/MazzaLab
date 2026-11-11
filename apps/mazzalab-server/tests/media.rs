//! Media upload, probing, waveforms and network cues, end to end through the
//! server's public API. No audio device is needed.

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use mazzalab_core::{
    Cue, CueId, CueKind, CueList, NetworkCue, NetworkPatch, NetworkProtocol, Workspace,
};
use mazzalab_server::{NetworkBackend, NoticeLevel, Output, RunningServer, ServerConfig, Update};
use mazzalab_show::Command;
use rosc::OscType;
use serde_json::{Value, json};
use tokio::net::UdpSocket;
use tokio::time::timeout;
use tower::ServiceExt;

const LIST: CueId = CueId::from_u128(1000);
const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const TIMEOUT: Duration = Duration::from_secs(5);

/// A fresh folder for one test's show.
fn show_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mazzalab-media-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn server(show: Option<&Path>, workspace: Workspace, network: bool) -> RunningServer {
    let config = ServerConfig {
        show_path: show.map(Path::to_path_buf),
        bind: LOCALHOST,
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        ..ServerConfig::default()
    };
    mazzalab_server::start(config, workspace, move |completion| {
        let mut outputs: Vec<Box<dyn Output>> = Vec::new();
        if network {
            outputs.push(Box::new(NetworkBackend::new(completion)));
        }
        outputs
    })
    .await
    .unwrap()
}

fn empty_workspace() -> Workspace {
    let mut workspace = Workspace::new("Media test");
    workspace.cue_lists = vec![CueList::new("Main").with_id(LIST)];
    workspace
}

/// A 16-bit mono WAV holding a full-scale square wave.
fn square_wav(rate: u32, frames: u32) -> Vec<u8> {
    let data_len = frames * 2;
    let mut bytes = Vec::new();
    for chunk in [
        &b"RIFF"[..],
        &(36 + data_len).to_le_bytes(),
        b"WAVEfmt ",
        &16_u32.to_le_bytes(),
        &1_u16.to_le_bytes(),
        &1_u16.to_le_bytes(),
        &rate.to_le_bytes(),
        &(rate * 2).to_le_bytes(),
        &2_u16.to_le_bytes(),
        &16_u16.to_le_bytes(),
        b"data",
        &data_len.to_le_bytes(),
    ] {
        bytes.extend_from_slice(chunk);
    }
    for frame in 0..frames {
        let value: i16 = if (frame / 50) % 2 == 0 {
            32_000
        } else {
            -32_000
        };
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

async fn json_of(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

async fn upload(app: &Router, name: &str, bytes: Vec<u8>) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::post(format!("/api/media?name={name}"))
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (status, json_of(response).await)
}

async fn get(app: &Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (status, json_of(response).await)
}

#[tokio::test]
async fn uploaded_media_is_stored_probed_and_drawn() {
    let dir = show_dir("upload");
    let show = dir.join("show.mazzalab");
    let server = server(Some(&show), empty_workspace(), false).await;
    let app = mazzalab_server::router(server.handle.clone(), None);
    let wav = square_wav(8000, 4000);

    let (status, body) = upload(&app, "thunder.wav", wav.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["file"], "media/thunder.wav");
    assert_eq!(std::fs::read(dir.join("media/thunder.wav")).unwrap(), wav);

    // The same bytes again reuse the file; different bytes get a new name.
    let (_, body) = upload(&app, "thunder.wav", wav.clone()).await;
    assert_eq!(body["file"], "media/thunder.wav");
    let (_, body) = upload(&app, "thunder.wav", square_wav(8000, 800)).await;
    assert_eq!(body["file"], "media/thunder (2).wav");
    // A path in the name cannot escape the media folder.
    let (_, body) = upload(&app, "..%2F..%2Fescape.wav", wav.clone()).await;
    assert_eq!(body["file"], "media/escape.wav");
    assert!(!dir.parent().unwrap().join("escape.wav").exists());
    let (status, _) = upload(&app, "..", wav.clone()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (_, listing) = get(&app, "/api/media").await;
    let names: Vec<&str> = listing["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["escape.wav", "thunder (2).wav", "thunder.wav"]);

    // A cue created from the upload gets its length without anyone asking.
    let cue = CueId::from_u128(1);
    server
        .handle
        .execute(
            serde_json::from_value::<Command>(json!({
                "command": "insert_cue",
                "container": { "kind": "list", "id": LIST },
                "cue": { "id": cue, "type": "audio", "file": "media/thunder.wav" },
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    let snapshot = serde_json::to_value(server.handle.snapshot().await.unwrap()).unwrap();
    let stored = &snapshot["workspace"]["cue_lists"][0]["cues"][0];
    assert_eq!(stored["media_duration"], 0.5);
    assert_eq!(stored["media_channels"], 1);
    // Probing is not an edit, but the insert was.
    assert_eq!(snapshot["dirty"], true);

    let (status, waveform) = get(&app, &format!("/api/waveform?cue={cue}&buckets=40")).await;
    assert_eq!(status, StatusCode::OK, "{waveform}");
    assert_eq!(waveform["duration"], 0.5);
    assert_eq!(waveform["channels"], 1);
    let peaks = waveform["peaks"].as_array().unwrap();
    assert_eq!(peaks.len(), 80);
    // Each slice spans whole periods of the square wave: full swing both ways.
    assert_eq!(peaks[0], -124);
    assert_eq!(peaks[1], 124);

    let (status, _) = get(
        &app,
        "/api/waveform?cue=00000000-0000-0000-0000-000000000099",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    server.stop().await;
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn media_needs_a_show_file_and_audio_reports_no_backend() {
    let server = server(None, empty_workspace(), false).await;
    let app = mazzalab_server::router(server.handle.clone(), None);
    let (status, body) = upload(&app, "a.wav", square_wav(8000, 80)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    let (status, audio) = get(&app, "/api/audio").await;
    assert_eq!(status, StatusCode::OK);
    assert!(audio["status"].is_null());
    assert!(audio["devices"].is_array());
    server.stop().await;
}

#[tokio::test]
async fn an_unreadable_file_warns_the_operator_once() {
    let dir = show_dir("missing");
    let show = dir.join("show.mazzalab");
    let server = server(Some(&show), empty_workspace(), false).await;
    let mut updates = server.handle.subscribe();
    let insert = |id: u128, file: &str| {
        serde_json::from_value::<Command>(json!({
            "command": "insert_cue",
            "container": { "kind": "list", "id": LIST },
            "cue": { "id": CueId::from_u128(id), "type": "audio", "file": file },
        }))
        .unwrap()
    };
    server
        .handle
        .execute(insert(1, "nowhere.wav"))
        .await
        .unwrap();
    // Another edit must not probe the missing file again.
    server
        .handle
        .execute(insert(2, "nowhere-too.wav"))
        .await
        .unwrap();
    server.handle.snapshot().await.unwrap();

    let mut warnings = Vec::new();
    while let Ok(Ok(update)) = timeout(Duration::from_millis(300), updates.recv()).await {
        if let Update::Notice(notice) = update {
            assert_eq!(notice.level, NoticeLevel::Warning);
            warnings.push(notice.message.clone());
        }
    }
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings[0].contains("nowhere.wav"));
    assert!(warnings[1].contains("nowhere-too.wav"));
    server.stop().await;
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn network_cues_send_osc_and_report_missing_patches() {
    let receiver = UdpSocket::bind((LOCALHOST, 0)).await.unwrap();
    let port = receiver.local_addr().unwrap().port();

    let to_desk = Cue::new(CueKind::Network(NetworkCue {
        patch: Some("Desk".into()),
        message: "/eos/cue/1/fire\n/eos/sub/3 0.75".into(),
        duration: Duration::ZERO,
    }))
    .with_number("1");
    let to_nobody = Cue::new(CueKind::Network(NetworkCue {
        patch: Some("Projector".into()),
        message: "/shutter 1".into(),
        duration: Duration::ZERO,
    }))
    .with_number("2");
    let mut workspace = empty_workspace();
    workspace.settings.network_patches = vec![NetworkPatch {
        name: "Desk".into(),
        host: "127.0.0.1".into(),
        port,
        protocol: NetworkProtocol::OscUdp,
    }];
    workspace.cue_lists[0].cues = vec![to_desk, to_nobody];

    let server = server(None, workspace, true).await;
    let mut updates = server.handle.subscribe();

    server
        .handle
        .execute(Command::Go { list: None })
        .await
        .unwrap();
    let mut buffer = [0_u8; 1024];
    let mut received = Vec::new();
    for _ in 0..2 {
        let (length, _) = timeout(TIMEOUT, receiver.recv_from(&mut buffer))
            .await
            .expect("no OSC message")
            .unwrap();
        received.push(mazzalab_osc::decode(&buffer[..length]).unwrap().remove(0));
    }
    assert_eq!(received[0].addr, "/eos/cue/1/fire");
    assert!(received[0].args.is_empty());
    assert_eq!(received[1].addr, "/eos/sub/3");
    assert_eq!(received[1].args, vec![OscType::Float(0.75)]);

    server
        .handle
        .execute(Command::Go { list: None })
        .await
        .unwrap();
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
    assert!(notice.message.contains("Projector"), "{}", notice.message);
    server.stop().await;
}

#[tokio::test]
async fn midi_cues_report_missing_patches_and_ports() {
    use mazzalab_core::{MidiCue, MidiMessage, MidiPatch};
    use mazzalab_server::MidiBackend;

    let program_change = |patch: Option<&str>, number: &str| {
        Cue::new(CueKind::Midi(MidiCue {
            patch: patch.map(str::to_owned),
            message: MidiMessage::ProgramChange {
                channel: 1,
                program: 3,
            },
        }))
        .with_number(number)
    };
    let mut workspace = empty_workspace();
    workspace.settings.midi_patches = vec![MidiPatch {
        name: "Desk".into(),
        port: "a port that is not there".into(),
    }];
    workspace.cue_lists[0].cues = vec![
        program_change(Some("Keys"), "1"),
        program_change(Some("Desk"), "2"),
    ];

    let config = ServerConfig {
        bind: LOCALHOST,
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        ..ServerConfig::default()
    };
    let server = mazzalab_server::start(config, workspace, |completion| {
        vec![Box::new(MidiBackend::new(completion)) as Box<dyn Output>]
    })
    .await
    .unwrap();
    let mut updates = server.handle.subscribe();
    let mut next_notice = async || {
        // Listing MIDI devices for the first time can take the system a while.
        timeout(Duration::from_secs(60), async {
            loop {
                if let Update::Notice(notice) = updates.recv().await.unwrap() {
                    break notice;
                }
            }
        })
        .await
        .expect("no notice")
    };

    // A patch that does not exist is caught before anything is sent.
    server
        .handle
        .execute(Command::Go { list: None })
        .await
        .unwrap();
    let notice = next_notice().await;
    assert_eq!(notice.level, NoticeLevel::Warning);
    assert!(notice.message.contains("Keys"), "{}", notice.message);

    // A patch whose port is missing fails on delivery, naming the port.
    server
        .handle
        .execute(Command::Go { list: None })
        .await
        .unwrap();
    let notice = next_notice().await;
    assert!(
        notice.message.contains("a port that is not there"),
        "{}",
        notice.message
    );

    let app = mazzalab_server::router(server.handle.clone(), None);
    let (status, ports) = get(&app, "/api/midi").await;
    assert_eq!(status, StatusCode::OK);
    assert!(ports["outputs"].is_array() && ports["inputs"].is_array());
    server.stop().await;
}
