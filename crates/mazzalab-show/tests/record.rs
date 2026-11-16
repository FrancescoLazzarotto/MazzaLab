//! A recorded performance replays to exactly the same events.

use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use mazzalab_core::{AudioCue, ContinueMode, Cue, CueId, CueKind, CueList, WaitCue, Workspace};
use mazzalab_engine::Event;
use mazzalab_show::{Command, Input, Show, replay};

/// A writer whose bytes the test can read back.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn secs(value: f64) -> Duration {
    Duration::from_secs_f64(value)
}

const SOUND: CueId = CueId::from_u128(2);

fn workspace() -> Workspace {
    let mut sound = Cue::new(CueKind::Audio(AudioCue::default()))
        .with_id(SOUND)
        .with_number("2");
    sound.triggers.osc = Some("/console/scene/*".into());
    let cues = vec![
        Cue::new(CueKind::Wait(WaitCue {
            duration: secs(1.5),
        }))
        .with_id(CueId::from_u128(1))
        .with_number("1")
        .with_post_wait(secs(0.25))
        .with_continue(ContinueMode::AutoContinue),
        sound,
        Cue::new(CueKind::Wait(WaitCue {
            duration: secs(4.0),
        }))
        .with_id(CueId::from_u128(3))
        .with_number("3")
        .with_pre_wait(secs(0.7)),
    ];
    let mut workspace = Workspace::new("Replay");
    workspace.cue_lists = vec![CueList::new("Main").with_cues(cues)];
    workspace
}

fn osc(address: &str) -> mazzalab_osc::Request {
    mazzalab_osc::parse(address, &[]).unwrap()
}

#[test]
fn a_performance_replays_event_for_event() {
    let record = Shared::default();
    let mut show = Show::new(workspace());
    show.record_to(Box::new(record.clone()), "test").unwrap();

    let mut live: Vec<Event> = Vec::new();
    let at = |show: &mut Show, time: f64, live: &mut Vec<Event>| {
        show.advance_to(secs(time));
        live.extend(show.take_events());
    };
    at(&mut show, 0.3, &mut live);
    show.execute(Command::Go { list: None }).unwrap();
    at(&mut show, 1.1, &mut live);
    show.handle_osc(&osc("/go"));
    at(&mut show, 2.37, &mut live);
    // The sound's file ended, as the audio backend would say.
    show.action_finished(SOUND).unwrap();
    show.set_media_info(SOUND, Some(secs(12.5)), Some(2));
    at(&mut show, 3.0, &mut live);
    show.execute(Command::RenameWorkspace {
        name: "Renamed".into(),
    })
    .unwrap();
    assert_eq!(show.osc_trigger("/console/scene/4"), 1);
    // A command that fails fails again on replay, harmlessly.
    assert!(
        show.execute(Command::Start {
            cue: CueId::from_u128(99)
        })
        .is_err()
    );
    show.execute(Command::Undo).unwrap();
    at(&mut show, 9.0, &mut live);
    assert!(show.record_error().is_none());

    let bytes = record.0.lock().unwrap().clone();
    let text = String::from_utf8(bytes.clone()).unwrap();
    // One header and one line per input from outside, nothing it set off.
    assert_eq!(text.lines().count(), 1 + 8, "{text}");

    let mut replayed: Vec<Event> = Vec::new();
    let mut inputs = Vec::new();
    let mut again = replay(bytes.as_slice(), |show, entry| {
        replayed.extend(show.take_events());
        inputs.push(entry.input.clone());
    })
    .unwrap();
    again.advance_to(secs(9.0));
    replayed.extend(again.take_events());

    assert!(matches!(
        &inputs[0],
        Input::Command { command } if matches!(**command, Command::Go { .. })
    ));
    assert_eq!(live, replayed);
    assert_eq!(show.workspace(), again.workspace());
    assert_eq!(show.status(), again.status());
    assert_eq!(show.snapshot().can_redo, again.snapshot().can_redo);
}

#[test]
fn a_record_must_start_with_its_header() {
    assert!(replay(&b""[..], |_, _| {}).is_err());
    assert!(replay(&b"{\"at\":0,\"input\":\"command\"}\n"[..], |_, _| {}).is_err());
}
