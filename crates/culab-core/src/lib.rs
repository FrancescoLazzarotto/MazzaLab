//! culab-core
//!
//! The show data model shared by every other CuLab crate: a [`Workspace`]
//! holds [`CueList`]s, which hold [`Cue`]s. A cue has common timing
//! properties (pre-wait, post-wait, continue mode, arming) plus a
//! type-specific payload described by [`CueKind`].
//!
//! This crate is deliberately free of I/O and playback logic so it can be
//! used unchanged by the engine, the file format, network remotes and UIs.

mod cue;
mod id;
pub mod time;
mod validate;
mod workspace;

pub use cue::{
    ActionLength, AudioCue, CameraCue, ContinueMode, Cue, CueColor, CueKind, FadeCue, FadeCurve,
    GroupCue, GroupMode, LightCue, LoadCue, MicCue, MidiCue, MidiFileCue, MidiMessage, NetworkCue,
    ScriptCue, SmpteFormat, TargetCue, TextCue, TimecodeCue, TimecodeSignal, VideoCue, WaitCue,
};
pub use id::{CueId, WorkspaceId};
pub use validate::{Issue, validate};
pub use workspace::{CueList, Workspace, WorkspaceSettings};
