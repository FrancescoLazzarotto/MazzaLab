use std::time::Duration;

use mazzalab_core::{
    Container, ContinueMode, Cue, CueId, CueKind, CueList, GroupCue, GroupMode, WaitCue, Workspace,
};
use mazzalab_osc::{Arg, ReplyStatus, parse};
use mazzalab_show::{Command, Phase, SessionRequest, Show, ShowError};
use serde_json::json;

const LIST: CueId = CueId::from_u128(1000);

fn id(n: u128) -> CueId {
    CueId::from_u128(n)
}

fn wait(n: u128, seconds: u64) -> Cue {
    Cue::new(CueKind::Wait(WaitCue {
        duration: Duration::from_secs(seconds),
    }))
    .with_id(id(n))
    .with_number(n.to_string())
}

fn show() -> Show {
    let group = Cue::new(CueKind::Group(GroupCue {
        mode: GroupMode::Timeline,
        children: vec![wait(11, 1), wait(12, 2)],
    }))
    .with_id(id(10))
    .with_number("10");
    let mut workspace = Workspace::new("Test");
    workspace.cue_lists = vec![CueList::new("Main").with_id(LIST).with_cues(vec![
        wait(1, 5),
        wait(2, 5),
        group,
    ])];
    Show::new(workspace)
}

fn osc(show: &mut Show, address: &str, args: &[Arg]) -> mazzalab_show::OscOutcome {
    show.handle_osc(&parse(address, args).unwrap()).unwrap()
}

#[test]
fn transport_commands_drive_the_engine() {
    let mut show = show();
    show.execute(Command::Go { list: None }).unwrap();
    assert!(show.engine().is_active(id(1)));

    let status = show.status();
    assert_eq!(status.running.len(), 1);
    assert_eq!(status.running[0].phase, Phase::Action);
    assert_eq!(status.running[0].length, Some(5.0));
    assert_eq!(status.playheads[0].cue, Some(id(2)));

    show.execute(Command::Panic).unwrap();
    show.advance_to(Duration::from_secs(5));
    assert!(show.status().running.is_empty());
}

#[test]
fn set_playhead_finds_the_list_from_the_cue() {
    let mut show = show();
    show.execute(Command::SetPlayhead {
        list: None,
        cue: Some(id(10)),
    })
    .unwrap();
    assert_eq!(show.engine().playhead(LIST), Some(id(10)));
    assert_eq!(
        show.execute(Command::SetPlayhead {
            list: None,
            cue: Some(id(99))
        }),
        Err(ShowError::UnknownCue(id(99)))
    );
}

#[test]
fn edits_are_undoable_and_tracked() {
    let mut show = show();
    assert!(!show.is_dirty());
    let revision = show.revision();

    show.execute(Command::InsertCue {
        container: Container::List(LIST),
        index: Some(0),
        cue: wait(3, 1),
    })
    .unwrap();
    assert_eq!(show.workspace().cue_lists[0].cues[0].id, id(3));
    assert!(show.is_dirty());
    assert!(show.revision() > revision);

    let mut renamed = show.workspace().find_cue(id(1)).unwrap().clone();
    renamed.name = "Renamed".into();
    show.execute(Command::UpdateCue { cue: renamed }).unwrap();

    show.execute(Command::Undo).unwrap();
    assert_eq!(show.workspace().find_cue(id(1)).unwrap().name, "");
    show.execute(Command::Undo).unwrap();
    assert!(show.workspace().find_cue(id(3)).is_none());
    assert_eq!(show.execute(Command::Undo), Err(ShowError::NothingToUndo));

    show.execute(Command::Redo).unwrap();
    assert!(show.workspace().find_cue(id(3)).is_some());
    assert!(show.snapshot().can_redo);

    show.mark_saved();
    assert!(!show.is_dirty());
}

#[test]
fn invalid_edits_change_nothing() {
    let mut show = show();
    let before = show.workspace().clone();

    assert_eq!(
        show.execute(Command::InsertCue {
            container: Container::List(LIST),
            index: None,
            cue: wait(1, 1),
        }),
        Err(ShowError::DuplicateId(id(1)))
    );
    assert_eq!(
        show.execute(Command::InsertCue {
            container: Container::Group(id(1)),
            index: None,
            cue: wait(50, 1),
        }),
        Err(ShowError::NotAGroup(id(1)))
    );
    assert_eq!(
        show.execute(Command::MoveCue {
            cue: id(10),
            container: Container::Group(id(11)),
            index: None,
        }),
        Err(ShowError::InvalidMove)
    );
    let mut clash = show.workspace().find_cue(id(2)).unwrap().clone();
    clash.number = Some("1".into());
    assert_eq!(
        show.execute(Command::UpdateCue { cue: clash }),
        Err(ShowError::DuplicateNumber("1".into()))
    );
    assert_eq!(
        show.execute(Command::DeleteCueList { list: LIST }),
        Err(ShowError::LastCueList)
    );

    assert_eq!(show.workspace(), &before);
    assert!(!show.snapshot().can_undo);
}

#[test]
fn updating_a_group_keeps_its_children() {
    let mut show = show();
    let mut group = Cue::new(CueKind::Group(GroupCue::default()))
        .with_id(id(10))
        .with_number("10");
    group.name = "Storm".into();
    show.execute(Command::UpdateCue { cue: group }).unwrap();
    let group = show.workspace().find_cue(id(10)).unwrap();
    assert_eq!(group.name, "Storm");
    assert_eq!(group.children().len(), 2);
}

#[test]
fn move_cue_into_group() {
    let mut show = show();
    show.execute(Command::MoveCue {
        cue: id(1),
        container: Container::Group(id(10)),
        index: Some(1),
    })
    .unwrap();
    let group = show.workspace().find_cue(id(10)).unwrap();
    assert_eq!(group.children()[1].id, id(1));
    assert_eq!(show.workspace().cue_lists[0].cues.len(), 2);
}

#[test]
fn deleting_the_selection_clears_it() {
    let mut show = show();
    show.execute(Command::Select { cue: Some(id(2)) }).unwrap();
    show.execute(Command::DeleteCue { cue: id(2) }).unwrap();
    assert_eq!(show.selection(), None);
}

#[test]
fn snapshot_serializes_for_clients() {
    let value = serde_json::to_value(show().snapshot()).unwrap();
    assert_eq!(value["current_list"], json!(LIST.to_string()));
    assert_eq!(
        value["workspace"]["cue_lists"][0]["cues"][0]["type"],
        "wait"
    );
    let command: Command = serde_json::from_value(json!({"command": "go"})).unwrap();
    assert_eq!(command, Command::Go { list: None });
}

#[test]
fn osc_transport_and_cue_actions() {
    let mut show = show();
    let outcome = osc(&mut show, "/go", &[]);
    assert_eq!(outcome.status, ReplyStatus::Ok);
    assert!(!outcome.is_query);
    assert!(show.engine().is_active(id(1)));

    osc(&mut show, "/cue/2/start", &[]);
    assert!(show.engine().is_active(id(2)));
    osc(&mut show, "/cue/*/stop", &[]);
    assert_eq!(show.engine().running_cues().count(), 0);

    osc(&mut show, "/go/10", &[]);
    assert!(show.engine().is_active(id(10)));
    assert_eq!(show.engine().playhead(LIST), None);

    let error = osc(&mut show, "/cue/77/start", &[]);
    assert_eq!(error.status, ReplyStatus::Error);
}

#[test]
fn osc_properties_read_and_write() {
    let mut show = show();
    let outcome = osc(&mut show, "/cue/1/preWait", &[]);
    assert!(outcome.is_query);
    assert_eq!(outcome.data, Some(json!(0.0)));

    osc(&mut show, "/cue/1/preWait", &[Arg::Float(1.5)]);
    osc(&mut show, "/cue/1/preWait/+", &[Arg::Int(1)]);
    assert_eq!(
        show.workspace().find_cue(id(1)).unwrap().pre_wait,
        Duration::from_secs_f64(2.5)
    );

    osc(&mut show, "/cue/1/name", &[Arg::String("Thunder".into())]);
    osc(&mut show, "/cue/1/flagged", &[Arg::String("yes".into())]);
    osc(&mut show, "/cue/1/continueMode/-", &[Arg::Int(1)]);
    osc(&mut show, "/cue/1/colorName", &[Arg::String("Red".into())]);
    let cue = show.workspace().find_cue(id(1)).unwrap();
    assert_eq!(cue.name, "Thunder");
    assert!(cue.flagged);
    assert_eq!(cue.continue_mode, ContinueMode::AutoFollow, "rolls over");
    assert_eq!(
        osc(&mut show, "/cue/1/colorName", &[]).data,
        Some(json!("red"))
    );

    osc(&mut show, "/cue/2/duration", &[Arg::Float(9.0)]);
    assert_eq!(
        osc(&mut show, "/cue/2/duration", &[]).data,
        Some(json!(9.0))
    );

    // Writes are undoable like any other edit.
    show.execute(Command::Undo).unwrap();
    assert_eq!(
        osc(&mut show, "/cue/2/duration", &[]).data,
        Some(json!(5.0))
    );

    assert_eq!(
        osc(&mut show, "/cue/1/type", &[Arg::String("x".into())]).status,
        ReplyStatus::Error
    );
    assert_eq!(
        osc(&mut show, "/cue/1/bogus", &[]).status,
        ReplyStatus::Error
    );
}

#[test]
fn osc_workspace_prefix_and_queries() {
    let mut show = show();
    let workspace_id = show.workspace().id.to_string();
    assert!(
        show.handle_osc(&parse("/workspace/other/go", &[]).unwrap())
            .is_none()
    );
    assert!(
        show.handle_osc(&parse(&format!("/workspace/{workspace_id}/go"), &[]).unwrap())
            .is_some()
    );
    assert!(
        show.handle_osc(&parse("/workspace/Test/stop", &[]).unwrap())
            .is_some()
    );

    let lists = osc(&mut show, "/cueLists", &[]).data.unwrap();
    assert_eq!(lists[0]["name"], "Main");
    assert_eq!(lists[0]["cues"][2]["type"], "Group");
    assert_eq!(lists[0]["cues"][2]["cues"][1]["number"], "12");

    let updates = osc(&mut show, "/updates", &[Arg::Int(1)]);
    assert_eq!(updates.session, Some(SessionRequest::Updates(true)));
}

#[test]
fn osc_selection_steps_through_the_list() {
    let mut show = show();
    osc(&mut show, "/select/next", &[]);
    assert_eq!(show.selection(), Some(id(1)));
    for _ in 0..3 {
        osc(&mut show, "/select/next", &[]);
    }
    assert_eq!(show.selection(), Some(id(11)), "descends into groups");
    osc(&mut show, "/select/previous", &[]);
    assert_eq!(show.selection(), Some(id(10)));
    assert_eq!(
        osc(&mut show, "/cue/selected/number", &[]).data,
        Some(json!("10"))
    );
}
