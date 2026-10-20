use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use mazzalab_core::{Cue, CueId, CueKind, CueList, WaitCue, Workspace};
use mazzalab_osc::slip;
use mazzalab_server::{RunningServer, ServerConfig};
use rosc::{OscMessage, OscType};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

const LIST: CueId = CueId::from_u128(1000);
const CUE: CueId = CueId::from_u128(1);
const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const TIMEOUT: Duration = Duration::from_secs(5);

fn workspace() -> Workspace {
    let cue = Cue::new(CueKind::Wait(WaitCue {
        duration: Duration::from_secs(60),
    }))
    .with_id(CUE)
    .with_number("1")
    .with_name("Long wait");
    let mut workspace = Workspace::new("Server test");
    workspace.cue_lists = vec![CueList::new("Main").with_id(LIST).with_cues(vec![cue])];
    workspace
}

async fn server() -> RunningServer {
    let config = ServerConfig {
        bind: LOCALHOST,
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        ..ServerConfig::default()
    };
    mazzalab_server::start(config, workspace(), |_| Vec::new())
        .await
        .unwrap()
}

fn osc(address: &str, args: Vec<OscType>) -> Vec<u8> {
    mazzalab_osc::encode(&OscMessage {
        addr: address.into(),
        args,
    })
    .unwrap()
}

fn reply_json(message: &OscMessage) -> Value {
    match &message.args[..] {
        [OscType::String(json)] => serde_json::from_str(json).unwrap(),
        other => panic!("unexpected reply arguments {other:?}"),
    }
}

async fn udp_reply(socket: &UdpSocket) -> OscMessage {
    let mut buffer = [0_u8; 65_536];
    let (length, _) = timeout(TIMEOUT, socket.recv_from(&mut buffer))
        .await
        .expect("no UDP reply")
        .unwrap();
    mazzalab_osc::decode(&buffer[..length]).unwrap().remove(0)
}

#[tokio::test]
async fn udp_go_and_query() {
    let server = server().await;
    let client = UdpSocket::bind((LOCALHOST, 0)).await.unwrap();
    client.connect(server.osc_udp).await.unwrap();

    client.send(&osc("/go", vec![])).await.unwrap();
    client.send(&osc("/cue/1/isRunning", vec![])).await.unwrap();
    // /go is an action and gets no reply; the query does.
    let reply = udp_reply(&client).await;
    assert_eq!(reply.addr, "/reply/cue/1/isRunning");
    let body = reply_json(&reply);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["data"], json!(true));
    assert_eq!(body["workspace_id"], json!(server.handle.workspace_id()));

    client.send(&osc("/no/such/thing", vec![])).await.unwrap();
    assert_eq!(reply_json(&udp_reply(&client).await)["status"], "error");
    server.shutdown();
}

#[tokio::test]
async fn udp_update_subscription() {
    let server = server().await;
    let client = UdpSocket::bind((LOCALHOST, 0)).await.unwrap();
    client.connect(server.osc_udp).await.unwrap();

    client
        .send(&osc("/updates", vec![OscType::Int(1)]))
        .await
        .unwrap();
    client.send(&osc("/go", vec![])).await.unwrap();
    let expected = format!(
        "/update/workspace/{}/cue_id/{CUE}",
        server.handle.workspace_id()
    );
    let mut seen = Vec::new();
    while !seen.contains(&expected) {
        seen.push(udp_reply(&client).await.addr);
    }
    server.shutdown();
}

#[tokio::test]
async fn tcp_slip_framing() {
    let server = server().await;
    let mut stream = TcpStream::connect(server.osc_tcp).await.unwrap();
    stream
        .write_all(&slip::encode(&osc("/cue/1/name", vec![])))
        .await
        .unwrap();

    let mut decoder = slip::Decoder::new();
    let mut buffer = [0_u8; 4096];
    let frame = loop {
        let length = timeout(TIMEOUT, stream.read(&mut buffer))
            .await
            .expect("no TCP reply")
            .unwrap();
        if let Some(frame) = decoder.push(&buffer[..length]).pop() {
            break frame;
        }
    };
    let reply = mazzalab_osc::decode(&frame).unwrap().remove(0);
    assert_eq!(reply_json(&reply)["data"], "Long wait");
    server.shutdown();
}

#[tokio::test]
async fn http_api() {
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let server = server().await;
    let app = mazzalab_server::router(server.handle.clone(), None);

    let response = app
        .clone()
        .oneshot(
            Request::post("/api/command")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"command":"go"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(Request::get("/api/status").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(status["running"][0]["cue"], json!(CUE.to_string()));

    let response = app
        .oneshot(
            Request::post("/api/command")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"command":"start","cue":"00000000-0000-0000-0000-000000000099"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // No file path configured, so saving reports an error rather than panicking.
    assert!(server.handle.save().await.is_err());
    server.shutdown();
}

#[tokio::test]
async fn websocket_snapshot_commands_and_events() {
    let server = server().await;
    let url = format!("ws://{}/ws", server.http);
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();

    let mut next = async || -> Value {
        loop {
            let message = timeout(TIMEOUT, socket.next())
                .await
                .expect("no WebSocket message")
                .unwrap()
                .unwrap();
            if let Message::Text(text) = message {
                return serde_json::from_str(text.as_str()).unwrap();
            }
        }
    };
    let first = next().await;
    assert_eq!(first["type"], "snapshot");
    assert_eq!(first["snapshot"]["workspace"]["name"], "Server test");
    assert_eq!(next().await["type"], "status");
    drop(next);

    socket
        .send(Message::Text(
            r#"{"type":"command","id":7,"command":{"command":"go"}}"#.into(),
        ))
        .await
        .unwrap();

    let mut got_result = false;
    let mut got_started = false;
    while !(got_result && got_started) {
        let message = timeout(TIMEOUT, socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(text.as_str()).unwrap();
        match value["type"].as_str() {
            Some("result") => {
                assert_eq!(value["id"], 7);
                assert!(value.get("error").is_none());
                got_result = true;
            }
            Some("events") => {
                got_started |= value["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|event| event["kind"] == "action_started");
            }
            _ => {}
        }
    }
    server.shutdown();
}
