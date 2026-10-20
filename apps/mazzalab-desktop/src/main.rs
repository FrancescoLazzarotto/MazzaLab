//! mazzalab-desktop entry point
//!
//! The desktop app runs a full show server in-process (engine, audio, OSC
//! and the web API, reachable by tablets on the network) and shows the
//! operator UI in a native window pointed at that server.
//!
//! Usage: `mazzalab-desktop [show.mazzalab]`. Without a path the show is
//! `Documents/MazzaLab/Untitled.mazzalab`.

// Release builds on Windows should not open a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::error::Error;
use std::path::{Path, PathBuf};

use mazzalab_server::{AudioBackend, Completion, LogOutput, Output, RunningServer, ServerConfig};
use tauri::{App, Manager, WebviewUrl, WebviewWindowBuilder};
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let show = show_path(app)?;
            let ui_dir = ui_dir(app);
            let server = tauri::async_runtime::block_on(start_server(&show, ui_dir))?;
            tracing::info!(show = %show.display(), osc = %server.osc_udp, http = %server.http, "show server running");

            let url = format!("http://127.0.0.1:{}/", server.http.port());
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url.parse()?))
                .title(format!("MazzaLab — {}", show.display()))
                .inner_size(1440.0, 900.0)
                .min_inner_size(900.0, 600.0)
                .build()?;
            // Keeps the server alive for as long as the app runs.
            app.manage(server);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("MazzaLab failed to start");
}

fn show_path(app: &App) -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = std::env::args().skip(1).find(|arg| !arg.starts_with('-')) {
        return Ok(PathBuf::from(path));
    }
    let folder = app.path().document_dir()?.join("MazzaLab");
    std::fs::create_dir_all(&folder)?;
    Ok(folder.join("Untitled.mazzalab"))
}

/// The built web UI: bundled as a resource in installers, or straight from
/// the web project while developing.
fn ui_dir(app: &App) -> Option<PathBuf> {
    let bundled = app.path().resource_dir().ok().map(|dir| dir.join("ui"));
    let development = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mazzalab-web/dist");
    bundled
        .into_iter()
        .chain([development])
        .find(|dir| dir.join("index.html").is_file())
}

async fn start_server(
    show: &Path,
    ui_dir: Option<PathBuf>,
) -> Result<RunningServer, Box<dyn Error>> {
    let base_dir = show
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let mut workspace = mazzalab_server::load_or_create(show)?;
    for (cue, problem) in mazzalab_audio::probe_workspace(&mut workspace, &base_dir) {
        tracing::warn!(%cue, %problem, "audio file unavailable");
    }
    let outputs = move |completion: Completion| -> Vec<Box<dyn Output>> {
        let mut outputs: Vec<Box<dyn Output>> = vec![Box::new(LogOutput)];
        match AudioBackend::start(&base_dir, completion) {
            Ok(audio) => outputs.push(Box::new(audio)),
            Err(error) => tracing::warn!(%error, "continuing without audio"),
        }
        outputs
    };
    let config = ServerConfig {
        show_path: Some(show.to_path_buf()),
        ui_dir,
        ..ServerConfig::default()
    };

    // The standard ports may be taken (e.g. by a headless server); fall back
    // to free ones rather than refusing to start.
    let fallback = ServerConfig {
        osc_port: 0,
        http_port: 0,
        ..config.clone()
    };
    match mazzalab_server::start(config, workspace.clone(), outputs.clone()).await {
        Ok(server) => Ok(server),
        Err(error) => {
            tracing::warn!(%error, "standard ports unavailable; using free ports");
            Ok(mazzalab_server::start(fallback, workspace, outputs).await?)
        }
    }
}
