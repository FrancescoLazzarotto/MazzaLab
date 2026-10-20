use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::id::CueId;
use crate::time::{option_seconds, seconds};

/// What happens after a cue has been started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinueMode {
    /// The sequence stops here; the operator has to GO again.
    #[default]
    DoNotContinue,
    /// The next cue starts once the post-wait has elapsed. The post-wait is
    /// counted from the moment this cue's action starts.
    AutoContinue,
    /// The next cue starts as soon as this cue's action completes.
    AutoFollow,
}

/// Colour label shown in cue lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueColor {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Grey,
}

/// How long a cue's action runs once started, as far as the model can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionLength {
    /// The action happens instantly (send a message, move the playhead...).
    Instant,
    /// The action runs for a known amount of time.
    Fixed(Duration),
    /// The action runs until something stops it (live input, held frame...).
    UntilStopped,
    /// The length is only known to the media backend (e.g. an audio file that
    /// has not been probed yet), which reports completion to the engine.
    UntilReported,
    /// A group: the action lasts until every child it started has finished.
    UntilChildrenComplete,
}

fn default_true() -> bool {
    true
}

fn default_play_count() -> u32 {
    1
}

fn default_rate() -> f64 {
    1.0
}

fn default_opacity() -> f64 {
    1.0
}

fn is_one(value: &u32) -> bool {
    *value == 1
}

fn is_unit(value: &f64) -> bool {
    *value == 1.0
}

/// A single cue. Timing fields are shared by every cue type; the
/// type-specific part lives in [`Cue::kind`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    pub id: CueId,
    /// Optional, user-facing cue number ("1", "12.5", "A"). Unique when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<CueColor>,
    #[serde(default, with = "seconds", skip_serializing_if = "Duration::is_zero")]
    pub pre_wait: Duration,
    #[serde(default, with = "seconds", skip_serializing_if = "Duration::is_zero")]
    pub post_wait: Duration,
    #[serde(default)]
    pub continue_mode: ContinueMode,
    /// A disarmed cue keeps its timing but performs no action.
    #[serde(default = "default_true")]
    pub armed: bool,
    /// When disarmed, skip the cue entirely (including its waits).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_if_disarmed: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flagged: bool,
    /// The cue acted upon by fade and control cues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<CueId>,
    #[serde(flatten)]
    pub kind: CueKind,
}

impl Cue {
    /// Creates a cue of the given kind with default timing and a fresh id.
    pub fn new(kind: CueKind) -> Self {
        Self {
            id: CueId::new(),
            number: None,
            name: String::new(),
            notes: String::new(),
            color: None,
            pre_wait: Duration::ZERO,
            post_wait: Duration::ZERO,
            continue_mode: ContinueMode::DoNotContinue,
            armed: true,
            skip_if_disarmed: false,
            flagged: false,
            target: None,
            kind,
        }
    }

    pub fn with_id(mut self, id: CueId) -> Self {
        self.id = id;
        self
    }

    pub fn with_number(mut self, number: impl Into<String>) -> Self {
        self.number = Some(number.into());
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_pre_wait(mut self, pre_wait: Duration) -> Self {
        self.pre_wait = pre_wait;
        self
    }

    pub fn with_post_wait(mut self, post_wait: Duration) -> Self {
        self.post_wait = post_wait;
        self
    }

    pub fn with_continue(mut self, mode: ContinueMode) -> Self {
        self.continue_mode = mode;
        self
    }

    pub fn with_target(mut self, target: CueId) -> Self {
        self.target = Some(target);
        self
    }

    pub fn with_armed(mut self, armed: bool) -> Self {
        self.armed = armed;
        self
    }

    pub fn type_name(&self) -> &'static str {
        self.kind.type_name()
    }

    /// The name shown to operators: the explicit name, else a default
    /// derived from the cue type.
    pub fn display_name(&self) -> String {
        if self.name.is_empty() {
            self.type_name().to_owned()
        } else {
            self.name.clone()
        }
    }

    /// Child cues, empty for anything that is not a group.
    pub fn children(&self) -> &[Cue] {
        match &self.kind {
            CueKind::Group(group) => &group.children,
            _ => &[],
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<Cue>> {
        match &mut self.kind {
            CueKind::Group(group) => Some(&mut group.children),
            _ => None,
        }
    }
}

/// The type-specific part of a cue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CueKind {
    // Media
    Audio(AudioCue),
    Video(VideoCue),
    Text(TextCue),
    Mic(MicCue),
    Camera(CameraCue),
    // Control of external systems
    Light(LightCue),
    Fade(FadeCue),
    Network(NetworkCue),
    Midi(MidiCue),
    MidiFile(MidiFileCue),
    Timecode(TimecodeCue),
    // Structure
    Group(GroupCue),
    // Control of other cues (all act on `Cue::target`)
    Start,
    Stop,
    Pause,
    Load(LoadCue),
    Reset,
    Devamp,
    Goto,
    Arm,
    Disarm,
    Target(TargetCue),
    // Utility
    Wait(WaitCue),
    Memo,
    Script(ScriptCue),
}

impl CueKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Audio(_) => "Audio",
            Self::Video(_) => "Video",
            Self::Text(_) => "Text",
            Self::Mic(_) => "Mic",
            Self::Camera(_) => "Camera",
            Self::Light(_) => "Light",
            Self::Fade(_) => "Fade",
            Self::Network(_) => "Network",
            Self::Midi(_) => "MIDI",
            Self::MidiFile(_) => "MIDI File",
            Self::Timecode(_) => "Timecode",
            Self::Group(_) => "Group",
            Self::Start => "Start",
            Self::Stop => "Stop",
            Self::Pause => "Pause",
            Self::Load(_) => "Load",
            Self::Reset => "Reset",
            Self::Devamp => "Devamp",
            Self::Goto => "Go To",
            Self::Arm => "Arm",
            Self::Disarm => "Disarm",
            Self::Target(_) => "Target",
            Self::Wait(_) => "Wait",
            Self::Memo => "Memo",
            Self::Script(_) => "Script",
        }
    }

    /// Whether this kind of cue is meaningless without a `target`.
    pub fn requires_target(&self) -> bool {
        matches!(
            self,
            Self::Fade(_)
                | Self::Start
                | Self::Stop
                | Self::Pause
                | Self::Load(_)
                | Self::Reset
                | Self::Devamp
                | Self::Goto
                | Self::Arm
                | Self::Disarm
                | Self::Target(_)
        )
    }

    pub fn action_length(&self) -> ActionLength {
        match self {
            Self::Audio(cue) => media_length(
                cue.media_duration,
                cue.start_time,
                cue.end_time,
                cue.play_count,
                cue.rate,
                cue.infinite_loop,
            ),
            Self::Video(cue) if cue.hold_at_end => ActionLength::UntilStopped,
            Self::Video(cue) => media_length(
                cue.media_duration,
                cue.start_time,
                cue.end_time,
                cue.play_count,
                cue.rate,
                cue.infinite_loop,
            ),
            Self::MidiFile(cue) => {
                media_length(cue.media_duration, Duration::ZERO, None, 1, cue.rate, false)
            }
            Self::Text(_) | Self::Mic(_) | Self::Camera(_) => ActionLength::UntilStopped,
            Self::Timecode(cue) => cue
                .duration
                .map_or(ActionLength::UntilStopped, fixed_or_instant),
            Self::Light(LightCue { duration, .. })
            | Self::Fade(FadeCue { duration, .. })
            | Self::Network(NetworkCue { duration, .. })
            | Self::Wait(WaitCue { duration }) => fixed_or_instant(*duration),
            Self::Group(_) => ActionLength::UntilChildrenComplete,
            Self::Midi(_)
            | Self::Start
            | Self::Stop
            | Self::Pause
            | Self::Load(_)
            | Self::Reset
            | Self::Devamp
            | Self::Goto
            | Self::Arm
            | Self::Disarm
            | Self::Target(_)
            | Self::Memo
            | Self::Script(_) => ActionLength::Instant,
        }
    }
}

fn fixed_or_instant(duration: Duration) -> ActionLength {
    if duration.is_zero() {
        ActionLength::Instant
    } else {
        ActionLength::Fixed(duration)
    }
}

fn media_length(
    media_duration: Option<Duration>,
    start: Duration,
    end: Option<Duration>,
    play_count: u32,
    rate: f64,
    infinite_loop: bool,
) -> ActionLength {
    if infinite_loop {
        return ActionLength::UntilStopped;
    }
    let Some(end) = end.or(media_duration) else {
        return ActionLength::UntilReported;
    };
    if !(rate.is_finite() && rate > 0.0) {
        return ActionLength::UntilReported;
    }
    let one_pass = end.saturating_sub(start);
    fixed_or_instant(one_pass.saturating_mul(play_count.max(1)).div_f64(rate))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioCue {
    /// Media file, relative to the project folder when possible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    #[serde(default, with = "seconds", skip_serializing_if = "Duration::is_zero")]
    pub start_time: Duration,
    #[serde(
        default,
        with = "option_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub end_time: Option<Duration>,
    #[serde(default = "default_play_count", skip_serializing_if = "is_one")]
    pub play_count: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub infinite_loop: bool,
    #[serde(default = "default_rate", skip_serializing_if = "is_unit")]
    pub rate: f64,
    /// Main output level in dB.
    #[serde(default)]
    pub level_db: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_patch: Option<String>,
    /// Length of the media file, filled in once the media layer has probed it.
    #[serde(
        default,
        with = "option_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub media_duration: Option<Duration>,
}

impl Default for AudioCue {
    fn default() -> Self {
        Self {
            file: None,
            start_time: Duration::ZERO,
            end_time: None,
            play_count: 1,
            infinite_loop: false,
            rate: 1.0,
            level_db: 0.0,
            output_patch: None,
            media_duration: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoCue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default)]
    pub layer: i32,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    #[serde(default, with = "seconds", skip_serializing_if = "Duration::is_zero")]
    pub start_time: Duration,
    #[serde(
        default,
        with = "option_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub end_time: Option<Duration>,
    #[serde(default = "default_play_count", skip_serializing_if = "is_one")]
    pub play_count: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub infinite_loop: bool,
    #[serde(default = "default_rate", skip_serializing_if = "is_unit")]
    pub rate: f64,
    /// Keep the last frame on screen until the cue is stopped.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hold_at_end: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_output_patch: Option<String>,
    #[serde(
        default,
        with = "option_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub media_duration: Option<Duration>,
}

impl Default for VideoCue {
    fn default() -> Self {
        Self {
            file: None,
            stage: None,
            layer: 0,
            opacity: 1.0,
            start_time: Duration::ZERO,
            end_time: None,
            play_count: 1,
            infinite_loop: false,
            rate: 1.0,
            hold_at_end: false,
            audio_output_patch: None,
            media_duration: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TextCue {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default)]
    pub layer: i32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MicCue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_patch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_patch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CameraCue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_patch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LightCue {
    /// Light commands such as `"front wash = 75"`.
    #[serde(default)]
    pub commands: Vec<String>,
    /// Fade time of the look.
    #[serde(default, with = "seconds")]
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FadeCurve {
    Linear,
    #[default]
    SCurve,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FadeCue {
    #[serde(default, with = "seconds")]
    pub duration: Duration,
    #[serde(default)]
    pub curve: FadeCurve,
    /// Target main level in dB, when the fade affects audio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level_db: Option<f64>,
    /// Target opacity, when the fade affects video.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stop_target_when_done: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NetworkCue {
    /// Network patch (destination device) to send to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    /// Message text, e.g. an OSC address followed by its arguments.
    pub message: String,
    /// Fade time for faded parameters; zero sends once.
    #[serde(default, with = "seconds", skip_serializing_if = "Duration::is_zero")]
    pub duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MidiMessage {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    ProgramChange {
        channel: u8,
        program: u8,
    },
    /// MIDI Show Control.
    Msc {
        device_id: u8,
        command_format: u8,
        command: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        q_number: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        q_list: Option<String>,
    },
    /// Raw System Exclusive payload, without the leading F0 and trailing F7.
    SysEx {
        data: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiCue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    pub message: MidiMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiFileCue {
    pub file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    #[serde(default = "default_rate", skip_serializing_if = "is_unit")]
    pub rate: f64,
    #[serde(
        default,
        with = "option_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub media_duration: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmpteFormat {
    Fps24,
    Fps25,
    #[default]
    Fps30NonDrop,
    Fps30Drop,
    Fps23_976,
    Fps29_97NonDrop,
    Fps29_97Drop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimecodeSignal {
    #[default]
    Mtc,
    Ltc,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TimecodeCue {
    #[serde(default)]
    pub signal: TimecodeSignal,
    #[serde(default)]
    pub format: SmpteFormat,
    /// First timecode value sent, `HH:MM:SS:FF`.
    pub start: String,
    /// How long to generate timecode; `None` runs until stopped.
    #[serde(
        default,
        with = "option_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub duration: Option<Duration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
}

/// How a group starts its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupMode {
    /// Start every child at once; each child's pre-wait is its offset.
    #[default]
    Timeline,
    /// Start the first child, then move the playhead to the second child.
    StartFirstAndEnter,
    /// Start the first child and let its continue chain run inside the group.
    StartFirst,
    /// Start one child chosen at random.
    StartRandom,
    /// Play children one after another.
    Playlist,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GroupCue {
    #[serde(default)]
    pub mode: GroupMode,
    #[serde(default)]
    pub children: Vec<Cue>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LoadCue {
    /// Position the target is loaded to.
    #[serde(default, with = "seconds")]
    pub time: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TargetCue {
    /// Number of the cue that becomes the new target of this cue's target.
    pub assigned_number: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WaitCue {
    #[serde(with = "seconds")]
    pub duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ScriptCue {
    /// Scripting language identifier, e.g. `"lua"`.
    pub language: String,
    pub source: String,
}
