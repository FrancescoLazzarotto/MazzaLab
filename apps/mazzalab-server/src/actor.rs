//! The show actor: a single task owns the [`Show`], drives it with the real
//! clock and serializes every request. Everything else talks to it through a
//! cloneable [`ShowHandle`] and listens to [`Update`] broadcasts.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use mazzalab_core::{AudioCue, CueId, CueKind, Passcode, SmpteFormat, Workspace};
use mazzalab_engine::{Event, EventKind};
use mazzalab_midi::MidiEvent;
use mazzalab_show::{Command, OscOutcome, Show, Snapshot, Status};
use mazzalab_timecode::{Rate, Timecode};
use mazzalab_video::StageState;
use rosc::{OscMessage, OscType};
use serde::Serialize;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::Instant;

use crate::access::Access;
use crate::follow::{RecordTap, Tee, Tick, state_hash};
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

    /// Relative media paths start from `base` from now on: another show
    /// was opened, or the show was saved somewhere else.
    fn rebase(&mut self, _base: &Path) {}

    /// Peak level in dBFS of every audio output channel since the last call.
    fn meters(&mut self) -> Option<Vec<f32>> {
        None
    }

    /// The state of the audio output, for the backend that owns it.
    fn audio_status(&self) -> Option<AudioStatus> {
        None
    }

    /// How hard the audio thread worked since the last call.
    fn audio_load(&mut self) -> Option<mazzalab_audio::AudioLoad> {
        None
    }

    /// Plays a cue by itself on the audition output, or stops what is being
    /// auditioned when `cue` is `None`. Nothing in the show changes.
    fn audition(&mut self, _show: &Show, _cue: Option<CueId>) {}

    /// The DMX slots that are lit, as `(universe, address, value)`, for the
    /// backend that drives lights. Unless `force` is set, `None` means
    /// nothing changed since the last call.
    fn dmx_levels(&mut self, _force: bool) -> Option<Vec<DmxLevel>> {
        None
    }

    /// What the stages show, for the backend that keeps it. Unless `force`
    /// is set, `None` means nothing changed since the last call.
    fn stage(&mut self, _force: bool) -> Option<StageState> {
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
    /// The cue being auditioned, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audition: Option<CueId>,
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
    midi: mpsc::UnboundedSender<MidiEvent>,
    timecode: mpsc::UnboundedSender<(Timecode, Rate)>,
    sysex: mpsc::UnboundedSender<Vec<u8>>,
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

    /// Hands the show a MIDI message that arrived on an input: it starts
    /// the cues and works the transport controls that wait for it.
    pub fn midi_received(&self, event: MidiEvent) {
        let _ = self.midi.send(event);
    }

    /// Hands the show a SysEx message from a MIDI input, F0 to F7: MIDI
    /// Show Control among them.
    pub fn sysex_received(&self, bytes: &[u8]) {
        let _ = self.sysex.send(bytes.to_vec());
    }

    /// Hands the show the timecode that came in: it starts the cues whose
    /// timecode trigger it passes.
    pub fn timecode_received(&self, timecode: Timecode, rate: Rate) {
        let _ = self.timecode.send((timecode, rate));
    }
}

/// Timecode coming into the show, as shown to operators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TimecodeIn {
    /// `HH:MM:SS:FF`.
    pub timecode: String,
    /// The rate it runs at, as far as MTC tells.
    pub format: SmpteFormat,
}

/// The show format a rate read off MTC stands for.
fn format_of(rate: Rate) -> SmpteFormat {
    match (rate.base, rate.drop_frame) {
        (24, _) => SmpteFormat::Fps24,
        (25, _) => SmpteFormat::Fps25,
        (30, true) => SmpteFormat::Fps29_97Drop,
        _ => SmpteFormat::Fps30NonDrop,
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
    /// A MIDI message arrived on an input.
    MidiIn(MidiEvent),
    /// Timecode came in.
    Timecode(Arc<TimecodeIn>),
    /// What the stages show changed.
    Stage(Arc<StageState>),
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
    OscTrigger {
        address: String,
        reply: oneshot::Sender<usize>,
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
    MidiIn {
        event: MidiEvent,
        reply: oneshot::Sender<()>,
    },
    TimecodeIn {
        timecode: Timecode,
        rate: Rate,
        reply: oneshot::Sender<()>,
    },
    Stage {
        reply: oneshot::Sender<Option<StageState>>,
    },
    MediaEnded {
        cue: CueId,
        reply: oneshot::Sender<()>,
    },
    Audition {
        cue: Option<CueId>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Open {
        workspace: Box<Workspace>,
        path: Option<PathBuf>,
        recovered: bool,
        reply: oneshot::Sender<()>,
    },
    SaveAs {
        path: PathBuf,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Diagnostics {
        reply: oneshot::Sender<Diagnostics>,
    },
    FollowStart {
        header: Box<mazzalab_show::Header>,
        reply: oneshot::Sender<()>,
    },
    FollowEntry {
        entry: Box<mazzalab_show::Entry>,
        reply: oneshot::Sender<()>,
    },
    FollowTick {
        tick: Tick,
        reply: oneshot::Sender<()>,
    },
    TakeOver {
        reply: oneshot::Sender<()>,
    },
}

/// What tells the open show apart, which changes when another is opened.
#[derive(Debug)]
struct Identity {
    workspace_id: Arc<str>,
    path: Option<Arc<Path>>,
    base_dir: Option<Arc<Path>>,
    /// The show's passcodes, so access is checked without asking the actor.
    passcodes: Arc<[Passcode]>,
    /// Clients on the show's own machine need no passcode.
    trust_local: bool,
    /// This server follows another as its backup: nobody may run or change
    /// the show here until it takes over.
    following: bool,
}

impl Identity {
    fn new(workspace: &Workspace, path: Option<&Path>) -> Self {
        Self {
            workspace_id: workspace.id.to_string().into(),
            path: path.map(|path| Arc::from(absolute(path).unwrap_or_else(|| path.to_path_buf()))),
            base_dir: path.and_then(folder).map(Arc::from),
            passcodes: workspace.settings.passcodes.clone().into(),
            trust_local: true,
            following: false,
        }
    }
}

const GONE: &str = "the show is no longer running";

#[derive(Debug, Clone)]
pub struct ShowHandle {
    requests: mpsc::Sender<Request>,
    updates: broadcast::Sender<Update>,
    identity: Arc<RwLock<Identity>>,
    clients: Arc<std::sync::atomic::AtomicUsize>,
    tap: Arc<RecordTap>,
}

/// Counts a connected client for as long as it is held.
#[derive(Debug)]
pub struct ClientGuard(Arc<std::sync::atomic::AtomicUsize>);

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// How the server is doing, for the diagnostics panel.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostics {
    /// Seconds since the server started.
    pub uptime: f64,
    /// WebSocket clients connected now.
    pub clients: usize,
    /// The audio thread's load since the previous reading, 0 to 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_load: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_peak_load: Option<f32>,
    /// Audio callbacks that came late since the device opened: likely
    /// dropouts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_late: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_callbacks: Option<u64>,
    /// Cues running now.
    pub running: usize,
    /// Where this show's inputs are recorded, and why not if it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_error: Option<String>,
}

impl ShowHandle {
    /// The id of the open show's workspace.
    pub fn workspace_id(&self) -> Arc<str> {
        Arc::clone(&self.identity().workspace_id)
    }

    /// The folder of the show file, which relative media paths start from.
    /// `None` for a show that has no file.
    pub fn base_dir(&self) -> Option<Arc<Path>> {
        self.identity().base_dir.clone()
    }

    /// What a client that gave `passcode` may do; `local` clients run on
    /// the show's own machine.
    pub fn access(&self, passcode: Option<&str>, local: bool) -> Access {
        let identity = self.identity();
        let access = Access::grant(&identity.passcodes, passcode, local && identity.trust_local);
        if identity.following {
            Access {
                control: false,
                edit: false,
                ..access
            }
        } else {
            access
        }
    }

    /// Whether this server is following another as its backup.
    pub fn is_following(&self) -> bool {
        self.identity().following
    }

    /// The record as it is written: every line so far and those to come.
    pub fn follow_stream(&self) -> (Vec<Arc<str>>, broadcast::Receiver<Arc<str>>) {
        self.tap.subscribe()
    }

    /// Following: the main server started (or opened) a show.
    pub async fn follow_start(&self, header: mazzalab_show::Header) {
        self.ask(|reply| Request::FollowStart {
            header: Box::new(header),
            reply,
        })
        .await;
    }

    /// Following: an input the main server received.
    pub async fn follow_entry(&self, entry: mazzalab_show::Entry) {
        self.ask(|reply| Request::FollowEntry {
            entry: Box::new(entry),
            reply,
        })
        .await;
    }

    /// Following: where the main server is.
    pub async fn follow_tick(&self, tick: Tick) {
        self.ask(|reply| Request::FollowTick { tick, reply }).await;
    }

    /// The main server is gone: run the show from here.
    pub async fn take_over(&self) {
        self.ask(|reply| Request::TakeOver { reply }).await;
    }

    /// Whether clients on the show's own machine need no passcode (the
    /// default). A server behind a proxy on the same machine turns it off.
    pub fn set_trust_local(&self, trust: bool) {
        self.identity
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .trust_local = trust;
    }

    /// Whether the show asks clients for a passcode.
    pub fn has_passcodes(&self) -> bool {
        !self.identity().passcodes.is_empty()
    }

    /// Counts a client as connected until the guard is dropped.
    pub fn client_connected(&self) -> ClientGuard {
        self.clients
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        ClientGuard(Arc::clone(&self.clients))
    }

    /// How the server is doing.
    pub async fn diagnostics(&self) -> Option<Diagnostics> {
        let mut diagnostics = self.ask(|reply| Request::Diagnostics { reply }).await?;
        diagnostics.clients = self.clients.load(std::sync::atomic::Ordering::Relaxed);
        Some(diagnostics)
    }

    /// The open show's file, if it has one.
    pub fn path(&self) -> Option<Arc<Path>> {
        self.identity().path.clone()
    }

    fn identity(&self) -> std::sync::RwLockReadGuard<'_, Identity> {
        self.identity.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// Replaces the running show with `workspace`, read from `path`, on the
    /// same sockets: clients stay connected and see the new show. Whatever
    /// was running stops at once, as with a reset. `recovered` says the
    /// workspace holds unsaved changes from an autosave.
    pub async fn open(&self, workspace: Workspace, path: Option<PathBuf>, recovered: bool) {
        self.ask(|reply| Request::Open {
            workspace: Box::new(workspace),
            path,
            recovered,
            reply,
        })
        .await;
    }

    /// Saves the show to another file, which becomes the show's file.
    /// Nothing stops: relative media paths are rewritten to start from the
    /// new folder, and the old file keeps its last saved state.
    pub async fn save_as(&self, path: PathBuf) -> Result<(), String> {
        self.ask(|reply| Request::SaveAs { path, reply })
            .await
            .unwrap_or_else(|| Err(GONE.to_owned()))
    }

    /// The state of the audio output, if the show has one.
    pub async fn audio_status(&self) -> Option<AudioStatus> {
        self.ask(|reply| Request::Audio { reply }).await.flatten()
    }

    /// Plays a cue by itself on the audition output (headphones), without
    /// running it in the show; `None` stops what is being auditioned.
    pub async fn audition(&self, cue: Option<CueId>) -> Result<(), String> {
        self.ask(|reply| Request::Audition { cue, reply })
            .await
            .unwrap_or_else(|| Err(GONE.to_owned()))
    }

    /// Feeds the show a MIDI message as if it had arrived on an input.
    pub async fn midi_in(&self, event: MidiEvent) {
        self.ask(|reply| Request::MidiIn { event, reply }).await;
    }

    /// What the stages show now, if the show has stages.
    pub async fn stage(&self) -> Option<StageState> {
        self.ask(|reply| Request::Stage { reply }).await.flatten()
    }

    /// A stage played a cue's file to its end: the cue completes, as an
    /// audio cue does when its sound ends.
    pub async fn media_ended(&self, cue: CueId) {
        self.ask(|reply| Request::MediaEnded { cue, reply }).await;
    }

    /// Feeds the show timecode as if it had come in.
    pub async fn timecode_in(&self, timecode: Timecode, rate: Rate) {
        self.ask(|reply| Request::TimecodeIn {
            timecode,
            rate,
            reply,
        })
        .await;
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

    /// Starts the cues whose OSC trigger matches `address`; returns how many.
    pub async fn osc_trigger(&self, address: String) -> usize {
        self.ask(|reply| Request::OscTrigger { address, reply })
            .await
            .unwrap_or(0)
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
    /// Every input is written to `logs/` next to the show file, to replay
    /// the performance later.
    pub record: bool,
}

/// The input records kept per show; older ones are deleted.
const RECORDS_KEPT: usize = 20;

/// Starts writing the inputs of `show` to a new record next to `path`.
/// Starts recording the inputs of `show` for followers (`tap`) and, when
/// `to_file`, to a new record next to `path`. Returns the file.
fn start_record(
    show: &mut Show,
    path: Option<&Path>,
    to_file: bool,
    tap: &Arc<RecordTap>,
) -> Option<PathBuf> {
    let now = chrono::Local::now();
    let target = path.filter(|_| to_file).and_then(|path| {
        let dir = folder(path)?.join(LOGS_FOLDER);
        let stem = path.file_stem().map_or_else(
            || "show".to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let file = dir.join(format!("{stem} {}.jsonl", now.format("%Y-%m-%d %H.%M.%S")));
        match std::fs::create_dir_all(&dir).and_then(|()| std::fs::File::create(&file)) {
            Ok(out) => Some((dir, stem, file, out)),
            Err(error) => {
                tracing::warn!(%error, record = %file.display(), "inputs not recorded");
                None
            }
        }
    });
    let (file, out) = match target {
        Some((dir, stem, file, out)) => {
            prune_records(&dir, &stem);
            let out: Box<dyn std::io::Write + Send> = Box::new(std::io::BufWriter::new(out));
            (Some(file), Some(out))
        }
        None => (None, None),
    };
    let tee = Tee::new(out, Arc::clone(tap));
    match show.record_to(Box::new(tee), &now.to_rfc3339()) {
        Ok(()) => file,
        Err(error) => {
            tracing::warn!(%error, "inputs not recorded");
            None
        }
    }
}

/// Deletes the oldest records of `stem` beyond [`RECORDS_KEPT`].
fn prune_records(dir: &Path, stem: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let prefix = format!("{stem} ");
    let mut records: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonl")
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
        })
        .collect();
    // The names carry the time, so they sort by it.
    records.sort();
    let excess = records.len().saturating_sub(RECORDS_KEPT);
    for old in &records[..excess] {
        let _ = std::fs::remove_file(old);
    }
}

/// Folder, next to the show file, that input records are written to.
pub const LOGS_FOLDER: &str = "logs";

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
        record,
    } = persistence;
    let mut show = show;
    let tap = RecordTap::new();
    let record_file = start_record(&mut show, path.as_deref(), record, &tap);
    let (requests, receiver) = mpsc::channel(256);
    let (updates, _) = broadcast::channel(1024);
    let (finished_tx, finished_rx) = mpsc::unbounded_channel();
    let (notices_tx, notices_rx) = mpsc::unbounded_channel();
    let (midi_tx, midi_rx) = mpsc::unbounded_channel();
    let (timecode_tx, timecode_rx) = mpsc::unbounded_channel();
    let (sysex_tx, sysex_rx) = mpsc::unbounded_channel();
    let (loudness_tx, loudness_rx) = mpsc::unbounded_channel();
    let measurer = spawn_measurer(loudness_tx);
    let outputs = make_outputs(Completion {
        finished: finished_tx,
        notices: notices_tx,
        midi: midi_tx,
        timecode: timecode_tx,
        sysex: sysex_tx,
    });
    let identity = Arc::new(RwLock::new(Identity::new(
        show.workspace(),
        path.as_deref(),
    )));
    let handle = ShowHandle {
        requests,
        updates: updates.clone(),
        identity: Arc::clone(&identity),
        clients: Arc::default(),
        tap: Arc::clone(&tap),
    };
    let show_revision = show.revision();
    let actor = Actor {
        show,
        start: Instant::now(),
        path,
        autosave,
        backups,
        record,
        record_file,
        tap: Arc::clone(&tap),
        catching_up: false,
        out_of_step: 0,
        born: Instant::now(),
        // Recovered changes are already in the autosave they came from.
        autosaved: show_revision,
        autosave_due: None,
        identity,
        outputs,
        updates,
        last_revision: None,
        last_status: None,
        last_audio: None,
        dmx_recheck: None,
        meters_live: false,
        last_timecode: None,
        last_wall_clock: None,
        unreadable: HashSet::new(),
        measurer,
        measuring: HashSet::new(),
        unmeasurable: HashSet::new(),
    };
    let task = tokio::spawn(actor.run(
        receiver,
        Inputs {
            loudness: loudness_rx,
            finished: finished_rx,
            notices: notices_rx,
            midi: midi_rx,
            timecode: timecode_rx,
            sysex: sysex_rx,
        },
    ));
    (handle, task)
}

struct Actor {
    show: Show,
    start: Instant,
    path: Option<PathBuf>,
    autosave: Option<Duration>,
    backups: usize,
    /// Whether inputs are recorded.
    record: bool,
    /// Where they are recorded.
    record_file: Option<PathBuf>,
    /// The record as it is written, for backups following this server.
    tap: Arc<RecordTap>,
    /// Following, before the first tick: past inputs are applied without
    /// sounding, and time comes from the record alone.
    catching_up: bool,
    /// Ticks in a row whose state did not match the main server's.
    out_of_step: u32,
    /// When the server started.
    born: Instant,
    /// The revision last written to the autosave file.
    autosaved: u64,
    /// When the changes made since then are written.
    autosave_due: Option<Instant>,
    /// Shared with every [`ShowHandle`].
    identity: Arc<RwLock<Identity>>,
    outputs: Vec<Box<dyn Output>>,
    updates: broadcast::Sender<Update>,
    last_revision: Option<u64>,
    last_status: Option<(Instant, Status)>,
    last_audio: Option<AudioStatus>,
    /// When to look at the lights again after cues fired.
    dmx_recheck: Option<Instant>,
    /// Whether the last meters sent showed any signal.
    meters_live: bool,
    /// The last timecode that came in, as a frame count, and its rate.
    last_timecode: Option<(u64, Rate)>,
    /// The local time when wall clock triggers were last looked at.
    last_wall_clock: Option<chrono::NaiveDateTime>,
    /// Media files that could not be probed, so they are not tried again on
    /// every edit.
    unreadable: HashSet<(CueId, PathBuf)>,
    /// Files waiting for their loudness to be measured, one at a time on a
    /// thread of their own; `None` when that thread could not start.
    measurer: Option<std::sync::mpsc::Sender<(CueId, PathBuf, PathBuf)>>,
    measuring: HashSet<(CueId, PathBuf)>,
    /// Files without a loudness (silent, too short, unreadable).
    unmeasurable: HashSet<(CueId, PathBuf)>,
}

/// Starts the thread that measures the loudness of audio files, one at a
/// time, away from the show: `(cue, file as the cue names it, absolute
/// path)` in, `(cue, file, LUFS)` out.
fn spawn_measurer(
    results: mpsc::UnboundedSender<(CueId, PathBuf, Option<f64>)>,
) -> Option<std::sync::mpsc::Sender<(CueId, PathBuf, PathBuf)>> {
    let (jobs, queue) = std::sync::mpsc::channel::<(CueId, PathBuf, PathBuf)>();
    std::thread::Builder::new()
        .name("mazzalab-loudness".into())
        .spawn(move || {
            for (cue, file, path) in queue {
                let lufs = mazzalab_audio::loudness(&path)
                    .inspect_err(|error| tracing::debug!(%error, "loudness not measured"))
                    .ok()
                    .flatten();
                if results.send((cue, file, lufs)).is_err() {
                    break;
                }
            }
        })
        .map_err(|error| tracing::warn!(%error, "loudness is not measured"))
        .ok()?;
    Some(jobs)
}

impl Actor {
    async fn run(mut self, mut requests: mpsc::Receiver<Request>, inputs: Inputs) {
        let Inputs {
            mut loudness,
            mut finished,
            mut notices,
            mut midi,
            mut timecode,
            mut sysex,
        } = inputs;
        let base = self.media_base();
        for output in &mut self.outputs {
            output.rebase(&base);
        }
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
                    // Following, the main server says when media ends.
                    if !self.following() {
                        // A backend may report a cue that was stopped meanwhile.
                        let _ = self.show.action_finished(cue);
                    }
                }
                Some(notice) = notices.recv() => {
                    let _ = self.updates.send(Update::Notice(Arc::new(notice)));
                }
                Some(event) = midi.recv() => {
                    self.advance();
                    self.midi_in(event);
                }
                Some((value, rate)) = timecode.recv() => {
                    self.advance();
                    self.timecode_in(value, rate);
                }
                Some(bytes) = sysex.recv() => {
                    self.advance();
                    self.sysex_in(&bytes);
                }
                Some((cue, file, lufs)) = loudness.recv() => {
                    self.advance();
                    self.measuring.remove(&(cue, file.clone()));
                    match lufs {
                        // Only if the cue still plays that file.
                        Some(lufs) if self.file_of(cue).as_ref() == Some(&file) => {
                            self.show.set_media_loudness(cue, lufs);
                        }
                        Some(_) => {}
                        None => {
                            self.unmeasurable.insert((cue, file));
                        }
                    }
                }
                () = tokio::time::sleep_until(wake) => {}
            }
        }
    }

    fn advance(&mut self) {
        // Catching up, time comes from the record alone.
        if !self.catching_up {
            self.show.advance_to(self.start.elapsed());
        }
    }

    fn following(&self) -> bool {
        self.identity
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .following
    }

    /// Following: the main server's record starts. The show it describes
    /// replaces this one; until the first tick its past inputs are applied
    /// in silence.
    fn follow_start(&mut self, header: mazzalab_show::Header) {
        let name = header.workspace.name.clone();
        self.open(header.workspace, None, header.recovered);
        self.identity_mut().following = true;
        // A backup records nothing of its own while it follows.
        self.show.stop_recording();
        self.record_file = None;
        self.catching_up = true;
        self.out_of_step = 0;
        tracing::info!(show = %name, "following the main server");
    }

    /// Following: the main server is at `tick`. This server's clock is set
    /// to the same time, and the states compared.
    fn follow_tick(&mut self, tick: Tick) {
        if !self.following() {
            return;
        }
        let at = Duration::from_nanos(tick.tick);
        if self.catching_up {
            self.show.advance_to(at);
            // What happened before now is history: nothing of it sounds.
            self.show.take_events();
            self.catching_up = false;
        }
        // Engine time only moves forward, so a late tick just holds it.
        self.start = Instant::now().checked_sub(at).unwrap_or(self.start);
        self.show.advance_to(at);
        if state_hash(&self.show.status()) == tick.hash {
            self.out_of_step = 0;
        } else {
            self.out_of_step += 1;
            if self.out_of_step == OUT_OF_STEP_TICKS {
                let _ = self.updates.send(Update::Notice(Arc::new(Notice {
                    level: NoticeLevel::Warning,
                    message: "This backup is out of step with the main server.".to_owned(),
                })));
            }
        }
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
            Request::OscTrigger { address, reply } => {
                let started = if self.following() {
                    0
                } else {
                    self.show.osc_trigger(&address)
                };
                let _ = reply.send(started);
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
            Request::MidiIn { event, reply } => {
                self.midi_in(event);
                let _ = reply.send(());
            }
            Request::TimecodeIn {
                timecode,
                rate,
                reply,
            } => {
                self.timecode_in(timecode, rate);
                let _ = reply.send(());
            }
            Request::Stage { reply } => {
                let stage = self
                    .outputs
                    .iter_mut()
                    .find_map(|output| output.stage(true));
                let _ = reply.send(stage);
            }
            Request::MediaEnded { cue, reply } => {
                // Several stages may say so, and the engine may know already.
                if !self.following() {
                    let _ = self.show.action_finished(cue);
                }
                let _ = reply.send(());
            }
            Request::Audition { cue, reply } => {
                let known = cue.is_none_or(|cue| self.show.workspace().find_cue(cue).is_some());
                if known {
                    for output in &mut self.outputs {
                        output.audition(&self.show, cue);
                    }
                    self.send_audio_status();
                }
                let _ = reply.send(if known {
                    Ok(())
                } else {
                    Err("unknown cue".to_owned())
                });
            }
            Request::Dmx { reply } => {
                let levels = self
                    .outputs
                    .iter_mut()
                    .find_map(|output| output.dmx_levels(true));
                let _ = reply.send(levels);
            }
            Request::Open {
                workspace,
                path,
                recovered,
                reply,
            } => {
                self.open(*workspace, path, recovered);
                let _ = reply.send(());
            }
            Request::SaveAs { path, reply } => {
                let _ = reply.send(self.save_as(path));
            }
            Request::FollowStart { header, reply } => {
                self.follow_start(*header);
                let _ = reply.send(());
            }
            Request::FollowEntry { entry, reply } => {
                if self.following() {
                    self.show.advance_to(entry.time());
                    self.show.apply(entry.input);
                }
                let _ = reply.send(());
            }
            Request::FollowTick { tick, reply } => {
                self.follow_tick(tick);
                let _ = reply.send(());
            }
            Request::TakeOver { reply } => {
                if self.following() {
                    self.identity_mut().following = false;
                    self.catching_up = false;
                    tracing::warn!("the main server went silent; taking over");
                    // From here on this machine records what it does.
                    let path = self.path.clone();
                    self.record_file =
                        start_record(&mut self.show, path.as_deref(), self.record, &self.tap);
                }
                let _ = reply.send(());
            }
            Request::Diagnostics { reply } => {
                let load = self
                    .outputs
                    .iter_mut()
                    .find_map(|output| output.audio_load());
                let _ = reply.send(Diagnostics {
                    uptime: self.born.elapsed().as_secs_f64(),
                    clients: 0,
                    audio_load: load.map(|load| load.load),
                    audio_peak_load: load.map(|load| load.peak),
                    audio_late: load.map(|load| load.late),
                    audio_callbacks: load.map(|load| load.callbacks),
                    running: self.show.status().running.len(),
                    record: self
                        .record_file
                        .as_ref()
                        .map(|file| file.display().to_string()),
                    record_error: self.show.record_error().map(str::to_owned),
                });
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

    /// Acts on a MIDI message from an input: the transport controls and the
    /// cues that wait for it. Clients hear about it too, so they can learn
    /// a trigger from the controller itself.
    fn midi_in(&mut self, event: MidiEvent) {
        let _ = self.updates.send(Update::MidiIn(event));
        if self.following() {
            return;
        }
        let workspace = self.show.workspace();
        let controls = &workspace.settings.midi_controls;
        let mut commands: Vec<Command> = [
            (controls.go, Command::Go { list: None }),
            (controls.stop_all, Command::StopAll),
            (controls.panic, Command::Panic),
            (controls.playhead_next, Command::PlayheadNext { list: None }),
            (
                controls.playhead_previous,
                Command::PlayheadPrevious { list: None },
            ),
        ]
        .into_iter()
        .filter(|(trigger, _)| {
            trigger.is_some_and(|trigger| mazzalab_midi::matches(&trigger, &event))
        })
        .map(|(_, command)| command)
        .collect();
        commands.extend(
            workspace
                .cues()
                .filter(|cue| {
                    cue.triggers
                        .midi
                        .is_some_and(|trigger| mazzalab_midi::matches(&trigger, &event))
                })
                .map(|cue| Command::Start { cue: cue.id }),
        );
        for command in commands {
            if let Err(error) = self.show.execute(command) {
                tracing::warn!(%error, "a MIDI trigger could not be carried out");
            }
        }
    }

    /// Acts on a SysEx message from a MIDI input: MIDI Show Control for
    /// the show's device ID (or for every device) runs the show.
    fn sysex_in(&mut self, bytes: &[u8]) {
        if self.following() {
            return;
        }
        let Some(message) = mazzalab_midi::parse_msc(bytes) else {
            return;
        };
        let workspace = self.show.workspace();
        let device = workspace.settings.msc_device_id.unwrap_or(ALL_MSC_DEVICES);
        let Some(action) = mazzalab_midi::action_for(&message, device, workspace) else {
            return;
        };
        use mazzalab_midi::MscAction;
        let command = match action {
            MscAction::Go => Command::Go { list: None },
            MscAction::Start(cue) => Command::Start { cue },
            MscAction::Pause(cue) => Command::Pause { cue },
            MscAction::PauseAll => Command::PauseAll,
            MscAction::Resume(cue) => Command::Resume { cue },
            MscAction::ResumeAll => Command::ResumeAll,
            MscAction::StopAll => Command::StopAll,
            MscAction::Stop(cue) => Command::Stop { cue },
            MscAction::Reset => Command::Reset,
            MscAction::SetPlayhead(cue) => Command::SetPlayhead {
                list: None,
                cue: Some(cue),
            },
            MscAction::PlayheadNext => Command::PlayheadNext { list: None },
            MscAction::PlayheadPrevious => Command::PlayheadPrevious { list: None },
        };
        if let Err(error) = self.show.execute(command) {
            tracing::warn!(%error, "an MSC command could not be carried out");
        }
    }

    /// Follows incoming timecode: clients see it, and while it runs forward
    /// the cues whose trigger it passes start. A jump (a locate, a loop of
    /// the source) starts nothing.
    fn timecode_in(&mut self, timecode: Timecode, rate: Rate) {
        let _ = self.updates.send(Update::Timecode(Arc::new(TimecodeIn {
            timecode: timecode.to_string(),
            format: format_of(rate),
        })));
        let now = rate.to_frames(timecode);
        let Some((before, before_rate)) = self.last_timecode.replace((now, rate)) else {
            return;
        };
        if self.following() {
            return;
        }
        let forward =
            before_rate == rate && now > before && now - before <= rate.fps().ceil() as u64;
        if !forward {
            return;
        }
        let starts: Vec<Command> = self
            .show
            .workspace()
            .cues()
            .filter(|cue| {
                cue.triggers
                    .timecode
                    .as_deref()
                    .and_then(|text| Timecode::parse(text).ok())
                    .is_some_and(|trigger| (before + 1..=now).contains(&rate.to_frames(trigger)))
            })
            .map(|cue| Command::Start { cue: cue.id })
            .collect();
        for command in starts {
            if let Err(error) = self.show.execute(command) {
                tracing::warn!(%error, "a timecode trigger could not be carried out");
            }
        }
    }

    /// Starts the cues whose time of day the clock has just gone past.
    fn wall_clock(&mut self) {
        let now = chrono::Local::now().naive_local();
        let Some(before) = self.last_wall_clock.replace(now) else {
            return;
        };
        if self.following() {
            return;
        }
        let starts: Vec<Command> = self
            .show
            .workspace()
            .cues()
            .filter(|cue| {
                cue.triggers
                    .wall_clock
                    .as_ref()
                    .is_some_and(|trigger| crate::wall_clock::passed(trigger, before, now))
            })
            .map(|cue| Command::Start { cue: cue.id })
            .collect();
        for command in starts {
            if let Err(error) = self.show.execute(command) {
                tracing::warn!(%error, "a wall clock trigger could not be carried out");
            }
        }
    }

    /// Puts another show in place of the running one. The old one is
    /// reset first and the outputs hear about it, so sound, light and
    /// pictures stop before the new show is there.
    fn open(&mut self, workspace: Workspace, path: Option<PathBuf>, recovered: bool) {
        if let Some(audition) = self.last_audio.as_ref().and_then(|audio| audio.audition) {
            tracing::debug!(%audition, "audition stopped by opening another show");
            for output in &mut self.outputs {
                output.audition(&self.show, None);
            }
        }
        let _ = self.show.execute(Command::Reset);
        let events = self.show.take_events();
        if !events.is_empty() {
            for output in &mut self.outputs {
                output.handle(&self.show, &events);
            }
            let dtos: Vec<EventDto> = events.iter().map(EventDto::from).collect();
            let _ = self.updates.send(Update::Events(Arc::new(dtos)));
        }

        let name = workspace.name.clone();
        let (trust_local, following) = {
            let identity = self.identity_mut();
            (identity.trust_local, identity.following)
        };
        *self.identity_mut() = Identity {
            trust_local,
            following,
            ..Identity::new(&workspace, path.as_deref())
        };
        self.show = if recovered {
            Show::recovered(workspace)
        } else {
            Show::new(workspace)
        };
        self.record_file = start_record(&mut self.show, path.as_deref(), self.record, &self.tap);
        // The new show's clock starts now; nothing of the old one carries on.
        self.start = Instant::now();
        self.path = path;
        self.autosaved = self.show.revision();
        self.autosave_due = None;
        self.last_revision = None;
        self.last_status = None;
        self.last_timecode = None;
        self.unreadable.clear();
        self.unmeasurable.clear();
        let base = self.media_base();
        for output in &mut self.outputs {
            output.rebase(&base);
        }
        tracing::info!(show = %name, path = ?self.path, "show opened");
        let _ = self.updates.send(Update::Notice(Arc::new(Notice {
            level: NoticeLevel::Info,
            message: format!("Opened “{name}”"),
        })));
    }

    /// Writes the show to `path`, which becomes its file, without stopping
    /// anything.
    fn save_as(&mut self, path: PathBuf) -> Result<(), String> {
        let to = folder(&path).ok_or_else(|| format!("no folder for {}", path.display()))?;
        let from = absolute(&self.media_base()).unwrap_or_else(|| self.media_base());
        let mut workspace = self.show.workspace().clone();
        mazzalab_project::rebase_media(&mut workspace, &from, &to);
        mazzalab_project::save_with_backups(&path, &workspace, self.backups)
            .map_err(|error| error.to_string())?;
        self.show
            .rewrite_all(|workspace| mazzalab_project::rebase_media(workspace, &from, &to));
        self.show.mark_saved();
        if let Some(old) = &self.path {
            // The old file keeps its saved state; its unsaved changes moved.
            mazzalab_project::discard_autosave(old);
        }
        mazzalab_project::discard_autosave(&path);
        let (trust_local, following) = {
            let identity = self.identity_mut();
            (identity.trust_local, identity.following)
        };
        *self.identity_mut() = Identity {
            trust_local,
            following,
            ..Identity::new(self.show.workspace(), Some(&path))
        };
        self.path = Some(path);
        self.autosaved = self.show.revision();
        self.autosave_due = None;
        for output in &mut self.outputs {
            output.rebase(&to);
        }
        Ok(())
    }

    fn identity_mut(&self) -> std::sync::RwLockWriteGuard<'_, Identity> {
        self.identity
            .write()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn workspace_id(&self) -> Arc<str> {
        Arc::clone(
            &self
                .identity
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .workspace_id,
        )
    }

    /// Where relative media paths start from.
    fn media_base(&self) -> PathBuf {
        self.identity
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .base_dir
            .as_deref()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    }

    fn audio_status(&self) -> Option<AudioStatus> {
        self.outputs.iter().find_map(|output| output.audio_status())
    }

    /// The file an audio cue plays, as the cue names it.
    fn file_of(&self, cue: CueId) -> Option<PathBuf> {
        match &self.show.workspace().find_cue(cue)?.kind {
            CueKind::Audio(audio) => audio.file.clone(),
            _ => None,
        }
    }

    /// Queues the audio files whose loudness is not known yet.
    fn measure_media(&mut self) {
        let Some(measurer) = &self.measurer else {
            return;
        };
        let base = self.media_base();
        let pending: Vec<(CueId, PathBuf)> = self
            .show
            .workspace()
            .cues()
            .filter_map(|cue| match &cue.kind {
                CueKind::Audio(AudioCue {
                    file: Some(file),
                    media_lufs: None,
                    ..
                }) => Some((cue.id, file.clone())),
                _ => None,
            })
            .filter(|entry| {
                !self.measuring.contains(entry)
                    && !self.unmeasurable.contains(entry)
                    && !self.unreadable.contains(entry)
            })
            .collect();
        for (cue, file) in pending {
            if measurer.send((cue, file.clone(), base.join(&file))).is_ok() {
                self.measuring.insert((cue, file));
            }
        }
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
        self.wall_clock();
        self.advance();
        self.autosave();
        let events = self.show.take_events();
        // Catching up, past inputs make no sound.
        let events = if self.catching_up { Vec::new() } else { events };
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

        // Following, the main server says what it found out about media.
        if self.last_revision != Some(self.show.revision()) && !self.following() {
            self.probe_media();
            self.measure_media();
        }
        let revision = self.show.revision();
        let structure_changed = self.last_revision != Some(revision);
        if structure_changed {
            self.last_revision = Some(revision);
            let passcodes = &self.show.workspace().settings.passcodes;
            if *self.identity_mut().passcodes != passcodes[..] {
                self.identity_mut().passcodes = passcodes.clone().into();
            }
            for output in &mut self.outputs {
                output.sync(&self.show);
            }
            self.send_audio_status();
            let _ = self
                .updates
                .send(Update::Snapshot(Arc::new(self.show.snapshot())));
            let address = format!("/update/workspace/{}", self.workspace_id());
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
                    || status.loaded != previous.loaded
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
            // An audition ends by itself: this is where clients find out.
            self.send_audio_status();
        }
        if due || recheck {
            self.send_dmx();
        }
        self.send_stage();
        // The engine knows nothing of an audition: its end is found by
        // looking, each time the actor wakes for it.
        if self
            .last_audio
            .as_ref()
            .is_some_and(|audio| audio.audition.is_some())
        {
            self.send_audio_status();
        }
    }

    /// Broadcasts the state of the audio output when it has changed.
    fn send_audio_status(&mut self) {
        let audio = self.audio_status();
        if audio != self.last_audio {
            if let Some(audio) = &audio {
                let _ = self.updates.send(Update::Audio(Arc::new(audio.clone())));
            }
            self.last_audio = audio;
        }
    }

    /// Broadcasts what the stages show when it has changed.
    fn send_stage(&mut self) {
        if let Some(stage) = self
            .outputs
            .iter_mut()
            .find_map(|output| output.stage(false))
        {
            let _ = self.updates.send(Update::Stage(Arc::new(stage)));
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
        if self
            .last_audio
            .as_ref()
            .is_some_and(|audio| audio.audition.is_some())
        {
            wake = wake.min(Instant::now() + AUDITION_TICK);
        }
        wake
    }

    fn cue_updates(&self, events: &[Event]) -> Vec<OscMessage> {
        let workspace = self.workspace_id();
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
                | EventKind::Loaded { cue, .. }
                | EventKind::Unloaded(cue)
                | EventKind::Devamped(cue)
                | EventKind::Panicking { cue, .. }
                | EventKind::FadingOut { cue, .. }
                | EventKind::Ducked { cue, .. }
                | EventKind::Unducked { cue, .. } => cue,
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

/// Ticks in a row out of step before the operators are told: about a
/// second, longer than inputs take to cross the network.
const OUT_OF_STEP_TICKS: u32 = 4;

/// The MSC device ID meaning "every device", which a show without its own
/// ID answers to.
const ALL_MSC_DEVICES: u8 = 0x7F;

/// What the backends hand the actor, besides requests.
struct Inputs {
    loudness: mpsc::UnboundedReceiver<(CueId, PathBuf, Option<f64>)>,
    finished: mpsc::UnboundedReceiver<CueId>,
    notices: mpsc::UnboundedReceiver<Notice>,
    midi: mpsc::UnboundedReceiver<MidiEvent>,
    timecode: mpsc::UnboundedReceiver<(Timecode, Rate)>,
    sysex: mpsc::UnboundedReceiver<Vec<u8>>,
}

/// How often the audio state is looked at while a cue is auditioned, which
/// the engine knows nothing about.
const AUDITION_TICK: Duration = Duration::from_millis(250);

fn message(addr: String, argument: Option<String>) -> OscMessage {
    OscMessage {
        addr,
        args: argument.map(OscType::String).into_iter().collect(),
    }
}
