//! Passcodes: what clients may see, run and change.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use futures_util::{SinkExt, StreamExt};
use mazzalab_core::{Cue, CueId, CueKind, CueList, Passcode, WaitCue, Workspace};
use mazzalab_server::{RunningServer, ServerConfig};
use rosc::{OscMessage, OscType};
use serde_json::{Value, json};
use tokio::net::UdpSocket;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tower::ServiceExt;

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const CUE: CueId = CueId::from_u128(1);
const TIMEOUT: Duration = Duration::from_secs(5);

const VIEW: &str = "1111";
const CONTROL: &str = "2222";
const EDIT: &str = "3333";

fn passcode(code: &str, view: bool, control: bool, edit: bool) -> Passcode {
    Passcode {
        code: code.into(),
        name: String::new(),
        view,
        control,
        edit,
    }
}

/// A show with three passcodes, on a server that asks local clients too,
/// since the tests run on the server's own machine.
async fn server() -> RunningServer {
    let cue = Cue::new(CueKind::Wait(WaitCue {
        duration: Duration::from_secs(60),
    }))
    .with_id(CUE)
    .with_number("1");
    let mut workspace = Workspace::new("Locked");
    workspace.cue_lists = vec![CueList::new("Main").with_cues(vec![cue])];
    workspace.settings.passcodes = vec![
        passcode(VIEW, true, false, false),
        passcode(CONTROL, false, true, false),
        passcode(EDIT, true, true, true),
    ];
    let config = ServerConfig {
        bind: LOCALHOST,
        osc_port: 0,
        udp_reply_port: None,
        http_port: 0,
        trust_local: false,
        announce: false,
        ..ServerConfig::default()
    };
    mazzalab_server::start(config, workspace, |_| Vec::new())
        .await
        .unwrap()
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    code: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(code) = code {
        request = request.header("authorization", format!("Bearer {code}"));
    }
    let request = match body {
        Some(body) => request
            .header("content-type", "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }
    .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn web_clients_see_run_and_change_what_their_passcode_allows() {
    let server = server().await;
    let app = mazzalab_server::router(server.handle.clone(), None);

    let (status, access) = call(&app, "GET", "/api/access", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        access,
        json!({ "required": true, "view": false, "control": false, "edit": false })
    );
    let (_, access) = call(&app, "GET", "/api/access", Some(CONTROL), None).await;
    assert_eq!(
        access,
        json!({ "required": true, "view": true, "control": true, "edit": false })
    );

    assert_eq!(
        call(&app, "GET", "/api/snapshot", None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&app, "GET", "/api/snapshot", Some("0000"), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    // Viewers do not get to read the passcodes; editors do.
    let (status, snapshot) = call(&app, "GET", "/api/snapshot", Some(VIEW), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snapshot["workspace"]["settings"].get("passcodes"), None);
    let (_, snapshot) = call(&app, "GET", "/api/snapshot", Some(EDIT), None).await;
    assert_eq!(
        snapshot["workspace"]["settings"]["passcodes"]
            .as_array()
            .map(Vec::len),
        Some(3)
    );
    // The passcode may come in the query, percent-encoded.
    let (status, _) = call(&app, "GET", "/api/status?passcode=%31111", None, None).await;
    assert_eq!(status, StatusCode::OK);

    let go = json!({ "command": "go" });
    let rename = json!({ "command": "rename_workspace", "name": "Changed" });
    assert_eq!(
        call(&app, "POST", "/api/command", Some(VIEW), Some(go.clone()))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&app, "POST", "/api/command", Some(CONTROL), Some(go))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/command",
            Some(CONTROL),
            Some(rename.clone())
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&app, "POST", "/api/save", Some(CONTROL), None).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&app, "POST", "/api/command", Some(EDIT), Some(rename))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        server.handle.snapshot().await.unwrap().workspace.name,
        "Changed"
    );
    server.shutdown();
}

#[tokio::test]
async fn a_websocket_needs_a_passcode_and_keeps_to_it() {
    let server = server().await;
    let url = format!("ws://{}/ws", server.http);
    assert!(
        tokio_tungstenite::connect_async(url.clone()).await.is_err(),
        "connected without a passcode"
    );

    let (mut socket, _) = tokio_tungstenite::connect_async(format!("{url}?passcode={CONTROL}"))
        .await
        .unwrap();
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
    assert_eq!(
        first["snapshot"]["workspace"]["settings"].get("passcodes"),
        None
    );
    drop(next);

    for (id, command) in [
        (1, r#"{"command":"rename_workspace","name":"No"}"#),
        (2, r#"{"command":"go"}"#),
    ] {
        socket
            .send(Message::Text(
                format!(r#"{{"type":"command","id":{id},"command":{command}}}"#).into(),
            ))
            .await
            .unwrap();
    }
    let mut results = Vec::new();
    while results.len() < 2 {
        let message = timeout(TIMEOUT, socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(text.as_str()).unwrap();
        if value["type"] == "result" {
            results.push((value["id"].as_u64().unwrap(), value.get("error").is_some()));
        }
    }
    assert_eq!(results, [(1, true), (2, false)]);
    assert_eq!(
        server.handle.snapshot().await.unwrap().workspace.name,
        "Locked"
    );
    assert!(!server.handle.status().await.unwrap().running.is_empty());
    server.shutdown();
}

fn osc(address: &str, args: Vec<OscType>) -> Vec<u8> {
    mazzalab_osc::encode(&OscMessage {
        addr: address.into(),
        args,
    })
    .unwrap()
}

async fn reply(socket: &UdpSocket) -> Value {
    let mut buffer = [0_u8; 65_536];
    let (length, _) = timeout(TIMEOUT, socket.recv_from(&mut buffer))
        .await
        .expect("no UDP reply")
        .unwrap();
    let message = mazzalab_osc::decode(&buffer[..length]).unwrap().remove(0);
    match &message.args[..] {
        [OscType::String(json)] => serde_json::from_str(json).unwrap(),
        other => panic!("unexpected reply arguments {other:?}"),
    }
}

#[tokio::test]
async fn osc_clients_connect_with_a_passcode_as_in_qlab() {
    let server = server().await;
    let client = UdpSocket::bind((LOCALHOST, 0)).await.unwrap();
    client.connect(server.osc_udp).await.unwrap();
    let send = async |address: &str, args: Vec<OscType>| {
        client.send(&osc(address, args)).await.unwrap();
    };

    send("/go", vec![]).await;
    assert_eq!(reply(&client).await["status"], "denied");

    send("/connect", vec![OscType::String("9999".into())]).await;
    assert_eq!(reply(&client).await["data"], "badpass");

    send("/connect", vec![OscType::String(CONTROL.into())]).await;
    assert_eq!(reply(&client).await["data"], "ok:view|control");
    send("/go", vec![]).await;
    send("/cue/1/isRunning", vec![]).await;
    assert_eq!(reply(&client).await["data"], json!(true));
    send("/cue/1/name", vec![OscType::String("Renamed".into())]).await;
    assert_eq!(reply(&client).await["status"], "denied");

    send("/disconnect", vec![]).await;
    send("/cue/1/isRunning", vec![]).await;
    assert_eq!(reply(&client).await["status"], "denied");
    server.shutdown();
}
