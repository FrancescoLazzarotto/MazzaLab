//! mazzalab-audio
//!
//! Audio cue playback:
//!
//! * [`probe`] reads a file's length and format (used to fill
//!   `AudioCue::media_duration` so the engine knows when cues end);
//! * [`output_devices`] lists the machine's outputs, and a show picks one
//!   with a [`DeviceRequest`];
//! * a reader thread per playing cue decodes ahead into a lock-free ring
//!   buffer; cues at a playhead are decoded before GO ([`AudioOutput::preload`])
//!   so they start without waiting for the disk;
//! * the mixer, running in the device callback, resamples, applies levels,
//!   fades and the routing matrix, and sums every voice without allocating
//!   or locking;
//! * [`AudioOutput`] turns engine events (start, stop, pause, fade, panic)
//!   into mixer commands.

mod decode;
mod device;
mod mixer;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use mazzalab_core::{ActionLength, AudioCue, Cue, CueId, CueKind, FadeCurve, Workspace};
use mazzalab_engine::{Event, EventKind};
use rtrb::{Producer, RingBuffer};
use thiserror::Error;

pub use decode::{MediaInfo, probe};
pub use device::{DeviceInfo, DeviceRequest, OutputDevice, output_devices};
pub use mixer::Meters;

use decode::Playback;
use mixer::{Mixer, MixerCommand, MixerEvent, Routes, Voice, VoiceConfig};

/// Seconds of audio each reader decodes ahead.
const READ_AHEAD_SECONDS: f64 = 1.0;
/// How many cues are kept decoded ahead of GO.
const MAX_PRELOADED: usize = 16;
/// Glide applied when a playing cue's level is edited.
const LIVE_EDIT_SECONDS: f64 = 0.03;
/// Meter readings below this are reported as this.
pub const METER_FLOOR_DB: f32 = -120.0;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("audio device: {0}")]
    Device(String),
    #[error("{path}: {message}")]
    Media { path: PathBuf, message: String },
}

/// Fills `media_duration` and `media_channels` for every audio cue whose
/// file can be probed. Returns the cues whose file could not be read, with
/// the reason.
pub fn probe_workspace(workspace: &mut Workspace, base_dir: &Path) -> Vec<(CueId, String)> {
    let mut problems = Vec::new();
    let audio_cues: Vec<(CueId, PathBuf)> = workspace
        .cues()
        .filter_map(|cue| match &cue.kind {
            CueKind::Audio(AudioCue {
                file: Some(file), ..
            }) => Some((cue.id, base_dir.join(file))),
            _ => None,
        })
        .collect();
    for (id, path) in audio_cues {
        match probe(&path) {
            Ok(info) => {
                if let Some(CueKind::Audio(audio)) =
                    workspace.find_cue_mut(id).map(|cue| &mut cue.kind)
                {
                    audio.media_duration = info.duration.or(audio.media_duration);
                    audio.media_channels = Some(info.channels as u8);
                }
            }
            Err(error) => problems.push((id, error.to_string())),
        }
    }
    problems
}

type FinishedCallback = Arc<dyn Fn(CueId) + Send + Sync>;

/// A cue with a voice in the mix.
#[derive(Debug)]
struct Playing {
    /// Tells this voice apart from an earlier one of the same cue.
    serial: u64,
    /// What the voice was built from, to notice live edits.
    definition: AudioCue,
    channels: usize,
}

/// A voice decoded ahead of GO.
struct Preloaded {
    definition: AudioCue,
    voice: Box<Voice>,
    channels: usize,
}

type PlayingMap = Arc<Mutex<HashMap<CueId, Playing>>>;

/// Plays audio cues on an output device.
pub struct AudioOutput {
    commands: Producer<MixerCommand>,
    info: DeviceInfo,
    meters: Arc<Meters>,
    base_dir: PathBuf,
    on_finished: FinishedCallback,
    playing: PlayingMap,
    preloaded: HashMap<CueId, Preloaded>,
    next_serial: u64,
    _device: device::DeviceThread,
    _housekeeping: JoinHandle<()>,
}

impl std::fmt::Debug for AudioOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioOutput")
            .field("info", &self.info)
            .field("base_dir", &self.base_dir)
            .finish_non_exhaustive()
    }
}

impl AudioOutput {
    /// Opens an output device. `base_dir` resolves relative media paths
    /// (normally the show file's folder); `on_finished` is called when a
    /// cue's audio plays to its natural end.
    pub fn start(
        base_dir: impl Into<PathBuf>,
        request: DeviceRequest,
        on_finished: impl Fn(CueId) + Send + Sync + 'static,
    ) -> Result<Self, AudioError> {
        let (commands, command_rx) = RingBuffer::new(1024);
        let (event_tx, mut events) = RingBuffer::new(1024);
        let (meters_tx, meters_rx) = std::sync::mpsc::channel();
        let (info, device) = device::start(request, move |info| {
            let meters = Arc::new(Meters::new(info.channels));
            let _ = meters_tx.send(meters.clone());
            Mixer::new(command_rx, event_tx, info.channels, meters)
        })?;
        let meters = meters_rx
            .recv()
            .map_err(|_| AudioError::Device("audio thread exited".into()))?;

        let on_finished: FinishedCallback = Arc::new(on_finished);
        let callback = on_finished.clone();
        let playing: PlayingMap = Arc::default();
        let playing_cues = playing.clone();
        // Receives voices leaving the mix: reports natural ends and frees them
        // away from the audio thread.
        let housekeeping = std::thread::Builder::new()
            .name("mazzalab-audio-housekeeping".into())
            .spawn(move || {
                loop {
                    while let Ok(event) = events.pop() {
                        match event {
                            MixerEvent::Done { voice, natural } => {
                                let mut playing = lock(&playing_cues);
                                if playing
                                    .get(&voice.cue)
                                    .is_some_and(|entry| entry.serial == voice.serial)
                                {
                                    playing.remove(&voice.cue);
                                }
                                drop(playing);
                                if natural {
                                    callback(voice.cue);
                                }
                                drop(voice);
                            }
                            MixerEvent::Garbage(garbage) => drop(garbage),
                        }
                    }
                    if events.is_abandoned() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            })
            .map_err(|error| AudioError::Device(error.to_string()))?;

        tracing::info!(
            device = %info.name,
            rate = info.sample_rate,
            channels = info.channels,
            buffer = ?info.buffer_size,
            "audio output started"
        );
        Ok(Self {
            commands,
            info,
            meters,
            base_dir: base_dir.into(),
            on_finished,
            playing,
            preloaded: HashMap::new(),
            next_serial: 0,
            _device: device,
            _housekeeping: housekeeping,
        })
    }

    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    pub fn set_master_db(&mut self, db: f32) {
        self.send(MixerCommand::Master(db));
    }

    /// Peak level in dBFS of every output channel since the previous call.
    pub fn take_peaks_db(&self) -> Vec<f32> {
        self.meters
            .take()
            .into_iter()
            .map(|peak| {
                if peak > 0.0 {
                    (20.0 * peak.log10()).max(METER_FLOOR_DB)
                } else {
                    METER_FLOOR_DB
                }
            })
            .collect()
    }

    /// The cues that currently have a voice in the mix.
    pub fn playing_cues(&self) -> Vec<CueId> {
        lock(&self.playing).keys().copied().collect()
    }

    /// Reacts to a batch of engine events.
    pub fn handle_events(&mut self, workspace: &Workspace, events: &[Event]) {
        for event in events {
            match event.kind {
                EventKind::ActionStarted { cue, armed: true } => {
                    let Some(definition) = workspace.find_cue(cue) else {
                        continue;
                    };
                    match &definition.kind {
                        CueKind::Audio(audio) => self.play(cue, audio),
                        CueKind::Fade(fade) => {
                            if let (Some(target), Some(level)) = (definition.target, fade.level_db)
                            {
                                self.send(MixerCommand::Fade {
                                    cue: target,
                                    to_db: level as f32,
                                    frames: self.frames(fade.duration),
                                    curve: fade.curve,
                                });
                            }
                        }
                        _ => {}
                    }
                }
                EventKind::Stopped(cue) => self.send(MixerCommand::Stop(cue)),
                EventKind::Paused(cue) => self.send(MixerCommand::Pause(cue)),
                EventKind::Resumed(cue) => self.send(MixerCommand::Resume(cue)),
                // A panic fades every sound, whichever cue owns it.
                EventKind::Panicking { fade, .. } => self.send(MixerCommand::FadeAll {
                    frames: self.frames(fade),
                }),
                _ => {}
            }
        }
    }

    /// Decodes ahead the audio of `cues` (and of the cues inside them), so
    /// that starting them does not wait for the disk. Call it with the cues
    /// standing by at the playheads whenever those move or the show is
    /// edited; anything preloaded earlier and no longer listed is let go.
    pub fn preload(&mut self, workspace: &Workspace, cues: &[CueId]) {
        let mut wanted = Vec::new();
        for cue in cues.iter().filter_map(|cue| workspace.find_cue(*cue)) {
            collect_audio(cue, &mut wanted);
        }
        wanted.truncate(MAX_PRELOADED);

        self.preloaded.retain(|id, preloaded| {
            wanted
                .iter()
                .any(|(cue, audio)| cue == id && **audio == preloaded.definition)
        });
        for (cue, audio) in wanted {
            if !self.preloaded.contains_key(&cue)
                && let Ok((voice, channels)) = self.build_voice(cue, audio)
            {
                self.preloaded.insert(
                    cue,
                    Preloaded {
                        definition: audio.clone(),
                        voice,
                        channels,
                    },
                );
            }
        }
    }

    /// Applies edits made to cues while they play: a changed level glides to
    /// its new value and a changed matrix is repatched.
    pub fn sync(&mut self, workspace: &Workspace) {
        let mut commands = Vec::new();
        {
            let mut playing = lock(&self.playing);
            for (cue, entry) in playing.iter_mut() {
                let Some(CueKind::Audio(audio)) = workspace.find_cue(*cue).map(|cue| &cue.kind)
                else {
                    continue;
                };
                if audio.level_db != entry.definition.level_db {
                    commands.push(MixerCommand::Fade {
                        cue: *cue,
                        to_db: audio.level_db as f32,
                        frames: self.frames(Duration::from_secs_f64(LIVE_EDIT_SECONDS)),
                        curve: FadeCurve::Linear,
                    });
                }
                if audio.matrix != entry.definition.matrix {
                    commands.push(MixerCommand::Route {
                        cue: *cue,
                        routes: Box::new(self.routes(audio, entry.channels)),
                    });
                }
                entry.definition = audio.clone();
            }
        }
        for command in commands {
            self.send(command);
        }
    }

    /// Stops every voice immediately (with a de-click fade).
    pub fn stop_all(&mut self) {
        self.send(MixerCommand::StopAll);
    }

    fn frames(&self, duration: Duration) -> u32 {
        (duration.as_secs_f64() * f64::from(self.info.sample_rate)).round() as u32
    }

    fn send(&mut self, command: MixerCommand) {
        if self.commands.push(command).is_err() {
            tracing::warn!("audio command queue full; command dropped");
        }
    }

    fn routes(&self, audio: &AudioCue, channels: usize) -> Routes {
        match &audio.matrix {
            Some(matrix) => Routes::from_matrix(matrix, channels, self.info.channels),
            None => Routes::default_for(channels, self.info.channels),
        }
    }

    /// Opens the cue's file and builds a voice whose reader is already
    /// decoding. Returns the voice and its channel count.
    fn build_voice(&mut self, cue: CueId, audio: &AudioCue) -> Result<(Box<Voice>, usize), ()> {
        let file = audio.file.as_ref().ok_or(())?;
        let path = self.base_dir.join(file);
        let source = decode::open(&path).map_err(|error| {
            tracing::warn!(%error, "cannot play audio cue");
        })?;
        let info = source.info;
        let capacity = (READ_AHEAD_SECONDS * f64::from(info.sample_rate)) as usize * info.channels;
        let (producer, consumer) = RingBuffer::new(capacity.max(1024));
        let ended = Arc::new(AtomicBool::new(false));

        let fade_out_frames = self.frames(audio.fade_out);
        // The fade out needs to know where the end is.
        let fade_out = match CueKind::Audio(audio.clone()).action_length() {
            ActionLength::Fixed(length) if fade_out_frames > 0 => {
                let total =
                    (length.as_secs_f64() * f64::from(self.info.sample_rate)).round() as u64;
                let frames = u64::from(fade_out_frames).min(total) as u32;
                Some((total - u64::from(frames), frames))
            }
            _ => None,
        };
        self.next_serial += 1;
        let voice = Voice::new(
            VoiceConfig {
                cue,
                serial: self.next_serial,
                channels: info.channels,
                source_rate: info.sample_rate,
                output_rate: self.info.sample_rate,
                rate: audio.rate,
                level_db: audio.level_db as f32,
                routes: self.routes(audio, info.channels),
                fade_in_frames: self.frames(audio.fade_in),
                fade_out,
            },
            consumer,
            ended.clone(),
        );
        let playback = Playback {
            start: audio.start_time,
            end: audio.end_time,
            passes: (!audio.infinite_loop).then_some(audio.play_count.max(1)),
        };
        decode::spawn_reader(source, playback, producer, ended).map_err(|error| {
            tracing::warn!(%error, "cannot start audio reader");
        })?;
        Ok((Box::new(voice), info.channels))
    }

    fn play(&mut self, cue: CueId, audio: &AudioCue) {
        let ready = match self.preloaded.remove(&cue) {
            Some(preloaded) if preloaded.definition == *audio => {
                Ok((preloaded.voice, preloaded.channels))
            }
            _ => self.build_voice(cue, audio),
        };
        let Ok((voice, channels)) = ready else {
            // Do not leave an open-ended cue hanging.
            (self.on_finished)(cue);
            return;
        };
        lock(&self.playing).insert(
            cue,
            Playing {
                serial: voice.serial,
                definition: audio.clone(),
                channels,
            },
        );
        self.send(MixerCommand::Add(voice));
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The audio cues with a file in `cue` and everything inside it.
fn collect_audio<'a>(cue: &'a Cue, out: &mut Vec<(CueId, &'a AudioCue)>) {
    if let CueKind::Audio(audio) = &cue.kind
        && audio.file.is_some()
    {
        out.push((cue.id, audio));
    }
    for child in cue.children() {
        collect_audio(child, out);
    }
}
