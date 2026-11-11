//! The show actor: a single task owns the [`Show`], drives it with the real
//! clock and serializes every request. Everything else talks to it through a
//! cloneable [`ShowHandle`] and listens to [`Update`] broadcasts.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use mazzalab_core::{AudioCue, CueId, CueKind};
use mazzalab_engine::{Event, EventKind};
use mazzalab_show::{Command, OscOutcome, Show, Snapshot, Status};
use rosc::{OscMessage, OscType};
use serde::Serialize;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::Instant;

use crate::protocol::EventDto;

/// Meter readings at or below this are silence.
const METER_SILENCE_DB: f32 = -100.0;

/// Status is broadcast at this rate while something is running.
const RUNNING_TICK: Duration = Duration::from_millis(50);
/// ...and at this rate when the show is idle, as a keep-alive.
const IDLE_TICK: Duration = Duration::from_secs(1);

/// A media or device backend fed with engine events.
pub trait Output: Send {
    /// Called after every batch of engine events, in order. `show` already
    /// reflects the events.
    fn handle(&mut self, show: &Show, events: &[Event]);

    /// Called once at start and then whenever the show's structure or
    /// settings change, so the backend can follow edits made while it runs.
    fn sync(&mut self, _show: &Show) {}

    /// Peak level in dBFS of every audio output channel since the last call.
    fn meters(&mut self) -> Option<Vec<f32>> {
        None
    }

    /// The state of the audio output, for the backend that owns it.
    fn audio_status(&self) -> Option<AudioStatus> {
        None
    }

    /// The DMX slots that are lit, as `(universe, address, value)`, for the
    /// backend that drives lights. Unless `force` is set, `None` means
    /// nothing changed since the last call.
    fn dmx_levels(&mut self, _force: bool) -> Option<Vec<DmxLevel>> {
        None
    }
}

/// A lit DMX slot: universe, address and value (1–255).
pub type DmxLevel = (u16, u16, u8);

/// How long after a batch of events the lights are looked at again: the
/// lighting thread needs a moment to act on a cue.
const DMX_SETTLE: Duration = Duration::from_millis(60);

/// What the audio output is doing, as shown to operators.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct AudioStatus {
    /// Whether a device is open and playing.
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    pub sample_rate: u32,
    pub channels: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer_size: Option<u32>,
    /// The device asked for was missing and the default one is in use.
    pub fallback: bool,
    /// Why no device could be opened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Something the operator should know: a file that cannot be played, a
/// device that does not answer...
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notice {
    pub level: NoticeLevel,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

/// Given to outputs so they can report that an open-ended action (e.g. an
/// audio file of unknown length) has finished, and tell operators about
/// problems.
#[derive(Debug, Clone)]
pub struct Completion {
    finished: mpsc::UnboundedSender<CueId>,
    notices: mpsc::UnboundedSender<Notice>,
}

impl Completion {
    pub fn action_finished(&self, cue: CueId) {
        // The actor only goes away on shutdown, when nobody cares anymore.
        let _ = self.finished.send(cue);
    }

    /// Shows a message to every connected operator.
    pub fn notify(&self, level: NoticeLevel, message: impl Into<String>) {
        let _ = self.notices.send(Notice {
            level,
            message: message.into(),
        });
    }
}

/// Broadcast to every connected client.
#[derive(Debug, Clone)]
pub enum Update {
    Snapshot(Arc<Snapshot>),
    Status(Arc<Status>),
    Events(Arc<Vec<EventDto>>),
    /// `/update/...` messages for OSC clients that subscribed with `/updates 1`.
    Osc(Arc<Vec<OscMessage>>),
    /// Peak level in dBFS of every audio output channel.
    Meters(Arc<Vec<f32>>),
    Audio(Arc<AudioStatus>),
    Notice(Arc<Notice>),
    /// The DMX slots that are lit, whenever they change.
    Dmx(Arc<Vec<DmxLevel>>),
}

enum Request {
    Command {
        command: Box<Command>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Osc {
        request: mazzalab_osc::Request,
        reply: oneshot::Sender<Option<OscOutcome>>,
    },
    Snapshot {
        reply: oneshot::Sender<Snapshot>,
    },
    Status {
        reply: oneshot::Sender<Status>,
    },
    Save {
        reply: oneshot::Sender<Result<(), String>>,
    },
    SaveCopy {
        path: PathBuf,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Audio {
        reply: oneshot::Sender<Option<AudioStatus>>,
    },
    CueFile {
        cue: CueId,
        reply: oneshot::Sender<Option<PathBuf>>,
    },
    Dmx {
        reply: oneshot::Sender<Option<Vec<DmxLevel>>>,
    },
}

const GONE: &str = "the show is no longer running";

#[derive(Debug, Clone)]
pub struct ShowHandle {
    requests: mpsc::Sender<Request>,
    updates: broadcast::Sender<Update>,
    workspace_id: Arc<str>,
    base_dir: Option<Arc<Path>>,
}

impl ShowHandle {
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    /// The folder of the show file, which relative media paths start from.
    /// `None` for a show that has no file.
    pub fn base_dir(&self) -> Option<&Path> {
        self.base_dir.as_deref()
    }

    /// The state of the audio output, if the show has one.
    pub async fn audio_status(&self) -> Option<AudioStatus> {
        self.ask(|reply| Request::Audio { reply }).await.flatten()
    }

    /// The DMX slots that are lit, if the show drives lights.
    pub async fn dmx_levels(&self) -> Option<Vec<DmxLevel>> {
        self.ask(|reply| Request::Dmx { reply }).await.flatten()
    }

    /// The absolute path of the media file of an audio or video cue.
    pub async fn cue_file(&self, cue: CueId) -> Option<PathBuf> {
        self.ask(|reply| Request::CueFile { cue, reply })
            .await
            .flatten()
    }

    /// Shows a message to every connected operator.
    pub fn notify(&self, level: NoticeLevel, message: impl Into<String>) {
        let _ = self.updates.send(Update::Notice(Arc::new(Notice {
            level,
            message: message.into(),
        })));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Update> {
        self.updates.subscribe()
    }

    pub async fn execute(&self, command: Command) -> Result<(), String> {
        self.ask(|reply| Request::Command {
            command: Box::new(command),
            reply,
        })
        .await
        .unwrap_or_else(|| Err(GONE.to_owned()))
    }

    pub async fn osc(&self, request: mazzalab_osc::Request) -> Option<OscOutcome> {
        self.ask(|reply| Request::Osc { request, reply })
            .await
            .flatten()
    }

    pub async fn snapshot(&self) -> Option<Snapshot> {
        self.ask(|reply| Request::Snapshot { reply }).await
    }

    pub async fn status(&self) -> Option<Status> {
        self.ask(|reply| Request::Status { reply }).await
    }

    /// Saves the show to its file.
    pub async fn save(&self) -> Result<(), String> {
        self.ask(|reply| Request::Save { reply })
            .await
            .unwrap_or_else(|| Err(GONE.to_owned()))
    }

    /// Writes the show to another file, leaving the show's own file and
    /// unsaved state alone. Relative media paths are rewritten so they still
    /// find the same files from the copy's folder.
    pub async fn save_copy(&self, path: PathBuf) -> Result<(), String> {
        self.ask(|reply| Request::SaveCopy { path, reply })
            .await
            .unwrap_or_else(|| Err(GONE.to_owned()))
    }

    async fn ask<T>(&self, request: impl FnOnce(oneshot::Sender<T>) -> Request) -> Option<T> {
        let (reply, response) = oneshot::channel();
        self.requests.send(request(reply)).await.ok()?;
        response.await.ok()
    }
}

/// How the show's file is looked after while the show runs.
#[derive(Debug, Clone)]
pub struct Persistence {
    /// The show file; without one nothing is written.
    pub path: Option<PathBuf>,
    /// Unsaved changes are written next to the show this long after they
    /// are made; `None` turns autosave off.
    pub autosave: Option<Duration>,
    /// How many replaced versions of the show file are kept.
    pub backups: usize,
}

/// Starts the actor task. `make_outputs` receives the completion channel the
/// outputs use to report finished actions.
pub fn spawn(
    show: Show,
    persistence: Persistence,
    make_outputs: impl FnOnce(Completion) -> Vec<Box<dyn Output>>,
) -> (ShowHandle, tokio::task::JoinHandle<()>) {
    let Persistence {
        path,
        autosave,
        backups,
    } = persistence;
    let (requests, receiver) = mpsc::channel(256);
    let (updates, _) = broadcast::channel(1024);
    let (finished_tx, finished_rx) = mpsc::unbounded_channel();
    let (notices_tx, notices_rx) = mpsc::unbounded_channel();
    let outputs = make_outputs(Completion {
        finished: finished_tx,
        notices: notices_tx,
    });
    let base_dir: Option<Arc<Path>> = path.as_deref().and_then(folder).map(Arc::from);
    let handle = ShowHandle {
        requests,
        updates: updates.clone(),
        workspace_id: show.workspace().id.to_string().into(),
        base_dir: base_dir.clone(),
    };
    let show_revision = show.revision();
    let actor = Actor {
        show,
        start: Instant::now(),
        path,
        autosave,
        backups,
        // Recovered changes are already in the autosave they came from.
        autosaved: show_revision,
        autosave_due: None,
        base_dir,
        outputs,
        updates,
        last_revision: None,
        last_status: None,
        last_audio: None,
        dmx_recheck: None,
        meters_live: false,
        unreadable: HashSet::new(),
        workspace_id: handle.workspace_id.clone(),
    };
    let task = tokio::spawn(actor.run(receiver, finished_rx, notices_rx));
    (handle, task)
}

struct Actor {
    show: Show,
    start: Instant,
    path: Option<PathBuf>,
    autosave: Option<Duration>,
    backups: usize,
    /// The revision last written to the autosave file.
    autosaved: u64,
    /// When the changes made since then are written.
    autosave_due: Option<Instant>,
    base_dir: Option<Arc<Path>>,
    outputs: Vec<Box<dyn Output>>,
    updates: broadcast::Sender<Update>,
    last_revision: Option<u64>,
    last_status: Option<(Instant, Status)>,
    last_audio: Option<AudioStatus>,
    /// When to look at the lights again after cues fired.
    dmx_recheck: Option<Instant>,
    /// Whether the last meters sent showed any signal.
    meters_live: bool,
    /// Media files that could not be probed, so they are not tried again on
    /// every edit.
    unreadable: HashSet<(CueId, PathBuf)>,
    workspace_id: Arc<str>,
}

impl Actor {
    async fn run(
        mut self,
        mut requests: mpsc::Receiver<Request>,
        mut finished: mpsc::UnboundedReceiver<CueId>,
        mut notices: mpsc::UnboundedReceiver<Notice>,
    ) {
        loop {
            self.sync();
            let wake = self.next_wake();
            tokio::select! {
                request = requests.recv() => match request {
                    Some(request) => {
                        self.advance();
                        self.handle(request);
                    }
                    None => break,
                },
                Some(cue) = finished.recv() => {
                    self.advance();
                    // A backend may report a cue that was stopped meanwhile.
                    let _ = self.show.action_finished(cue);
                }
                Some(notice) = notices.recv() => {
                    let _ = self.updates.send(Update::Notice(Arc::new(notice)));
                }
                () = tokio::time::sleep_until(wake) => {}
            }
        }
    }

    fn advance(&mut self) {
        self.show.advance_to(self.start.elapsed());
    }

    fn handle(&mut self, request: Request) {
        match request {
            Request::Command { command, reply } => {
                let result = self
                    .show
                    .execute(*command)
                    .map_err(|error| error.to_string());
                let _ = reply.send(result);
            }
            Request::Osc { request, reply } => {
                let _ = reply.send(self.show.handle_osc(&request));
            }
            Request::Snapshot { reply } => {
                let _ = reply.send(self.show.snapshot());
            }
            Request::Status { reply } => {
                let _ = reply.send(self.show.status());
            }
            Request::Save { reply } => {
                let result = match &self.path {
                    None => Err("the show has no file to save to".to_owned()),
                    Some(path) => {
                        mazzalab_project::save_with_backups(
                            path,
                            self.show.workspace(),
                            self.backups,
                        )
                        .map(|()| {
                            self.show.mark_saved();
                            // What was autosaved is now in the show file.
                            mazzalab_project::discard_autosave(path);
                            self.autosaved = self.show.revision();
                            self.autosave_due = None;
                        })
                        .map_err(|error| error.to_string())
                    }
                };
                let _ = reply.send(result);
            }
            Request::SaveCopy { path, reply } => {
                let mut workspace = self.show.workspace().clone();
                let from = self
                    .path
                    .as_deref()
                    .map_or(Some(PathBuf::from(".")), folder);
                if let (Some(from), Some(to)) = (from.as_deref().and_then(absolute), folder(&path))
                {
                    mazzalab_project::rebase_media(&mut workspace, &from, &to);
                }
                let result =
                    mazzalab_project::save(&path, &workspace).map_err(|error| error.to_string());
                let _ = reply.send(result);
            }
            Request::Audio { reply } => {
                let _ = reply.send(self.audio_status());
            }
            Request::Dmx { reply } => {
                let levels = self
                    .outputs
                    .iter_mut()
                    .find_map(|output| output.dmx_levels(true));
                let _ = reply.send(levels);
            }
            Request::CueFile { cue, reply } => {
                let file = self
                    .show
                    .workspace()
                    .find_cue(cue)
                    .and_then(|cue| match &cue.kind {
                        CueKind::Audio(audio) => audio.file.clone(),
                        CueKind::Video(video) => video.file.clone(),
                        _ => None,
                    })
                    .map(|file| self.media_base().join(file));
                let _ = reply.send(file);
            }
        }
    }

    /// Where relative media paths start from.
    fn media_base(&self) -> PathBuf {
        self.base_dir
            .as_deref()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    }

    fn audio_status(&self) -> Option<AudioStatus> {
        self.outputs.iter().find_map(|output| output.audio_status())
    }

    /// Reads the length and channel count of audio files that do not have
    /// them yet: new cues, cues whose file was changed, restored edits.
    fn probe_media(&mut self) {
        let base = self.media_base();
        let pending: Vec<(CueId, PathBuf)> = self
            .show
            .workspace()
            .cues()
            .filter_map(|cue| match &cue.kind {
                CueKind::Audio(AudioCue {
                    file: Some(file),
                    media_duration,
                    media_channels,
                    ..
                }) if media_duration.is_none() || media_channels.is_none() => {
                    Some((cue.id, file.clone()))
                }
                _ => None,
            })
            .filter(|entry| !self.unreadable.contains(entry))
            .collect();
        for (cue, file) in pending {
            match mazzalab_audio::probe(&base.join(&file)) {
                Ok(info) => {
                    if info.duration.is_none() {
                        // Nothing more to learn from this file.
                        self.unreadable.insert((cue, file));
                    }
                    self.show
                        .set_media_info(cue, info.duration, Some(info.channels as u8));
                }
                Err(error) => {
                    tracing::warn!(%cue, %error, "audio file unavailable");
                    let _ = self.updates.send(Update::Notice(Arc::new(Notice {
                        level: NoticeLevel::Warning,
                        message: error.to_string(),
                    })));
                    self.unreadable.insert((cue, file));
                }
            }
        }
    }

    /// Writes unsaved changes next to the show file once they have been
    /// around for the autosave interval.
    fn autosave(&mut self) {
        let (Some(path), Some(interval)) = (&self.path, self.autosave) else {
            return;
        };
        if !self.show.is_dirty() || self.autosaved == self.show.revision() {
            self.autosave_due = None;
            return;
        }
        let due = *self
            .autosave_due
            .get_or_insert_with(|| Instant::now() + interval);
        if Instant::now() < due {
            return;
        }
        self.autosave_due = None;
        self.autosaved = self.show.revision();
        if let Err(error) = mazzalab_project::autosave(path, self.show.workspace()) {
            tracing::warn!(%error, "autosave failed");
            let _ = self.updates.send(Update::Notice(Arc::new(Notice {
                level: NoticeLevel::Warning,
                message: format!("Autosave failed: {error}"),
            })));
        }
    }

    /// Advances the show, feeds outputs and broadcasts what changed.
    fn sync(&mut self) {
        self.advance();
        self.autosave();
        let events = self.show.take_events();
        if !events.is_empty() {
            for output in &mut self.outputs {
                output.handle(&self.show, &events);
            }
            self.dmx_recheck = Some(Instant::now() + DMX_SETTLE);
            let dtos: Vec<EventDto> = events.iter().map(EventDto::from).collect();
            let _ = self.updates.send(Update::Events(Arc::new(dtos)));
            let osc = self.cue_updates(&events);
            if !osc.is_empty() {
                let _ = self.updates.send(Update::Osc(Arc::new(osc)));
            }
        }

        if self.last_revision != Some(self.show.revision()) {
            self.probe_media();
        }
        let revision = self.show.revision();
        let structure_changed = self.last_revision != Some(revision);
        if structure_changed {
            self.last_revision = Some(revision);
            for output in &mut self.outputs {
                output.sync(&self.show);
            }
            let audio = self.audio_status();
            if audio != self.last_audio {
                if let Some(audio) = &audio {
                    let _ = self.updates.send(Update::Audio(Arc::new(audio.clone())));
                }
                self.last_audio = audio;
            }
            let _ = self
                .updates
                .send(Update::Snapshot(Arc::new(self.show.snapshot())));
            let address = format!("/update/workspace/{}", self.workspace_id);
            let _ = self
                .updates
                .send(Update::Osc(Arc::new(vec![message(address, None)])));
        }

        let status = self.show.status();
        let due = match &self.last_status {
            None => true,
            Some((sent, previous)) => {
                let tick = if is_busy(previous) {
                    RUNNING_TICK
                } else {
                    IDLE_TICK
                };
                sent.elapsed() >= tick
                    || !events.is_empty()
                    || structure_changed
                    || status.playheads != previous.playheads
            }
        };
        let recheck = self.dmx_recheck.is_some_and(|at| Instant::now() >= at);
        if recheck {
            self.dmx_recheck = None;
        }
        if due {
            let _ = self.updates.send(Update::Status(Arc::new(status.clone())));
            self.last_status = Some((Instant::now(), status));
            self.send_meters();
        }
        if due || recheck {
            self.send_dmx();
        }
    }

    /// Broadcasts the lit DMX slots when they have changed.
    fn send_dmx(&mut self) {
        if let Some(levels) = self
            .outputs
            .iter_mut()
            .find_map(|output| output.dmx_levels(false))
        {
            let _ = self.updates.send(Update::Dmx(Arc::new(levels)));
        }
    }

    /// Broadcasts the audio meters while there is signal, and once more when
    /// it ends so that clients see them fall.
    fn send_meters(&mut self) {
        let Some(peaks) = self.outputs.iter_mut().find_map(|output| output.meters()) else {
            return;
        };
        let live = peaks.iter().any(|peak| *peak > METER_SILENCE_DB);
        if live || self.meters_live {
            let _ = self.updates.send(Update::Meters(Arc::new(peaks)));
        }
        self.meters_live = live;
    }

    fn next_wake(&self) -> Instant {
        let mut wake = match &self.last_status {
            Some((sent, status)) if is_busy(status) => *sent + RUNNING_TICK,
            Some((sent, _)) => *sent + IDLE_TICK,
            None => Instant::now(),
        };
        if let Some(deadline) = self.show.next_deadline() {
            wake = wake.min(self.start + deadline);
        }
        if let Some(due) = self.autosave_due {
            wake = wake.min(due);
        }
        if let Some(recheck) = self.dmx_recheck {
            wake = wake.min(recheck);
        }
        wake
    }

    fn cue_updates(&self, events: &[Event]) -> Vec<OscMessage> {
        let workspace = &self.workspace_id;
        let mut cues = Vec::new();
        let mut messages = Vec::new();
        for event in events {
            let cue = match event.kind {
                EventKind::Triggered(cue)
                | EventKind::Skipped(cue)
                | EventKind::ActionStarted { cue, .. }
                | EventKind::ActionCompleted(cue)
                | EventKind::Stopped(cue)
                | EventKind::Paused(cue)
                | EventKind::Resumed(cue)
                | EventKind::Panicking { cue, .. } => cue,
                EventKind::PlayheadMoved { list, cue } => {
                    messages.push(message(
                        format!("/update/workspace/{workspace}/cueList/{list}/playbackPosition"),
                        cue.map(|cue| cue.to_string()),
                    ));
                    continue;
                }
            };
            if !cues.contains(&cue) {
                cues.push(cue);
                messages.push(message(
                    format!("/update/workspace/{workspace}/cue_id/{cue}"),
                    None,
                ));
            }
        }
        messages
    }
}

/// The absolute folder holding `file`.
fn folder(file: &Path) -> Option<PathBuf> {
    absolute(file)?.parent().map(Path::to_path_buf)
}

fn absolute(path: &Path) -> Option<PathBuf> {
    std::path::absolute(path).ok()
}

fn is_busy(status: &Status) -> bool {
    status.panicking || !status.running.is_empty()
}

fn message(addr: String, argument: Option<String>) -> OscMessage {
    OscMessage {
        addr,
        args: argument.map(OscType::String).into_iter().collect(),
    }
}
