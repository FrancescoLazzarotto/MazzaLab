//! Incoming MIDI: what arrived, whether it is what a trigger waits for, and
//! a listener that follows every input port.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use mazzalab_core::{MidiTrigger, MidiTriggerKind};
use midir::{Ignore, MidiInput, MidiInputConnection};
use serde::{Deserialize, Serialize};

use crate::CLIENT_NAME;

/// How often the inputs are looked at again, to pick up a controller that
/// was plugged in and let go of one that was pulled out.
const RESCAN: Duration = Duration::from_secs(2);

/// What is called with the name of an input and a message it received.
type EventCallback = Arc<dyn Fn(&str, MidiEvent) + Send + Sync>;
/// What is called with the name of an input and an MTC message from it.
type TimecodeCallback = Arc<dyn Fn(&str, &[u8]) + Send + Sync>;

/// Whether `bytes` is MIDI Time Code: a quarter frame, or a full frame.
pub fn is_timecode(bytes: &[u8]) -> bool {
    matches!(bytes, [0xF1, _] | [0xF0, 0x7F, _, 0x01, 0x01, ..])
}

/// A channel message received from an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiEvent {
    pub kind: MidiTriggerKind,
    /// Channel, from 1 to 16.
    pub channel: u8,
    /// The note, controller or program number.
    pub number: u8,
    /// The velocity or controller value; 0 for a program change.
    #[serde(default)]
    pub value: u8,
}

/// Reads a channel message. A note on with velocity 0 is the note off it
/// stands for; anything that cannot trigger (clock, pitch bend, system
/// messages...) is `None`.
pub fn parse_event(bytes: &[u8]) -> Option<MidiEvent> {
    let status = *bytes.first()?;
    let channel = (status & 0x0F) + 1;
    let data = |index: usize| bytes.get(index).copied().filter(|byte| *byte < 0x80);
    let (kind, number, value) = match status & 0xF0 {
        0x90 if data(2)? > 0 => (MidiTriggerKind::NoteOn, data(1)?, data(2)?),
        0x80 | 0x90 => (MidiTriggerKind::NoteOff, data(1)?, data(2)?),
        0xB0 => (MidiTriggerKind::ControlChange, data(1)?, data(2)?),
        0xC0 => (MidiTriggerKind::ProgramChange, data(1)?, 0),
        _ => return None,
    };
    Some(MidiEvent {
        kind,
        channel,
        number,
        value,
    })
}

/// Whether `event` is the message `trigger` waits for.
pub fn matches(trigger: &MidiTrigger, event: &MidiEvent) -> bool {
    trigger.kind == event.kind
        && trigger.number == event.number
        && trigger
            .channel
            .is_none_or(|channel| channel == event.channel)
        && trigger.value.is_none_or(|value| value == event.value)
}

/// Listens on every MIDI input of the machine for as long as it lives,
/// including inputs plugged in later.
#[derive(Debug)]
pub struct Listener {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Listener {
    /// `on_event` is called, on MIDI's own threads, with the name of the
    /// input and each channel message it receives.
    pub fn start(
        on_event: impl Fn(&str, MidiEvent) + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        Self::start_with_timecode(on_event, |_, _| {})
    }

    /// Like [`start`](Self::start), also handing `on_timecode` the MIDI
    /// Time Code that arrives, as it came.
    pub fn start_with_timecode(
        on_event: impl Fn(&str, MidiEvent) + Send + Sync + 'static,
        on_timecode: impl Fn(&str, &[u8]) + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let on_event: EventCallback = Arc::new(on_event);
        let on_timecode: TimecodeCallback = Arc::new(on_timecode);
        let thread = std::thread::Builder::new()
            .name("mazzalab-midi-in".into())
            .spawn(move || {
                // Connections live on this thread: they are not all `Send`.
                let mut open: HashMap<String, MidiInputConnection<()>> = HashMap::new();
                while !stopping.load(Ordering::Relaxed) {
                    rescan(&mut open, &on_event, &on_timecode);
                    let mut waited = Duration::ZERO;
                    while waited < RESCAN && !stopping.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(100));
                        waited += Duration::from_millis(100);
                    }
                }
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Opens the inputs that appeared and closes the ones that went away.
fn rescan(
    open: &mut HashMap<String, MidiInputConnection<()>>,
    on_event: &EventCallback,
    on_timecode: &TimecodeCallback,
) {
    let Ok(probe) = MidiInput::new(CLIENT_NAME) else {
        return;
    };
    let ports = probe.ports();
    let names: Vec<String> = ports
        .iter()
        .filter_map(|port| probe.port_name(port).ok())
        .collect();
    open.retain(|name, _| names.contains(name));
    for name in names {
        if open.contains_key(&name) {
            continue;
        }
        // Connecting uses the input up, so each port gets its own.
        let Ok(mut input) = MidiInput::new(CLIENT_NAME) else {
            continue;
        };
        // Time code is a system message: let it through.
        input.ignore(Ignore::ActiveSense);
        let Some(port) = input
            .ports()
            .into_iter()
            .find(|port| input.port_name(port).is_ok_and(|found| found == name))
        else {
            continue;
        };
        let callback = on_event.clone();
        let timecode = on_timecode.clone();
        let source = name.clone();
        match input.connect(
            &port,
            CLIENT_NAME,
            move |_, bytes, ()| {
                if let Some(event) = parse_event(bytes) {
                    callback(&source, event);
                } else if is_timecode(bytes) {
                    timecode(&source, bytes);
                }
            },
            (),
        ) {
            Ok(connection) => {
                tracing::info!(input = %name, "listening to MIDI input");
                open.insert(name, connection);
            }
            Err(error) => tracing::warn!(input = %name, %error, "cannot open MIDI input"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: MidiTriggerKind, channel: u8, number: u8, value: u8) -> MidiEvent {
        MidiEvent {
            kind,
            channel,
            number,
            value,
        }
    }

    #[test]
    fn channel_messages_are_read() {
        use MidiTriggerKind::{ControlChange, NoteOff, NoteOn, ProgramChange};
        assert_eq!(
            parse_event(&[0x90, 60, 100]),
            Some(event(NoteOn, 1, 60, 100))
        );
        assert_eq!(
            parse_event(&[0x8F, 60, 64]),
            Some(event(NoteOff, 16, 60, 64))
        );
        // Running a note on down to zero is how many keyboards say note off.
        assert_eq!(parse_event(&[0x92, 60, 0]), Some(event(NoteOff, 3, 60, 0)));
        assert_eq!(
            parse_event(&[0xB0, 64, 127]),
            Some(event(ControlChange, 1, 64, 127))
        );
        assert_eq!(
            parse_event(&[0xC9, 5]),
            Some(event(ProgramChange, 10, 5, 0))
        );
    }

    #[test]
    fn everything_else_is_ignored() {
        assert_eq!(parse_event(&[]), None);
        assert_eq!(parse_event(&[0xF8]), None, "clock");
        assert_eq!(parse_event(&[0xE0, 0, 64]), None, "pitch bend");
        assert_eq!(parse_event(&[0xF0, 0x7E, 0xF7]), None, "system exclusive");
        assert_eq!(parse_event(&[0x90, 60]), None, "cut short");
        assert_eq!(
            parse_event(&[0x90, 0x90, 5]),
            None,
            "a status where data belongs"
        );
    }

    #[test]
    fn triggers_match_on_what_they_name() {
        let pedal = MidiTrigger {
            kind: MidiTriggerKind::ControlChange,
            channel: None,
            number: 64,
            value: None,
        };
        assert!(matches(
            &pedal,
            &event(MidiTriggerKind::ControlChange, 1, 64, 127)
        ));
        assert!(matches(
            &pedal,
            &event(MidiTriggerKind::ControlChange, 9, 64, 0)
        ));
        assert!(!matches(
            &pedal,
            &event(MidiTriggerKind::ControlChange, 1, 65, 127)
        ));
        assert!(!matches(
            &pedal,
            &event(MidiTriggerKind::NoteOn, 1, 64, 127)
        ));

        let exact = MidiTrigger {
            channel: Some(2),
            value: Some(127),
            ..pedal
        };
        assert!(matches(
            &exact,
            &event(MidiTriggerKind::ControlChange, 2, 64, 127)
        ));
        assert!(!matches(
            &exact,
            &event(MidiTriggerKind::ControlChange, 1, 64, 127)
        ));
        assert!(!matches(
            &exact,
            &event(MidiTriggerKind::ControlChange, 2, 64, 126)
        ));
    }

    #[test]
    fn a_listener_starts_and_stops_without_inputs() {
        let listener = Listener::start(|_, _| {}).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        // Dropping it joins its thread; a hang here would fail the test run.
        drop(listener);
    }
}
