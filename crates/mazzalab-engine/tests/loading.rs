//! Load, Reset, Devamp and Target cues.

use std::time::Duration;

use mazzalab_core::{
    ActionLength, AudioCue, ContinueMode, Cue, CueId, CueKind, CueList, FadeCue, GroupCue,
    GroupMode, LoadCue, SliceMarker, TargetCue, WaitCue, Workspace,
};
use mazzalab_engine::{Engine, EventKind, RunPhase};

const LIST: CueId = CueId::from_u128(1000);

fn secs(value: f64) -> Duration {
    Duration::from_secs_f64(value)
}

fn id(n: u128) -> CueId {
    CueId::from_u128(n)
}

fn wait(n: u128, duration: f64) -> Cue {
    Cue::new(CueKind::Wait(WaitCue {
        duration: secs(duration),
    }))
    .with_id(id(n))
}

fn audio(n: u128, length: f64, play_count: u32, infinite_loop: bool) -> Cue {
    Cue::new(CueKind::Audio(AudioCue {
        file: Some("a.wav".into()),
        media_duration: Some(secs(length)),
        play_count,
        infinite_loop,
        ..AudioCue::default()
    }))
    .with_id(id(n))
}

fn engine_with(cues: Vec<Cue>) -> Engine {
    let mut workspace = Workspace::new("test");
    workspace.cue_lists = vec![CueList::new("Main").with_id(LIST).with_cues(cues)];
    Engine::new(workspace)
}

fn kinds(engine: &mut Engine) -> Vec<(f64, EventKind)> {
    engine
        .take_events()
        .into_iter()
        .filter(|event| !matches!(event.kind, EventKind::PlayheadMoved { .. }))
        .map(|event| (event.at.as_secs_f64(), event.kind))
        .collect()
}

#[test]
fn a_loaded_cue_starts_partway_through_its_action() {
    let mut engine = engine_with(vec![wait(1, 10.0)]);
    engine.load(id(1), secs(4.0)).unwrap();
    assert_eq!(engine.loaded(id(1)), Some(secs(4.0)));
    assert_eq!(
        kinds(&mut engine),
        [(
            0.0,
            EventKind::Loaded {
                cue: id(1),
                time: secs(4.0)
            }
        )]
    );

    engine.advance_to(secs(100.0));
    engine.start(id(1)).unwrap();
    // Starting uses the load up: the next start is from the top again.
    assert_eq!(engine.loaded(id(1)), None);
    let info = engine.run_info(id(1)).unwrap();
    assert_eq!((info.phase, info.elapsed), (RunPhase::Action, secs(4.0)));

    engine.advance_to(secs(105.0));
    assert_eq!(engine.run_info(id(1)).unwrap().elapsed, secs(9.0));
    engine.advance_to(secs(106.0));
    assert!(!engine.is_active(id(1)), "six seconds were left, not ten");
}

#[test]
fn load_time_counts_from_the_start_of_the_pre_wait() {
    let mut engine = engine_with(vec![wait(1, 10.0).with_pre_wait(secs(5.0))]);
    engine.load(id(1), secs(2.0)).unwrap();
    engine.start(id(1)).unwrap();
    let info = engine.run_info(id(1)).unwrap();
    assert_eq!((info.phase, info.elapsed), (RunPhase::PreWait, secs(2.0)));
    // Three seconds of pre-wait are left, then the whole action.
    engine.advance_to(secs(3.0));
    let info = engine.run_info(id(1)).unwrap();
    assert_eq!((info.phase, info.elapsed), (RunPhase::Action, secs(0.0)));
    engine.advance_to(secs(13.0));
    assert!(!engine.is_active(id(1)));

    // Past the pre-wait, the rest goes into the action.
    engine.load(id(1), secs(8.0)).unwrap();
    engine.start(id(1)).unwrap();
    let info = engine.run_info(id(1)).unwrap();
    assert_eq!((info.phase, info.elapsed), (RunPhase::Action, secs(3.0)));
}

#[test]
fn a_cue_loaded_past_its_end_completes_at_once() {
    let mut engine = engine_with(vec![wait(1, 3.0), wait(2, 1.0)]);
    engine.load(id(1), secs(30.0)).unwrap();
    engine.take_events();
    engine.start(id(1)).unwrap();
    assert!(!engine.is_active(id(1)));
    assert_eq!(
        kinds(&mut engine),
        [
            (0.0, EventKind::Triggered(id(1))),
            (
                0.0,
                EventKind::ActionStarted {
                    cue: id(1),
                    armed: true
                }
            ),
            (0.0, EventKind::ActionCompleted(id(1))),
        ]
    );
}

#[test]
fn a_running_cue_cannot_be_loaded() {
    let mut engine = engine_with(vec![wait(1, 10.0)]);
    engine.start(id(1)).unwrap();
    engine.load(id(1), secs(4.0)).unwrap();
    assert_eq!(engine.loaded(id(1)), None);
    assert!(engine.load(id(99), secs(1.0)).is_err());
}

#[test]
fn a_load_cue_loads_its_target_and_a_post_wait_shrinks_with_it() {
    let load = Cue::new(CueKind::Load(LoadCue { time: secs(6.0) }))
        .with_id(id(1))
        .with_target(id(2));
    let music = wait(2, 10.0)
        .with_post_wait(secs(8.0))
        .with_continue(ContinueMode::AutoContinue);
    let mut engine = engine_with(vec![load, music, wait(3, 1.0)]);

    engine.go(LIST).unwrap();
    assert_eq!(engine.loaded(id(2)), Some(secs(6.0)));
    assert!(!engine.is_active(id(2)), "loading does not start the cue");

    engine.go(LIST).unwrap();
    assert_eq!(engine.run_info(id(2)).unwrap().elapsed, secs(6.0));
    // The auto-continue was due 8 s into the cue: 2 s from now.
    engine.advance_to(secs(1.9));
    assert!(!engine.is_active(id(3)));
    engine.advance_to(secs(2.0));
    assert!(engine.is_active(id(3)));
}

#[test]
fn a_timeline_group_loads_every_child_on_the_same_clock() {
    let group = Cue::new(CueKind::Group(GroupCue {
        mode: GroupMode::Timeline,
        children: vec![
            wait(11, 10.0),
            wait(12, 10.0).with_pre_wait(secs(5.0)),
            wait(13, 2.0),
        ],
    }))
    .with_id(id(10));
    let mut engine = engine_with(vec![group]);
    engine.load(id(10), secs(3.0)).unwrap();
    engine.start(id(10)).unwrap();

    let first = engine.run_info(id(11)).unwrap();
    assert_eq!((first.phase, first.elapsed), (RunPhase::Action, secs(3.0)));
    let second = engine.run_info(id(12)).unwrap();
    assert_eq!(
        (second.phase, second.elapsed),
        (RunPhase::PreWait, secs(3.0))
    );
    assert!(!engine.is_active(id(13)), "the short one is already over");
    // The group lasts as long as it would have from that point.
    engine.advance_to(secs(11.9));
    assert!(engine.is_active(id(10)));
    engine.advance_to(secs(12.0));
    assert!(!engine.is_active(id(10)));
}

#[test]
fn a_reset_cue_stops_and_unloads_its_target() {
    let reset = Cue::new(CueKind::Reset).with_id(id(1)).with_target(id(10));
    let group = Cue::new(CueKind::Group(GroupCue {
        mode: GroupMode::Timeline,
        children: vec![wait(11, 10.0), wait(12, 10.0)],
    }))
    .with_id(id(10));
    let mut engine = engine_with(vec![reset, group]);

    engine.start(id(11)).unwrap();
    engine.load(id(12), secs(4.0)).unwrap();
    engine.load(id(10), secs(2.0)).unwrap();
    engine.take_events();

    engine.start(id(1)).unwrap();
    assert!(!engine.is_active(id(11)));
    assert_eq!(engine.loaded(id(10)), None);
    assert_eq!(engine.loaded(id(12)), None);
    let events = kinds(&mut engine);
    assert!(events.contains(&(0.0, EventKind::Unloaded(id(12)))));
    assert!(events.contains(&(0.0, EventKind::Unloaded(id(10)))));

    // Resetting everything unloads too.
    engine.load(id(11), secs(1.0)).unwrap();
    engine.reset();
    assert_eq!(engine.loaded_cues().count(), 0);
}

#[test]
fn devamp_ends_a_loop_at_the_end_of_the_pass_it_is_in() {
    let devamp = Cue::new(CueKind::Devamp).with_id(id(2)).with_target(id(1));
    let mut engine = engine_with(vec![audio(1, 4.0, 1, true), devamp, wait(3, 1.0)]);
    engine.start(id(1)).unwrap();
    engine.advance_to(secs(9.0));
    engine.take_events();

    // Nine seconds in, the third pass has three seconds to go.
    engine.start(id(2)).unwrap();
    assert!(kinds(&mut engine).contains(&(9.0, EventKind::Devamped(id(1)))));
    engine.advance_to(secs(11.9));
    assert!(engine.is_active(id(1)));
    engine.advance_to(secs(12.0));
    assert!(!engine.is_active(id(1)));
}

#[test]
fn devamp_cuts_the_passes_left_of_a_counted_loop_and_follows_on() {
    let devamp = Cue::new(CueKind::Devamp).with_id(id(2)).with_target(id(1));
    let looped = audio(1, 4.0, 5, false).with_continue(ContinueMode::AutoFollow);
    let mut engine = engine_with(vec![looped, wait(3, 1.0), devamp]);
    engine.start(id(1)).unwrap();
    engine.advance_to(secs(5.0));
    engine.start(id(2)).unwrap();
    // The second pass ends at 8 s instead of the fifth at 20 s.
    engine.advance_to(secs(8.0));
    assert!(!engine.is_active(id(1)));
    assert!(engine.is_active(id(3)), "auto-follow still follows");
}

/// Ten seconds of file: an intro to 2 s, a vamp from 2 to 6 s that loops
/// until devamped, then an ending played once.
fn sliced(n: u128) -> Cue {
    Cue::new(CueKind::Audio(AudioCue {
        file: Some("a.wav".into()),
        media_duration: Some(secs(10.0)),
        slices: vec![
            SliceMarker {
                time: secs(2.0),
                play_count: 1,
            },
            SliceMarker {
                time: secs(6.0),
                play_count: 0,
            },
        ],
        ..AudioCue::default()
    }))
    .with_id(id(n))
}

#[test]
fn devamp_moves_a_sliced_cue_from_its_vamp_to_the_ending() {
    assert_eq!(sliced(1).kind.action_length(), ActionLength::UntilStopped);
    let devamp = Cue::new(CueKind::Devamp).with_id(id(2)).with_target(id(1));
    let mut engine = engine_with(vec![sliced(1), devamp]);
    engine.start(id(1)).unwrap();
    // The vamp goes round for as long as it takes.
    engine.advance_to(secs(60.0));
    assert!(engine.is_active(id(1)));
    // 61 s in: 59 s into the vamp, three into its fifteenth pass. It
    // finishes that pass (1 s), then plays the four-second ending.
    engine.advance_to(secs(61.0));
    engine.start(id(2)).unwrap();
    engine.advance_to(secs(65.9));
    assert!(engine.is_active(id(1)));
    engine.advance_to(secs(66.0));
    assert!(!engine.is_active(id(1)));
}

#[test]
fn devamp_in_a_slice_before_a_vamp_leaves_the_end_open() {
    let devamp = Cue::new(CueKind::Devamp).with_id(id(2)).with_target(id(1));
    let mut engine = engine_with(vec![sliced(1), devamp]);
    engine.start(id(1)).unwrap();
    engine.advance_to(secs(1.0));
    engine.start(id(2)).unwrap();
    assert!(
        kinds(&mut engine)
            .iter()
            .any(|(_, kind)| *kind == EventKind::Devamped(id(1)))
    );
    // Into the vamp it goes, and there it stays.
    engine.advance_to(secs(30.0));
    assert!(engine.is_active(id(1)));
}

#[test]
fn devamp_leaves_alone_what_does_not_loop() {
    let devamp = Cue::new(CueKind::Devamp).with_id(id(2)).with_target(id(1));
    let mut engine = engine_with(vec![audio(1, 4.0, 1, false), devamp]);
    engine.start(id(1)).unwrap();
    engine.advance_to(secs(1.0));
    engine.take_events();
    engine.start(id(2)).unwrap();
    assert!(
        !kinds(&mut engine)
            .iter()
            .any(|(_, kind)| matches!(kind, EventKind::Devamped(_)))
    );
    engine.advance_to(secs(4.0));
    assert!(!engine.is_active(id(1)));
}

#[test]
fn a_target_cue_points_another_cue_somewhere_else() {
    let fade = Cue::new(CueKind::Fade(FadeCue {
        duration: secs(1.0),
        ..FadeCue::default()
    }))
    .with_id(id(1))
    .with_target(id(10));
    let retarget = Cue::new(CueKind::Target(TargetCue {
        assigned_number: "B".into(),
    }))
    .with_id(id(2))
    .with_target(id(1));
    let lost = Cue::new(CueKind::Target(TargetCue {
        assigned_number: "no such number".into(),
    }))
    .with_id(id(3))
    .with_target(id(1));
    let mut engine = engine_with(vec![
        fade,
        retarget,
        lost,
        wait(10, 5.0).with_number("A"),
        wait(11, 5.0).with_number("B"),
    ]);
    let target = |engine: &Engine| engine.workspace().find_cue(id(1)).unwrap().target;

    engine.start(id(2)).unwrap();
    assert_eq!(target(&engine), Some(id(11)));
    // A number that does not exist changes nothing.
    engine.start(id(3)).unwrap();
    assert_eq!(target(&engine), Some(id(11)));
}
