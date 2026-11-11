//! A rig running in real time, feeding the show's DMX outputs.

use std::collections::HashSet;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use mazzalab_core::{DmxOutput, DmxProtocol};

use crate::command::{Assignment, Slot};
use crate::protocol::{ARTNET_PORT, SACN_PORT, artnet_dmx, sacn_data, sacn_multicast};
use crate::rig::Rig;

/// How often levels are sent while they are moving.
const FRAME: Duration = Duration::from_millis(25);
/// How often they are sent when nothing moves, so receivers keep the look
/// (sACN receivers give up after 2.5 seconds of silence).
const KEEP_ALIVE: Duration = Duration::from_secs(1);
/// Priority of our sACN data; 100 is the default every source uses.
const SACN_PRIORITY: u8 = 100;

/// The slots that are not at zero, with their DMX value.
pub type Levels = Vec<(Slot, u8)>;

enum Command {
    Apply {
        assignments: Vec<Assignment>,
        fade: Duration,
    },
    Outputs(Vec<DmxOutput>),
}

type ErrorCallback = Box<dyn Fn(&DmxOutput, &str) + Send>;

/// Runs a [`Rig`] on a background thread and sends its universes to the
/// configured outputs: at 40 Hz while levels move, once a second otherwise.
#[derive(Debug)]
pub struct Sender {
    commands: mpsc::Sender<Command>,
    levels: Arc<Mutex<Levels>>,
    version: Arc<AtomicU64>,
}

impl Sender {
    /// `on_error` is called (on the sender's thread) when an output cannot
    /// be reached, once until it works again.
    pub fn new(on_error: impl Fn(&DmxOutput, &str) + Send + 'static) -> std::io::Result<Self> {
        let (commands, queue) = mpsc::channel();
        let levels: Arc<Mutex<Levels>> = Arc::default();
        let version = Arc::new(AtomicU64::new(0));
        let socket = UdpSocket::bind(("0.0.0.0", 0))?;
        // Art-Net is usually broadcast.
        socket.set_broadcast(true)?;
        let worker = Worker {
            rig: Rig::new(),
            outputs: Vec::new(),
            failed: HashSet::new(),
            sequence: 0,
            socket,
            cid: *uuid::Uuid::new_v4().as_bytes(),
            levels: levels.clone(),
            version: version.clone(),
            on_error: Box::new(on_error),
        };
        std::thread::Builder::new()
            .name("mazzalab-lighting".into())
            .spawn(move || worker.run(&queue))?;
        Ok(Self {
            commands,
            levels,
            version,
        })
    }

    /// Sends slots to new levels over `fade`, starting now.
    pub fn apply(&self, assignments: Vec<Assignment>, fade: Duration) {
        let _ = self.commands.send(Command::Apply { assignments, fade });
    }

    /// Replaces the outputs the universes are sent to.
    pub fn set_outputs(&self, outputs: Vec<DmxOutput>) {
        let _ = self.commands.send(Command::Outputs(outputs));
    }

    /// A number that changes whenever the levels do, to poll cheaply.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    /// The slots that are lit right now.
    pub fn levels(&self) -> Levels {
        self.levels
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

struct Worker {
    rig: Rig,
    outputs: Vec<DmxOutput>,
    /// Outputs whose last send failed, by position, to report each once.
    failed: HashSet<usize>,
    sequence: u8,
    socket: UdpSocket,
    cid: [u8; 16],
    levels: Arc<Mutex<Levels>>,
    version: Arc<AtomicU64>,
    on_error: ErrorCallback,
}

/// Where an output's packets go: the address it names (with an optional
/// port), else broadcast for Art-Net and the universe's group for sACN.
fn destination(output: &DmxOutput) -> Result<SocketAddr, String> {
    let port = match output.protocol {
        DmxProtocol::ArtNet => ARTNET_PORT,
        DmxProtocol::Sacn => SACN_PORT,
    };
    let host = output.host.trim();
    if host.is_empty() {
        return Ok(match output.protocol {
            DmxProtocol::ArtNet => SocketAddr::from(([255, 255, 255, 255], port)),
            DmxProtocol::Sacn => SocketAddr::from((sacn_multicast(output.net_universe), port)),
        });
    }
    let resolved = if host.contains(':') {
        host.to_socket_addrs()
    } else {
        (host, port).to_socket_addrs()
    };
    resolved
        .map_err(|error| error.to_string())?
        .find(SocketAddr::is_ipv4)
        .ok_or_else(|| format!("cannot resolve {host}"))
}

impl Worker {
    fn run(mut self, queue: &mpsc::Receiver<Command>) {
        let start = Instant::now();
        let mut last_sent = Instant::now();
        loop {
            let wait = if self.rig.is_fading() {
                FRAME
            } else {
                KEEP_ALIVE.saturating_sub(last_sent.elapsed())
            };
            let mut changed = false;
            match queue.recv_timeout(wait) {
                Ok(command) => {
                    changed |= self.handle(command, start.elapsed());
                    // Take everything that is waiting, so a burst of cues
                    // goes out as one frame.
                    while let Ok(command) = queue.try_recv() {
                        changed |= self.handle(command, start.elapsed());
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
            let moved = self.rig.advance(start.elapsed());
            if changed || moved {
                *self.levels.lock().unwrap_or_else(PoisonError::into_inner) = self.rig.lit();
                self.version.fetch_add(1, Ordering::Relaxed);
            }
            if changed || moved || last_sent.elapsed() >= KEEP_ALIVE {
                self.send();
                last_sent = Instant::now();
            }
        }
    }

    /// Returns whether the output has to be refreshed right away.
    fn handle(&mut self, command: Command, now: Duration) -> bool {
        match command {
            Command::Apply { assignments, fade } => self.rig.apply(&assignments, fade, now),
            Command::Outputs(outputs) => {
                self.outputs = outputs;
                self.failed.clear();
            }
        }
        true
    }

    fn send(&mut self) {
        self.sequence = self.sequence.wrapping_add(1);
        for (index, output) in self.outputs.iter().enumerate() {
            let frame = self.rig.frame(output.universe);
            let packet = match output.protocol {
                DmxProtocol::ArtNet => artnet_dmx(output.net_universe, self.sequence, &frame),
                DmxProtocol::Sacn => sacn_data(
                    self.cid,
                    "MazzaLab",
                    output.net_universe,
                    self.sequence,
                    SACN_PRIORITY,
                    &frame,
                ),
            };
            let sent = destination(output).and_then(|address| {
                self.socket
                    .send_to(&packet, address)
                    .map_err(|error| error.to_string())
            });
            match sent {
                Ok(_) => {
                    self.failed.remove(&index);
                }
                Err(error) => {
                    if self.failed.insert(index) {
                        tracing::warn!(universe = output.universe, %error, "DMX output failed");
                        (self.on_error)(output, &error);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_command;

    fn output(protocol: DmxProtocol, universe: u16, net_universe: u16, port: u16) -> DmxOutput {
        DmxOutput {
            universe,
            protocol,
            host: format!("127.0.0.1:{port}"),
            net_universe,
        }
    }

    fn receiver() -> (UdpSocket, u16) {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let port = socket.local_addr().unwrap().port();
        (socket, port)
    }

    /// Reads packets until one satisfies `done`.
    fn wait_for(socket: &UdpSocket, done: impl Fn(&[u8]) -> bool) -> Vec<u8> {
        let mut buffer = [0_u8; 1024];
        for _ in 0..400 {
            let (length, _) = socket.recv_from(&mut buffer).expect("no DMX packet");
            if done(&buffer[..length]) {
                return buffer[..length].to_vec();
            }
        }
        panic!("the expected DMX packet never came");
    }

    #[test]
    fn destinations() {
        let artnet = |host: &str| DmxOutput {
            universe: 1,
            protocol: DmxProtocol::ArtNet,
            host: host.into(),
            net_universe: 0,
        };
        assert_eq!(
            destination(&artnet("")).unwrap().to_string(),
            "255.255.255.255:6454"
        );
        assert_eq!(
            destination(&artnet("10.0.0.5")).unwrap().to_string(),
            "10.0.0.5:6454"
        );
        assert_eq!(
            destination(&artnet(" 10.0.0.5:7000 ")).unwrap().to_string(),
            "10.0.0.5:7000"
        );
        let sacn = DmxOutput {
            universe: 1,
            protocol: DmxProtocol::Sacn,
            host: String::new(),
            net_universe: 3,
        };
        assert_eq!(destination(&sacn).unwrap().to_string(), "239.255.0.3:5568");
    }

    #[test]
    fn looks_reach_artnet_and_sacn_outputs_and_fade() {
        let (artnet, artnet_port) = receiver();
        let (sacn, sacn_port) = receiver();
        let sender = Sender::new(|_, _| {}).unwrap();
        sender.set_outputs(vec![
            output(DmxProtocol::ArtNet, 1, 0, artnet_port),
            output(DmxProtocol::Sacn, 2, 7, sacn_port),
        ]);
        sender.apply(
            vec![
                parse_command("1 thru 2 @ full").unwrap(),
                parse_command("2/5 @ 50").unwrap(),
            ],
            Duration::ZERO,
        );

        let packet = wait_for(&artnet, |packet| packet[18] == 255);
        assert_eq!(&packet[..8], b"Art-Net\0");
        assert_eq!(packet[14..16], [0, 0]);
        assert_eq!(packet[18..21], [255, 255, 0]);

        let packet = wait_for(&sacn, |packet| packet[126 + 4] == 128);
        assert_eq!(packet[113..115], [0, 7]);
        assert_eq!(packet[126], 0, "universe 2 has nothing on its first slot");

        // A fade passes through the middle on its way down.
        sender.apply(
            vec![parse_command("1 @ out").unwrap()],
            Duration::from_millis(400),
        );
        wait_for(&artnet, |packet| (60..200).contains(&packet[18]));
        let packet = wait_for(&artnet, |packet| packet[18] == 0);
        assert_eq!(packet[19], 255, "the other slot stays where it was");
        // Sequence numbers move on.
        let next = wait_for(&artnet, |_| true);
        assert_ne!(next[12], packet[12]);

        assert!(sender.version() > 0);
        let lit = sender.levels();
        assert_eq!(lit.len(), 2);
        assert_eq!((lit[0].0.address, lit[0].1), (2, 255));
        assert_eq!((lit[1].0.universe, lit[1].0.address, lit[1].1), (2, 5, 128));
    }

    #[test]
    fn an_unreachable_output_is_reported_once() {
        let (errors_tx, errors) = mpsc::channel();
        let sender = Sender::new(move |output, error| {
            let _ = errors_tx.send((output.universe, error.to_owned()));
        })
        .unwrap();
        sender.set_outputs(vec![DmxOutput {
            universe: 4,
            protocol: DmxProtocol::ArtNet,
            host: "no such host.invalid".into(),
            net_universe: 0,
        }]);
        sender.apply(vec![parse_command("4/1 @ full").unwrap()], Duration::ZERO);
        let (universe, _) = errors.recv_timeout(Duration::from_secs(30)).unwrap();
        assert_eq!(universe, 4);
        // The keep-alive that follows does not repeat the complaint.
        assert!(errors.recv_timeout(Duration::from_millis(1500)).is_err());
    }
}
