//! Panic, reset, playhead navigation, progress reporting and live edits.

use std::time::Duration;

use mazzalab_core::{ContinueMode, Cue, CueId, CueKind, CueList, WaitCue, Workspace};
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

fn engine_with(cues: Vec<Cue>, panic: f64) -> Engine {
    let mut workspace = Workspace::new("test");
    workspace.settings.panic_duration = secs(panic);
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
fn panic_fades_then_stops_and_blocks_continues() {
    let mut engine = engine_with(
        vec![
            wait(1, 100.0)
                .with_post_wait(secs(1.0))
                .with_continue(ContinueMode::AutoContinue),
            wait(2, 100.0).with_pre_wait(secs(5.0)),
            wait(3, 100.0),
        ],
        2.0,
    );
    engine.go(LIST).unwrap();
    engine.advance_to(secs(1.5)); // cue 2 is now in its pre-wait
    engine.take_events();

    engine.panic();
    assert!(engine.is_panicking());
    engine.advance_to(secs(10.0));

    let events = kinds(&mut engine);
    assert!(events.contains(&(1.5, EventKind::Stopped(id(2)))));
    assert!(events.contains(&(
        1.5,
        EventKind::Panicking {
            cue: id(1),
            fade: secs(2.0)
        }
    )));
    assert!(events.contains(&(3.5, EventKind::Stopped(id(1)))));
    assert!(!engine.is_panicking());
    assert_eq!(engine.running_cues().count(), 0);
}

#[test]
fn panic_with_zero_duration_stops_immediately() {
    let mut engine = engine_with(vec![wait(1, 100.0)], 0.0);
    engine.go(LIST).unwrap();
    engine.panic();
    assert!(!engine.is_active(id(1)));
}

#[test]
fn reset_stops_and_rewinds_playheads() {
    let mut engine = engine_with(vec![wait(1, 100.0), wait(2, 100.0)], 1.0);
    engine.go(LIST).unwrap();
    engine.go(LIST).unwrap();
    assert_eq!(engine.playhead(LIST), None);

    engine.reset();
    assert_eq!(engine.running_cues().count(), 0);
    assert_eq!(engine.playhead(LIST), Some(id(1)));
}

#[test]
fn playhead_navigation() {
    let mut engine = engine_with(
        vec![
            wait(1, 1.0).with_continue(ContinueMode::AutoFollow),
            wait(2, 1.0),
            wait(3, 1.0),
        ],
        1.0,
    );
    engine.playhead_next(LIST).unwrap();
    assert_eq!(engine.playhead(LIST), Some(id(2)));
    engine.playhead_previous(LIST).unwrap();
    assert_eq!(engine.playhead(LIST), Some(id(1)));
    engine.playhead_previous(LIST).unwrap();
    assert_eq!(engine.playhead(LIST), Some(id(1)), "stays at the top");

    engine.playhead_next_sequence(LIST).unwrap();
    assert_eq!(
        engine.playhead(LIST),
        Some(id(3)),
        "skips the 1 -> 2 sequence"
    );
    engine.playhead_previous_sequence(LIST).unwrap();
    assert_eq!(engine.playhead(LIST), Some(id(1)));

    engine.set_playhead(LIST, Some(id(2))).unwrap();
    engine.playhead_previous_sequence(LIST).unwrap();
    assert_eq!(
        engine.playhead(LIST),
        Some(id(1)),
        "top of the current sequence"
    );

    engine.set_playhead(LIST, None).unwrap();
    engine.playhead_previous(LIST).unwrap();
    assert_eq!(
        engine.playhead(LIST),
        Some(id(3)),
        "from the end goes to the last cue"
    );
}

#[test]
fn run_info_reports_progress_excluding_pauses() {
    let mut engine = engine_with(vec![wait(1, 10.0).with_pre_wait(secs(1.0))], 1.0);
    engine.go(LIST).unwrap();
    engine.advance_to(secs(0.5));
    let info = engine.run_info(id(1)).unwrap();
    assert_eq!(info.phase, RunPhase::PreWait);
    assert_eq!(info.elapsed, secs(0.5));

    engine.advance_to(secs(3.0));
    engine.pause(id(1)).unwrap();
    engine.advance_to(secs(10.0));
    let info = engine.run_info(id(1)).unwrap();
    assert_eq!(info.phase, RunPhase::Action);
    assert_eq!(info.elapsed, secs(2.0));
    assert!(info.paused);

    engine.resume(id(1)).unwrap();
    engine.advance_to(secs(11.0));
    assert_eq!(engine.run_info(id(1)).unwrap().elapsed, secs(3.0));
    assert_eq!(engine.run_info(id(2)), None);
}

#[test]
fn live_edits_stop_deleted_cues_and_fix_playheads() {
    let mut engine = engine_with(vec![wait(1, 100.0), wait(2, 100.0), wait(3, 1.0)], 1.0);
    engine.go(LIST).unwrap();
    assert_eq!(engine.playhead(LIST), Some(id(2)));
    engine.take_events();

    // Rename cue 1 while it runs, delete cue 2 (the playhead), add cue 4.
    let renamed = engine.edit_workspace(|workspace| {
        let cues = &mut workspace.cue_lists[0].cues;
        cues[0].name = "Renamed".into();
        cues.remove(1);
        cues.push(wait(4, 1.0));
        cues.len()
    });
    assert_eq!(renamed, 3);
    assert!(engine.is_active(id(1)), "edited cue keeps running");
    assert_eq!(
        engine.playhead(LIST),
        Some(id(1)),
        "playhead returns to the top"
    );

    engine.edit_workspace(|workspace| workspace.cue_lists[0].cues.remove(0));
    assert!(!engine.is_active(id(1)));
    assert!(kinds(&mut engine).contains(&(0.0, EventKind::Stopped(id(1)))));
    assert_eq!(engine.playhead(LIST), Some(id(3)));
}
