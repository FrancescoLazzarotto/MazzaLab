//! mazzalab-audio
//!
//! Audio cue playback:
//!
//! * [`probe`] reads a file's length and format (used to fill
//!   `AudioCue::media_duration` so the engine knows when cues end);
//! * a reader thread per playing cue decodes ahead into a lock-free ring
//!   buffer;
//! * the mixer, running in the device callback, resamples, applies levels
//!   and fades, and sums every voice without allocating or locking;
//! * [`AudioOutput`] turns engine events (start, stop, pause, fade, panic)
//!   into mixer commands.

mod decode;
mod device;
mod mixer;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread::JoinHandle;
use std::time::Duration;

use mazzalab_core::{AudioCue, CueId, CueKind, Workspace};
use mazzalab_engine::{Event, EventKind};
use rtrb::{Producer, RingBuffer};
use thiserror::Error;

pub use decode::{MediaInfo, probe};
pub use device::DeviceInfo;

use decode::Playback;
use mixer::{Mixer, MixerCommand, MixerEvent, Voice};

/// Seconds of audio each reader decodes ahead.
const READ_AHEAD_SECONDS: f64 = 1.0;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("audio device: {0}")]
    Device(String),
    #[error("{path}: {message}")]
    Media { path: PathBuf, message: String },
}

/// Fills `media_duration` for every audio cue whose file can be probed.
/// Returns the cues whose file could not be read, with the reason.
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
                }
            }
            Err(error) => problems.push((id, error.to_string())),
        }
    }
    problems
}

type FinishedCallback = Arc<dyn Fn(CueId) + Send + Sync>;

/// Plays audio cues on the default output device.
pub struct AudioOutput {
    commands: Producer<MixerCommand>,
    info: DeviceInfo,
    base_dir: PathBuf,
    on_finished: FinishedCallback,
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
    /// Opens the default output device. `base_dir` resolves relative media
    /// paths (normally the show file's folder); `on_finished` is called when
    /// a cue's audio plays to its natural end.
    pub fn start(
        base_dir: impl Into<PathBuf>,
        on_finished: impl Fn(CueId) + Send + Sync + 'static,
    ) -> Result<Self, AudioError> {
        let (commands, command_rx) = RingBuffer::new(1024);
        let (event_tx, mut events) = RingBuffer::new(1024);
        let (info, device) =
            device::start(move |info| Mixer::new(command_rx, event_tx, info.channels))?;

        let on_finished: FinishedCallback = Arc::new(on_finished);
        let callback = on_finished.clone();
        // Receives voices leaving the mix: reports natural ends and frees them
        // away from the audio thread.
        let housekeeping = std::thread::Builder::new()
            .name("mazzalab-audio-housekeeping".into())
            .spawn(move || {
                loop {
                    while let Ok(MixerEvent::Done { voice, natural }) = events.pop() {
                        if natural {
                            callback(voice.cue);
                        }
                        drop(voice);
                    }
                    if events.is_abandoned() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            })
            .map_err(|error| AudioError::Device(error.to_string()))?;

        tracing::info!(device = %info.name, rate = info.sample_rate, channels = info.channels, "audio output started");
        Ok(Self {
            commands,
            info,
            base_dir: base_dir.into(),
            on_finished,
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

    fn play(&mut self, cue: CueId, audio: &AudioCue) {
        let Some(file) = &audio.file else {
            (self.on_finished)(cue);
            return;
        };
        let path = self.base_dir.join(file);
        let source = match decode::open(&path) {
            Ok(source) => source,
            Err(error) => {
                tracing::warn!(%error, "cannot play audio cue");
                // Do not leave an open-ended cue hanging.
                (self.on_finished)(cue);
                return;
            }
        };
        let info = source.info;
        let capacity = (READ_AHEAD_SECONDS * f64::from(info.sample_rate)) as usize * info.channels;
        let (producer, consumer) = RingBuffer::new(capacity.max(1024));
        let ended = Arc::new(AtomicBool::new(false));
        let voice = Voice::new(
            cue,
            consumer,
            ended.clone(),
            info.channels,
            info.sample_rate,
            self.info.sample_rate,
            audio.rate,
            audio.level_db as f32,
        );
        let playback = Playback {
            start: audio.start_time,
            end: audio.end_time,
            passes: (!audio.infinite_loop).then_some(audio.play_count.max(1)),
        };
        if let Err(error) = decode::spawn_reader(source, playback, producer, ended) {
            tracing::warn!(%error, "cannot start audio reader");
            (self.on_finished)(cue);
            return;
        }
        self.send(MixerCommand::Add(Box::new(voice)));
    }
}
