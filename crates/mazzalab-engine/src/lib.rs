//! mazzalab-engine
//!
//! The playback engine: given a [`Workspace`], it decides *when* each cue
//! starts, runs and completes. It performs no I/O and never reads a clock.
//! Callers drive time explicitly with [`Engine::advance_to`] and collect the
//! resulting [`Event`]s; media backends react to `ActionStarted` events and
//! report the end of open-ended actions with [`Engine::action_finished`].
//!
//! Because the engine is deterministic, a show can be simulated faster than
//! real time, tested in CI, replayed from a log, and mirrored exactly on a
//! backup machine.
//!
//! # Timing model
//!
//! * A triggered cue first waits its **pre-wait**, then its **action** runs.
//! * **Auto-continue** starts the next cue once the **post-wait** has
//!   elapsed, counted from the start of the action.
//! * **Auto-follow** starts the next cue when the action completes.
//! * A GO starts the cue at the playhead and moves the playhead past the
//!   whole *sequence* (the cue plus every cue chained to it by continue
//!   modes).
//! * A group's action lasts until every child it started has finished.
//!
//! Load, Reset, Devamp, Target, Script and outgoing message cues are instant
//! as far as the engine is concerned; their effect is up to the backend that
//! handles their `ActionStarted` event.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::Duration;

use mazzalab_core::{
    ActionLength, ContinueMode, Cue, CueId, CueKind, FadeCue, GroupMode, Workspace,
};
use thiserror::Error;

/// Maximum depth of synchronously chained triggers. Protects against cue
/// loops with zero waits (e.g. two Start cues starting each other).
const MAX_CHAIN_DEPTH: u32 = 1024;

const DEFAULT_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// Something that happened at a given engine time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub at: Duration,
    pub kind: EventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// The cue was started; its pre-wait (if any) is running.
    Triggered(CueId),
    /// The cue is disarmed with "skip if disarmed" and was passed over.
    Skipped(CueId),
    /// The cue's action began. Backends perform the action only if `armed`.
    ActionStarted {
        cue: CueId,
        armed: bool,
    },
    ActionCompleted(CueId),
    Stopped(CueId),
    Paused(CueId),
    Resumed(CueId),
    /// A panic started: backends fade the cue out over `fade`; the engine
    /// stops it when the fade is over.
    Panicking {
        cue: CueId,
        fade: Duration,
    },
    PlayheadMoved {
        list: CueId,
        cue: Option<CueId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EngineError {
    #[error("unknown cue {0}")]
    UnknownCue(CueId),
    #[error("unknown cue list {0}")]
    UnknownCueList(CueId),
    #[error("cue {cue} is not in cue list {list}")]
    NotInList { cue: CueId, list: CueId },
}

#[derive(Debug, Clone)]
struct Location {
    list: CueId,
    list_index: usize,
    /// Enclosing group, `None` for top-level cues of a list.
    parent: Option<CueId>,
    /// Indices from the list's top level down to the cue.
    path: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunPhase {
    PreWait,
    Action,
}

/// Progress of a running cue, for displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunInfo {
    pub phase: RunPhase,
    /// Time spent in the current phase, excluding pauses.
    pub elapsed: Duration,
    pub paused: bool,
    /// The group whose action this run belongs to, if any.
    pub owner: Option<CueId>,
}

#[derive(Debug, Clone, Copy)]
struct Run {
    phase: RunPhase,
    /// The group whose action this run belongs to, if any.
    owner: Option<CueId>,
    /// Start of the current phase; shifted forward by pauses.
    phase_started: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimerKind {
    PreWaitEnd,
    ActionEnd,
    Continue,
    PanicStop,
}

#[derive(Debug, Clone, Copy)]
struct Timer {
    at: Duration,
    seq: u64,
    cue: CueId,
    owner: Option<CueId>,
    kind: TimerKind,
}

#[derive(Debug, Clone, Copy)]
enum Control {
    Start,
    Stop,
    Pause,
    Goto,
    Arm,
    Disarm,
}

impl Control {
    fn of(kind: &CueKind) -> Option<Self> {
        Some(match kind {
            CueKind::Start => Self::Start,
            CueKind::Stop => Self::Stop,
            CueKind::Pause => Self::Pause,
            CueKind::Goto => Self::Goto,
            CueKind::Arm => Self::Arm,
            CueKind::Disarm => Self::Disarm,
            _ => return None,
        })
    }
}

/// Deterministic cue playback state machine.
#[derive(Debug)]
pub struct Engine {
    workspace: Workspace,
    locations: HashMap<CueId, Location>,
    now: Duration,
    runs: BTreeMap<CueId, Run>,
    timers: Vec<Timer>,
    next_seq: u64,
    paused: HashMap<CueId, Duration>,
    playheads: HashMap<CueId, Option<CueId>>,
    last_go: Option<Duration>,
    /// Groups whose children are being started or stopped; their completion
    /// is evaluated once the operation is over.
    settling: HashSet<CueId>,
    /// While a panic fade is running, cues do not continue automatically.
    panic_until: Option<Duration>,
    chain_depth: u32,
    rng: u64,
    events: Vec<Event>,
}

impl Engine {
    /// Creates an engine at time zero with every playhead on the first cue
    /// of its list.
    pub fn new(workspace: Workspace) -> Self {
        let locations = index_workspace(&workspace);
        let playheads = workspace
            .cue_lists
            .iter()
            .map(|list| (list.id, list.cues.first().map(|cue| cue.id)))
            .collect();

        Self {
            workspace,
            locations,
            now: Duration::ZERO,
            runs: BTreeMap::new(),
            timers: Vec::new(),
            next_seq: 0,
            paused: HashMap::new(),
            playheads,
            last_go: None,
            settling: HashSet::new(),
            panic_until: None,
            chain_depth: 0,
            rng: DEFAULT_SEED,
            events: Vec::new(),
        }
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Current engine time.
    pub fn now(&self) -> Duration {
        self.now
    }

    /// Reseeds the generator used by "start random" groups.
    pub fn seed_random(&mut self, seed: u64) {
        self.rng = seed.max(1);
    }

    pub fn playhead(&self, list: CueId) -> Option<CueId> {
        self.playheads.get(&list).copied().flatten()
    }

    pub fn set_playhead(&mut self, list: CueId, cue: Option<CueId>) -> Result<(), EngineError> {
        if !self.playheads.contains_key(&list) {
            return Err(EngineError::UnknownCueList(list));
        }
        if let Some(cue) = cue {
            let location = self
                .locations
                .get(&cue)
                .ok_or(EngineError::UnknownCue(cue))?;
            if location.list != list {
                return Err(EngineError::NotInList { cue, list });
            }
        }
        self.move_playhead(list, cue);
        Ok(())
    }

    /// Whether the cue is in its pre-wait, its action, or still has a
    /// pending auto-continue.
    pub fn is_active(&self, cue: CueId) -> bool {
        self.runs.contains_key(&cue) || self.timers.iter().any(|timer| timer.cue == cue)
    }

    pub fn is_paused(&self, cue: CueId) -> bool {
        self.paused.contains_key(&cue)
    }

    /// Cues currently in their pre-wait or action, in id order.
    pub fn running_cues(&self) -> impl Iterator<Item = CueId> + '_ {
        self.runs.keys().copied()
    }

    /// Phase and progress of a running cue.
    pub fn run_info(&self, cue: CueId) -> Option<RunInfo> {
        let run = self.runs.get(&cue)?;
        let paused_at = self.paused.get(&cue).copied();
        let now = paused_at.unwrap_or(self.now);
        Some(RunInfo {
            phase: run.phase,
            elapsed: now.saturating_sub(run.phase_started),
            paused: paused_at.is_some(),
            owner: run.owner,
        })
    }

    /// Whether a panic fade is in progress.
    pub fn is_panicking(&self) -> bool {
        self.panic_until.is_some_and(|until| self.now < until)
    }

    /// The next time at which something is scheduled to happen, if any.
    /// A real-time runtime sleeps until then (or until an external input).
    pub fn next_deadline(&self) -> Option<Duration> {
        self.timers
            .iter()
            .filter(|timer| !self.paused.contains_key(&timer.cue))
            .map(|timer| timer.at)
            .min()
    }

    /// Returns and clears the events produced since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Starts the cue at the playhead of `list` and advances the playhead.
    /// Returns the started cue, or `None` if there was nothing to start or
    /// the GO fell inside the double-GO protection window.
    pub fn go(&mut self, list: CueId) -> Result<Option<CueId>, EngineError> {
        if !self.playheads.contains_key(&list) {
            return Err(EngineError::UnknownCueList(list));
        }
        let protection = self.workspace.settings.double_go_protection;
        if let Some(last) = self.last_go
            && !protection.is_zero()
            && self.now < last + protection
        {
            return Ok(None);
        }
        let Some(cue) = self.playhead(list) else {
            return Ok(None);
        };
        self.last_go = Some(self.now);
        // Move first: the started cue may itself move the playhead
        // (Go To cues, "start first and enter" groups).
        let next = self.after_sequence(cue);
        self.move_playhead(list, next);
        self.trigger(cue, None);
        self.settle();
        Ok(Some(cue))
    }

    /// Starts a cue directly, without touching the playhead.
    pub fn start(&mut self, cue: CueId) -> Result<(), EngineError> {
        self.ensure_cue(cue)?;
        self.trigger(cue, None);
        self.settle();
        Ok(())
    }

    /// Stops a cue (and, for a group, everything it started).
    pub fn stop(&mut self, cue: CueId) -> Result<(), EngineError> {
        self.ensure_cue(cue)?;
        self.stop_cue(cue);
        self.settle();
        Ok(())
    }

    pub fn stop_all(&mut self) {
        for cue in self.roots() {
            self.stop_cue(cue);
        }
    }

    pub fn pause(&mut self, cue: CueId) -> Result<(), EngineError> {
        self.ensure_cue(cue)?;
        self.pause_cue(cue);
        Ok(())
    }

    pub fn resume(&mut self, cue: CueId) -> Result<(), EngineError> {
        self.ensure_cue(cue)?;
        self.resume_cue(cue);
        self.settle();
        Ok(())
    }

    pub fn pause_all(&mut self) {
        for cue in self.roots() {
            self.pause_cue(cue);
        }
    }

    pub fn resume_all(&mut self) {
        let paused: BTreeSet<CueId> = self.paused.keys().copied().collect();
        for cue in paused {
            self.resume_cue(cue);
        }
        self.settle();
    }

    /// Fades everything out over the workspace panic duration, then stops
    /// it. Cues waiting in a pre-wait or paused stop immediately, and
    /// nothing continues automatically while the fade runs.
    pub fn panic(&mut self) {
        let fade = self.workspace.settings.panic_duration;
        if fade.is_zero() {
            self.stop_all();
            return;
        }
        let waiting: Vec<CueId> = self
            .runs
            .iter()
            .filter(|(cue, run)| run.phase == RunPhase::PreWait || self.paused.contains_key(cue))
            .map(|(cue, _)| *cue)
            .collect();
        for cue in waiting {
            self.stop_cue(cue);
        }
        self.timers
            .retain(|timer| timer.kind != TimerKind::Continue);
        self.panic_until = Some(self.now + fade);
        for cue in self.roots() {
            let already = self
                .timers
                .iter()
                .any(|timer| timer.cue == cue && timer.kind == TimerKind::PanicStop);
            if !already {
                self.emit(EventKind::Panicking { cue, fade });
                self.schedule(fade, cue, None, TimerKind::PanicStop);
            }
        }
    }

    /// Stops everything and returns every playhead to the top of its list.
    pub fn reset(&mut self) {
        self.stop_all();
        self.panic_until = None;
        self.last_go = None;
        let firsts: Vec<_> = self
            .workspace
            .cue_lists
            .iter()
            .map(|list| (list.id, list.cues.first().map(|cue| cue.id)))
            .collect();
        for (list, first) in firsts {
            self.move_playhead(list, first);
        }
    }

    /// Moves the playhead to the next cue in list order (entering groups'
    /// following siblings, climbing out at the end of a group).
    pub fn playhead_next(&mut self, list: CueId) -> Result<(), EngineError> {
        let next = match self.list_playhead(list)? {
            Some(cue) => self.next_after(cue),
            None => None,
        };
        if next.is_some() {
            self.move_playhead(list, next);
        }
        Ok(())
    }

    /// Moves the playhead to the previous cue; from the end of the list it
    /// goes to the last cue.
    pub fn playhead_previous(&mut self, list: CueId) -> Result<(), EngineError> {
        let previous = match self.list_playhead(list)? {
            Some(cue) => self
                .previous_sibling(cue)
                .or_else(|| self.locations.get(&cue).and_then(|loc| loc.parent)),
            None => self.last_cue_of(list),
        };
        if previous.is_some() {
            self.move_playhead(list, previous);
        }
        Ok(())
    }

    /// Moves the playhead past the current cue sequence.
    pub fn playhead_next_sequence(&mut self, list: CueId) -> Result<(), EngineError> {
        if let Some(cue) = self.list_playhead(list)? {
            let next = self.after_sequence(cue);
            self.move_playhead(list, next);
        }
        Ok(())
    }

    /// Moves the playhead to the top of the current sequence, or of the
    /// previous one if it is already at the top.
    pub fn playhead_previous_sequence(&mut self, list: CueId) -> Result<(), EngineError> {
        let Some(current) = self.list_playhead(list)?.or_else(|| self.last_cue_of(list)) else {
            return Ok(());
        };
        let start = self.sequence_start(current);
        let target = if start != current || self.playhead(list).is_none() {
            start
        } else {
            self.previous_sibling(start)
                .map_or(start, |previous| self.sequence_start(previous))
        };
        self.move_playhead(list, Some(target));
        Ok(())
    }

    /// Applies an edit to the workspace while the show may be running.
    /// Cues removed by the edit are stopped; playheads pointing at cues that
    /// left their list return to the top of the list.
    pub fn edit_workspace<R>(&mut self, edit: impl FnOnce(&mut Workspace) -> R) -> R {
        let result = edit(&mut self.workspace);
        self.locations = index_workspace(&self.workspace);

        let gone: BTreeSet<CueId> = self
            .runs
            .keys()
            .copied()
            .chain(self.timers.iter().map(|timer| timer.cue))
            .filter(|cue| !self.locations.contains_key(cue))
            .collect();
        for cue in gone {
            self.stop_cue(cue);
        }

        let lists: Vec<_> = self
            .workspace
            .cue_lists
            .iter()
            .map(|list| (list.id, list.cues.first().map(|cue| cue.id)))
            .collect();
        self.playheads
            .retain(|list, _| lists.iter().any(|(id, _)| id == list));
        for (list, first) in lists {
            let current = self.playheads.get(&list).copied();
            let valid = match current {
                Some(Some(cue)) => self
                    .locations
                    .get(&cue)
                    .is_some_and(|location| location.list == list),
                Some(None) => true,
                None => false,
            };
            if !valid {
                self.move_playhead(list, first);
            }
        }
        self.settle();
        result
    }

    /// Called by a backend when an open-ended action (unknown media length)
    /// has finished. Returns `false` if the cue's action was not running.
    pub fn action_finished(&mut self, cue: CueId) -> Result<bool, EngineError> {
        self.ensure_cue(cue)?;
        if self
            .runs
            .get(&cue)
            .is_none_or(|run| run.phase != RunPhase::Action)
        {
            return Ok(false);
        }
        self.finish_action(cue);
        self.settle();
        Ok(true)
    }

    /// Advances engine time, firing every scheduled transition up to and
    /// including `time` in chronological order. Going backwards is a no-op.
    pub fn advance_to(&mut self, time: Duration) {
        while let Some(index) = self.next_due(time) {
            let timer = self.timers.swap_remove(index);
            self.now = self.now.max(timer.at);
            self.fire(timer);
        }
        self.now = self.now.max(time);
    }

    pub fn advance_by(&mut self, delta: Duration) {
        self.advance_to(self.now + delta);
    }

    // ----- lookup -------------------------------------------------------

    fn ensure_cue(&self, cue: CueId) -> Result<(), EngineError> {
        if self.locations.contains_key(&cue) {
            Ok(())
        } else {
            Err(EngineError::UnknownCue(cue))
        }
    }

    fn cue(&self, id: CueId) -> Option<&Cue> {
        let location = self.locations.get(&id)?;
        let (last, parents) = location.path.split_last()?;
        let mut cues = &self.workspace.cue_lists[location.list_index].cues[..];
        for &index in parents {
            cues = cues[index].children();
        }
        cues.get(*last)
    }

    fn siblings(&self, id: CueId) -> &[Cue] {
        let Some(location) = self.locations.get(&id) else {
            return &[];
        };
        match location.parent {
            Some(parent) => self.cue(parent).map_or(&[], Cue::children),
            None => &self.workspace.cue_lists[location.list_index].cues,
        }
    }

    fn next_sibling(&self, id: CueId) -> Option<CueId> {
        let index = *self.locations.get(&id)?.path.last()?;
        self.siblings(id).get(index + 1).map(|cue| cue.id)
    }

    fn previous_sibling(&self, id: CueId) -> Option<CueId> {
        let index = *self.locations.get(&id)?.path.last()?;
        let previous = index.checked_sub(1)?;
        self.siblings(id).get(previous).map(|cue| cue.id)
    }

    fn list_playhead(&self, list: CueId) -> Result<Option<CueId>, EngineError> {
        self.playheads
            .get(&list)
            .copied()
            .ok_or(EngineError::UnknownCueList(list))
    }

    fn last_cue_of(&self, list: CueId) -> Option<CueId> {
        self.workspace.cue_list(list)?.cues.last().map(|cue| cue.id)
    }

    /// The first cue of the sequence containing `cue`: walks back while the
    /// previous sibling continues into it.
    fn sequence_start(&self, mut cue: CueId) -> CueId {
        while let Some(previous) = self.previous_sibling(cue) {
            if self
                .cue(previous)
                .is_none_or(|c| c.continue_mode == ContinueMode::DoNotContinue)
            {
                break;
            }
            cue = previous;
        }
        cue
    }

    /// The next cue in list order after `cue`, climbing out of groups.
    fn next_after(&self, mut cue: CueId) -> Option<CueId> {
        loop {
            if let Some(next) = self.next_sibling(cue) {
                return Some(next);
            }
            cue = self.locations.get(&cue)?.parent?;
        }
    }

    /// The cue after the sequence that starts at `cue`.
    fn after_sequence(&self, mut cue: CueId) -> Option<CueId> {
        while self
            .cue(cue)
            .is_some_and(|c| c.continue_mode != ContinueMode::DoNotContinue)
        {
            match self.next_sibling(cue) {
                Some(next) => cue = next,
                None => break,
            }
        }
        self.next_after(cue)
    }

    fn group_mode(&self, id: CueId) -> Option<GroupMode> {
        match &self.cue(id)?.kind {
            CueKind::Group(group) => Some(group.mode),
            _ => None,
        }
    }

    /// Cues whose current run or pending continue belongs to group `id`.
    fn members(&self, id: CueId) -> BTreeSet<CueId> {
        self.runs
            .iter()
            .filter(|(_, run)| run.owner == Some(id))
            .map(|(cue, _)| *cue)
            .chain(
                self.timers
                    .iter()
                    .filter(|timer| timer.owner == Some(id))
                    .map(|timer| timer.cue),
            )
            .collect()
    }

    /// Active cues not owned by a group.
    fn roots(&self) -> BTreeSet<CueId> {
        self.runs
            .iter()
            .filter(|(_, run)| run.owner.is_none())
            .map(|(cue, _)| *cue)
            .chain(
                self.timers
                    .iter()
                    .filter(|timer| timer.owner.is_none())
                    .map(|timer| timer.cue),
            )
            .collect()
    }

    // ----- state changes ------------------------------------------------

    fn emit(&mut self, kind: EventKind) {
        self.events.push(Event { at: self.now, kind });
    }

    fn move_playhead(&mut self, list: CueId, cue: Option<CueId>) {
        if self.playheads.insert(list, cue) != Some(cue) {
            self.emit(EventKind::PlayheadMoved { list, cue });
        }
    }

    fn schedule(&mut self, delay: Duration, cue: CueId, owner: Option<CueId>, kind: TimerKind) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.timers.push(Timer {
            at: self.now + delay,
            seq,
            cue,
            owner,
            kind,
        });
    }

    fn next_due(&self, limit: Duration) -> Option<usize> {
        self.timers
            .iter()
            .enumerate()
            .filter(|(_, timer)| timer.at <= limit && !self.paused.contains_key(&timer.cue))
            .min_by_key(|(_, timer)| (timer.at, timer.seq))
            .map(|(index, _)| index)
    }

    /// Fires every transition that is already due.
    fn settle(&mut self) {
        self.advance_to(self.now);
    }

    fn fire(&mut self, timer: Timer) {
        match timer.kind {
            TimerKind::PreWaitEnd => self.begin_action(timer.cue),
            TimerKind::ActionEnd => self.finish_action(timer.cue),
            TimerKind::Continue => {
                self.continue_after(timer.cue, timer.owner);
                if let Some(group) = timer.owner {
                    self.check_group(group);
                }
            }
            TimerKind::PanicStop => self.stop_cue(timer.cue),
        }
    }

    fn trigger(&mut self, id: CueId, owner: Option<CueId>) {
        if self.runs.contains_key(&id) || self.chain_depth >= MAX_CHAIN_DEPTH {
            return;
        }
        let Some(cue) = self.cue(id) else {
            return;
        };
        let (armed, skip, pre_wait, mode) = (
            cue.armed,
            cue.skip_if_disarmed,
            cue.pre_wait,
            cue.continue_mode,
        );

        self.chain_depth += 1;
        if !armed && skip {
            self.emit(EventKind::Skipped(id));
            if mode != ContinueMode::DoNotContinue {
                self.continue_after(id, owner);
            }
        } else {
            self.emit(EventKind::Triggered(id));
            self.runs.insert(
                id,
                Run {
                    phase: RunPhase::PreWait,
                    owner,
                    phase_started: self.now,
                },
            );
            if pre_wait.is_zero() {
                self.begin_action(id);
            } else {
                self.schedule(pre_wait, id, owner, TimerKind::PreWaitEnd);
            }
        }
        self.chain_depth -= 1;
    }

    fn begin_action(&mut self, id: CueId) {
        let Some(run) = self.runs.get_mut(&id) else {
            return;
        };
        run.phase = RunPhase::Action;
        run.phase_started = self.now;
        let owner = run.owner;
        let Some(cue) = self.cue(id) else {
            return;
        };
        let armed = cue.armed;
        let continue_mode = cue.continue_mode;
        let post_wait = cue.post_wait;
        let target = cue.target;
        // A disarmed cue keeps its timing but has no action.
        let length = if armed {
            cue.kind.action_length()
        } else {
            ActionLength::Instant
        };
        let control = if armed { Control::of(&cue.kind) } else { None };

        self.emit(EventKind::ActionStarted { cue: id, armed });
        if continue_mode == ContinueMode::AutoContinue {
            self.schedule(post_wait, id, owner, TimerKind::Continue);
        }
        if let (Some(control), Some(target)) = (control, target) {
            self.apply_control(control, target);
        }
        match length {
            ActionLength::Instant => self.finish_action(id),
            ActionLength::Fixed(duration) => {
                self.schedule(duration, id, owner, TimerKind::ActionEnd);
            }
            ActionLength::UntilStopped | ActionLength::UntilReported => {}
            ActionLength::UntilChildrenComplete => self.start_group(id),
        }
    }

    fn apply_control(&mut self, control: Control, target: CueId) {
        match control {
            Control::Start => self.trigger(target, None),
            Control::Stop => self.stop_cue(target),
            Control::Pause => self.pause_cue(target),
            Control::Goto => {
                if let Some(list) = self.locations.get(&target).map(|location| location.list) {
                    self.move_playhead(list, Some(target));
                }
            }
            Control::Arm | Control::Disarm => {
                if let Some(cue) = self.workspace.find_cue_mut(target) {
                    cue.armed = matches!(control, Control::Arm);
                }
            }
        }
    }

    fn start_group(&mut self, id: CueId) {
        let Some(CueKind::Group(group)) = self.cue(id).map(|cue| &cue.kind) else {
            return;
        };
        let mode = group.mode;
        let children: Vec<CueId> = group.children.iter().map(|cue| cue.id).collect();
        if children.is_empty() {
            self.finish_action(id);
            return;
        }

        self.settling.insert(id);
        match mode {
            GroupMode::Timeline => {
                for child in &children {
                    self.trigger(*child, Some(id));
                }
            }
            GroupMode::StartFirst | GroupMode::Playlist => self.trigger(children[0], Some(id)),
            GroupMode::StartFirstAndEnter => {
                self.trigger(children[0], Some(id));
                let list = self.locations[&id].list;
                let next = children.get(1).copied().or_else(|| self.next_after(id));
                self.move_playhead(list, next);
            }
            GroupMode::StartRandom => {
                let index = (self.next_random() % children.len() as u64) as usize;
                self.trigger(children[index], Some(id));
            }
        }
        self.settling.remove(&id);
        self.check_group(id);
    }

    fn finish_action(&mut self, id: CueId) {
        let Some(run) = self.runs.remove(&id) else {
            return;
        };
        self.timers
            .retain(|timer| !(timer.cue == id && timer.kind != TimerKind::Continue));
        self.emit(EventKind::ActionCompleted(id));

        let (continue_mode, stop_target) =
            self.cue(id)
                .map_or((ContinueMode::DoNotContinue, None), |cue| {
                    let stop_target = match (&cue.kind, cue.armed) {
                        (
                            CueKind::Fade(FadeCue {
                                stop_target_when_done: true,
                                ..
                            }),
                            true,
                        ) => cue.target,
                        _ => None,
                    };
                    (cue.continue_mode, stop_target)
                });

        if let Some(target) = stop_target {
            self.stop_cue(target);
        }
        let playlist_step = continue_mode == ContinueMode::DoNotContinue
            && run
                .owner
                .is_some_and(|group| self.group_mode(group) == Some(GroupMode::Playlist));
        if continue_mode == ContinueMode::AutoFollow || playlist_step {
            self.continue_after(id, run.owner);
        }
        if let Some(group) = run.owner {
            self.check_group(group);
        }
    }

    fn continue_after(&mut self, id: CueId, owner: Option<CueId>) {
        if self.is_panicking() {
            return;
        }
        if let Some(next) = self.next_sibling(id) {
            self.trigger(next, owner);
        }
    }

    /// Completes a group's action once nothing it started is still active.
    fn check_group(&mut self, group: CueId) {
        if self.settling.contains(&group)
            || self
                .runs
                .get(&group)
                .is_none_or(|run| run.phase != RunPhase::Action)
        {
            return;
        }
        let busy = self.runs.values().any(|run| run.owner == Some(group))
            || self.timers.iter().any(|timer| timer.owner == Some(group));
        if !busy {
            self.finish_action(group);
        }
    }

    fn stop_cue(&mut self, id: CueId) {
        self.settling.insert(id);
        for member in self.members(id) {
            self.stop_cue(member);
        }
        let timer_owner = self
            .timers
            .iter()
            .find(|timer| timer.cue == id)
            .and_then(|timer| timer.owner);
        let run = self.runs.remove(&id);
        let timers_before = self.timers.len();
        self.timers.retain(|timer| timer.cue != id);
        let had_timers = self.timers.len() != timers_before;
        self.paused.remove(&id);
        self.settling.remove(&id);

        if run.is_some() || had_timers {
            self.emit(EventKind::Stopped(id));
            if let Some(group) = run.and_then(|run| run.owner).or(timer_owner) {
                self.check_group(group);
            }
        }
    }

    fn pause_cue(&mut self, id: CueId) {
        if !self.is_active(id) || self.paused.contains_key(&id) {
            return;
        }
        for member in self.members(id) {
            self.pause_cue(member);
        }
        self.paused.insert(id, self.now);
        self.emit(EventKind::Paused(id));
    }

    fn resume_cue(&mut self, id: CueId) {
        for member in self.members(id) {
            self.resume_cue(member);
        }
        if let Some(since) = self.paused.remove(&id) {
            let delta = self.now - since;
            for timer in self.timers.iter_mut().filter(|timer| timer.cue == id) {
                timer.at += delta;
            }
            if let Some(run) = self.runs.get_mut(&id) {
                run.phase_started += delta;
            }
            self.emit(EventKind::Resumed(id));
        }
    }

    /// xorshift64: tiny, deterministic and good enough to pick a cue.
    fn next_random(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }
}

fn index_workspace(workspace: &Workspace) -> HashMap<CueId, Location> {
    let mut locations = HashMap::new();
    for (list_index, list) in workspace.cue_lists.iter().enumerate() {
        index_cues(
            &list.cues,
            list.id,
            list_index,
            None,
            &mut Vec::new(),
            &mut locations,
        );
    }
    locations
}

fn index_cues(
    cues: &[Cue],
    list: CueId,
    list_index: usize,
    parent: Option<CueId>,
    path: &mut Vec<usize>,
    out: &mut HashMap<CueId, Location>,
) {
    for (index, cue) in cues.iter().enumerate() {
        path.push(index);
        out.insert(
            cue.id,
            Location {
                list,
                list_index,
                parent,
                path: path.clone(),
            },
        );
        index_cues(cue.children(), list, list_index, Some(cue.id), path, out);
        path.pop();
    }
}
