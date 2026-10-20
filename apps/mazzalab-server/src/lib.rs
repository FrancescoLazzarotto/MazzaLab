//! mazzalab-server
//!
//! Runs a show in real time and exposes it to the outside world:
//!
//! * OSC on UDP and TCP (QLab-compatible addresses, default port 53000);
//! * an HTTP + WebSocket API for the web UI (default port 53080), which can
//!   also serve the built UI;
//! * pluggable [`Output`]s (audio, lighting, ...) fed with engine events.
//!
//! The desktop app embeds this crate; `mazzalab-server` runs it headless.

mod actor;
mod http;
mod osc_net;
pub mod protocol;

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use mazzalab_core::Workspace;
use mazzalab_engine::{Event, EventKind};
use mazzalab_show::Show;
use tokio::net::{TcpListener, UdpSocket};
use tokio::task::JoinHandle;

pub use actor::{Completion, Output, ShowHandle, Update};
pub use http::router;

pub const DEFAULT_HTTP_PORT: u16 = 53080;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Show file used by "save".
    pub show_path: Option<PathBuf>,
    pub bind: IpAddr,
    /// OSC port, UDP and TCP; 0 picks a free port.
    pub osc_port: u16,
    /// Where UDP replies go on the sender's host; `None` replies to the
    /// sender's own port.
    pub udp_reply_port: Option<u16>,
    /// HTTP/WebSocket port; 0 picks a free port.
    pub http_port: u16,
    /// Directory with the built web UI to serve.
    pub ui_dir: Option<PathBuf>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            show_path: None,
            bind: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            osc_port: mazzalab_osc::DEFAULT_PORT,
            udp_reply_port: Some(mazzalab_osc::DEFAULT_REPLY_PORT),
            http_port: DEFAULT_HTTP_PORT,
            ui_dir: None,
        }
    }
}

/// A started server. Dropping it does not stop it; call [`shutdown`](Self::shutdown).
#[derive(Debug)]
pub struct RunningServer {
    pub handle: ShowHandle,
    pub osc_udp: SocketAddr,
    pub osc_tcp: SocketAddr,
    pub http: SocketAddr,
    tasks: Vec<JoinHandle<()>>,
}

impl RunningServer {
    pub fn shutdown(self) {
        for task in self.tasks {
            task.abort();
        }
    }
}

/// Loads a show file, or creates an empty show when the file does not exist.
pub fn load_or_create(path: &Path) -> Result<Workspace, String> {
    if path.exists() {
        mazzalab_project::load(path).map_err(|error| error.to_string())
    } else {
        let name = path.file_stem().map_or_else(
            || "Untitled".to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        Ok(Workspace::new(name))
    }
}

/// Binds every socket and starts serving `workspace`.
pub async fn start(
    config: ServerConfig,
    workspace: Workspace,
    make_outputs: impl FnOnce(Completion) -> Vec<Box<dyn Output>>,
) -> io::Result<RunningServer> {
    let udp = UdpSocket::bind((config.bind, config.osc_port)).await?;
    let osc_udp = udp.local_addr()?;
    let tcp = TcpListener::bind((config.bind, osc_udp.port())).await?;
    let osc_tcp = tcp.local_addr()?;
    let http_listener = TcpListener::bind((config.bind, config.http_port)).await?;
    let http = http_listener.local_addr()?;

    let (handle, actor) = actor::spawn(Show::new(workspace), config.show_path, make_outputs);
    let app = router(handle.clone(), config.ui_dir);
    let tasks = vec![
        actor,
        tokio::spawn(osc_net::serve_udp(
            udp,
            handle.clone(),
            config.udp_reply_port,
        )),
        tokio::spawn(osc_net::serve_tcp(tcp, handle.clone())),
        tokio::spawn(async move {
            if let Err(error) = axum::serve(http_listener, app).await {
                tracing::error!(%error, "HTTP server stopped");
            }
        }),
    ];
    Ok(RunningServer {
        handle,
        osc_udp,
        osc_tcp,
        http,
        tasks,
    })
}

/// Logs every engine event; useful headless and while developing backends.
#[derive(Debug, Default)]
pub struct LogOutput;

impl Output for LogOutput {
    fn handle(&mut self, show: &Show, events: &[Event]) {
        for event in events {
            let label = |cue| {
                show.workspace().find_cue(cue).map_or_else(
                    || cue.to_string(),
                    |cue| {
                        format!(
                            "{} {}",
                            cue.number.as_deref().unwrap_or("-"),
                            cue.display_name()
                        )
                    },
                )
            };
            let at = event.at.as_secs_f64();
            match event.kind {
                EventKind::ActionStarted { cue, armed } => {
                    tracing::info!(at, armed, "action {}", label(cue));
                }
                EventKind::ActionCompleted(cue) => tracing::info!(at, "done {}", label(cue)),
                EventKind::Stopped(cue) => tracing::info!(at, "stop {}", label(cue)),
                EventKind::Panicking { cue, .. } => tracing::info!(at, "panic {}", label(cue)),
                _ => tracing::debug!(at, ?event.kind),
            }
        }
    }
}

/// Plays audio cues on the default output device.
#[derive(Debug)]
pub struct AudioBackend(pub mazzalab_audio::AudioOutput);

impl AudioBackend {
    /// Starts audio output; finished sounds are reported through `completion`.
    pub fn start(
        base_dir: &Path,
        completion: Completion,
    ) -> Result<Self, mazzalab_audio::AudioError> {
        mazzalab_audio::AudioOutput::start(base_dir, move |cue| completion.action_finished(cue))
            .map(Self)
    }
}

impl Output for AudioBackend {
    fn handle(&mut self, show: &Show, events: &[Event]) {
        self.0.handle_events(show.workspace(), events);
    }
}
