//! Plays through the real default output device. Ignored by default because
//! CI machines have no sound card: run with `cargo test -- --ignored`.

use std::f32::consts::TAU;
use std::sync::mpsc;
use std::time::Duration;

use mazzalab_audio::{AudioOutput, DeviceRequest, output_devices, probe_workspace};
use mazzalab_core::{
    ActionLength, AudioCue, Cue, CueKind, SliceMarker, SmpteFormat, TimecodeCue, TimecodeSignal,
    Workspace,
};
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

#[test]
#[ignore = "needs an audio output device"]
fn a_loaded_cue_plays_only_what_is_left_and_a_loop_can_be_devamped() {
    let dir = std::env::temp_dir().join(format!("mazzalab-device-load-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write_tone(&dir.join("tone.wav"), 44_100, 1.0);

    let mut workspace = Workspace::new("device test");
    let once = Cue::new(CueKind::Audio(AudioCue {
        file: Some("tone.wav".into()),
        level_db: -6.0,
        ..AudioCue::default()
    }));
    let looped = Cue::new(CueKind::Audio(AudioCue {
        file: Some("tone.wav".into()),
        level_db: -6.0,
        infinite_loop: true,
        ..AudioCue::default()
    }));
    let devamp = Cue::new(CueKind::Devamp).with_target(looped.id);
    let (once_id, looped_id, devamp_id) = (once.id, looped.id, devamp.id);
    workspace.cue_lists[0].cues = vec![once, looped, devamp];
    assert!(probe_workspace(&mut workspace, &dir).is_empty());

    let (finished_tx, finished) = mpsc::channel();
    let mut audio = AudioOutput::start(&dir, DeviceRequest::default(), move |cue| {
        let _ = finished_tx.send(cue);
    })
    .unwrap();
    let mut engine = Engine::new(workspace);
    let run = |engine: &mut Engine, audio: &mut AudioOutput| {
        let events = engine.take_events();
        audio.handle_events(engine.workspace(), &events);
    };

    // Loaded 0.8 s into a one second file: a fifth of a second is left.
    engine.load(once_id, Duration::from_millis(800)).unwrap();
    engine.start(once_id).unwrap();
    let started = std::time::Instant::now();
    run(&mut engine, &mut audio);
    assert_eq!(
        finished.recv_timeout(Duration::from_secs(5)).unwrap(),
        once_id
    );
    let took = started.elapsed();
    println!("loaded cue took {took:?}");
    assert!(took < Duration::from_millis(600), "{took:?}");

    // A loop devamped 1.3 s in ends with its second pass, at about 2 s.
    engine.start(looped_id).unwrap();
    let started = std::time::Instant::now();
    run(&mut engine, &mut audio);
    std::thread::sleep(Duration::from_millis(1300));
    engine.advance_to(Duration::from_millis(1300));
    engine.start(devamp_id).unwrap();
    run(&mut engine, &mut audio);
    assert_eq!(
        finished.recv_timeout(Duration::from_secs(5)).unwrap(),
        looped_id
    );
    let took = started.elapsed();
    println!("devamped loop took {took:?}");
    assert!(
        (Duration::from_millis(1800)..Duration::from_millis(2600)).contains(&took),
        "{took:?}"
    );
}

#[test]
#[ignore = "needs an audio output device"]
fn a_sliced_cue_vamps_until_devamped_then_plays_its_ending() {
    let dir = std::env::temp_dir().join(format!("mazzalab-device-slices-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write_tone(&dir.join("tone.wav"), 44_100, 1.0);

    // An intro to 0.3 s, a vamp from 0.3 to 0.6 s, an ending to 1 s.
    let mut workspace = Workspace::new("device test");
    let sliced = Cue::new(CueKind::Audio(AudioCue {
        file: Some("tone.wav".into()),
        level_db: -6.0,
        slices: vec![
            SliceMarker {
                time: Duration::from_millis(300),
                play_count: 1,
            },
            SliceMarker {
                time: Duration::from_millis(600),
                play_count: 0,
            },
        ],
        ..AudioCue::default()
    }));
    let devamp = Cue::new(CueKind::Devamp).with_target(sliced.id);
    let (sliced_id, devamp_id) = (sliced.id, devamp.id);
    workspace.cue_lists[0].cues = vec![sliced, devamp];
    assert!(probe_workspace(&mut workspace, &dir).is_empty());

    let (finished_tx, finished) = mpsc::channel();
    let mut audio = AudioOutput::start(&dir, DeviceRequest::default(), move |cue| {
        let _ = finished_tx.send(cue);
    })
    .unwrap();
    let mut engine = Engine::new(workspace);
    let run = |engine: &mut Engine, audio: &mut AudioOutput| {
        let events = engine.take_events();
        audio.handle_events(engine.workspace(), &events);
    };

    engine.start(sliced_id).unwrap();
    let started = std::time::Instant::now();
    run(&mut engine, &mut audio);
    // Well past the length of the file, the vamp still goes round.
    assert!(finished.recv_timeout(Duration::from_millis(1400)).is_err());
    // 1.4 s in: 1.1 s into the vamp, whose fourth pass ends at 1.5 s; then
    // the 0.4 s ending.
    engine.advance_to(Duration::from_millis(1400));
    engine.start(devamp_id).unwrap();
    run(&mut engine, &mut audio);
    assert_eq!(
        finished.recv_timeout(Duration::from_secs(5)).unwrap(),
        sliced_id
    );
    let took = started.elapsed();
    println!("sliced cue took {took:?}");
    assert!(
        (Duration::from_millis(1800)..Duration::from_millis(2400)).contains(&took),
        "{took:?}"
    );
}

#[test]
#[ignore = "needs an audio output device"]
fn a_timecode_cue_sends_ltc_for_its_duration() {
    let dir = std::env::temp_dir().join(format!("mazzalab-device-ltc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut workspace = Workspace::new("device test");
    let ltc = Cue::new(CueKind::Timecode(TimecodeCue {
        signal: TimecodeSignal::Ltc,
        format: SmpteFormat::Fps25,
        duration: Some(Duration::from_millis(500)),
        level_db: -30.0,
        ..TimecodeCue::default()
    }));
    let ltc_id = ltc.id;
    workspace.cue_lists[0].cues = vec![ltc];
    let (finished_tx, finished) = mpsc::channel();
    let mut audio = AudioOutput::start(&dir, DeviceRequest::default(), move |cue| {
        let _ = finished_tx.send(cue);
    })
    .unwrap();
    let mut engine = Engine::new(workspace);
    engine.start(ltc_id).unwrap();
    let started = std::time::Instant::now();
    let events = engine.take_events();
    audio.handle_events(engine.workspace(), &events);
    assert!(audio.playing_cues().contains(&ltc_id));
    assert_eq!(
        finished.recv_timeout(Duration::from_secs(5)).unwrap(),
        ltc_id
    );
    let took = started.elapsed();
    println!("LTC cue took {took:?}");
    assert!(
        (Duration::from_millis(400)..Duration::from_millis(900)).contains(&took),
        "{took:?}"
    );
}
