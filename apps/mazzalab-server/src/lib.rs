//! mazzalab-server
//!
//! Runs a show in real time and exposes it to the outside world:
//!
//! * OSC on UDP and TCP (QLab-compatible addresses, default port 53000);
//! * an HTTP + WebSocket API for the web UI (default port 53080), which can
//!   also serve the built UI;
//! * pluggable [`Output`]s fed with engine events: [`AudioBackend`] plays
//!   audio cues, [`NetworkBackend`] and [`MidiBackend`] send the messages
//!   of Network and MIDI cues, [`LightingBackend`] runs Light cues and
//!   sends DMX.
//!
//! The desktop app embeds this crate; `mazzalab-server` runs it headless.

mod access;
mod actor;
mod announce;
mod follow;
mod http;
mod media_manage;
mod osc_net;
mod outputs;
pub mod protocol;
mod screens;
mod wall_clock;

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use mazzalab_core::Workspace;
use mazzalab_show::Show;
use tokio::net::{TcpListener, UdpSocket};
use tokio::task::JoinHandle;

pub use access::{Access, Need};
pub use actor::{
    AudioStatus, ClientGuard, Completion, Diagnostics, DmxLevel, Notice, NoticeLevel, Output,
    ShowHandle, TimecodeIn, Update,
};
pub use http::{MEDIA_FOLDER, router, router_with_screens};
pub use outputs::{
    AudioBackend, LightingBackend, LogOutput, LtcBackend, MidiBackend, NetworkBackend, StageBackend,
};
pub use screens::{Screen, Screens, ScreensHandle};

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
    /// Unsaved changes are written next to the show file this long after
    /// they are made; `None` turns autosave off.
    pub autosave: Option<Duration>,
    /// How many replaced versions of the show file are kept on save.
    pub backups: usize,
    /// The workspace comes from an autosave ([`recoverable`]): the show
    /// starts out with unsaved changes.
    pub recovered: bool,
    /// The machine's displays, to put stages on full screen; `None` on a
    /// server without any.
    pub screens: Option<ScreensHandle>,
    /// Clients on this machine need no passcode. Off when something on
    /// the machine relays clients from elsewhere, such as a proxy.
    pub trust_local: bool,
    /// Announce the server on the local network (mDNS) so tablets and
    /// control surfaces find it. Off in tests.
    pub announce: bool,
    /// Every input is recorded in `logs/` next to the show file, to replay
    /// a performance with `mazzalab replay`.
    pub record_inputs: bool,
    /// Follow the server at this address as its hot backup, e.g.
    /// `http://10.0.0.2:53080` (with `?passcode=` if it asks for one), and
    /// take over when it goes silent.
    pub follow: Option<String>,
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
            autosave: Some(Duration::from_secs(15)),
            backups: 10,
            recovered: false,
            screens: None,
            trust_local: true,
            announce: true,
            record_inputs: true,
            follow: None,
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

    /// Like [`shutdown`](Self::shutdown), but waits until every task has
    /// ended, so the ports are free again when it returns.
    pub async fn stop(self) {
        for task in &self.tasks {
            task.abort();
        }
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

/// Loads a show file, or creates an empty show when the file does not exist.
pub fn load_or_create(path: &Path) -> Result<Workspace, String> {
    if path.exists() {
        mazzalab_project::load(path).map_err(|error| error.to_string())
    } else {
        Ok(Workspace::new(show_name(path)))
    }
}

/// The unsaved changes an earlier session left behind for this show, if
/// any: start the server on them with [`ServerConfig::recovered`] set.
pub fn recoverable(path: &Path) -> Option<Workspace> {
    mazzalab_project::recoverable(path)
}

/// Reads a show to run it: the unsaved changes an earlier session left
/// behind (a crash, a power cut) if there are any, the file otherwise, or
/// an empty show when the file does not exist yet. Says whether what it
/// read is such an autosave. The lengths of audio files are filled in.
pub fn read_show(path: &Path) -> Result<(Workspace, bool), String> {
    let (mut workspace, recovered) = match recoverable(path) {
        Some(autosaved) => {
            tracing::warn!(show = %path.display(), "recovered unsaved changes from an autosave");
            (autosaved, true)
        }
        None => (load_or_create(path)?, false),
    };
    let base = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    for (cue, problem) in mazzalab_audio::probe_workspace(&mut workspace, base) {
        tracing::warn!(%cue, %problem, "audio file unavailable");
    }
    Ok((workspace, recovered))
}

/// The name of the show in `path`: its file name without the extension.
pub fn show_name(path: &Path) -> String {
    path.file_stem().map_or_else(
        || "Untitled".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    )
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

    let show = if config.recovered {
        Show::recovered(workspace)
    } else {
        Show::new(workspace)
    };
    let persistence = actor::Persistence {
        path: config.show_path,
        autosave: config.autosave,
        backups: config.backups,
        record: config.record_inputs,
    };
    let (handle, actor) = actor::spawn(show, persistence, make_outputs);
    handle.set_trust_local(config.trust_local);
    let app = router_with_screens(handle.clone(), config.ui_dir, config.screens);
    let mut tasks = vec![
        actor,
        tokio::spawn(osc_net::serve_udp(
            udp,
            handle.clone(),
            config.udp_reply_port,
        )),
        tokio::spawn(osc_net::serve_tcp(tcp, handle.clone())),
        tokio::spawn(async move {
            // Clients on the show's own machine need no passcode.
            let app = app.into_make_service_with_connect_info::<SocketAddr>();
            if let Err(error) = axum::serve(http_listener, app).await {
                tracing::error!(%error, "HTTP server stopped");
            }
        }),
    ];
    if config.announce {
        match announce::announce(config.bind, http.port(), osc_udp.port()) {
            // Held until the server stops; dropping it says goodbye on the network.
            Ok(announcement) => tasks.push(tokio::spawn(async move {
                let _announcement = announcement;
                std::future::pending::<()>().await;
            })),
            Err(error) => tracing::warn!(%error, "could not announce the server on the network"),
        }
    }
    if let Some(main) = config.follow {
        tasks.push(tokio::spawn(follow::follow(main, handle.clone())));
    }
    Ok(RunningServer {
        handle,
        osc_udp,
        osc_tcp,
        http,
        tasks,
    })
}
