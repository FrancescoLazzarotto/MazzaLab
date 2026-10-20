//! JSON messages exchanged with web clients over the WebSocket.

use mazzalab_core::CueId;
use mazzalab_engine::{Event, EventKind};
use mazzalab_show::{Command, Snapshot, Status};
use serde::{Deserialize, Serialize};

/// Server to client.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage<'a> {
    /// Full show structure: sent on connect and whenever the revision changes.
    Snapshot { snapshot: &'a Snapshot },
    /// Live playback state, sent continuously while cues run.
    Status { status: &'a Status },
    /// Engine events, in order.
    Events { events: &'a [EventDto] },
    /// Answer to a client request carrying the same `id`.
    Result {
        id: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

/// Client to server.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Command { id: u64, command: Box<Command> },
    Save { id: u64 },
}

/// An engine event in client form, times in seconds.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventDto {
    pub at: f64,
    #[serde(flatten)]
    pub kind: EventKindDto,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKindDto {
    Triggered { cue: CueId },
    Skipped { cue: CueId },
    ActionStarted { cue: CueId, armed: bool },
    ActionCompleted { cue: CueId },
    Stopped { cue: CueId },
    Paused { cue: CueId },
    Resumed { cue: CueId },
    Panicking { cue: CueId, fade: f64 },
    PlayheadMoved { list: CueId, cue: Option<CueId> },
}

impl From<&Event> for EventDto {
    fn from(event: &Event) -> Self {
        let kind = match event.kind {
            EventKind::Triggered(cue) => EventKindDto::Triggered { cue },
            EventKind::Skipped(cue) => EventKindDto::Skipped { cue },
            EventKind::ActionStarted { cue, armed } => EventKindDto::ActionStarted { cue, armed },
            EventKind::ActionCompleted(cue) => EventKindDto::ActionCompleted { cue },
            EventKind::Stopped(cue) => EventKindDto::Stopped { cue },
            EventKind::Paused(cue) => EventKindDto::Paused { cue },
            EventKind::Resumed(cue) => EventKindDto::Resumed { cue },
            EventKind::Panicking { cue, fade } => EventKindDto::Panicking {
                cue,
                fade: fade.as_secs_f64(),
            },
            EventKind::PlayheadMoved { list, cue } => EventKindDto::PlayheadMoved { list, cue },
        };
        Self {
            at: event.at.as_secs_f64(),
            kind,
        }
    }
}
