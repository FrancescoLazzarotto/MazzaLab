//! A record of everything that changes a running show, and its replay.
//!
//! The engine is deterministic: given the same workspace and the same
//! inputs at the same instants it produces the same events. So a show is
//! recorded as its workspace at the start plus every input with the
//! engine time it arrived at, one JSON object per line, and replaying the
//! lines reproduces the performance exactly: for finding out what happened
//! during a show, and for a backup machine that follows the main one.

use std::io::{self, BufRead, Write};
use std::time::Duration;

use mazzalab_core::{CueId, Workspace};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Command, Show};

/// The format of the first line of a record.
pub const RECORD_FORMAT: u32 = 1;

/// The first line of a record: what the show was when recording began.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub format: u32,
    /// When recording began, as the recorder's wall clock had it (text,
    /// for people; replay does not use it).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub started: String,
    /// The workspace held unsaved changes recovered from an autosave.
    #[serde(default)]
    pub recovered: bool,
    pub workspace: Workspace,
}

/// One input, at engine time `at` in nanoseconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub at: u64,
    #[serde(flatten)]
    pub input: Input,
}

impl Entry {
    pub fn time(&self) -> Duration {
        Duration::from_nanos(self.at)
    }
}

/// Something from outside that changed the show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "input", rename_all = "snake_case")]
pub enum Input {
    Command {
        command: Box<Command>,
    },
    Osc {
        request: mazzalab_osc::Request,
    },
    OscTrigger {
        address: String,
    },
    /// A media backend said a cue's action ended.
    ActionFinished {
        cue: CueId,
    },
    /// The media layer read a file's length and channels.
    MediaInfo {
        cue: CueId,
        /// Nanoseconds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channels: Option<u8>,
    },
    /// The media layer measured a file's loudness.
    MediaLoudness {
        cue: CueId,
        lufs: f64,
    },
}

/// Where a show writes its record.
pub(crate) struct Recorder {
    out: Box<dyn Write + Send>,
}

impl std::fmt::Debug for Recorder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Recorder")
    }
}

impl Recorder {
    pub(crate) fn new(mut out: Box<dyn Write + Send>, header: &Header) -> io::Result<Self> {
        serde_json::to_writer(&mut out, header)?;
        out.write_all(b"\n")?;
        out.flush()?;
        Ok(Self { out })
    }

    pub(crate) fn write(&mut self, entry: &Entry) -> io::Result<()> {
        serde_json::to_writer(&mut self.out, entry)?;
        self.out.write_all(b"\n")?;
        // Inputs are few and a crash must not lose the last ones.
        self.out.flush()
    }
}

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("the record is empty")]
    Empty,
    #[error("line {line}: {error}")]
    Line {
        line: usize,
        error: serde_json::Error,
    },
    #[error("record format {0} is not supported")]
    Format(u32),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Replays a record: builds the show it started from and feeds it every
/// input at its instant. `each` sees the show after every input, e.g. to
/// collect its events. Inputs that failed when recorded fail again and are
/// passed over, as they were.
pub fn replay(
    reader: impl BufRead,
    mut each: impl FnMut(&mut Show, &Entry),
) -> Result<Show, ReplayError> {
    let mut lines = reader.lines().enumerate();
    let header: Header = match lines.next() {
        None => return Err(ReplayError::Empty),
        Some((_, line)) => {
            serde_json::from_str(&line?).map_err(|error| ReplayError::Line { line: 1, error })?
        }
    };
    if header.format != RECORD_FORMAT {
        return Err(ReplayError::Format(header.format));
    }
    let mut show = if header.recovered {
        Show::recovered(header.workspace)
    } else {
        Show::new(header.workspace)
    };
    for (index, line) in lines {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: Entry = serde_json::from_str(&line).map_err(|error| ReplayError::Line {
            line: index + 1,
            error,
        })?;
        show.advance_to(entry.time());
        show.apply(entry.input.clone());
        each(&mut show, &entry);
    }
    Ok(show)
}

impl Show {
    /// Writes everything that changes the show from now on to `out`,
    /// starting with the workspace as it is. Call it on a fresh show, before
    /// anything has run: the record replays from there.
    pub fn record_to(&mut self, out: Box<dyn Write + Send>, started: &str) -> io::Result<()> {
        let header = Header {
            format: RECORD_FORMAT,
            started: started.to_owned(),
            recovered: self.recovered,
            workspace: self.workspace().clone(),
        };
        self.recorder = Some(Recorder::new(out, &header)?);
        self.record_error = None;
        Ok(())
    }

    /// Stops recording inputs.
    pub fn stop_recording(&mut self) {
        self.recorder = None;
    }

    /// Why recording stopped, if writing the record failed.
    pub fn record_error(&self) -> Option<&str> {
        self.record_error.as_deref()
    }

    /// Records an input that arrived from outside; what it sets off inside
    /// the show is not recorded again.
    pub(crate) fn log(&mut self, input: impl FnOnce() -> Input) {
        if self.nesting > 0 {
            return;
        }
        let Some(recorder) = &mut self.recorder else {
            return;
        };
        let entry = Entry {
            at: u64::try_from(self.engine.now().as_nanos()).unwrap_or(u64::MAX),
            input: input(),
        };
        if let Err(error) = recorder.write(&entry) {
            self.record_error = Some(error.to_string());
            self.recorder = None;
        }
    }

    /// Runs `body` as part of an input already recorded.
    pub(crate) fn nested<R>(&mut self, body: impl FnOnce(&mut Self) -> R) -> R {
        self.nesting += 1;
        let result = body(self);
        self.nesting -= 1;
        result
    }

    /// Feeds the show a recorded input.
    pub fn apply(&mut self, input: Input) {
        match input {
            Input::Command { command } => {
                let _ = self.execute(*command);
            }
            Input::Osc { request } => {
                let _ = self.handle_osc(&request);
            }
            Input::OscTrigger { address } => {
                self.osc_trigger(&address);
            }
            Input::ActionFinished { cue } => {
                let _ = self.action_finished(cue);
            }
            Input::MediaInfo {
                cue,
                duration,
                channels,
            } => {
                self.set_media_info(cue, duration.map(Duration::from_nanos), channels);
            }
            Input::MediaLoudness { cue, lufs } => {
                self.set_media_loudness(cue, lufs);
            }
        }
    }
}
