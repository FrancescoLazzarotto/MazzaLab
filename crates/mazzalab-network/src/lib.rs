//! mazzalab-network
//!
//! What Network cues send. A cue holds message text; a [`NetworkPatch`]
//! says where it goes and in which form:
//!
//! * OSC patches read each line as `/address arg arg ...` ([`osc_from_text`]);
//! * plain patches send each line's text as bytes ([`unescape`]).
//!
//! [`Sender`] delivers the encoded messages from a background thread, so a
//! slow name lookup or a dead TCP peer never holds up the show.

use std::collections::HashMap;
use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::mpsc;
use std::time::Duration;

use mazzalab_core::{NetworkPatch, NetworkProtocol};
use rosc::{OscMessage, OscType};
use thiserror::Error;

/// How long a TCP patch may take to accept a connection or a message.
const TCP_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NetworkError {
    #[error("the message is empty")]
    Empty,
    #[error("an OSC message starts with an address like /cue/1/start, not {0:?}")]
    BadAddress(String),
    #[error("a quoted string is not closed")]
    UnterminatedString,
    #[error("the message cannot be encoded: {0}")]
    Encode(String),
    #[error("{0}")]
    Io(String),
}

/// Splits a line into words; double quotes keep spaces together and `\"`
/// and `\\` stand for themselves inside them. Returns each word and whether
/// it was quoted.
fn words(line: &str) -> Result<Vec<(String, bool)>, NetworkError> {
    let mut words = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&first) = chars.peek() {
        if first.is_whitespace() {
            chars.next();
            continue;
        }
        let mut word = String::new();
        if first == '"' {
            chars.next();
            loop {
                match chars.next() {
                    None => return Err(NetworkError::UnterminatedString),
                    Some('"') => break,
                    Some('\\') => match chars.next() {
                        Some(escaped @ ('"' | '\\')) => word.push(escaped),
                        Some(other) => {
                            word.push('\\');
                            word.push(other);
                        }
                        None => return Err(NetworkError::UnterminatedString),
                    },
                    Some(other) => word.push(other),
                }
            }
            words.push((word, true));
        } else {
            while let Some(&next) = chars.peek() {
                if next.is_whitespace() {
                    break;
                }
                word.push(next);
                chars.next();
            }
            words.push((word, false));
        }
    }
    Ok(words)
}

/// Reads `/address arg arg ...` as an OSC message. Whole numbers become
/// integers, numbers with a decimal point floats, `true`/`false` booleans,
/// and everything else (or anything in double quotes) strings.
pub fn osc_from_text(line: &str) -> Result<OscMessage, NetworkError> {
    let mut words = words(line)?.into_iter();
    let (addr, quoted) = words.next().ok_or(NetworkError::Empty)?;
    if quoted || !addr.starts_with('/') {
        return Err(NetworkError::BadAddress(addr));
    }
    let args = words
        .map(|(word, quoted)| {
            if quoted {
                return OscType::String(word);
            }
            if let Ok(value) = word.parse::<i32>() {
                OscType::Int(value)
            } else if let Ok(value) = word.parse::<i64>() {
                OscType::Long(value)
            } else if let Some(value) = word
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite() && word.contains(['.', 'e', 'E']))
            {
                OscType::Float(value)
            } else {
                match word.as_str() {
                    "true" => OscType::Bool(true),
                    "false" => OscType::Bool(false),
                    _ => OscType::String(word),
                }
            }
        })
        .collect();
    Ok(OscMessage { addr, args })
}

/// Turns plain message text into bytes: `\n`, `\r`, `\t`, `\\` and `\xHH`
/// are replaced by the bytes they name.
pub fn unescape(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 1 < bytes.len() {
            let simple = match bytes[index + 1] {
                b'n' => Some(b'\n'),
                b'r' => Some(b'\r'),
                b't' => Some(b'\t'),
                b'\\' => Some(b'\\'),
                _ => None,
            };
            if let Some(byte) = simple {
                out.push(byte);
                index += 2;
                continue;
            }
            if bytes[index + 1] == b'x'
                && let Some(byte) = text
                    .get(index + 2..index + 4)
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                out.push(byte);
                index += 4;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    out
}

/// Encodes a cue's message text for a protocol: one payload per non-empty
/// line, ready to be written to the socket.
pub fn payloads(protocol: NetworkProtocol, text: &str) -> Result<Vec<Vec<u8>>, NetworkError> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return Err(NetworkError::Empty);
    }
    lines
        .into_iter()
        .map(|line| match protocol {
            NetworkProtocol::OscUdp | NetworkProtocol::OscTcp => {
                let packet = mazzalab_osc::encode(&osc_from_text(line)?)
                    .map_err(|error| NetworkError::Encode(error.to_string()))?;
                Ok(if protocol == NetworkProtocol::OscTcp {
                    mazzalab_osc::slip::encode(&packet)
                } else {
                    packet
                })
            }
            NetworkProtocol::PlainUdp | NetworkProtocol::PlainTcp => Ok(unescape(line)),
        })
        .collect()
}

struct Job {
    patch: NetworkPatch,
    payloads: Vec<Vec<u8>>,
}

type ErrorCallback = Box<dyn Fn(&NetworkPatch, &NetworkError) + Send>;

/// Sends messages to network patches from a background thread, in the order
/// they were queued. TCP connections stay open between messages.
#[derive(Debug)]
pub struct Sender {
    jobs: mpsc::Sender<Job>,
}

impl Sender {
    /// `on_error` is called (on the sender's thread) for every message that
    /// could not be delivered.
    pub fn new(
        on_error: impl Fn(&NetworkPatch, &NetworkError) + Send + 'static,
    ) -> std::io::Result<Self> {
        let (jobs, queue) = mpsc::channel::<Job>();
        let on_error: ErrorCallback = Box::new(on_error);
        std::thread::Builder::new()
            .name("mazzalab-network".into())
            .spawn(move || {
                let mut connections = Connections::default();
                for job in queue {
                    if let Err(error) = connections.deliver(&job) {
                        tracing::warn!(patch = %job.patch.name, %error, "network message not sent");
                        on_error(&job.patch, &error);
                    }
                }
            })?;
        Ok(Self { jobs })
    }

    /// Encodes `text` for the patch and queues it. Encoding problems are
    /// returned right away; delivery problems go to the error callback.
    pub fn send(&self, patch: &NetworkPatch, text: &str) -> Result<(), NetworkError> {
        let payloads = payloads(patch.protocol, text)?;
        self.jobs
            .send(Job {
                patch: patch.clone(),
                payloads,
            })
            .map_err(|_| NetworkError::Io("the network sender has stopped".into()))
    }
}

#[derive(Default)]
struct Connections {
    udp: Option<UdpSocket>,
    tcp: HashMap<(String, u16), TcpStream>,
}

impl Connections {
    fn deliver(&mut self, job: &Job) -> Result<(), NetworkError> {
        let io = |error: std::io::Error| NetworkError::Io(error.to_string());
        let patch = &job.patch;
        let target = (patch.host.as_str(), patch.port);
        match patch.protocol {
            NetworkProtocol::OscUdp | NetworkProtocol::PlainUdp => {
                let address = target
                    .to_socket_addrs()
                    .map_err(io)?
                    .find(std::net::SocketAddr::is_ipv4)
                    .ok_or_else(|| NetworkError::Io(format!("cannot resolve {}", patch.host)))?;
                let socket = match &self.udp {
                    Some(socket) => socket,
                    None => {
                        let socket = UdpSocket::bind(("0.0.0.0", 0)).map_err(io)?;
                        // Lighting desks and mixers are often addressed by
                        // broadcast.
                        let _ = socket.set_broadcast(true);
                        self.udp.insert(socket)
                    }
                };
                for payload in &job.payloads {
                    socket.send_to(payload, address).map_err(io)?;
                }
                Ok(())
            }
            NetworkProtocol::OscTcp | NetworkProtocol::PlainTcp => {
                let key = (patch.host.clone(), patch.port);
                // A connection the peer closed fails on write: retry once on
                // a fresh one.
                let mut last_error = None;
                for _ in 0..2 {
                    let stream = match self.tcp.entry(key.clone()) {
                        std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            let address =
                                target
                                    .to_socket_addrs()
                                    .map_err(io)?
                                    .next()
                                    .ok_or_else(|| {
                                        NetworkError::Io(format!("cannot resolve {}", patch.host))
                                    })?;
                            let stream =
                                TcpStream::connect_timeout(&address, TCP_TIMEOUT).map_err(io)?;
                            let _ = stream.set_nodelay(true);
                            let _ = stream.set_write_timeout(Some(TCP_TIMEOUT));
                            entry.insert(stream)
                        }
                    };
                    let written = job
                        .payloads
                        .iter()
                        .try_for_each(|payload| stream.write_all(payload))
                        .and_then(|()| stream.flush());
                    match written {
                        Ok(()) => return Ok(()),
                        Err(error) => {
                            self.tcp.remove(&key);
                            last_error = Some(error);
                        }
                    }
                }
                Err(last_error.map_or_else(
                    || NetworkError::Io("the message could not be written".into()),
                    io,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;

    #[test]
    fn text_becomes_typed_osc_arguments() {
        let message =
            osc_from_text(r#"/eos/chan/1 75 0.5 true "two words" plain 5000000000"#).unwrap();
        assert_eq!(message.addr, "/eos/chan/1");
        assert_eq!(
            message.args,
            vec![
                OscType::Int(75),
                OscType::Float(0.5),
                OscType::Bool(true),
                OscType::String("two words".into()),
                OscType::String("plain".into()),
                OscType::Long(5_000_000_000),
            ]
        );
    }

    #[test]
    fn quotes_keep_numbers_as_strings_and_allow_escapes() {
        let message = osc_from_text(r#"/label "12" "say \"hi\"""#).unwrap();
        assert_eq!(
            message.args,
            vec![
                OscType::String("12".into()),
                OscType::String("say \"hi\"".into()),
            ]
        );
    }

    #[test]
    fn bad_messages_are_refused() {
        assert_eq!(osc_from_text("   "), Err(NetworkError::Empty));
        assert_eq!(
            osc_from_text("go 1"),
            Err(NetworkError::BadAddress("go".into()))
        );
        assert_eq!(
            osc_from_text(r#"/a "open"#),
            Err(NetworkError::UnterminatedString)
        );
        assert_eq!(
            payloads(NetworkProtocol::OscUdp, "\n \n"),
            Err(NetworkError::Empty)
        );
    }

    #[test]
    fn plain_text_escapes_become_bytes() {
        assert_eq!(unescape(r"PLAY 1\r\n"), b"PLAY 1\r\n");
        assert_eq!(unescape(r"\x02AB\x03\\"), [0x02, b'A', b'B', 0x03, b'\\']);
        // Unknown escapes are left alone.
        assert_eq!(unescape(r"a\qb\"), br"a\qb\");
    }

    #[test]
    fn each_line_is_a_message() {
        let packets = payloads(NetworkProtocol::OscUdp, "/a 1\n\n  /b 2  \n").unwrap();
        assert_eq!(packets.len(), 2);
        let decoded = mazzalab_osc::decode(&packets[1]).unwrap();
        assert_eq!(decoded[0].addr, "/b");
        assert_eq!(decoded[0].args, vec![OscType::Int(2)]);
    }

    fn patch(protocol: NetworkProtocol, port: u16) -> NetworkPatch {
        NetworkPatch {
            name: "test".into(),
            host: "127.0.0.1".into(),
            port,
            protocol,
        }
    }

    #[test]
    fn udp_osc_arrives() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let port = receiver.local_addr().unwrap().port();
        let sender = Sender::new(|_, _| {}).unwrap();
        sender
            .send(&patch(NetworkProtocol::OscUdp, port), "/cue/5/start 1")
            .unwrap();
        let mut buffer = [0_u8; 1024];
        let (length, _) = receiver.recv_from(&mut buffer).unwrap();
        let message = mazzalab_osc::decode(&buffer[..length]).unwrap().remove(0);
        assert_eq!(message.addr, "/cue/5/start");
        assert_eq!(message.args, vec![OscType::Int(1)]);
    }

    #[test]
    fn tcp_plain_text_arrives_and_errors_are_reported() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (errors_tx, errors) = mpsc::channel();
        let sender = Sender::new(move |patch, error| {
            let _ = errors_tx.send((patch.port, error.clone()));
        })
        .unwrap();
        sender
            .send(&patch(NetworkProtocol::PlainTcp, port), r"PLAY\r\n")
            .unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut received = [0_u8; 6];
        stream.read_exact(&mut received).unwrap();
        assert_eq!(&received, b"PLAY\r\n");

        // Nothing listens on a port that was just released.
        let dead = TcpListener::bind("127.0.0.1:0").unwrap();
        let dead_port = dead.local_addr().unwrap().port();
        drop(dead);
        sender
            .send(&patch(NetworkProtocol::PlainTcp, dead_port), "x")
            .unwrap();
        let (port, error) = errors.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(port, dead_port);
        assert!(matches!(error, NetworkError::Io(_)));
    }
}
