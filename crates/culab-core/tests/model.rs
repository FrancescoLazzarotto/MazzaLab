use std::time::Duration;

use culab_core::{
    ActionLength, AudioCue, ContinueMode, Cue, CueId, CueKind, CueList, FadeCue, GroupCue,
    GroupMode, Issue, MidiCue, MidiMessage, VideoCue, WaitCue, Workspace, validate,
};

fn secs(value: f64) -> Duration {
    Duration::from_secs_f64(value)
}

fn sample_workspace() -> Workspace {
    let music = Cue::new(CueKind::Audio(AudioCue {
        file: Some("audio/overture.wav".into()),
        media_duration: Some(secs(90.0)),
        ..AudioCue::default()
    }))
    .with_id(CueId::from_u128(1))
    .with_number("1")
    .with_name("Overture")
    .with_post_wait(secs(2.5))
    .with_continue(ContinueMode::AutoContinue);

    let fade = Cue::new(CueKind::Fade(FadeCue {
        duration: secs(5.0),
        level_db: Some(-60.0),
        stop_target_when_done: true,
        ..FadeCue::default()
    }))
    .with_number("2")
    .with_target(music.id);

    let group = Cue::new(CueKind::Group(GroupCue {
        mode: GroupMode::Timeline,
        children: vec![
            Cue::new(CueKind::Midi(MidiCue {
                patch: None,
                message: MidiMessage::ProgramChange {
                    channel: 1,
                    program: 12,
                },
            })),
            Cue::new(CueKind::Memo).with_pre_wait(secs(1.0)),
        ],
    }))
    .with_number("3");

    let mut workspace = Workspace::new("Test show");
    workspace.cue_lists[0].cues = vec![music, fade, group];
    workspace
}

#[test]
fn workspace_round_trips_through_json() {
    let workspace = sample_workspace();
    let json = serde_json::to_string_pretty(&workspace).unwrap();
    let back: Workspace = serde_json::from_str(&json).unwrap();
    assert_eq!(back, workspace);
}

#[test]
fn cue_json_is_flat_and_uses_seconds() {
    let cue = Cue::new(CueKind::Wait(WaitCue {
        duration: secs(1.5),
    }))
    .with_id(CueId::from_u128(7))
    .with_number("7")
    .with_pre_wait(secs(0.25));

    let value = serde_json::to_value(&cue).unwrap();
    assert_eq!(value["type"], "wait");
    assert_eq!(value["duration"], 1.5);
    assert_eq!(value["pre_wait"], 0.25);
    assert_eq!(value["armed"], true);
    // Default values are omitted to keep files small and diffs readable.
    assert!(value.get("post_wait").is_none());
    assert!(value.get("flagged").is_none());
}

#[test]
fn minimal_cue_json_gets_defaults() {
    let id = CueId::from_u128(42);
    let cue: Cue = serde_json::from_str(&format!(r#"{{"id":"{id}","type":"memo"}}"#)).unwrap();
    assert!(cue.armed);
    assert_eq!(cue.continue_mode, ContinueMode::DoNotContinue);
    assert_eq!(cue.kind, CueKind::Memo);
}

#[test]
fn negative_times_are_rejected() {
    let id = CueId::from_u128(42);
    let json = format!(r#"{{"id":"{id}","type":"memo","pre_wait":-1.0}}"#);
    assert!(serde_json::from_str::<Cue>(&json).is_err());
}

#[test]
fn audio_length_accounts_for_trim_loops_and_rate() {
    let audio = |cue: AudioCue| CueKind::Audio(cue).action_length();

    assert_eq!(audio(AudioCue::default()), ActionLength::UntilReported);
    assert_eq!(
        audio(AudioCue {
            media_duration: Some(secs(10.0)),
            start_time: secs(2.0),
            play_count: 2,
            rate: 2.0,
            ..AudioCue::default()
        }),
        ActionLength::Fixed(secs(8.0))
    );
    assert_eq!(
        audio(AudioCue {
            media_duration: Some(secs(10.0)),
            infinite_loop: true,
            ..AudioCue::default()
        }),
        ActionLength::UntilStopped
    );
}

#[test]
fn held_video_runs_until_stopped() {
    let video = CueKind::Video(VideoCue {
        media_duration: Some(secs(3.0)),
        hold_at_end: true,
        ..VideoCue::default()
    });
    assert_eq!(video.action_length(), ActionLength::UntilStopped);
}

#[test]
fn zero_length_actions_are_instant() {
    assert_eq!(
        CueKind::Wait(WaitCue::default()).action_length(),
        ActionLength::Instant
    );
    assert_eq!(CueKind::Memo.action_length(), ActionLength::Instant);
    assert_eq!(
        CueKind::Group(GroupCue::default()).action_length(),
        ActionLength::UntilChildrenComplete
    );
}

#[test]
fn find_helpers_reach_nested_cues() {
    let mut workspace = sample_workspace();
    let nested = workspace.cue_lists[0].cues[2].children()[1].id;

    assert_eq!(workspace.cues().count(), 5);
    assert_eq!(workspace.find_cue(nested).unwrap().type_name(), "Memo");
    assert_eq!(workspace.find_by_number("2").unwrap().type_name(), "Fade");

    workspace.find_cue_mut(nested).unwrap().name = "Renamed".into();
    assert_eq!(
        workspace.find_cue(nested).unwrap().display_name(),
        "Renamed"
    );
}

#[test]
fn valid_workspace_has_no_issues() {
    assert_eq!(validate(&sample_workspace()), Vec::new());
}

#[test]
fn validate_reports_structural_problems() {
    let missing = CueId::from_u128(99);
    let stop_without_target = Cue::new(CueKind::Stop).with_number("5");
    let stop_id = stop_without_target.id;
    let dangling = Cue::new(CueKind::Start).with_target(missing);
    let dangling_id = dangling.id;
    let bad_rate = Cue::new(CueKind::Audio(AudioCue {
        rate: 0.0,
        ..AudioCue::default()
    }))
    .with_number("5");
    let bad_rate_id = bad_rate.id;

    let mut workspace = Workspace::new("Broken");
    workspace
        .cue_lists
        .push(CueList::new("Second").with_cues(vec![stop_without_target, dangling, bad_rate]));

    let issues = validate(&workspace);
    assert!(issues.contains(&Issue::DuplicateNumber("5".into())));
    assert!(issues.contains(&Issue::MissingTarget {
        cue: stop_id,
        kind: "Stop"
    }));
    assert!(issues.contains(&Issue::DanglingTarget {
        cue: dangling_id,
        target: missing
    }));
    assert!(issues.contains(&Issue::InvalidRate(bad_rate_id)));
}
