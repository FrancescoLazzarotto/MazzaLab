//! A hot backup: a second machine follows the main one input by input and
//! takes over when the main one goes silent.
//!
//! The main server keeps its input record (see `mazzalab_show::replay`) in
//! memory as well as on disk, and streams it on `/api/follow`: every line
//! so far, then each new one as it is written, and a tick with its engine
//! time and a hash of its state every [`TICK`]. The engine being
//! deterministic, a server that applies the same inputs at the same instants
//! is in the same state, cue for cue. While it follows, the backup is
//! read-only and ignores its own triggers; when no tick has come for
//! [`SILENCE`] it takes over and runs the show from where it is.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures_util::StreamExt;
use mazzalab_show::{Entry, Header, Status};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

use crate::actor::{NoticeLevel, ShowHandle};

/// How often the main server tells followers where it is.
pub const TICK: Duration = Duration::from_millis(250);
/// How long without a tick before a follower takes over.
pub const SILENCE: Duration = Duration::from_millis(1_000);

/// The record of the running show, line by line, for followers.
#[derive(Debug)]
pub struct RecordTap {
    state: Mutex<TapState>,
    lines: broadcast::Sender<Arc<str>>,
}

#[derive(Debug, Default)]
struct TapState {
    lines: Vec<Arc<str>>,
    partial: Vec<u8>,
}

impl RecordTap {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::default(),
            lines: broadcast::channel(4096).0,
        })
    }

    /// Every line so far, and the lines still to come.
    pub fn subscribe(&self) -> (Vec<Arc<str>>, broadcast::Receiver<Arc<str>>) {
        let state = self.lock();
        // Subscribed under the lock, so no line falls between the two.
        let receiver = self.lines.subscribe();
        (state.lines.clone(), receiver)
    }

    /// A new record starts: another show was opened.
    fn reset(&self) {
        let mut state = self.lock();
        state.lines.clear();
        state.partial.clear();
    }

    fn take(&self, bytes: &[u8]) {
        let mut state = self.lock();
        for byte in bytes {
            if *byte == b'\n' {
                let line: Arc<str> = String::from_utf8_lossy(&state.partial).into();
                state.partial.clear();
                state.lines.push(Arc::clone(&line));
                let _ = self.lines.send(line);
            } else {
                state.partial.push(*byte);
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, TapState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Writes a record to its file, if it has one, and to the tap.
pub(crate) struct Tee {
    file: Option<Box<dyn Write + Send>>,
    tap: Arc<RecordTap>,
}

impl Tee {
    /// Starts a new record on `tap`.
    pub(crate) fn new(file: Option<Box<dyn Write + Send>>, tap: Arc<RecordTap>) -> Self {
        tap.reset();
        Self { file, tap }
    }
}

impl Write for Tee {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(file) = &mut self.file
            && let Err(error) = file.write_all(bytes)
        {
            // The disk failing must not stop the backup from following.
            tracing::warn!(%error, "input record file not written");
            self.file = None;
        }
        self.tap.take(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = &mut self.file {
            let _ = file.flush();
        }
        Ok(())
    }
}

/// A tick from the main server: its engine time in nanoseconds and the
/// hash of its state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tick {
    pub tick: u64,
    pub hash: u64,
}

/// What both machines compare: which cues run and where the playheads
/// stand.
pub fn state_hash(status: &Status) -> u64 {
    let mut running: Vec<String> = status
        .running
        .iter()
        .map(|run| run.cue.to_string())
        .collect();
    running.sort();
    let mut playheads: Vec<(String, Option<String>)> = status
        .playheads
        .iter()
        .map(|playhead| {
            (
                playhead.list.to_string(),
                playhead.cue.map(|cue| cue.to_string()),
            )
        })
        .collect();
    playheads.sort();
    let mut hasher = DefaultHasher::new();
    running.hash(&mut hasher);
    playheads.hash(&mut hasher);
    hasher.finish()
}

/// One line of the follow stream.
enum Line {
    Header(Box<Header>),
    Entry(Box<Entry>),
    Tick(Tick),
}

fn parse(line: &str) -> Option<Line> {
    if let Ok(tick) = serde_json::from_str::<Tick>(line) {
        return Some(Line::Tick(tick));
    }
    if let Ok(entry) = serde_json::from_str::<Entry>(line) {
        return Some(Line::Entry(Box::new(entry)));
    }
    serde_json::from_str::<Header>(line)
        .ok()
        .map(|header| Line::Header(Box::new(header)))
}

/// Follows the server at `url` (its HTTP address, e.g.
/// `http://10.0.0.2:53080`, with `?passcode=` if it asks for one) until it
/// goes silent, then takes over. Until it has been reached once, it keeps
/// trying.
pub async fn follow(url: String, handle: ShowHandle) {
    let address = follow_address(&url);
    let mut reached = false;
    loop {
        match tokio_tungstenite::connect_async(address.as_str()).await {
            Ok((mut socket, _)) => {
                if !reached {
                    handle.notify(NoticeLevel::Info, format!("Following {url}"));
                }
                reached = true;
                loop {
                    let next = tokio::time::timeout(SILENCE, socket.next()).await;
                    let Ok(Some(Ok(message))) = next else {
                        break;
                    };
                    let Message::Text(text) = message else {
                        continue;
                    };
                    match parse(text.as_str()) {
                        Some(Line::Header(header)) => handle.follow_start(*header).await,
                        Some(Line::Entry(entry)) => handle.follow_entry(*entry).await,
                        Some(Line::Tick(tick)) => handle.follow_tick(tick).await,
                        None => tracing::debug!(line = %text.as_str(), "unreadable follow line"),
                    }
                }
            }
            Err(error) if !reached => {
                tracing::debug!(%error, %url, "the main server is not reachable yet");
            }
            Err(_) => {}
        }
        if reached {
            handle.take_over().await;
            handle.notify(
                NoticeLevel::Warning,
                format!("The main server ({url}) went silent: this machine has taken over."),
            );
            return;
        }
        tokio::time::sleep(SILENCE).await;
    }
}

/// The WebSocket address of `/api/follow` on the server at `url`.
fn follow_address(url: &str) -> String {
    let (base, query) = url.split_once('?').unwrap_or((url, ""));
    let base = base.trim_end_matches('/');
    let base = base
        .strip_prefix("https://")
        .map(|rest| format!("wss://{rest}"))
        .or_else(|| {
            base.strip_prefix("http://")
                .map(|rest| format!("ws://{rest}"))
        })
        .unwrap_or_else(|| format!("ws://{base}"));
    if query.is_empty() {
        format!("{base}/api/follow")
    } else {
        format!("{base}/api/follow?{query}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_follow_address_comes_from_the_server_address() {
        assert_eq!(
            follow_address("http://10.0.0.2:53080"),
            "ws://10.0.0.2:53080/api/follow"
        );
        assert_eq!(
            follow_address("http://main.local:53080/?passcode=1234"),
            "ws://main.local:53080/api/follow?passcode=1234"
        );
        assert_eq!(
            follow_address("10.0.0.2:53080"),
            "ws://10.0.0.2:53080/api/follow"
        );
    }

    #[test]
    fn the_tap_splits_lines_and_starts_again_with_a_new_record() {
        let tap = RecordTap::new();
        let mut tee = Tee::new(None, Arc::clone(&tap));
        tee.write_all(b"{\"a\":1}\n{\"b\"").unwrap();
        let (lines, mut later) = tap.subscribe();
        assert_eq!(lines.len(), 1);
        tee.write_all(b":2}\n").unwrap();
        assert_eq!(&*later.try_recv().unwrap(), "{\"b\":2}");
        drop(tee);
        let _ = Tee::new(None, Arc::clone(&tap));
        assert!(tap.subscribe().0.is_empty());
    }
}
