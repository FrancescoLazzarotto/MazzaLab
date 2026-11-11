//! mazzalab-show
//!
//! The show controller sits between front ends (web UI, OSC, CLI) and the
//! engine. It owns the [`Engine`], tracks the current cue list and the
//! selection, applies edits with undo/redo, and produces serializable
//! [`Snapshot`]s (structure) and [`Status`]es (live playback state) for
//! clients. Like the engine, it performs no I/O.

mod osc;

use std::time::Duration;

use mazzalab_core::{
    ActionLength, Container, Cue, CueId, CueKind, CueList, Workspace, WorkspaceSettings,
};
use mazzalab_engine::{Engine, EngineError, Event, RunPhase};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use osc::{OscOutcome, SessionRequest};

/// Maximum number of undo steps kept in memory.
const UNDO_LIMIT: usize = 200;

/// Everything a client can ask the show to do. Serialized as
/// `{"command": "go", ...}` on the client API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// GO on a list (the current list by default).
    Go {
        #[serde(default)]
        list: Option<CueId>,
    },
    Start {
        cue: CueId,
    },
    Stop {
        cue: CueId,
    },
    Pause {
        cue: CueId,
    },
    Resume {
        cue: CueId,
    },
    StopAll,
    PauseAll,
    ResumeAll,
    Panic,
    Reset,
    /// Sets a playhead. Without `list`, the list containing `cue` (or the
    /// current list when `cue` is `None`).
    SetPlayhead {
        #[serde(default)]
        list: Option<CueId>,
        cue: Option<CueId>,
    },
    PlayheadNext {
        #[serde(default)]
        list: Option<CueId>,
    },
    PlayheadPrevious {
        #[serde(default)]
        list: Option<CueId>,
    },
    Select {
        cue: Option<CueId>,
    },
    SetCurrentList {
        list: CueId,
    },
    /// Replaces a cue's properties. A group keeps its current children;
    /// children are changed with insert, move and delete.
    UpdateCue {
        cue: Cue,
    },
    InsertCue {
        container: Container,
        /// Position in the container; appended when absent.
        #[serde(default)]
        index: Option<usize>,
        cue: Cue,
    },
    DeleteCue {
        cue: CueId,
    },
    /// Moves a cue. `index` is the position in the target container after
    /// the cue has been taken out of its old place.
    MoveCue {
        cue: CueId,
        container: Container,
        #[serde(default)]
        index: Option<usize>,
    },
    AddCueList {
        name: String,
    },
    RenameCueList {
        list: CueId,
        name: String,
    },
    DeleteCueList {
        list: CueId,
    },
    RenameWorkspace {
        name: String,
    },
    UpdateSettings {
        settings: WorkspaceSettings,
    },
    /// Sets (or clears, with `None`) the numbers of several cues at once, so
    /// that renumbering never trips over the numbers it is about to replace.
    SetNumbers {
        numbers: Vec<(CueId, Option<String>)>,
    },
    /// Several edits applied as one: either all of them succeed, as a single
    /// undo step, or the show is left untouched. Only editing commands are
    /// allowed inside.
    Batch {
        commands: Vec<Command>,
    },
    Undo,
    Redo,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum ShowError {
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error("unknown cue {0}")]
    UnknownCue(CueId),
    #[error("unknown cue list {0}")]
    UnknownCueList(CueId),
    #[error("cue {0} is not a group")]
    NotAGroup(CueId),
    #[error("a cue cannot be moved into itself or one of its children")]
    InvalidMove,
    #[error("cue id {0} is already used")]
    DuplicateId(CueId),
    #[error("cue number {0:?} is already used")]
    DuplicateNumber(String),
    #[error("a workspace needs at least one cue list")]
    LastCueList,
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
    #[error("only editing commands can be batched")]
    NotAnEdit,
    #[error("{0}")]
    Invalid(String),
}

/// The structure of the show, sent to clients when it changes.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    /// Increments on every structural change.
    pub revision: u64,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub current_list: CueId,
    pub selection: Option<CueId>,
    pub workspace: Workspace,
}

/// Live playback state, sent to clients continuously while cues run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    /// Engine time in seconds.
    pub now: f64,
    pub panicking: bool,
    pub playheads: Vec<Playhead>,
    pub running: Vec<RunningCue>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Playhead {
    pub list: CueId,
    pub cue: Option<CueId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    PreWait,
    Action,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunningCue {
    pub cue: CueId,
    pub phase: Phase,
    /// Seconds spent in the current phase.
    pub elapsed: f64,
    /// Length of the current phase in seconds, when known.
    pub length: Option<f64>,
    pub paused: bool,
}

#[derive(Debug)]
pub struct Show {
    engine: Engine,
    current_list: CueId,
    selection: Option<CueId>,
    undo: Vec<Workspace>,
    redo: Vec<Workspace>,
    revision: u64,
    dirty: bool,
}

impl Show {
    pub fn new(mut workspace: Workspace) -> Self {
        if workspace.cue_lists.is_empty() {
            workspace.cue_lists.push(CueList::new("Main Cue List"));
        }
        let current_list = workspace.cue_lists[0].id;
        Self {
            engine: Engine::new(workspace),
            current_list,
            selection: None,
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 0,
            dirty: false,
        }
    }

    pub fn workspace(&self) -> &Workspace {
        self.engine.workspace()
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn current_list(&self) -> CueId {
        self.current_list
    }

    pub fn selection(&self) -> Option<CueId> {
        self.selection
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Whether there are edits since the last save.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_saved(&mut self) {
        if self.dirty {
            self.dirty = false;
            self.revision += 1;
        }
    }

    pub fn now(&self) -> Duration {
        self.engine.now()
    }

    pub fn advance_to(&mut self, now: Duration) {
        self.engine.advance_to(now);
    }

    pub fn next_deadline(&self) -> Option<Duration> {
        self.engine.next_deadline()
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        self.engine.take_events()
    }

    /// Forwards a media backend's "this open-ended action has ended".
    pub fn action_finished(&mut self, cue: CueId) -> Result<bool, ShowError> {
        Ok(self.engine.action_finished(cue)?)
    }

    pub fn execute(&mut self, command: Command) -> Result<(), ShowError> {
        match command {
            Command::Go { list } => {
                let list = list.unwrap_or(self.current_list);
                self.engine.go(list)?;
            }
            Command::Start { cue } => self.engine.start(cue)?,
            Command::Stop { cue } => self.engine.stop(cue)?,
            Command::Pause { cue } => self.engine.pause(cue)?,
            Command::Resume { cue } => self.engine.resume(cue)?,
            Command::StopAll => self.engine.stop_all(),
            Command::PauseAll => self.engine.pause_all(),
            Command::ResumeAll => self.engine.resume_all(),
            Command::Panic => self.engine.panic(),
            Command::Reset => self.engine.reset(),
            Command::SetPlayhead { list, cue } => {
                let list = match (list, cue) {
                    (Some(list), _) => list,
                    (None, Some(cue)) => self.list_of(cue)?,
                    (None, None) => self.current_list,
                };
                self.engine.set_playhead(list, cue)?;
            }
            Command::PlayheadNext { list } => {
                self.engine
                    .playhead_next(list.unwrap_or(self.current_list))?;
            }
            Command::PlayheadPrevious { list } => {
                self.engine
                    .playhead_previous(list.unwrap_or(self.current_list))?;
            }
            Command::Select { cue } => {
                if let Some(cue) = cue {
                    self.list_of(cue)?;
                }
                self.selection = cue;
            }
            Command::SetCurrentList { list } => {
                if self.workspace().cue_list(list).is_none() {
                    return Err(ShowError::UnknownCueList(list));
                }
                self.current_list = list;
            }
            Command::Batch { commands } => self.edit(|workspace| {
                commands
                    .into_iter()
                    .try_for_each(|command| apply_edit(workspace, command))
            })?,
            edit @ (Command::UpdateCue { .. }
            | Command::InsertCue { .. }
            | Command::DeleteCue { .. }
            | Command::MoveCue { .. }
            | Command::AddCueList { .. }
            | Command::RenameCueList { .. }
            | Command::DeleteCueList { .. }
            | Command::RenameWorkspace { .. }
            | Command::UpdateSettings { .. }
            | Command::SetNumbers { .. }) => self.edit(|workspace| apply_edit(workspace, edit))?,
            Command::Undo => {
                let previous = self.undo.pop().ok_or(ShowError::NothingToUndo)?;
                let current = self.replace_workspace(previous);
                self.redo.push(current);
            }
            Command::Redo => {
                let next = self.redo.pop().ok_or(ShowError::NothingToRedo)?;
                let current = self.replace_workspace(next);
                self.undo.push(current);
            }
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            revision: self.revision,
            dirty: self.dirty,
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            current_list: self.current_list,
            selection: self.selection,
            workspace: self.workspace().clone(),
        }
    }

    pub fn status(&self) -> Status {
        let playheads = self
            .workspace()
            .cue_lists
            .iter()
            .map(|list| Playhead {
                list: list.id,
                cue: self.engine.playhead(list.id),
            })
            .collect();
        let running = self
            .engine
            .running_cues()
            .filter_map(|cue| {
                let info = self.engine.run_info(cue)?;
                let definition = self.workspace().find_cue(cue)?;
                let (phase, length) = match info.phase {
                    RunPhase::PreWait => (Phase::PreWait, Some(definition.pre_wait)),
                    RunPhase::Action => (
                        Phase::Action,
                        match definition.kind.action_length() {
                            ActionLength::Fixed(length) => Some(length),
                            _ => None,
                        },
                    ),
                };
                Some(RunningCue {
                    cue,
                    phase,
                    elapsed: info.elapsed.as_secs_f64(),
                    length: length.map(|length| length.as_secs_f64()),
                    paused: info.paused,
                })
            })
            .collect();
        Status {
            now: self.engine.now().as_secs_f64(),
            panicking: self.engine.is_panicking(),
            playheads,
            running,
        }
    }

    /// Records what the media layer found out about an audio cue's file.
    /// This is not an edit: it leaves undo history and the saved state alone.
    /// Returns whether anything changed.
    pub fn set_media_info(
        &mut self,
        cue: CueId,
        duration: Option<Duration>,
        channels: Option<u8>,
    ) -> bool {
        let changed = self.engine.edit_workspace(|workspace| {
            let Some(CueKind::Audio(audio)) = workspace.find_cue_mut(cue).map(|cue| &mut cue.kind)
            else {
                return false;
            };
            let duration = duration.or(audio.media_duration);
            let changed = audio.media_duration != duration || audio.media_channels != channels;
            audio.media_duration = duration;
            audio.media_channels = channels;
            changed
        });
        if changed {
            self.revision += 1;
        }
        changed
    }

    fn list_of(&self, cue: CueId) -> Result<CueId, ShowError> {
        self.workspace()
            .list_of(cue)
            .map(|list| list.id)
            .ok_or(ShowError::UnknownCue(cue))
    }

    /// Applies an undoable edit. A failed edit leaves the workspace as it was.
    fn edit<R>(
        &mut self,
        edit: impl FnOnce(&mut Workspace) -> Result<R, ShowError>,
    ) -> Result<R, ShowError> {
        let before = self.workspace().clone();
        let result = self.engine.edit_workspace(|workspace| {
            let result = edit(workspace);
            if result.is_err() {
                *workspace = before.clone();
            }
            result
        });
        if result.is_ok() {
            self.undo.push(before);
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.changed();
        }
        result
    }

    fn replace_workspace(&mut self, workspace: Workspace) -> Workspace {
        let current = self
            .engine
            .edit_workspace(|current| std::mem::replace(current, workspace));
        self.changed();
        current
    }

    fn changed(&mut self) {
        self.revision += 1;
        self.dirty = true;
        let workspace = self.engine.workspace();
        if workspace.cue_list(self.current_list).is_none() {
            self.current_list = workspace.cue_lists[0].id;
        }
        if self
            .selection
            .is_some_and(|cue| workspace.find_cue(cue).is_none())
        {
            self.selection = None;
        }
    }
}

/// Applies one editing command to the workspace.
fn apply_edit(workspace: &mut Workspace, command: Command) -> Result<(), ShowError> {
    match command {
        Command::UpdateCue { cue } => update_cue(workspace, cue),
        Command::InsertCue {
            container,
            index,
            cue,
        } => insert_cue(workspace, container, index, cue),
        Command::DeleteCue { cue } => workspace
            .remove_cue(cue)
            .map(drop)
            .ok_or(ShowError::UnknownCue(cue)),
        Command::MoveCue {
            cue,
            container,
            index,
        } => move_cue(workspace, cue, container, index),
        Command::AddCueList { name } => {
            workspace.cue_lists.push(CueList::new(name));
            Ok(())
        }
        Command::RenameCueList { list, name } => workspace
            .cue_list_mut(list)
            .map(|cue_list| cue_list.name = name)
            .ok_or(ShowError::UnknownCueList(list)),
        Command::DeleteCueList { list } => {
            if workspace.cue_list(list).is_none() {
                return Err(ShowError::UnknownCueList(list));
            }
            if workspace.cue_lists.len() == 1 {
                return Err(ShowError::LastCueList);
            }
            workspace.cue_lists.retain(|cue_list| cue_list.id != list);
            Ok(())
        }
        Command::RenameWorkspace { name } => {
            workspace.name = name;
            Ok(())
        }
        Command::UpdateSettings { settings } => {
            workspace.settings = settings;
            Ok(())
        }
        Command::SetNumbers { numbers } => set_numbers(workspace, numbers),
        Command::Batch { commands } => commands
            .into_iter()
            .try_for_each(|command| apply_edit(workspace, command)),
        _ => Err(ShowError::NotAnEdit),
    }
}

fn set_numbers(
    workspace: &mut Workspace,
    numbers: Vec<(CueId, Option<String>)>,
) -> Result<(), ShowError> {
    // Clear first, so a cue can take the number another one is giving up.
    for (cue, _) in &numbers {
        workspace
            .find_cue_mut(*cue)
            .ok_or(ShowError::UnknownCue(*cue))?
            .number = None;
    }
    for (cue, number) in numbers {
        let number = number.filter(|number| !number.is_empty());
        if let Some(number) = &number
            && workspace.find_by_number(number).is_some()
        {
            return Err(ShowError::DuplicateNumber(number.clone()));
        }
        if let Some(target) = workspace.find_cue_mut(cue) {
            target.number = number;
        }
    }
    Ok(())
}

fn update_cue(workspace: &mut Workspace, mut cue: Cue) -> Result<(), ShowError> {
    if let Some(number) = &cue.number
        && workspace
            .cues()
            .any(|other| other.id != cue.id && other.number.as_ref() == Some(number))
    {
        return Err(ShowError::DuplicateNumber(number.clone()));
    }
    let existing = workspace
        .find_cue_mut(cue.id)
        .ok_or(ShowError::UnknownCue(cue.id))?;
    if let (Some(children), Some(old)) = (cue.children_mut(), existing.children_mut()) {
        *children = std::mem::take(old);
    }
    *existing = cue;
    Ok(())
}

fn insert_cue(
    workspace: &mut Workspace,
    container: Container,
    index: Option<usize>,
    cue: Cue,
) -> Result<(), ShowError> {
    let mut ids = Vec::new();
    collect_ids(&cue, &mut ids);
    if let Some(id) = ids
        .iter()
        .find(|id| workspace.find_cue(**id).is_some() || workspace.cue_list(**id).is_some())
    {
        return Err(ShowError::DuplicateId(*id));
    }
    let mut numbers = Vec::new();
    collect_numbers(&cue, &mut numbers);
    if let Some(number) = numbers
        .iter()
        .find(|number| workspace.find_by_number(number).is_some())
    {
        return Err(ShowError::DuplicateNumber((*number).clone()));
    }
    workspace
        .insert_cue(container, index, cue)
        .map_err(|_| container_error(workspace, container))
}

fn move_cue(
    workspace: &mut Workspace,
    cue: CueId,
    container: Container,
    index: Option<usize>,
) -> Result<(), ShowError> {
    let (_, _, removed) = workspace
        .remove_cue(cue)
        .ok_or(ShowError::UnknownCue(cue))?;
    let mut ids = Vec::new();
    collect_ids(&removed, &mut ids);
    if matches!(container, Container::Group(group) if ids.contains(&group)) {
        return Err(ShowError::InvalidMove);
    }
    workspace
        .insert_cue(container, index, removed)
        .map_err(|_| container_error(workspace, container))
}

fn container_error(workspace: &Workspace, container: Container) -> ShowError {
    match container {
        Container::List(list) => ShowError::UnknownCueList(list),
        Container::Group(cue) if workspace.find_cue(cue).is_some() => ShowError::NotAGroup(cue),
        Container::Group(cue) => ShowError::UnknownCue(cue),
    }
}

fn collect_ids(cue: &Cue, out: &mut Vec<CueId>) {
    out.push(cue.id);
    for child in cue.children() {
        collect_ids(child, out);
    }
}

fn collect_numbers<'a>(cue: &'a Cue, out: &mut Vec<&'a String>) {
    out.extend(cue.number.as_ref());
    for child in cue.children() {
        collect_numbers(child, out);
    }
}

/// Whether a cue kind's duration can be edited directly.
pub(crate) fn duration_mut(kind: &mut CueKind) -> Option<&mut Duration> {
    match kind {
        CueKind::Wait(cue) => Some(&mut cue.duration),
        CueKind::Fade(cue) => Some(&mut cue.duration),
        CueKind::Light(cue) => Some(&mut cue.duration),
        CueKind::Network(cue) => Some(&mut cue.duration),
        _ => None,
    }
}
