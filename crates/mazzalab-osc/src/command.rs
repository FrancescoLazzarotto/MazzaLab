use rosc::OscType;
use thiserror::Error;

/// An OSC argument reduced to the types the API cares about.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
}

impl Arg {
    pub fn from_osc(value: &OscType) -> Option<Self> {
        Some(match value {
            OscType::Int(v) => Self::Int(i64::from(*v)),
            OscType::Long(v) => Self::Int(*v),
            OscType::Float(v) => Self::Float(f64::from(*v)),
            OscType::Double(v) => Self::Float(*v),
            OscType::String(v) => Self::String(v.clone()),
            OscType::Bool(v) => Self::Bool(*v),
            _ => return None,
        })
    }

    /// Reads an argument embedded in the address (`/preWait/+/1`).
    fn from_segment(segment: &str) -> Self {
        segment
            .parse::<f64>()
            .map_or_else(|_| Self::String(segment.to_owned()), Self::Float)
    }

    /// Lenient boolean reading: numbers are true unless zero; strings are
    /// true when they start with `y`, `t` or a digit 1-9, false when they
    /// start with `n`, `f` or `0`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            Self::Int(v) => Some(*v != 0),
            Self::Float(v) => Some(*v != 0.0),
            Self::String(s) => match s.chars().next()? {
                'y' | 'Y' | 't' | 'T' | '1'..='9' => Some(true),
                'n' | 'N' | 'f' | 'F' | '0' => Some(false),
                _ => None,
            },
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }
}

/// Which cue(s) a `/cue/...` message addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CueSelector {
    /// A cue number; may contain OSC wildcards such as `*`.
    Number(String),
    /// `/cue_id/{id}`.
    Id(String),
    Selected,
    Active,
    Playhead,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PropertyOp {
    Get,
    Set(Arg),
    Add(Arg),
    Subtract(Arg),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CueMethod {
    Start,
    Stop,
    HardStop,
    Pause,
    Resume,
    Load,
    Preview,
    Panic,
    Reset,
    /// Read or write a property, e.g. `preWait`, `name`, `level/0/1`.
    Property {
        name: String,
        op: PropertyOp,
        /// `/live` form: changes the running value only, not the saved show.
        live: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// `/go`, or `/go/{cue_number}` to set the playhead and GO.
    Go {
        cue: Option<String>,
    },
    Stop,
    HardStop,
    Pause,
    Resume,
    Panic,
    Reset,
    PlayheadNext,
    PlayheadPrevious,
    PlayheadNextSequence,
    PlayheadPreviousSequence,
    Playhead(CueSelector),
    SelectNext,
    SelectPrevious,
    Select(CueSelector),
    Cue {
        cue: CueSelector,
        method: CueMethod,
    },
    /// Subscribe to (or stop) push updates.
    Updates(bool),
    AlwaysReply(bool),
    Connect {
        passcode: Option<String>,
    },
    Disconnect,
    Version,
    Workspaces,
    CueLists,
    /// Keep-alive ping.
    Thump,
}

/// A parsed message: an optional workspace address plus the command.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// Id or name from a `/workspace/{id}/...` prefix; `None` addresses
    /// every workspace listening on the port.
    pub workspace: Option<String>,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("OSC address must start with '/': {0:?}")]
    NotAnAddress(String),
    #[error("unsupported OSC address {0}")]
    Unsupported(String),
    #[error("{address} expects a {expected} argument")]
    MissingArgument {
        address: String,
        expected: &'static str,
    },
}

/// Parses an OSC address and its arguments into a [`Request`].
pub fn parse(address: &str, args: &[Arg]) -> Result<Request, ParseError> {
    let Some(path) = address.strip_prefix('/') else {
        return Err(ParseError::NotAnAddress(address.to_owned()));
    };
    let segments: Vec<&str> = path.split('/').collect();
    let (workspace, rest) = match segments.as_slice() {
        ["workspace", id, rest @ ..] if !rest.is_empty() => (Some((*id).to_owned()), rest),
        all => (None, all),
    };
    let unsupported = || ParseError::Unsupported(address.to_owned());
    let bool_arg = || {
        args.first()
            .and_then(Arg::as_bool)
            .ok_or_else(|| ParseError::MissingArgument {
                address: address.to_owned(),
                expected: "boolean",
            })
    };

    let command = match rest {
        ["go"] => Command::Go { cue: None },
        ["go", number] => Command::Go {
            cue: Some((*number).to_owned()),
        },
        ["stop"] => Command::Stop,
        ["hardStop"] => Command::HardStop,
        ["pause"] => Command::Pause,
        ["resume"] => Command::Resume,
        ["panic"] => Command::Panic,
        ["reset"] => Command::Reset,
        ["playhead" | "playbackPosition", "next"] => Command::PlayheadNext,
        ["playhead" | "playbackPosition", "previous"] => Command::PlayheadPrevious,
        ["playhead" | "playbackPosition", "nextSequence"] => Command::PlayheadNextSequence,
        ["playhead" | "playbackPosition", "previousSequence"] => Command::PlayheadPreviousSequence,
        ["playhead" | "playbackPosition", number] => {
            Command::Playhead(CueSelector::Number((*number).to_owned()))
        }
        ["playheadID" | "playbackPositionID", id] => {
            Command::Playhead(CueSelector::Id((*id).to_owned()))
        }
        ["select", "next"] => Command::SelectNext,
        ["select", "previous"] => Command::SelectPrevious,
        ["select", number] => Command::Select(CueSelector::Number((*number).to_owned())),
        ["select_id", id] => Command::Select(CueSelector::Id((*id).to_owned())),
        ["cue", selector, method @ ..] if !method.is_empty() => Command::Cue {
            cue: cue_selector(selector),
            method: cue_method(method, args).ok_or_else(unsupported)?,
        },
        ["cue_id", id, method @ ..] if !method.is_empty() => Command::Cue {
            cue: CueSelector::Id((*id).to_owned()),
            method: cue_method(method, args).ok_or_else(unsupported)?,
        },
        ["updates"] => Command::Updates(bool_arg()?),
        ["alwaysReply"] => Command::AlwaysReply(bool_arg()?),
        ["connect"] => Command::Connect {
            passcode: args.first().and_then(Arg::as_str).map(str::to_owned),
        },
        ["disconnect"] => Command::Disconnect,
        ["version"] => Command::Version,
        ["workspaces"] => Command::Workspaces,
        ["cueLists"] => Command::CueLists,
        ["thump"] => Command::Thump,
        _ => return Err(unsupported()),
    };
    Ok(Request { workspace, command })
}

fn cue_selector(segment: &str) -> CueSelector {
    match segment {
        "selected" => CueSelector::Selected,
        "active" => CueSelector::Active,
        "playhead" => CueSelector::Playhead,
        number => CueSelector::Number(number.to_owned()),
    }
}

fn cue_method(segments: &[&str], args: &[Arg]) -> Option<CueMethod> {
    if let [action] = segments {
        let method = match *action {
            "start" => Some(CueMethod::Start),
            "stop" => Some(CueMethod::Stop),
            "hardStop" => Some(CueMethod::HardStop),
            "pause" => Some(CueMethod::Pause),
            "resume" => Some(CueMethod::Resume),
            "load" => Some(CueMethod::Load),
            "preview" => Some(CueMethod::Preview),
            "panic" => Some(CueMethod::Panic),
            "reset" => Some(CueMethod::Reset),
            _ => None,
        };
        if method.is_some() {
            return method;
        }
    }

    let (segments, live) = match segments {
        [rest @ .., "live"] => (rest, true),
        all => (all, false),
    };
    let first_arg = args.first().cloned();
    let (name, op) = match segments {
        [name @ .., "+"] => (name, PropertyOp::Add(first_arg?)),
        [name @ .., "-"] => (name, PropertyOp::Subtract(first_arg?)),
        [name @ .., "+", delta] => (name, PropertyOp::Add(Arg::from_segment(delta))),
        [name @ .., "-", delta] => (name, PropertyOp::Subtract(Arg::from_segment(delta))),
        name => (name, first_arg.map_or(PropertyOp::Get, PropertyOp::Set)),
    };
    if name.is_empty() || name.iter().any(|segment| segment.is_empty()) {
        return None;
    }
    Some(CueMethod::Property {
        name: name.join("/"),
        op,
        live,
    })
}
