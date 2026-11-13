//! mazzalab-midi
//!
//! What MIDI cues send: [`encode`] turns a cue's [`MidiMessage`] into the
//! bytes that go down the cable (voice messages, System Exclusive and MIDI
//! Show Control), and [`Sender`] delivers them to output ports from a
//! background thread, keeping the ports open between messages.
//!
//! What comes in: [`Listener`] follows every input port, [`parse_event`]
//! reads channel messages and [`matches`] tells whether one is the message
//! a [`MidiTrigger`] waits for.

mod input;
mod mtc;

pub use input::{Listener, MidiEvent, is_timecode, matches, parse_event};
pub use mtc::MtcGenerator;

use std::collections::HashMap;
use std::sync::mpsc;

use mazzalab_core::MidiMessage;
use midir::{MidiInput, MidiOutput, MidiOutputConnection};

#[cfg(doc)]
use mazzalab_core::MidiTrigger;
use thiserror::Error;

/// Name MazzaLab shows up with in the system's MIDI clients.
pub(crate) const CLIENT_NAME: &str = "MazzaLab";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MidiError {
    #[error("{what} must be between {min} and {max}, not {value}")]
    OutOfRange {
        what: &'static str,
        min: u8,
        max: u8,
        value: u8,
    },
    #[error("System Exclusive data bytes must be below 0x80; byte {index} is {value:#04X}")]
    SysExByte { index: usize, value: u8 },
    #[error("a cue number can only hold digits and dots, not {0:?}")]
    CueNumber(String),
    #[error("there is no MIDI output named {0:?}")]
    NoPort(String),
    #[error("{0}")]
    Io(String),
}

fn data(what: &'static str, value: u8) -> Result<u8, MidiError> {
    if value > 127 {
        return Err(MidiError::OutOfRange {
            what,
            min: 0,
            max: 127,
            value,
        });
    }
    Ok(value)
}

/// The status byte for a channel message; channels are numbered from 1, as
/// on every piece of MIDI gear.
fn status(kind: u8, channel: u8) -> Result<u8, MidiError> {
    if !(1..=16).contains(&channel) {
        return Err(MidiError::OutOfRange {
            what: "the channel",
            min: 1,
            max: 16,
            value: channel,
        });
    }
    Ok(kind | (channel - 1))
}

/// A cue number or list in MIDI Show Control: ASCII digits and dots.
fn msc_number(text: &str) -> Result<&[u8], MidiError> {
    if text
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        Ok(text.as_bytes())
    } else {
        Err(MidiError::CueNumber(text.to_owned()))
    }
}

/// The bytes of a message, ready to be sent.
pub fn encode(message: &MidiMessage) -> Result<Vec<u8>, MidiError> {
    Ok(match message {
        MidiMessage::NoteOn {
            channel,
            note,
            velocity,
        } => vec![
            status(0x90, *channel)?,
            data("the note", *note)?,
            data("the velocity", *velocity)?,
        ],
        MidiMessage::NoteOff {
            channel,
            note,
            velocity,
        } => vec![
            status(0x80, *channel)?,
            data("the note", *note)?,
            data("the velocity", *velocity)?,
        ],
        MidiMessage::ControlChange {
            channel,
            controller,
            value,
        } => vec![
            status(0xB0, *channel)?,
            data("the controller", *controller)?,
            data("the value", *value)?,
        ],
        MidiMessage::ProgramChange { channel, program } => {
            vec![status(0xC0, *channel)?, data("the program", *program)?]
        }
        MidiMessage::Msc {
            device_id,
            command_format,
            command,
            q_number,
            q_list,
        } => {
            // F0 7F <device> 02 <format> <command> <number> [00 <list>] F7
            let mut bytes = vec![
                0xF0,
                0x7F,
                data("the device ID", *device_id)?,
                0x02,
                data("the command format", *command_format)?,
                data("the command", *command)?,
            ];
            let number = q_number.as_deref().filter(|number| !number.is_empty());
            let list = q_list.as_deref().filter(|list| !list.is_empty());
            if let Some(number) = number {
                bytes.extend_from_slice(msc_number(number)?);
            }
            if let Some(list) = list {
                // A list can only follow a cue number.
                if number.is_some() {
                    bytes.push(0x00);
                    bytes.extend_from_slice(msc_number(list)?);
                }
            }
            bytes.push(0xF7);
            bytes
        }
        MidiMessage::SysEx { data: payload } => {
            if let Some((index, value)) = payload
                .iter()
                .copied()
                .enumerate()
                .find(|(_, byte)| *byte > 127)
            {
                return Err(MidiError::SysExByte { index, value });
            }
            let mut bytes = Vec::with_capacity(payload.len() + 2);
            bytes.push(0xF0);
            bytes.extend_from_slice(payload);
            bytes.push(0xF7);
            bytes
        }
    })
}

/// The names of the MIDI outputs on this machine.
pub fn output_ports() -> Vec<String> {
    let Ok(output) = MidiOutput::new(CLIENT_NAME) else {
        return Vec::new();
    };
    output
        .ports()
        .iter()
        .filter_map(|port| output.port_name(port).ok())
        .collect()
}

/// The names of the MIDI inputs on this machine.
pub fn input_ports() -> Vec<String> {
    let Ok(input) = MidiInput::new(CLIENT_NAME) else {
        return Vec::new();
    };
    input
        .ports()
        .iter()
        .filter_map(|port| input.port_name(port).ok())
        .collect()
}

struct Job {
    port: String,
    bytes: Vec<u8>,
}

type ErrorCallback = Box<dyn Fn(&str, &MidiError) + Send>;

/// Sends messages to MIDI outputs from a background thread, in the order
/// they were queued. Ports are opened on first use and stay open. Clones
/// share the thread.
#[derive(Debug, Clone)]
pub struct Sender {
    jobs: mpsc::Sender<Job>,
}

impl Sender {
    /// `on_error` is called (on the sender's thread) with the port name for
    /// every message that could not be delivered.
    pub fn new(on_error: impl Fn(&str, &MidiError) + Send + 'static) -> std::io::Result<Self> {
        let (jobs, queue) = mpsc::channel::<Job>();
        let on_error: ErrorCallback = Box::new(on_error);
        std::thread::Builder::new()
            .name("mazzalab-midi".into())
            .spawn(move || {
                let mut ports = Ports::default();
                for job in queue {
                    if let Err(error) = ports.deliver(&job) {
                        tracing::warn!(port = %job.port, %error, "MIDI message not sent");
                        on_error(&job.port, &error);
                    }
                }
            })?;
        Ok(Self { jobs })
    }

    /// Encodes `message` and queues it for `port`. Invalid messages are
    /// refused right away; delivery problems go to the error callback.
    pub fn send(&self, port: &str, message: &MidiMessage) -> Result<(), MidiError> {
        self.send_bytes(port, encode(message)?)
    }

    /// Queues a message already encoded, such as MIDI Time Code.
    pub fn send_bytes(&self, port: &str, bytes: Vec<u8>) -> Result<(), MidiError> {
        self.jobs
            .send(Job {
                port: port.to_owned(),
                bytes,
            })
            .map_err(|_| MidiError::Io("the MIDI sender has stopped".into()))
    }
}

#[derive(Default)]
struct Ports {
    open: HashMap<String, MidiOutputConnection>,
}

impl Ports {
    fn connect(name: &str) -> Result<MidiOutputConnection, MidiError> {
        let output =
            MidiOutput::new(CLIENT_NAME).map_err(|error| MidiError::Io(error.to_string()))?;
        let port = output
            .ports()
            .into_iter()
            .find(|port| output.port_name(port).is_ok_and(|found| found == name))
            .ok_or_else(|| MidiError::NoPort(name.to_owned()))?;
        output
            .connect(&port, CLIENT_NAME)
            .map_err(|error| MidiError::Io(error.to_string()))
    }

    fn deliver(&mut self, job: &Job) -> Result<(), MidiError> {
        // A port that was unplugged fails on send: retry once on a fresh
        // connection, which also finds it again after it is plugged back.
        let mut last_error = None;
        for _ in 0..2 {
            let connection = match self.open.entry(job.port.clone()) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(Self::connect(&job.port)?)
                }
            };
            match connection.send(&job.bytes) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last_error = Some(MidiError::Io(error.to_string()));
                    self.open.remove(&job.port);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| MidiError::Io("the message could not be sent".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_messages_use_one_based_channels() {
        assert_eq!(
            encode(&MidiMessage::NoteOn {
                channel: 1,
                note: 60,
                velocity: 100
            }),
            Ok(vec![0x90, 60, 100])
        );
        assert_eq!(
            encode(&MidiMessage::NoteOff {
                channel: 16,
                note: 60,
                velocity: 0
            }),
            Ok(vec![0x8F, 60, 0])
        );
        assert_eq!(
            encode(&MidiMessage::ControlChange {
                channel: 2,
                controller: 7,
                value: 127
            }),
            Ok(vec![0xB1, 7, 127])
        );
        assert_eq!(
            encode(&MidiMessage::ProgramChange {
                channel: 10,
                program: 5
            }),
            Ok(vec![0xC9, 5])
        );
    }

    #[test]
    fn values_out_of_range_are_refused() {
        assert!(matches!(
            encode(&MidiMessage::ProgramChange {
                channel: 0,
                program: 1
            }),
            Err(MidiError::OutOfRange {
                min: 1,
                max: 16,
                ..
            })
        ));
        assert!(matches!(
            encode(&MidiMessage::NoteOn {
                channel: 17,
                note: 1,
                velocity: 1
            }),
            Err(MidiError::OutOfRange { .. })
        ));
        assert!(matches!(
            encode(&MidiMessage::NoteOn {
                channel: 1,
                note: 128,
                velocity: 1
            }),
            Err(MidiError::OutOfRange { max: 127, .. })
        ));
        assert_eq!(
            encode(&MidiMessage::SysEx {
                data: vec![0x7E, 0x80]
            }),
            Err(MidiError::SysExByte {
                index: 1,
                value: 0x80
            })
        );
    }

    #[test]
    fn sysex_is_framed() {
        assert_eq!(
            encode(&MidiMessage::SysEx {
                data: vec![0x7E, 0x7F, 0x06, 0x01]
            }),
            Ok(vec![0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7])
        );
    }

    #[test]
    fn show_control_carries_cue_number_and_list() {
        // GO (0x01) to lighting (0x01) on device 5, cue 12.5 in list 2.
        let message = MidiMessage::Msc {
            device_id: 5,
            command_format: 0x01,
            command: 0x01,
            q_number: Some("12.5".into()),
            q_list: Some("2".into()),
        };
        assert_eq!(
            encode(&message),
            Ok(vec![
                0xF0, 0x7F, 5, 0x02, 0x01, 0x01, b'1', b'2', b'.', b'5', 0x00, b'2', 0xF7
            ])
        );
        // Without a cue number the list is left out too.
        let bare = MidiMessage::Msc {
            device_id: 0x7F,
            command_format: 0x7F,
            command: 0x0A,
            q_number: None,
            q_list: Some("2".into()),
        };
        assert_eq!(
            encode(&bare),
            Ok(vec![0xF0, 0x7F, 0x7F, 0x02, 0x7F, 0x0A, 0xF7])
        );
        let bad = MidiMessage::Msc {
            device_id: 1,
            command_format: 1,
            command: 1,
            q_number: Some("A1".into()),
            q_list: None,
        };
        assert_eq!(encode(&bad), Err(MidiError::CueNumber("A1".into())));
    }

    #[test]
    fn an_unknown_port_is_reported() {
        let (errors_tx, errors) = mpsc::channel();
        let sender = Sender::new(move |port, error| {
            let _ = errors_tx.send((port.to_owned(), error.clone()));
        })
        .unwrap();
        // An invalid message never reaches the queue.
        assert!(
            sender
                .send(
                    "anything",
                    &MidiMessage::ProgramChange {
                        channel: 99,
                        program: 0
                    }
                )
                .is_err()
        );
        sender
            .send(
                "no such MIDI port",
                &MidiMessage::ProgramChange {
                    channel: 1,
                    program: 0,
                },
            )
            .unwrap();
        let (port, error) = errors
            // The first enumeration after boot can take the system a while.
            .recv_timeout(std::time::Duration::from_secs(60))
            .unwrap();
        assert_eq!(port, "no such MIDI port");
        // Machines without any MIDI system report an I/O error instead.
        assert!(matches!(error, MidiError::NoPort(_) | MidiError::Io(_)));
    }

    #[test]
    #[ignore = "plays a note on a real MIDI output"]
    fn plays_a_note_on_the_first_output() {
        let ports = output_ports();
        println!("outputs: {ports:?}");
        println!("inputs: {:?}", input_ports());
        let port = ports.first().expect("no MIDI output on this machine");
        let (errors_tx, errors) = mpsc::channel();
        let sender = Sender::new(move |port, error| {
            let _ = errors_tx.send((port.to_owned(), error.clone()));
        })
        .unwrap();
        let note = |on: bool| {
            if on {
                MidiMessage::NoteOn {
                    channel: 1,
                    note: 60,
                    velocity: 40,
                }
            } else {
                MidiMessage::NoteOff {
                    channel: 1,
                    note: 60,
                    velocity: 0,
                }
            }
        };
        sender.send(port, &note(true)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(400));
        sender.send(port, &note(false)).unwrap();
        // Nothing went wrong within a moment of the last message.
        assert!(
            errors
                .recv_timeout(std::time::Duration::from_millis(500))
                .is_err()
        );
    }
}
