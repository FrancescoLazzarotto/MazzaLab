//! The backends that turn engine events into sound and network messages.

use std::path::{Path, PathBuf};

use mazzalab_audio::{AudioOutput, DeviceRequest};
use mazzalab_core::{ActionLength, AudioSettings, CueId, CueKind};
use mazzalab_engine::{Event, EventKind};
use mazzalab_show::Show;

use crate::actor::{AudioStatus, Completion, NoticeLevel, Output};

/// Logs every engine event; useful headless and while developing backends.
#[derive(Debug, Default)]
pub struct LogOutput;

impl Output for LogOutput {
    fn handle(&mut self, show: &Show, events: &[Event]) {
        for event in events {
            let at = event.at.as_secs_f64();
            match event.kind {
                EventKind::ActionStarted { cue, armed } => {
                    tracing::info!(at, armed, "action {}", label(show, cue));
                }
                EventKind::ActionCompleted(cue) => tracing::info!(at, "done {}", label(show, cue)),
                EventKind::Stopped(cue) => tracing::info!(at, "stop {}", label(show, cue)),
                EventKind::Panicking { cue, .. } => {
                    tracing::info!(at, "panic {}", label(show, cue));
                }
                _ => tracing::debug!(at, ?event.kind),
            }
        }
    }
}

/// A cue as operators know it: number and name.
fn label(show: &Show, cue: CueId) -> String {
    show.workspace().find_cue(cue).map_or_else(
        || cue.to_string(),
        |cue| {
            format!(
                "{} {}",
                cue.number.as_deref().unwrap_or("-"),
                cue.display_name()
            )
        },
    )
}

/// The cues standing by at the playhead of every list.
fn playheads(show: &Show) -> Vec<CueId> {
    show.workspace()
        .cue_lists
        .iter()
        .filter_map(|list| show.engine().playhead(list.id))
        .collect()
}

/// Plays audio cues on the device chosen in the show's audio settings, and
/// follows those settings while the show runs.
#[derive(Debug)]
pub struct AudioBackend {
    base_dir: PathBuf,
    completion: Completion,
    output: Option<AudioOutput>,
    /// The settings the device was last opened with; `None` before the
    /// first attempt.
    applied: Option<AudioSettings>,
    error: Option<String>,
}

impl AudioBackend {
    /// The device is opened on the first [`Output::sync`], when the show's
    /// settings are known. Finished sounds are reported through `completion`.
    pub fn new(base_dir: &Path, completion: Completion) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
            completion,
            output: None,
            applied: None,
            error: None,
        }
    }

    /// (Re)opens the output device for `settings`. Sounds playing on the
    /// previous device are cut and their cues end.
    fn open(&mut self, settings: &AudioSettings) {
        if let Some(previous) = self.output.take() {
            let cut = previous.playing_cues();
            drop(previous);
            for cue in cut {
                self.completion.action_finished(cue);
            }
        }
        let request = DeviceRequest {
            device: settings.device.clone(),
            sample_rate: settings.sample_rate,
            buffer_size: settings.buffer_size,
        };
        let completion = self.completion.clone();
        match AudioOutput::start(&self.base_dir, request, move |cue| {
            completion.action_finished(cue);
        }) {
            Ok(mut output) => {
                output.set_master_db(settings.master_db as f32);
                if output.info().fallback {
                    self.completion.notify(
                        NoticeLevel::Warning,
                        format!(
                            "The audio device chosen for this show is not connected; playing on {}.",
                            output.info().name
                        ),
                    );
                }
                self.output = Some(output);
                self.error = None;
            }
            Err(error) => {
                tracing::warn!(%error, "no audio output");
                self.completion
                    .notify(NoticeLevel::Error, format!("No audio output: {error}"));
                self.error = Some(error.to_string());
            }
        }
    }
}

impl Output for AudioBackend {
    fn handle(&mut self, show: &Show, events: &[Event]) {
        let workspace = show.workspace();
        let Some(output) = &mut self.output else {
            // Without a device nothing reports the end of sounds whose
            // length is unknown: end them at once rather than leave them
            // running forever.
            for event in events {
                if let EventKind::ActionStarted { cue, armed: true } = event.kind
                    && let Some(CueKind::Audio(audio)) =
                        workspace.find_cue(cue).map(|cue| &cue.kind)
                    && CueKind::Audio(audio.clone()).action_length() == ActionLength::UntilReported
                {
                    self.completion.action_finished(cue);
                }
            }
            return;
        };
        output.handle_events(workspace, events);
        let moved = events.iter().any(|event| {
            matches!(
                event.kind,
                EventKind::PlayheadMoved { .. } | EventKind::ActionStarted { .. }
            )
        });
        if moved {
            output.preload(workspace, &playheads(show));
        }
    }

    fn sync(&mut self, show: &Show) {
        let settings = &show.workspace().settings.audio;
        let reopen = self.applied.as_ref().is_none_or(|applied| {
            applied.device != settings.device
                || applied.sample_rate != settings.sample_rate
                || applied.buffer_size != settings.buffer_size
        });
        if reopen {
            self.open(settings);
        } else if self
            .applied
            .as_ref()
            .is_some_and(|applied| applied.master_db != settings.master_db)
            && let Some(output) = &mut self.output
        {
            output.set_master_db(settings.master_db as f32);
        }
        self.applied = Some(settings.clone());
        if let Some(output) = &mut self.output {
            output.sync(show.workspace());
            output.preload(show.workspace(), &playheads(show));
        }
    }

    fn meters(&mut self) -> Option<Vec<f32>> {
        self.output.as_ref().map(AudioOutput::take_peaks_db)
    }

    fn audio_status(&self) -> Option<AudioStatus> {
        Some(match &self.output {
            Some(output) => {
                let info = output.info();
                AudioStatus {
                    running: true,
                    device_id: Some(info.id.clone()),
                    device_name: Some(info.name.clone()),
                    sample_rate: info.sample_rate,
                    channels: info.channels,
                    buffer_size: info.buffer_size,
                    fallback: info.fallback,
                    error: None,
                }
            }
            None => AudioStatus {
                error: self.error.clone(),
                ..AudioStatus::default()
            },
        })
    }
}

/// Sends the messages of Network cues to the show's network patches.
#[derive(Debug)]
pub struct NetworkBackend {
    sender: Option<mazzalab_network::Sender>,
    completion: Completion,
}

impl NetworkBackend {
    pub fn new(completion: Completion) -> Self {
        let on_error = completion.clone();
        let sender = mazzalab_network::Sender::new(move |patch, error| {
            on_error.notify(
                NoticeLevel::Warning,
                format!(
                    "Network patch \"{}\" ({}:{}): {error}",
                    patch.name, patch.host, patch.port
                ),
            );
        })
        .map_err(|error| tracing::error!(%error, "network cues are disabled"))
        .ok();
        Self { sender, completion }
    }
}

impl Output for NetworkBackend {
    fn handle(&mut self, show: &Show, events: &[Event]) {
        let workspace = show.workspace();
        for event in events {
            let EventKind::ActionStarted { cue, armed: true } = event.kind else {
                continue;
            };
            let Some(CueKind::Network(network)) = workspace.find_cue(cue).map(|cue| &cue.kind)
            else {
                continue;
            };
            let patches = &workspace.settings.network_patches;
            let patch = match network.patch.as_deref().filter(|name| !name.is_empty()) {
                Some(name) => patches.iter().find(|patch| patch.name == name),
                None => patches.first(),
            };
            let problem = match (patch, &self.sender) {
                (Some(patch), Some(sender)) => sender
                    .send(patch, &network.message)
                    .err()
                    .map(|error| error.to_string()),
                (None, _) => Some(match &network.patch {
                    Some(name) if !name.is_empty() => {
                        format!("there is no network patch named \"{name}\"")
                    }
                    _ => "the show has no network patch; add one in the settings".to_owned(),
                }),
                (_, None) => Some("network cues are disabled".to_owned()),
            };
            if let Some(problem) = problem {
                self.completion.notify(
                    NoticeLevel::Warning,
                    format!("Cue {}: {problem}", label(show, cue)),
                );
            }
        }
    }
}

/// Sends the messages of MIDI cues to the show's MIDI patches.
#[derive(Debug)]
pub struct MidiBackend {
    sender: Option<mazzalab_midi::Sender>,
    completion: Completion,
}

impl MidiBackend {
    pub fn new(completion: Completion) -> Self {
        let on_error = completion.clone();
        let sender = mazzalab_midi::Sender::new(move |port, error| {
            on_error.notify(
                NoticeLevel::Warning,
                format!("MIDI output \"{port}\": {error}"),
            );
        })
        .map_err(|error| tracing::error!(%error, "MIDI cues are disabled"))
        .ok();
        Self { sender, completion }
    }
}

impl Output for MidiBackend {
    fn handle(&mut self, show: &Show, events: &[Event]) {
        let workspace = show.workspace();
        for event in events {
            let EventKind::ActionStarted { cue, armed: true } = event.kind else {
                continue;
            };
            let Some(CueKind::Midi(midi)) = workspace.find_cue(cue).map(|cue| &cue.kind) else {
                continue;
            };
            let patches = &workspace.settings.midi_patches;
            let patch = match midi.patch.as_deref().filter(|name| !name.is_empty()) {
                Some(name) => patches.iter().find(|patch| patch.name == name),
                None => patches.first(),
            };
            let problem = match (patch, &self.sender) {
                (Some(patch), Some(sender)) => sender
                    .send(&patch.port, &midi.message)
                    .err()
                    .map(|error| error.to_string()),
                (None, _) => Some(match &midi.patch {
                    Some(name) if !name.is_empty() => {
                        format!("there is no MIDI patch named \"{name}\"")
                    }
                    _ => "the show has no MIDI patch; add one in the settings".to_owned(),
                }),
                (_, None) => Some("MIDI cues are disabled".to_owned()),
            };
            if let Some(problem) = problem {
                self.completion.notify(
                    NoticeLevel::Warning,
                    format!("Cue {}: {problem}", label(show, cue)),
                );
            }
        }
    }
}
