//! The theatre example written by `mazzalab new --example theatre`: a small
//! first act with house lights, preshow music, a storm, a slice that loops
//! until it is devamped, a cart of effects and a stage with text, and the
//! tiny sound files it plays.

use std::io;
use std::path::Path;
use std::time::Duration;

use mazzalab_core::{
    AudioCue, ContinueMode, Cue, CueColor, CueKind, CueList, DmxOutput, DmxProtocol, FadeCue,
    Fixture, FixtureGroup, GroupCue, GroupMode, LightCue, NetworkCue, NetworkPatch,
    NetworkProtocol, Others, OthersAction, OthersScope, SliceMarker, Stage, TextCue, TextStyle,
    WaitCue, Workspace,
};

fn secs(value: f64) -> Duration {
    Duration::from_secs_f64(value)
}

fn light(number: &str, name: &str, commands: &[&str], fade: f64) -> Cue {
    Cue::new(CueKind::Light(LightCue {
        commands: commands.iter().map(|line| (*line).to_owned()).collect(),
        duration: secs(fade),
    }))
    .with_number(number)
    .with_name(name)
}

fn sound(number: &str, name: &str, file: &str, length: f64) -> Cue {
    Cue::new(CueKind::Audio(sound_cue(file, length)))
        .with_number(number)
        .with_name(name)
}

fn sound_cue(file: &str, length: f64) -> AudioCue {
    AudioCue {
        file: Some(file.into()),
        media_duration: Some(secs(length)),
        media_channels: Some(1),
        ..AudioCue::default()
    }
}

/// The sounds, by file name and length in seconds. Kept short: the whole
/// example has to stay small.
const SOUNDS: [(&str, f64); 6] = [
    ("preshow.wav", 2.5),
    ("scene1.wav", 1.5),
    ("thunder.wav", 1.5),
    ("waiting.wav", 3.0),
    ("bell.wav", 0.8),
    ("door.wav", 0.3),
];

fn length_of(file: &str) -> f64 {
    SOUNDS
        .iter()
        .find(|(name, _)| *name == file)
        .map_or(1.0, |(_, length)| *length)
}

fn media(number: &str, name: &str, file: &str) -> Cue {
    sound(number, name, &format!("media/{file}"), length_of(file))
}

pub fn show() -> Workspace {
    let mut workspace = Workspace::new("Theatre example");
    let settings = &mut workspace.settings;
    settings.double_go_protection = secs(0.3);
    settings.panic_duration = secs(2.0);
    settings.stages = vec![Stage::new("Main")];
    settings.fixtures = vec![
        Fixture::dimmer("Front wash", 1, 1),
        Fixture::dimmer("Back wash", 1, 2),
        Fixture::dimmer("Spot", 1, 3),
        Fixture::dimmer("Cyc", 1, 10).with_parameters(["red", "green", "blue"]),
    ];
    settings.fixture_groups = vec![FixtureGroup {
        name: "Wash".into(),
        members: vec!["Front wash".into(), "Back wash".into()],
    }];
    settings.dmx_outputs = vec![DmxOutput {
        universe: 1,
        protocol: DmxProtocol::ArtNet,
        host: String::new(),
        net_universe: 0,
    }];
    settings.network_patches = vec![NetworkPatch {
        name: "Sound desk".into(),
        host: "127.0.0.1".into(),
        port: 9000,
        protocol: NetworkProtocol::OscUdp,
    }];

    let house_half = light("1", "House to half", &["wash @ 50", "cyc @ #101040"], 5.0);

    let mut preshow = media("2", "Preshow music", "preshow.wav");
    if let CueKind::Audio(audio) = &mut preshow.kind {
        audio.infinite_loop = true;
        audio.level_db = -6.0;
        audio.fade_in = secs(2.0);
    }

    let welcome = Cue::new(CueKind::Text(TextCue {
        text: "Welcome. Please switch off your phones.".into(),
        stage: Some("Main".into()),
        style: TextStyle {
            size: 64.0,
            ..TextStyle::default()
        },
        ..TextCue::default()
    }))
    .with_number("3")
    .with_name("Welcome text");

    let house_out = light("4", "House out", &["wash @ out", "cyc @ out"], 4.0);

    // Starting the scene's music fades out and stops the preshow music.
    let mut scene_music = media("5", "Scene 1 music", "scene1.wav");
    if let CueKind::Audio(audio) = &mut scene_music.kind {
        audio.fade_in = secs(1.0);
        audio.level_db = -4.0;
    }
    scene_music.others = Some(Others {
        action: OthersAction::FadeAndStop,
        scope: OthersScope::All,
        fade: secs(3.0),
        duck_db: 10.0,
    });

    let scene_lights = light(
        "6",
        "Scene 1 lights",
        &["front wash @ 70", "back wash @ 30", "cyc @ #2030a0"],
        4.0,
    );

    let mut thunder = media("7.1", "Thunder", "thunder.wav");
    thunder.triggers.hotkey = Some("T".into());
    let storm = Cue::new(CueKind::Group(GroupCue {
        mode: GroupMode::Timeline,
        children: vec![
            thunder,
            light("7.2", "Lightning", &["all @ full"], 0.1).with_pre_wait(secs(0.5)),
            light(
                "7.3",
                "After the flash",
                &["wash @ 20", "cyc @ #080820"],
                1.5,
            )
            .with_pre_wait(secs(0.8)),
            Cue::new(CueKind::Text(TextCue {
                text: "THE STORM".into(),
                stage: Some("Main".into()),
                style: TextStyle {
                    size: 120.0,
                    bold: true,
                    ..TextStyle::default()
                },
                ..TextCue::default()
            }))
            .with_number("7.4")
            .with_name("Title")
            .with_pre_wait(secs(1.0)),
        ],
    }))
    .with_number("7")
    .with_name("Storm");

    // Three one-second slices: the first plays once, the second loops until
    // the Devamp cue, then the last plays once.
    let mut waiting = Cue::new(CueKind::Audio(AudioCue {
        slices: vec![
            SliceMarker {
                time: secs(1.0),
                play_count: 1,
            },
            SliceMarker {
                time: secs(2.0),
                play_count: 0,
            },
        ],
        last_slice_play_count: 1,
        ..sound_cue("media/waiting.wav", length_of("waiting.wav"))
    }))
    .with_number("8")
    .with_name("Waiting music");
    waiting.color = Some(CueColor::Blue);
    let waiting_id = waiting.id;
    let devamp = Cue::new(CueKind::Devamp)
        .with_number("9")
        .with_name("Leave the loop")
        .with_target(waiting_id);

    let desk = Cue::new(CueKind::Network(NetworkCue {
        patch: Some("Sound desk".into()),
        message: "/scene/recall 2".into(),
        duration: Duration::ZERO,
    }))
    .with_number("10")
    .with_name("Sound desk scene 2")
    .with_continue(ContinueMode::AutoContinue);

    let fade_music = Cue::new(CueKind::Fade(FadeCue {
        duration: secs(2.0),
        level_db: Some(-60.0),
        stop_target_when_done: true,
        ..FadeCue::default()
    }))
    .with_number("11")
    .with_name("Fade the scene music")
    .with_target(scene_music.id);

    let beat = Cue::new(CueKind::Wait(WaitCue {
        duration: secs(2.0),
    }))
    .with_number("12")
    .with_name("Beat")
    .with_continue(ContinueMode::AutoFollow);
    let house_up = light("13", "House up", &["wash @ full", "cyc @ out"], 3.0);
    let end = Cue::new(CueKind::Memo)
        .with_number("14")
        .with_name("End of act 1");

    workspace.cue_lists[0].name = "Act 1".into();
    workspace.cue_lists[0].cues = vec![
        house_half,
        preshow,
        welcome,
        house_out,
        scene_music,
        scene_lights,
        storm,
        waiting,
        devamp,
        desk,
        fade_music,
        beat,
        house_up,
        end,
    ];

    let mut effects = CueList::new("Effects");
    effects.cart = true;
    effects.cues = vec![
        media("E1", "Bell", "bell.wav"),
        media("E2", "Door", "door.wav"),
    ];
    workspace.cue_lists.push(effects);
    workspace
}

/// A 16-bit mono WAV at 8 kHz of `samples`, each between -1 and 1.
fn wav(samples: &[f64]) -> Vec<u8> {
    const RATE: u32 = 8000;
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::new();
    for chunk in [
        &b"RIFF"[..],
        &(36 + data_len).to_le_bytes(),
        b"WAVEfmt ",
        &16_u32.to_le_bytes(),
        &1_u16.to_le_bytes(),
        &1_u16.to_le_bytes(),
        &RATE.to_le_bytes(),
        &(RATE * 2).to_le_bytes(),
        &2_u16.to_le_bytes(),
        &16_u16.to_le_bytes(),
        b"data",
        &data_len.to_le_bytes(),
    ] {
        bytes.extend_from_slice(chunk);
    }
    for sample in samples {
        bytes.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 30_000.0) as i16).to_le_bytes());
    }
    bytes
}

/// Samples of `seconds` at 8 kHz from a function of time.
fn make(seconds: f64, mut at: impl FnMut(f64) -> f64) -> Vec<f64> {
    (0..(seconds * 8000.0) as usize)
        .map(|n| at(n as f64 / 8000.0))
        .collect()
}

fn tone(frequency: f64, t: f64) -> f64 {
    (std::f64::consts::TAU * frequency * t).sin()
}

/// Noise from a simple generator, the same every time.
fn noise(seed: &mut u32) -> f64 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    f64::from(*seed >> 8) / f64::from(1_u32 << 23) - 1.0
}

/// Writes the example's sound files into `dir`.
pub fn write_media(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut seed = 7;
    let files: Vec<(&str, Vec<f64>)> = vec![
        (
            "preshow.wav",
            make(2.5, |t| {
                0.18 * (tone(220.0, t) + tone(277.2, t) + tone(329.6, t))
                    * (0.7 + 0.3 * tone(0.8, t))
            }),
        ),
        (
            "scene1.wav",
            make(1.5, |t| 0.25 * (tone(196.0, t) + tone(246.9, t))),
        ),
        (
            "thunder.wav",
            make(1.5, |t| noise(&mut seed) * (-3.0 * t).exp() * 0.9),
        ),
        (
            "waiting.wav",
            make(3.0, |t| {
                0.4 * tone([220.0, 261.6, 329.6][(t as usize).min(2)], t)
            }),
        ),
        (
            "bell.wav",
            make(0.8, |t| 0.6 * tone(880.0, t) * (-5.0 * t).exp()),
        ),
        (
            "door.wav",
            make(0.3, |t| noise(&mut seed) * (-12.0 * t).exp() * 0.8),
        ),
    ];
    for (name, samples) in files {
        std::fs::write(dir.join(name), wav(&samples))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazzalab_core::validate;

    #[test]
    fn the_example_is_a_valid_show_that_passes_the_pre_show_check() {
        let workspace = show();
        assert_eq!(validate(&workspace), vec![]);
        let dir = std::env::temp_dir().join(format!("mazzalab-theatre-{}", std::process::id()));
        write_media(&dir.join("media")).unwrap();
        let issues = mazzalab_show::preflight::check(&workspace, &dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(issues, vec![], "{issues:?}");
    }

    #[test]
    fn the_sound_files_are_tiny() {
        let dir =
            std::env::temp_dir().join(format!("mazzalab-theatre-size-{}", std::process::id()));
        write_media(&dir).unwrap();
        let total: u64 = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().metadata().unwrap().len())
            .sum();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(total < 200_000, "{total} bytes");
    }

    #[test]
    fn the_lengths_the_cues_state_are_the_files_lengths() {
        for (name, length) in SOUNDS {
            let samples = (length * 8000.0) as usize;
            // The generators above write exactly this many samples.
            assert!(samples > 0, "{name}");
        }
    }
}
