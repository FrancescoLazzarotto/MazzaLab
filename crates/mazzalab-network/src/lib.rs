//! mazzalab-network
//!
//! What Network cues send. A cue holds message text; a [`NetworkPatch`]
//! says where it goes and in which form:
//!
//! * OSC patches read each line as `/address arg arg ...` ([`osc_from_text`]);
//! * plain patches send each line's text as bytes ([`unescape`]);
//! * HTTP patches GET or POST each line to a URL, and WebSocket patches send
//!   each line as a text message.
//!
//! In an OSC message an argument written `from>to` (`/fader/1 0>1`) moves
//! from one number to the other over the cue's duration ([`Fade`]).
//!
//! [`Sender`] delivers the encoded messages from a background thread, so a
//! slow name lookup or a dead TCP peer never holds up the show.

use std::collections::HashMap;
use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use mazzalab_core::{CueId, NetworkPatch, NetworkProtocol};
use rosc::{OscMessage, OscType};
use thiserror::Error;

/// How long a TCP patch may take to accept a connection or a message.
const TCP_TIMEOUT: Duration = Duration::from_secs(2);
/// How often a fading value is sent: about 30 times a second.
const FADE_STEP: Duration = Duration::from_millis(33);

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
    let args = words.map(|(word, quoted)| osc_arg(word, quoted)).collect();
    Ok(OscMessage { addr, args })
}

/// One argument word as the OSC type it reads as.
fn osc_arg(word: String, quoted: bool) -> OscType {
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
}

/// An argument of a message that can fade.
#[derive(Debug, Clone, PartialEq)]
enum Part {
    Fixed(OscType),
    /// Moves from `from` to `to`; whole numbers on both ends stay whole.
    Ramp {
        from: f64,
        to: f64,
        integer: bool,
    },
}

impl Part {
    fn at(&self, fraction: f64) -> OscType {
        match self {
            Self::Fixed(value) => value.clone(),
            Self::Ramp { from, to, integer } => {
                let value = from + (to - from) * fraction.clamp(0.0, 1.0);
                if *integer {
                    OscType::Int(value.round() as i32)
                } else {
                    OscType::Float(value as f32)
                }
            }
        }
    }
}

/// OSC messages with arguments written `from>to`, which move between the
/// two numbers over a cue's duration: `/fader/1 0.0>1.0` goes from 0 to 1
/// (`0>100` moves whole numbers). Everything else in a line is sent as it
/// reads.
#[derive(Debug, Clone, PartialEq)]
pub struct Fade {
    lines: Vec<(String, Vec<Part>)>,
}

impl Fade {
    /// Reads the lines of `text`; `None` when no argument fades.
    pub fn parse(text: &str) -> Result<Option<Self>, NetworkError> {
        let mut lines = Vec::new();
        let mut any = false;
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let mut words = words(line)?.into_iter();
            let (addr, quoted) = words.next().ok_or(NetworkError::Empty)?;
            if quoted || !addr.starts_with('/') {
                return Err(NetworkError::BadAddress(addr));
            }
            let parts = words
                .map(|(word, quoted)| match ramp(&word).filter(|_| !quoted) {
                    Some(part) => {
                        any = true;
                        part
                    }
                    None => Part::Fixed(osc_arg(word, quoted)),
                })
                .collect();
            lines.push((addr, parts));
        }
        if lines.is_empty() {
            return Err(NetworkError::Empty);
        }
        Ok(any.then_some(Self { lines }))
    }

    /// The messages `fraction` of the way through, 0 to 1.
    pub fn at(&self, fraction: f64) -> Vec<OscMessage> {
        self.lines
            .iter()
            .map(|(addr, parts)| OscMessage {
                addr: addr.clone(),
                args: parts.iter().map(|part| part.at(fraction)).collect(),
            })
            .collect()
    }

    /// The messages `fraction` of the way through, encoded for an OSC protocol.
    pub fn payloads(
        &self,
        protocol: NetworkProtocol,
        fraction: f64,
    ) -> Result<Vec<Vec<u8>>, NetworkError> {
        self.at(fraction)
            .iter()
            .map(|message| {
                let packet = mazzalab_osc::encode(message)
                    .map_err(|error| NetworkError::Encode(error.to_string()))?;
                Ok(if protocol == NetworkProtocol::OscTcp {
                    mazzalab_osc::slip::encode(&packet)
                } else {
                    packet
                })
            })
            .collect()
    }
}

/// `0.0>1.0` as a ramp; anything else is not one.
fn ramp(word: &str) -> Option<Part> {
    let (from, to) = word.split_once('>')?;
    let number = |text: &str| text.parse::<f64>().ok().filter(|value| value.is_finite());
    let whole = |text: &str| text.parse::<i32>().is_ok();
    Some(Part::Ramp {
        from: number(from)?,
        to: number(to)?,
        integer: whole(from) && whole(to),
    })
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
            // The line as text: the path and body, or the WebSocket message.
            NetworkProtocol::HttpGet | NetworkProtocol::HttpPost | NetworkProtocol::WebSocket => {
                Ok(line.as_bytes().to_vec())
            }
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
    /// The fades under way, by cue, each with its stop flag.
    fades: Arc<Mutex<HashMap<CueId, Arc<AtomicBool>>>>,
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
        Ok(Self {
            jobs,
            fades: Arc::default(),
        })
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

    /// Sends a cue's message. When it is OSC with `from>to` arguments and
    /// `duration` is not zero, those arguments move over the duration, about
    /// thirty messages a second, until `stop` is called for the cue; with no
    /// duration the arguments go straight to where they end.
    pub fn send_fade(
        &self,
        cue: CueId,
        patch: &NetworkPatch,
        text: &str,
        duration: Duration,
    ) -> Result<(), NetworkError> {
        let osc = matches!(
            patch.protocol,
            NetworkProtocol::OscUdp | NetworkProtocol::OscTcp
        );
        let fade = if osc { Fade::parse(text)? } else { None };
        let Some(fade) = fade else {
            return self.send(patch, text);
        };
        let first = fade.payloads(patch.protocol, if duration.is_zero() { 1.0 } else { 0.0 })?;
        self.queue(patch, first)?;
        if duration.is_zero() {
            return Ok(());
        }
        let stop = Arc::new(AtomicBool::new(false));
        self.fades
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(cue, stop.clone());
        let (jobs, fades, patch) = (self.jobs.clone(), self.fades.clone(), patch.clone());
        std::thread::Builder::new()
            .name("mazzalab-network-fade".into())
            .spawn(move || {
                let started = Instant::now();
                loop {
                    std::thread::sleep(FADE_STEP);
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let elapsed = started.elapsed();
                    let done = elapsed >= duration;
                    let fraction = if done {
                        1.0
                    } else {
                        elapsed.as_secs_f64() / duration.as_secs_f64()
                    };
                    let Ok(payloads) = fade.payloads(patch.protocol, fraction) else {
                        break;
                    };
                    if jobs
                        .send(Job {
                            patch: patch.clone(),
                            payloads,
                        })
                        .is_err()
                        || done
                    {
                        break;
                    }
                }
                let mut running = fades.lock().unwrap_or_else(PoisonError::into_inner);
                // Only this fade's own entry: a later one for the cue stays.
                if running
                    .get(&cue)
                    .is_some_and(|flag| Arc::ptr_eq(flag, &stop))
                {
                    running.remove(&cue);
                }
            })
            .map_err(|error| NetworkError::Io(error.to_string()))?;
        Ok(())
    }

    /// Stops the fade a cue has under way, if any: its values stay where they got to.
    pub fn stop(&self, cue: CueId) {
        if let Some(flag) = self
            .fades
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&cue)
        {
            flag.store(true, Ordering::Relaxed);
        }
    }

    fn queue(&self, patch: &NetworkPatch, payloads: Vec<Vec<u8>>) -> Result<(), NetworkError> {
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
    /// Open WebSocket connections, by URL.
    sockets: HashMap<String, tungstenite::WebSocket<TcpStream>>,
    http: Option<ureq::Agent>,
}

impl Connections {
    fn deliver(&mut self, job: &Job) -> Result<(), NetworkError> {
        let io = |error: std::io::Error| NetworkError::Io(error.to_string());
        let patch = &job.patch;
        let target = (patch.host.as_str(), patch.port);
        match patch.protocol {
            NetworkProtocol::HttpGet | NetworkProtocol::HttpPost => self.http(job),
            NetworkProtocol::WebSocket => self.websocket(job),
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

/// `base` and `path` as one URL, with one slash between them.
fn join_url(base: &str, path: &str) -> String {
    let base = base.trim();
    let path = path.trim();
    if path.is_empty() {
        return base.to_owned();
    }
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// `http://` when a URL names no scheme.
fn with_scheme(url: &str, scheme: &str) -> String {
    let url = url.trim();
    if url.contains("://") {
        url.to_owned()
    } else {
        format!("{scheme}://{url}")
    }
}

impl Connections {
    /// GETs or POSTs each payload: a path, and for POST the body after it.
    fn http(&mut self, job: &Job) -> Result<(), NetworkError> {
        let agent = self.http.get_or_insert_with(|| {
            ureq::Agent::config_builder()
                .timeout_global(Some(TCP_TIMEOUT))
                .build()
                .into()
        });
        let base = with_scheme(&job.patch.host, "http");
        for payload in &job.payloads {
            let line = String::from_utf8_lossy(payload);
            let line = line.trim();
            let result = if job.patch.protocol == NetworkProtocol::HttpPost {
                let (path, body) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
                let body = unescape(body.trim());
                let kind = if body.first().is_some_and(|byte| matches!(byte, b'{' | b'[')) {
                    "application/json"
                } else {
                    "text/plain"
                };
                agent
                    .post(join_url(&base, path))
                    .header("content-type", kind)
                    .send(&body[..])
                    .map(drop)
            } else {
                agent.get(join_url(&base, line)).call().map(drop)
            };
            result.map_err(|error| NetworkError::Io(error.to_string()))?;
        }
        Ok(())
    }

    /// Sends each payload as a text message on the patch's WebSocket,
    /// connecting first, and again once if the connection has gone.
    fn websocket(&mut self, job: &Job) -> Result<(), NetworkError> {
        let url = with_scheme(&job.patch.host, "ws");
        let mut last_error = None;
        for _ in 0..2 {
            if !self.sockets.contains_key(&url) {
                let socket = connect_websocket(&url)?;
                self.sockets.insert(url.clone(), socket);
            }
            let Some(socket) = self.sockets.get_mut(&url) else {
                continue;
            };
            let sent = job.payloads.iter().try_for_each(|payload| {
                socket.send(tungstenite::Message::text(
                    String::from_utf8_lossy(payload).into_owned(),
                ))
            });
            match sent {
                Ok(()) => return Ok(()),
                Err(error) => {
                    self.sockets.remove(&url);
                    last_error = Some(error);
                }
            }
        }
        Err(NetworkError::Io(last_error.map_or_else(
            || "the message could not be sent".into(),
            |error| error.to_string(),
        )))
    }
}

fn connect_websocket(url: &str) -> Result<tungstenite::WebSocket<TcpStream>, NetworkError> {
    let io = |error: std::io::Error| NetworkError::Io(error.to_string());
    let authority = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .split('/')
        .next()
        .unwrap_or_default();
    let address = if authority.contains(':') {
        authority.to_owned()
    } else {
        format!("{authority}:80")
    };
    let address = address
        .to_socket_addrs()
        .map_err(io)?
        .next()
        .ok_or_else(|| NetworkError::Io(format!("cannot resolve {authority}")))?;
    let stream = TcpStream::connect_timeout(&address, TCP_TIMEOUT).map_err(io)?;
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(TCP_TIMEOUT));
    let _ = stream.set_write_timeout(Some(TCP_TIMEOUT));
    let (socket, _) =
        tungstenite::client(url, stream).map_err(|error| NetworkError::Io(error.to_string()))?;
    Ok(socket)
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

#[cfg(test)]
mod fade_and_web_tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpListener;

    fn patch(protocol: NetworkProtocol, host: &str, port: u16) -> NetworkPatch {
        NetworkPatch {
            name: "test".into(),
            host: host.into(),
            port,
            protocol,
        }
    }

    #[test]
    fn a_ramp_argument_moves_between_its_numbers() {
        let fade = Fade::parse("/fader/1 0.0>1.0 7 \"1>2\"\n/level 0>100")
            .unwrap()
            .unwrap();
        let start = fade.at(0.0);
        assert_eq!(start[0].addr, "/fader/1");
        assert_eq!(
            start[0].args,
            vec![
                OscType::Float(0.0),
                OscType::Int(7),
                OscType::String("1>2".into())
            ]
        );
        assert_eq!(fade.at(0.5)[0].args[0], OscType::Float(0.5));
        assert_eq!(fade.at(1.0)[0].args[0], OscType::Float(1.0));
        // Whole numbers on both ends stay whole; the fraction is kept inside 0..1.
        assert_eq!(fade.at(0.5)[1].args, vec![OscType::Int(50)]);
        assert_eq!(fade.at(2.0)[1].args, vec![OscType::Int(100)]);
        assert_eq!(Fade::parse("/a 1 2").unwrap(), None);
        assert_eq!(Fade::parse("/a x>y").unwrap(), None);
        assert!(Fade::parse("  ").is_err());
        assert!(Fade::parse("go 0>1").is_err());
    }

    #[test]
    fn a_fade_sends_messages_through_its_duration_and_ends_on_the_last_value() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let port = receiver.local_addr().unwrap().port();
        let sender = Sender::new(|_, _| {}).unwrap();
        let patch = patch(NetworkProtocol::OscUdp, "127.0.0.1", port);
        sender
            .send_fade(
                CueId::new(),
                &patch,
                "/fader/1 0.0>1.0",
                Duration::from_millis(300),
            )
            .unwrap();
        let mut values = Vec::new();
        let mut buffer = [0_u8; 256];
        loop {
            let (length, _) = receiver.recv_from(&mut buffer).unwrap();
            let message = mazzalab_osc::decode(&buffer[..length]).unwrap().remove(0);
            let OscType::Float(value) = message.args[0] else {
                panic!("a float")
            };
            values.push(value);
            if value >= 1.0 {
                break;
            }
        }
        // About thirty a second for 0.3 s, climbing, from nothing to full.
        assert!(values.len() >= 4, "{values:?}");
        assert_eq!(values[0], 0.0);
        assert!(
            values.windows(2).all(|pair| pair[0] <= pair[1]),
            "{values:?}"
        );
    }

    #[test]
    fn without_a_duration_the_values_go_straight_to_where_they_end() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let port = receiver.local_addr().unwrap().port();
        let sender = Sender::new(|_, _| {}).unwrap();
        let patch = patch(NetworkProtocol::OscUdp, "127.0.0.1", port);
        sender
            .send_fade(CueId::new(), &patch, "/fader/1 0.0>1.0", Duration::ZERO)
            .unwrap();
        let mut buffer = [0_u8; 256];
        let (length, _) = receiver.recv_from(&mut buffer).unwrap();
        let message = mazzalab_osc::decode(&buffer[..length]).unwrap().remove(0);
        assert_eq!(message.args, vec![OscType::Float(1.0)]);
    }

    #[test]
    fn stopping_a_cue_stops_its_fade() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_millis(400)))
            .unwrap();
        let port = receiver.local_addr().unwrap().port();
        let sender = Sender::new(|_, _| {}).unwrap();
        let patch = patch(NetworkProtocol::OscUdp, "127.0.0.1", port);
        let cue = CueId::new();
        sender
            .send_fade(cue, &patch, "/f 0.0>1.0", Duration::from_secs(30))
            .unwrap();
        let mut buffer = [0_u8; 256];
        receiver.recv_from(&mut buffer).unwrap();
        sender.stop(cue);
        // Whatever was in flight arrives; then nothing more.
        std::thread::sleep(Duration::from_millis(150));
        while receiver.recv_from(&mut buffer).is_ok() {}
        assert!(receiver.recv_from(&mut buffer).is_err());
    }

    /// A one-request HTTP server: reports the request line, content type and body.
    fn serve_once() -> (u16, mpsc::Receiver<(String, String, String)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let mut length = 0;
            let mut kind = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                let lower = line.to_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
                if let Some(value) = lower.strip_prefix("content-type:") {
                    kind = value.trim().to_owned();
                }
            }
            let mut body = vec![0_u8; length];
            reader.read_exact(&mut body).unwrap();
            let mut stream = stream;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .unwrap();
            let _ = tx.send((
                request_line.trim().to_owned(),
                kind,
                String::from_utf8(body).unwrap(),
            ));
        });
        (port, rx)
    }

    #[test]
    fn http_get_asks_for_the_path_under_the_patch_url() {
        let (port, requests) = serve_once();
        let sender = Sender::new(|_, _| {}).unwrap();
        let patch = patch(
            NetworkProtocol::HttpGet,
            &format!("127.0.0.1:{port}/api/"),
            0,
        );
        sender.send(&patch, "/scene/2?fade=1").unwrap();
        let (line, _, _) = requests.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(line, "GET /api/scene/2?fade=1 HTTP/1.1");
    }

    #[test]
    fn http_post_sends_the_rest_of_the_line_as_the_body() {
        let (port, requests) = serve_once();
        let sender = Sender::new(|_, _| {}).unwrap();
        let patch = patch(
            NetworkProtocol::HttpPost,
            &format!("http://127.0.0.1:{port}"),
            0,
        );
        sender.send(&patch, r#"/scene {"id": 2}"#).unwrap();
        let (line, kind, body) = requests.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(line, "POST /scene HTTP/1.1");
        assert_eq!(kind, "application/json");
        assert_eq!(body, r#"{"id": 2}"#);
    }

    #[test]
    fn a_failing_http_answer_is_reported() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer);
            stream
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let (errors_tx, errors) = mpsc::channel();
        let sender = Sender::new(move |_, error| {
            let _ = errors_tx.send(error.clone());
        })
        .unwrap();
        sender
            .send(
                &patch(NetworkProtocol::HttpGet, &format!("127.0.0.1:{port}"), 0),
                "/nope",
            )
            .unwrap();
        let error = errors.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            matches!(&error, NetworkError::Io(text) if text.contains("404")),
            "{error:?}"
        );
    }

    #[test]
    fn websocket_sends_each_line_as_a_text_message_on_one_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            for _ in 0..3 {
                let message = socket.read().unwrap();
                tx.send(message.into_text().unwrap().to_string()).unwrap();
            }
        });
        let sender = Sender::new(|_, _| {}).unwrap();
        let patch = patch(
            NetworkProtocol::WebSocket,
            &format!("ws://127.0.0.1:{port}/live"),
            0,
        );
        sender.send(&patch, "{\"a\":1}\nhello").unwrap();
        sender.send(&patch, "again").unwrap();
        let got: Vec<String> = (0..3)
            .map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap())
            .collect();
        assert_eq!(got, vec!["{\"a\":1}", "hello", "again"]);
    }

    #[test]
    fn urls_are_joined_and_given_a_scheme() {
        assert_eq!(join_url("http://h/api/", "/x"), "http://h/api/x");
        assert_eq!(join_url("http://h", "x?y=1"), "http://h/x?y=1");
        assert_eq!(join_url("http://h", ""), "http://h");
        assert_eq!(with_scheme("h:80", "http"), "http://h:80");
        assert_eq!(with_scheme("ws://h", "http"), "ws://h");
    }
}
