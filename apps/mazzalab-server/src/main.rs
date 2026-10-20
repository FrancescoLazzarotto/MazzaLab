//! mazzalab-server entry point
//!
//! Headless show server: runs a show file and exposes OSC and the web API.

use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use mazzalab_server::{DEFAULT_HTTP_PORT, LogOutput, Output, ServerConfig};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "mazzalab-server",
    version,
    about = "Headless MazzaLab show server"
)]
struct Args {
    /// Show file; created on first save if it does not exist.
    show: PathBuf,
    /// Address to listen on.
    #[arg(long, default_value = "0.0.0.0")]
    bind: IpAddr,
    /// OSC port (UDP and TCP).
    #[arg(long, default_value_t = mazzalab_osc::DEFAULT_PORT)]
    osc_port: u16,
    /// Port on the client's host that UDP replies are sent to.
    #[arg(long, default_value_t = mazzalab_osc::DEFAULT_REPLY_PORT)]
    udp_reply_port: u16,
    /// HTTP / WebSocket port.
    #[arg(long, default_value_t = DEFAULT_HTTP_PORT)]
    http_port: u16,
    /// Directory containing the built web UI.
    #[arg(long)]
    ui_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();

    let workspace = match mazzalab_server::load_or_create(&args.show) {
        Ok(workspace) => workspace,
        Err(error) => {
            tracing::error!(%error, "cannot open {}", args.show.display());
            return ExitCode::FAILURE;
        }
    };
    let config = ServerConfig {
        show_path: Some(args.show.clone()),
        bind: args.bind,
        osc_port: args.osc_port,
        udp_reply_port: Some(args.udp_reply_port),
        http_port: args.http_port,
        ui_dir: args.ui_dir,
    };
    let server = match mazzalab_server::start(config, workspace, |_| {
        vec![Box::new(LogOutput) as Box<dyn Output>]
    })
    .await
    {
        Ok(server) => server,
        Err(error) => {
            tracing::error!(%error, "cannot start the server");
            return ExitCode::FAILURE;
        }
    };
    tracing::info!(
        show = %args.show.display(),
        osc = %server.osc_udp,
        http = %server.http,
        "MazzaLab server running; press Ctrl+C to stop"
    );

    let _ = tokio::signal::ctrl_c().await;
    if server
        .handle
        .snapshot()
        .await
        .is_some_and(|snapshot| snapshot.dirty)
    {
        tracing::warn!("the show has unsaved changes");
    }
    server.shutdown();
    ExitCode::SUCCESS
}
