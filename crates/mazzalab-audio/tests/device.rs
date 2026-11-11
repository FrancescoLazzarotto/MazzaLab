//! Plays through the real default output device. Ignored by default because
//! CI machines have no sound card: run with `cargo test -- --ignored`.

use std::f32::consts::TAU;
use std::sync::mpsc;
use std::time::Duration;

use mazzalab_audio::{AudioOutput, DeviceRequest, output_devices, probe_workspace};
use mazzalab_core::{ActionLength, AudioCue, Cue, CueKind, Workspace};
use mazzalab_engine::Engine;

fn write_tone(path: &std::path::Path, rate: u32, seconds: f32) {
    let frames = (rate as f32 * seconds) as u32;
    let mut bytes = Vec::new();
    let data_len = frames * 2;
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
        let value = (TAU * 440.0 * frame as f32 / rate as f32).sin() * 0.1;
        bytes.extend_from_slice(&((value * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, bytes).unwrap();
}

#[test]
#[ignore = "needs an audio output device"]
fn plays_a_cue_to_its_end() {
    let dir = std::env::temp_dir().join(format!("mazzalab-device-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write_tone(&dir.join("tone.wav"), 44_100, 0.3);

    let mut workspace = Workspace::new("device test");
    let cue = Cue::new(CueKind::Audio(AudioCue {
        file: Some("tone.wav".into()),
        level_db: -6.0,
        ..AudioCue::default()
    }));
    let cue_id = cue.id;
    workspace.cue_lists[0].cues.push(cue);
    assert!(probe_workspace(&mut workspace, &dir).is_empty());
    let length = workspace.find_cue(cue_id).unwrap().kind.action_length();
    assert_eq!(length, ActionLength::Fixed(Duration::from_millis(300)));

    let (finished_tx, finished) = mpsc::channel();
    let mut audio = AudioOutput::start(&dir, DeviceRequest::default(), move |cue| {
        let _ = finished_tx.send(cue);
    })
    .unwrap();
    println!("device: {:?}", audio.info());
    assert!(!audio.info().fallback);

    let list = workspace.cue_lists[0].id;
    let mut engine = Engine::new(workspace);
    engine.go(list).unwrap();
    let events = engine.take_events();
    audio.handle_events(engine.workspace(), &events);

    let done = finished.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(done, cue_id);
    // The tone was at -6 dB of 0.1 full scale, about -26 dBFS.
    let peaks = audio.take_peaks_db();
    println!("peaks: {peaks:?}");
    assert!(peaks.iter().any(|peak| (-30.0..-20.0).contains(peak)));
}

#[test]
#[ignore = "needs an audio output device"]
fn lists_devices_and_falls_back_from_an_unknown_one() {
    let devices = output_devices();
    for device in &devices {
        println!("{device:?}");
    }
    assert!(devices.iter().any(|device| device.is_default));

    let request = DeviceRequest {
        device: Some("nowhere:no such device".into()),
        ..DeviceRequest::default()
    };
    let audio = AudioOutput::start(".", request, |_| {}).unwrap();
    assert!(audio.info().fallback);
}

#[test]
#[ignore = "needs an audio output device"]
fn opens_a_listed_device_by_id_with_a_buffer_size() {
    let device = output_devices()
        .into_iter()
        .find(|device| device.is_default)
        .unwrap();
    let request = DeviceRequest {
        device: Some(device.id.clone()),
        sample_rate: device.default_sample_rate,
        buffer_size: Some(512),
    };
    let audio = AudioOutput::start(".", request, |_| {}).unwrap();
    println!("opened: {:?}", audio.info());
    assert_eq!(audio.info().id, device.id);
    assert!(!audio.info().fallback);
}
