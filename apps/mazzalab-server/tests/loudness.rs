//! The server measures each audio file's loudness in the background.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::time::Duration;

use mazzalab_core::{AudioCue, Cue, CueKind, CueList, Workspace};
use mazzalab_server::ServerConfig;

/// A 16-bit mono WAV of a 1 kHz sine at `amplitude`.
fn sine_wav(amplitude: f64, seconds: u32) -> Vec<u8> {
    let rate = 48_000_u32;
    let frames = rate * seconds;
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
        let value = amplitude
            * (std::f64::consts::TAU * 1_000.0 * f64::from(frame) / f64::from(rate)).sin();
        bytes.extend_from_slice(&((value * 32_767.0) as i16).to_le_bytes());
    }
    bytes
}

#[tokio::test]
async fn audio_files_get_their_loudness() {
    let dir = std::env::temp_dir().join(format!("mazzalab-loudness-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("media")).unwrap();
    // A sine at -20 dBFS in one channel: about -23 LUFS.
    std::fs::write(dir.join("media/tone.wav"), sine_wav(0.1, 3)).unwrap();
    let cue = Cue::new(CueKind::Audio(AudioCue {
        file: Some(PathBuf::from("media/tone.wav")),
        ..AudioCue::default()
    }));
    let id = cue.id;
    let mut workspace = Workspace::new("Loud");
    workspace.cue_lists = vec![CueList::new("Main").with_cues(vec![cue])];
    let config = ServerConfig {
        show_path: Some(dir.join("Loud.mazzalab")),
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        autosave: None,
        announce: false,
        record_inputs: false,
        ..ServerConfig::default()
    };
    let server = mazzalab_server::start(config, workspace, |_| Vec::new())
        .await
        .unwrap();

    let lufs = async || match &server
        .handle
        .snapshot()
        .await
        .unwrap()
        .workspace
        .find_cue(id)
        .unwrap()
        .kind
    {
        CueKind::Audio(audio) => audio.media_lufs,
        _ => None,
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while lufs().await.is_none() {
        assert!(tokio::time::Instant::now() < deadline, "never measured");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let measured = lufs().await.unwrap();
    assert!((measured + 23.0).abs() < 0.3, "{measured}");
    // Measuring is not an edit.
    assert!(!server.handle.snapshot().await.unwrap().dirty);
    server.shutdown();
    let _ = std::fs::remove_dir_all(dir);
}
