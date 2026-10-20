//! The demo show written by `culab new`.

use std::time::Duration;

use culab_core::{
    AudioCue, ContinueMode, Cue, CueKind, FadeCue, GroupCue, GroupMode, LightCue, MidiCue,
    MidiMessage, NetworkCue, VideoCue, WaitCue, Workspace,
};

fn secs(value: f64) -> Duration {
    Duration::from_secs_f64(value)
}

pub fn show() -> Workspace {
    let preshow = Cue::new(CueKind::Audio(AudioCue {
        file: Some("audio/preshow.wav".into()),
        level_db: -6.0,
        media_duration: Some(secs(180.0)),
        ..AudioCue::default()
    }))
    .with_number("1")
    .with_name("Preshow music");

    let fade_out = Cue::new(CueKind::Fade(FadeCue {
        duration: secs(5.0),
        level_db: Some(-60.0),
        stop_target_when_done: true,
        ..FadeCue::default()
    }))
    .with_number("2")
    .with_name("Fade out preshow")
    .with_target(preshow.id)
    .with_post_wait(secs(3.0))
    .with_continue(ContinueMode::AutoContinue);

    let blackout = Cue::new(CueKind::Light(LightCue {
        commands: vec!["all = 0".into()],
        duration: secs(3.0),
    }))
    .with_number("3")
    .with_name("Blackout");

    let storm = Cue::new(CueKind::Group(GroupCue {
        mode: GroupMode::Timeline,
        children: vec![
            Cue::new(CueKind::Audio(AudioCue {
                file: Some("audio/thunder.wav".into()),
                media_duration: Some(secs(8.0)),
                ..AudioCue::default()
            }))
            .with_number("4.1")
            .with_name("Thunder"),
            Cue::new(CueKind::Light(LightCue {
                commands: vec!["strobe = 100".into()],
                duration: secs(0.2),
            }))
            .with_number("4.2")
            .with_name("Lightning")
            .with_pre_wait(secs(0.5)),
            Cue::new(CueKind::Video(VideoCue {
                file: Some("video/rain.mp4".into()),
                stage: Some("Main stage".into()),
                media_duration: Some(secs(30.0)),
                ..VideoCue::default()
            }))
            .with_number("4.3")
            .with_name("Rain projection")
            .with_pre_wait(secs(1.0)),
        ],
    }))
    .with_number("4")
    .with_name("Storm")
    .with_continue(ContinueMode::AutoFollow);

    let scene_change = Cue::new(CueKind::Network(NetworkCue {
        patch: Some("Sound desk".into()),
        message: "/scene/recall 2".into(),
        duration: Duration::ZERO,
    }))
    .with_number("5")
    .with_name("Sound desk scene 2")
    .with_continue(ContinueMode::AutoContinue);

    let program_change = Cue::new(CueKind::Midi(MidiCue {
        patch: Some("Keyboard rig".into()),
        message: MidiMessage::ProgramChange {
            channel: 1,
            program: 12,
        },
    }))
    .with_number("6")
    .with_name("Keys patch: strings");

    let interval = Cue::new(CueKind::Wait(WaitCue {
        duration: secs(2.0),
    }))
    .with_number("7")
    .with_name("Interval countdown")
    .with_continue(ContinueMode::AutoFollow);

    let end = Cue::new(CueKind::Memo)
        .with_number("8")
        .with_name("End of act 1");

    let mut workspace = Workspace::new("CuLab demo");
    workspace.settings.double_go_protection = secs(0.3);
    workspace.cue_lists[0].cues = vec![
        preshow,
        fade_out,
        blackout,
        storm,
        scene_change,
        program_change,
        interval,
        end,
    ];
    workspace
}
