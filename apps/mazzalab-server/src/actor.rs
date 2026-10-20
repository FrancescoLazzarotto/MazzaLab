//! The show actor: a single task owns the [`Show`], drives it with the real
//! clock and serializes every request. Everything else talks to it through a
//! cloneable [`ShowHandle`] and listens to [`Update`] broadcasts.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mazzalab_core::CueId;
use mazzalab_engine::{Event, EventKind};
use mazzalab_show::{Command, OscOutcome, Show, Snapshot, Status};
use rosc::{OscMessage, OscType};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::Instant;

use crate::protocol::EventDto;

/// Status is broadcast at this rate while something is running.
const RUNNING_TICK: Duration = Duration::from_millis(50);
/// ...and at this rate when the show is idle, as a keep-alive.
const IDLE_TICK: Duration = Duration::from_secs(1);

/// A media or device backend fed with engine events.
pub trait Output: Send {
    /// Called after every batch of engine events, in order. `show` already
    /// reflects the events.
    fn handle(&mut self, show: &Show, events: &[Event]);
}

/// Given to outputs so they can report that an open-ended action (e.g. an
/// audio file of unknown length) has finished.
#[derive(Debug, Clone)]
pub struct Completion(mpsc::UnboundedSender<CueId>);

impl Completion {
    pub fn action_finished(&self, cue: CueId) {
        // The actor only goes away on shutdown, when nobody cares anymore.
        let _ = self.0.send(cue);
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
}

const GONE: &str = "the show is no longer running";

#[derive(Debug, Clone)]
pub struct ShowHandle {
    requests: mpsc::Sender<Request>,
    updates: broadcast::Sender<Update>,
    workspace_id: Arc<str>,
}

impl ShowHandle {
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
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

    async fn ask<T>(&self, request: impl FnOnce(oneshot::Sender<T>) -> Request) -> Option<T> {
        let (reply, response) = oneshot::channel();
        self.requests.send(request(reply)).await.ok()?;
        response.await.ok()
    }
}

/// Starts the actor task. `make_outputs` receives the completion channel the
/// outputs use to report finished actions.
pub fn spawn(
    show: Show,
    path: Option<PathBuf>,
    make_outputs: impl FnOnce(Completion) -> Vec<Box<dyn Output>>,
) -> (ShowHandle, tokio::task::JoinHandle<()>) {
    let (requests, receiver) = mpsc::channel(256);
    let (updates, _) = broadcast::channel(1024);
    let (finished_tx, finished_rx) = mpsc::unbounded_channel();
    let outputs = make_outputs(Completion(finished_tx));
    let handle = ShowHandle {
        requests,
        updates: updates.clone(),
        workspace_id: show.workspace().id.to_string().into(),
    };
    let actor = Actor {
        show,
        start: Instant::now(),
        path,
        outputs,
        updates,
        last_revision: None,
        last_status: None,
        workspace_id: handle.workspace_id.clone(),
    };
    let task = tokio::spawn(actor.run(receiver, finished_rx));
    (handle, task)
}

struct Actor {
    show: Show,
    start: Instant,
    path: Option<PathBuf>,
    outputs: Vec<Box<dyn Output>>,
    updates: broadcast::Sender<Update>,
    last_revision: Option<u64>,
    last_status: Option<(Instant, Status)>,
    workspace_id: Arc<str>,
}

impl Actor {
    async fn run(
        mut self,
        mut requests: mpsc::Receiver<Request>,
        mut finished: mpsc::UnboundedReceiver<CueId>,
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
                    Some(path) => mazzalab_project::save(path, self.show.workspace())
                        .map(|()| self.show.mark_saved())
                        .map_err(|error| error.to_string()),
                };
                let _ = reply.send(result);
            }
        }
    }

    /// Advances the show, feeds outputs and broadcasts what changed.
    fn sync(&mut self) {
        self.advance();
        let events = self.show.take_events();
        if !events.is_empty() {
            for output in &mut self.outputs {
                output.handle(&self.show, &events);
            }
            let dtos: Vec<EventDto> = events.iter().map(EventDto::from).collect();
            let _ = self.updates.send(Update::Events(Arc::new(dtos)));
            let osc = self.cue_updates(&events);
            if !osc.is_empty() {
                let _ = self.updates.send(Update::Osc(Arc::new(osc)));
            }
        }

        let revision = self.show.revision();
        let structure_changed = self.last_revision != Some(revision);
        if structure_changed {
            self.last_revision = Some(revision);
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
        if due {
            let _ = self.updates.send(Update::Status(Arc::new(status.clone())));
            self.last_status = Some((Instant::now(), status));
        }
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

fn is_busy(status: &Status) -> bool {
    status.panicking || !status.running.is_empty()
}

fn message(addr: String, argument: Option<String>) -> OscMessage {
    OscMessage {
        addr,
        args: argument.map(OscType::String).into_iter().collect(),
    }
}
