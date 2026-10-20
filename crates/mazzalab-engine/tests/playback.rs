use std::time::Duration;

use mazzalab_core::{
    AudioCue, ContinueMode, Cue, CueId, CueKind, CueList, FadeCue, GroupCue, GroupMode, WaitCue,
    Workspace,
};
use mazzalab_engine::{Engine, EngineError, Event, EventKind};

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
    .with_number(n.to_string())
}

fn memo(n: u128) -> Cue {
    Cue::new(CueKind::Memo)
        .with_id(id(n))
        .with_number(n.to_string())
}

fn group(n: u128, mode: GroupMode, children: Vec<Cue>) -> Cue {
    Cue::new(CueKind::Group(GroupCue { mode, children }))
        .with_id(id(n))
        .with_number(n.to_string())
}

fn engine(cues: Vec<Cue>) -> Engine {
    let mut workspace = Workspace::new("test");
    workspace.cue_lists = vec![CueList::new("Main").with_id(LIST).with_cues(cues)];
    Engine::new(workspace)
}

/// Events reduced to (seconds, kind) for compact assertions, playhead moves omitted.
fn timeline(engine: &mut Engine) -> Vec<(f64, EventKind)> {
    engine
        .take_events()
        .into_iter()
        .filter(|event| !matches!(event.kind, EventKind::PlayheadMoved { .. }))
        .map(|Event { at, kind }| (at.as_secs_f64(), kind))
        .collect()
}

fn started(n: u128) -> EventKind {
    EventKind::ActionStarted {
        cue: id(n),
        armed: true,
    }
}

#[test]
fn go_runs_the_playhead_cue_and_advances() {
    let mut engine = engine(vec![memo(1), memo(2)]);
    assert_eq!(engine.playhead(LIST), Some(id(1)));

    assert_eq!(engine.go(LIST), Ok(Some(id(1))));
    assert_eq!(engine.playhead(LIST), Some(id(2)));
    assert_eq!(
        timeline(&mut engine),
        vec![
            (0.0, EventKind::Triggered(id(1))),
            (0.0, started(1)),
            (0.0, EventKind::ActionCompleted(id(1))),
        ]
    );

    engine.go(LIST).unwrap();
    assert_eq!(engine.playhead(LIST), None);
    assert_eq!(engine.go(LIST), Ok(None));
}

#[test]
fn pre_wait_delays_the_action() {
    let mut engine = engine(vec![wait(1, 2.0).with_pre_wait(secs(1.0))]);
    engine.go(LIST).unwrap();
    engine.advance_to(secs(10.0));

    assert_eq!(
        timeline(&mut engine),
        vec![
            (0.0, EventKind::Triggered(id(1))),
            (1.0, started(1)),
            (3.0, EventKind::ActionCompleted(id(1))),
        ]
    );
    assert_eq!(engine.now(), secs(10.0));
}

#[test]
fn auto_continue_counts_post_wait_from_action_start() {
    let mut engine = engine(vec![
        wait(1, 5.0)
            .with_pre_wait(secs(1.0))
            .with_post_wait(secs(2.0))
            .with_continue(ContinueMode::AutoContinue),
        wait(2, 1.0),
        memo(3),
    ]);
    engine.go(LIST).unwrap();
    // The whole 1 -> 2 sequence was consumed by one GO.
    assert_eq!(engine.playhead(LIST), Some(id(3)));

    engine.advance_to(secs(10.0));
    assert_eq!(
        timeline(&mut engine),
        vec![
            (0.0, EventKind::Triggered(id(1))),
            (1.0, started(1)),
            (3.0, EventKind::Triggered(id(2))),
            (3.0, started(2)),
            (4.0, EventKind::ActionCompleted(id(2))),
            (6.0, EventKind::ActionCompleted(id(1))),
        ]
    );
}

#[test]
fn auto_follow_starts_next_when_action_completes() {
    let mut engine = engine(vec![
        wait(1, 3.0).with_continue(ContinueMode::AutoFollow),
        memo(2),
    ]);
    engine.go(LIST).unwrap();
    engine.advance_to(secs(5.0));

    assert_eq!(
        timeline(&mut engine),
        vec![
            (0.0, EventKind::Triggered(id(1))),
            (0.0, started(1)),
            (3.0, EventKind::ActionCompleted(id(1))),
            (3.0, EventKind::Triggered(id(2))),
            (3.0, started(2)),
            (3.0, EventKind::ActionCompleted(id(2))),
        ]
    );
}

#[test]
fn timeline_group_lasts_until_its_longest_child() {
    let mut engine = engine(vec![
        group(
            10,
            GroupMode::Timeline,
            vec![wait(11, 2.0), wait(12, 1.0).with_pre_wait(secs(3.0))],
        )
        .with_continue(ContinueMode::AutoFollow),
        memo(20),
    ]);
    engine.go(LIST).unwrap();
    engine.advance_to(secs(10.0));

    let events = timeline(&mut engine);
    assert!(events.contains(&(4.0, EventKind::ActionCompleted(id(10)))));
    assert!(events.contains(&(4.0, EventKind::Triggered(id(20)))));
    assert!(
        !events
            .iter()
            .any(|(at, kind)| *kind == EventKind::ActionCompleted(id(10)) && *at < 4.0)
    );
}

#[test]
fn start_first_group_runs_children_chain() {
    let mut engine = engine(vec![group(
        10,
        GroupMode::StartFirst,
        vec![
            wait(11, 1.0).with_continue(ContinueMode::AutoFollow),
            wait(12, 1.0),
            memo(13),
        ],
    )]);
    engine.go(LIST).unwrap();
    engine.advance_to(secs(5.0));

    let events = timeline(&mut engine);
    assert!(events.contains(&(1.0, EventKind::Triggered(id(12)))));
    assert!(!events.contains(&(2.0, EventKind::Triggered(id(13)))));
    assert!(events.contains(&(2.0, EventKind::ActionCompleted(id(10)))));
}

#[test]
fn start_first_and_enter_moves_playhead_into_group() {
    let mut engine = engine(vec![
        group(10, GroupMode::StartFirstAndEnter, vec![memo(11), memo(12)]),
        memo(20),
    ]);
    engine.go(LIST).unwrap();
    assert_eq!(engine.playhead(LIST), Some(id(12)));

    engine.go(LIST).unwrap();
    // Leaving the end of the group continues after the group.
    assert_eq!(engine.playhead(LIST), Some(id(20)));
}

#[test]
fn playlist_group_plays_children_back_to_back() {
    let mut engine = engine(vec![group(
        10,
        GroupMode::Playlist,
        vec![wait(11, 1.0), wait(12, 2.0)],
    )]);
    engine.go(LIST).unwrap();
    engine.advance_to(secs(5.0));

    let events = timeline(&mut engine);
    assert!(events.contains(&(1.0, EventKind::Triggered(id(12)))));
    assert!(events.contains(&(3.0, EventKind::ActionCompleted(id(10)))));
}

#[test]
fn start_random_is_reproducible_with_a_seed() {
    let make = || {
        let mut engine = engine(vec![group(
            10,
            GroupMode::StartRandom,
            vec![memo(11), memo(12), memo(13)],
        )]);
        engine.seed_random(7);
        engine.go(LIST).unwrap();
        timeline(&mut engine)
    };
    let first = make();
    assert_eq!(first, make());
    let picked: Vec<_> = first
        .iter()
        .filter(|(_, kind)| matches!(kind, EventKind::Triggered(cue) if *cue != id(10)))
        .collect();
    assert_eq!(picked.len(), 1);
}

#[test]
fn stopping_a_cue_cancels_its_pending_continue() {
    let mut engine = engine(vec![
        wait(1, 10.0)
            .with_post_wait(secs(5.0))
            .with_continue(ContinueMode::AutoContinue),
        memo(2),
    ]);
    engine.go(LIST).unwrap();
    engine.advance_to(secs(2.0));
    engine.stop(id(1)).unwrap();
    engine.advance_to(secs(20.0));

    let events = timeline(&mut engine);
    assert!(events.contains(&(2.0, EventKind::Stopped(id(1)))));
    assert!(!events.contains(&(5.0, EventKind::Triggered(id(2)))));
    assert!(!engine.is_active(id(1)));
}

#[test]
fn stopping_a_group_stops_children_without_following() {
    let mut engine = engine(vec![
        group(10, GroupMode::Timeline, vec![wait(11, 5.0), wait(12, 5.0)])
            .with_continue(ContinueMode::AutoFollow),
        memo(20),
    ]);
    engine.go(LIST).unwrap();
    engine.advance_to(secs(1.0));
    engine.stop(id(10)).unwrap();
    engine.advance_to(secs(10.0));

    let events = timeline(&mut engine);
    for cue in [11, 12, 10] {
        assert!(events.contains(&(1.0, EventKind::Stopped(id(cue)))));
    }
    assert!(
        !events
            .iter()
            .any(|(_, kind)| *kind == EventKind::Triggered(id(20)))
    );
}

#[test]
fn pause_shifts_remaining_time() {
    let mut engine = engine(vec![wait(1, 4.0)]);
    engine.go(LIST).unwrap();
    engine.advance_to(secs(1.0));
    engine.pause(id(1)).unwrap();
    engine.advance_to(secs(11.0));
    assert!(engine.is_active(id(1)));
    engine.resume(id(1)).unwrap();
    engine.advance_to(secs(20.0));

    let events = timeline(&mut engine);
    assert!(events.contains(&(1.0, EventKind::Paused(id(1)))));
    assert!(events.contains(&(11.0, EventKind::Resumed(id(1)))));
    assert!(events.contains(&(14.0, EventKind::ActionCompleted(id(1)))));
}

#[test]
fn disarmed_cues_keep_timing_but_do_nothing() {
    let mut engine = engine(vec![
        wait(1, 5.0)
            .with_armed(false)
            .with_continue(ContinueMode::AutoFollow),
        memo(2),
    ]);
    engine.go(LIST).unwrap();
    assert_eq!(
        timeline(&mut engine),
        vec![
            (0.0, EventKind::Triggered(id(1))),
            (
                0.0,
                EventKind::ActionStarted {
                    cue: id(1),
                    armed: false
                }
            ),
            (0.0, EventKind::ActionCompleted(id(1))),
            (0.0, EventKind::Triggered(id(2))),
            (0.0, started(2)),
            (0.0, EventKind::ActionCompleted(id(2))),
        ]
    );
}

#[test]
fn skip_if_disarmed_passes_over_the_cue() {
    let mut skipped = wait(1, 5.0)
        .with_armed(false)
        .with_continue(ContinueMode::AutoContinue);
    skipped.skip_if_disarmed = true;
    let mut engine = engine(vec![skipped, memo(2)]);
    engine.go(LIST).unwrap();

    let events = timeline(&mut engine);
    assert_eq!(events[0], (0.0, EventKind::Skipped(id(1))));
    assert!(events.contains(&(0.0, EventKind::Triggered(id(2)))));
}

#[test]
fn control_cues_act_on_their_targets() {
    let mut engine = engine(vec![
        wait(1, 10.0),
        Cue::new(CueKind::Start).with_id(id(2)).with_target(id(1)),
        Cue::new(CueKind::Stop).with_id(id(3)).with_target(id(1)),
        Cue::new(CueKind::Disarm).with_id(id(4)).with_target(id(1)),
        Cue::new(CueKind::Goto).with_id(id(5)).with_target(id(2)),
    ]);
    engine.set_playhead(LIST, Some(id(2))).unwrap();

    engine.go(LIST).unwrap(); // Start 1
    assert!(engine.is_active(id(1)));
    engine.go(LIST).unwrap(); // Stop 1
    assert!(!engine.is_active(id(1)));
    engine.go(LIST).unwrap(); // Disarm 1
    assert!(!engine.workspace().find_cue(id(1)).unwrap().armed);
    engine.go(LIST).unwrap(); // Go To 2
    assert_eq!(engine.playhead(LIST), Some(id(2)));
}

#[test]
fn fade_can_stop_its_target_when_done() {
    let fade = Cue::new(CueKind::Fade(FadeCue {
        duration: secs(2.0),
        level_db: Some(-60.0),
        stop_target_when_done: true,
        ..FadeCue::default()
    }))
    .with_id(id(2))
    .with_target(id(1));
    let mut engine = engine(vec![wait(1, 100.0), fade]);
    engine.go(LIST).unwrap();
    engine.go(LIST).unwrap();
    engine.advance_to(secs(3.0));

    assert!(timeline(&mut engine).contains(&(2.0, EventKind::Stopped(id(1)))));
}

#[test]
fn backend_reports_end_of_unknown_length_media() {
    let audio = Cue::new(CueKind::Audio(AudioCue::default()))
        .with_id(id(1))
        .with_continue(ContinueMode::AutoFollow);
    let mut engine = engine(vec![audio, memo(2)]);
    engine.go(LIST).unwrap();
    engine.advance_to(secs(60.0));
    assert!(engine.is_active(id(1)));

    assert_eq!(engine.action_finished(id(1)), Ok(true));
    assert_eq!(engine.action_finished(id(1)), Ok(false));
    let events = timeline(&mut engine);
    assert!(events.contains(&(60.0, EventKind::ActionCompleted(id(1)))));
    assert!(events.contains(&(60.0, EventKind::Triggered(id(2)))));
}

#[test]
fn double_go_protection_ignores_quick_repeats() {
    let mut workspace = Workspace::new("test");
    workspace.settings.double_go_protection = secs(0.5);
    workspace.cue_lists = vec![
        CueList::new("Main")
            .with_id(LIST)
            .with_cues(vec![memo(1), memo(2)]),
    ];
    let mut engine = Engine::new(workspace);

    assert_eq!(engine.go(LIST), Ok(Some(id(1))));
    engine.advance_to(secs(0.2));
    assert_eq!(engine.go(LIST), Ok(None));
    engine.advance_to(secs(0.6));
    assert_eq!(engine.go(LIST), Ok(Some(id(2))));
}

#[test]
fn start_cue_loops_do_not_recurse_forever() {
    let mut engine = engine(vec![
        Cue::new(CueKind::Start)
            .with_id(id(1))
            .with_target(id(2))
            .with_continue(ContinueMode::AutoFollow),
        Cue::new(CueKind::Start).with_id(id(2)).with_target(id(1)),
    ]);
    engine.go(LIST).unwrap();
    assert!(!timeline(&mut engine).is_empty());
}

#[test]
fn unknown_ids_are_errors() {
    let mut engine = engine(vec![memo(1)]);
    assert_eq!(engine.go(id(5)), Err(EngineError::UnknownCueList(id(5))));
    assert_eq!(engine.start(id(5)), Err(EngineError::UnknownCue(id(5))));
    assert_eq!(
        engine.set_playhead(LIST, Some(id(5))),
        Err(EngineError::UnknownCue(id(5)))
    );
}

#[test]
fn next_deadline_reports_the_earliest_pending_transition() {
    let mut engine = engine(vec![wait(1, 3.0).with_pre_wait(secs(1.0))]);
    assert_eq!(engine.next_deadline(), None);
    engine.go(LIST).unwrap();
    assert_eq!(engine.next_deadline(), Some(secs(1.0)));
    engine.advance_to(secs(1.0));
    assert_eq!(engine.next_deadline(), Some(secs(4.0)));
}
