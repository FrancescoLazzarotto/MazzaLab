//! "Fade and stop others" and "duck others".

use std::time::Duration;

use mazzalab_core::{
    AudioCue, Cue, CueId, CueKind, CueList, Others, OthersAction, OthersScope, WaitCue, Workspace,
};
use mazzalab_engine::{Engine, EventKind};

const LIST: CueId = CueId::from_u128(1000);
const OTHER_LIST: CueId = CueId::from_u128(2000);

fn secs(value: f64) -> Duration {
    Duration::from_secs_f64(value)
}

fn id(n: u128) -> CueId {
    CueId::from_u128(n)
}

/// An audio cue, which plays until its sound ends.
fn sound(n: u128) -> Cue {
    Cue::new(CueKind::Audio(AudioCue::default())).with_id(id(n))
}

fn others(action: OthersAction, scope: OthersScope, fade: f64) -> Option<Others> {
    Some(Others {
        action,
        scope,
        fade: secs(fade),
        duck_db: 12.0,
    })
}

fn engine(main: Vec<Cue>, other: Vec<Cue>) -> Engine {
    let mut workspace = Workspace::new("test");
    workspace.cue_lists = vec![
        CueList::new("Main").with_id(LIST).with_cues(main),
        CueList::new("Other").with_id(OTHER_LIST).with_cues(other),
    ];
    Engine::new(workspace)
}

fn events(engine: &mut Engine) -> Vec<EventKind> {
    engine
        .take_events()
        .into_iter()
        .map(|event| event.kind)
        .filter(|kind| !matches!(kind, EventKind::PlayheadMoved { .. }))
        .collect()
}

#[test]
fn a_cue_fades_out_and_stops_the_sounds_of_its_list() {
    let mut music = sound(3);
    music.others = others(OthersAction::FadeAndStop, OthersScope::List, 2.0);
    let wait = Cue::new(CueKind::Wait(WaitCue {
        duration: secs(30.0),
    }))
    .with_id(id(4));
    let mut engine = engine(vec![sound(1), sound(2), music, wait], vec![sound(10)]);
    for cue in [1, 2, 4, 10] {
        engine.start(id(cue)).unwrap();
    }
    engine.take_events();

    engine.advance_to(secs(1.0));
    engine.start(id(3)).unwrap();
    let started = events(&mut engine);
    for cue in [1, 2] {
        assert!(
            started.contains(&EventKind::FadingOut {
                cue: id(cue),
                fade: secs(2.0)
            }),
            "{started:?}"
        );
    }
    // Neither the Wait, nor a sound in another list, nor the cue itself.
    assert!(!started.iter().any(|kind| matches!(
        kind,
        EventKind::FadingOut { cue, .. } if [id(3), id(4), id(10)].contains(cue)
    )));

    engine.advance_to(secs(2.9));
    assert!(engine.is_active(id(1)) && engine.is_active(id(2)));
    engine.advance_to(secs(3.0));
    assert!(!engine.is_active(id(1)) && !engine.is_active(id(2)));
    assert!(engine.is_active(id(3)) && engine.is_active(id(4)) && engine.is_active(id(10)));
}

#[test]
fn across_every_list_and_at_once_without_a_fade() {
    let mut music = sound(3);
    music.others = others(OthersAction::FadeAndStop, OthersScope::All, 0.0);
    let mut engine = engine(vec![sound(1), music], vec![sound(10)]);
    engine.start(id(1)).unwrap();
    engine.start(id(10)).unwrap();
    engine.take_events();
    engine.start(id(3)).unwrap();
    let started = events(&mut engine);
    assert!(started.contains(&EventKind::Stopped(id(1))));
    assert!(started.contains(&EventKind::Stopped(id(10))));
    assert!(engine.is_active(id(3)));
}

#[test]
fn ducked_sounds_come_back_when_the_ducking_cue_ends() {
    let mut announcement = sound(3);
    announcement.others = others(OthersAction::Duck, OthersScope::List, 0.5);
    let mut deeper = sound(4);
    deeper.others = Some(Others {
        duck_db: 20.0,
        ..others(OthersAction::Duck, OthersScope::List, 1.0).unwrap()
    });
    let mut engine = engine(vec![sound(1), announcement, deeper], Vec::new());
    engine.start(id(1)).unwrap();
    engine.take_events();

    engine.start(id(3)).unwrap();
    assert!(events(&mut engine).contains(&EventKind::Ducked {
        cue: id(1),
        by_db: 12.0,
        fade: secs(0.5)
    }));
    assert_eq!(engine.duck_of(id(1)), Some(12.0));

    // A second, deeper duck takes over; when it ends the first holds.
    engine.start(id(4)).unwrap();
    let deeper_events = events(&mut engine);
    assert!(deeper_events.contains(&EventKind::Ducked {
        cue: id(1),
        by_db: 20.0,
        fade: secs(1.0)
    }));
    assert!(
        deeper_events.contains(&EventKind::Ducked {
            cue: id(3),
            by_db: 20.0,
            fade: secs(1.0)
        }),
        "the announcement is ducked by the deeper one too"
    );
    engine.stop(id(4)).unwrap();
    assert!(events(&mut engine).contains(&EventKind::Ducked {
        cue: id(1),
        by_db: 12.0,
        fade: secs(1.0)
    }));

    // The announcement ends by itself (its sound is over).
    engine.action_finished(id(3)).unwrap();
    assert!(events(&mut engine).contains(&EventKind::Unducked {
        cue: id(1),
        fade: secs(0.5)
    }));
    assert_eq!(engine.duck_of(id(1)), None);
    assert!(engine.is_active(id(1)));
}
