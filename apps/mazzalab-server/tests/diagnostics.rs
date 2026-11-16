//! The diagnostics the server reports about itself.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use mazzalab_core::Workspace;
use mazzalab_server::ServerConfig;
use serde_json::Value;

async fn diagnostics(http: std::net::SocketAddr) -> Value {
    let mut stream = tokio::net::TcpStream::connect(http).await.unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream
        .write_all(b"GET /api/diagnostics HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let body = response.split("\r\n\r\n").nth(1).unwrap_or_default();
    serde_json::from_str(body).unwrap_or_else(|_| panic!("not JSON: {response}"))
}

#[tokio::test]
async fn the_server_reports_clients_uptime_and_its_record() {
    let dir = std::env::temp_dir().join(format!("mazzalab-diagnostics-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let config = ServerConfig {
        show_path: Some(dir.join("Diag.mazzalab")),
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        autosave: None,
        announce: false,
        ..ServerConfig::default()
    };
    let server = mazzalab_server::start(config, Workspace::new("Diag"), |_| Vec::new())
        .await
        .unwrap();

    let before = diagnostics(server.http).await;
    assert_eq!(before["clients"], 0);
    assert_eq!(before["running"], 0);
    assert!(before["uptime"].as_f64().unwrap() >= 0.0);
    assert!(
        before["record"]
            .as_str()
            .is_some_and(|record| record.contains("logs") && record.ends_with(".jsonl")),
        "{before}"
    );
    // No audio backend, no audio figures.
    assert!(before.get("audio_load").is_none());

    let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", server.http))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(diagnostics(server.http).await["clients"], 1);
    drop(socket);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while diagnostics(server.http).await["clients"] != 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the client was never let go"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    server.stop().await;
    let _ = std::fs::remove_dir_all(dir);
}
