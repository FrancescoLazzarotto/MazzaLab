//! HTTP + WebSocket API for web clients, and static hosting of the web UI.

use std::path::PathBuf;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use mazzalab_show::Command;
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::actor::{ShowHandle, Update};
use crate::protocol::{ClientMessage, ServerMessage};

pub fn router(handle: ShowHandle, ui_dir: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/api/snapshot", get(snapshot))
        .route("/api/status", get(status))
        .route("/api/command", post(command))
        .route("/api/save", post(save))
        .route("/ws", get(websocket))
        .with_state(handle);
    let router = match ui_dir {
        Some(dir) => {
            api.fallback_service(ServeDir::new(dir).append_index_html_on_directories(true))
        }
        None => api.route("/", get(placeholder)),
    };
    // The UI dev server runs on another origin during development.
    router.layer(CorsLayer::permissive())
}

async fn placeholder() -> Html<&'static str> {
    Html(
        "<!doctype html><title>MazzaLab</title><h1>MazzaLab server is running</h1>\
         <p>The web UI is not being served; start the server with <code>--ui-dir</code>.</p>",
    )
}

async fn snapshot(State(handle): State<ShowHandle>) -> impl IntoResponse {
    match handle.snapshot().await {
        Some(snapshot) => {
            Json(serde_json::to_value(snapshot).unwrap_or(Value::Null)).into_response()
        }
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

async fn status(State(handle): State<ShowHandle>) -> impl IntoResponse {
    match handle.status().await {
        Some(status) => Json(serde_json::to_value(status).unwrap_or(Value::Null)).into_response(),
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn outcome(result: Result<(), String>) -> (StatusCode, Json<Value>) {
    match result {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))),
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))),
    }
}

async fn command(
    State(handle): State<ShowHandle>,
    Json(command): Json<Command>,
) -> (StatusCode, Json<Value>) {
    outcome(handle.execute(command).await)
}

async fn save(State(handle): State<ShowHandle>) -> (StatusCode, Json<Value>) {
    outcome(handle.save().await)
}

async fn websocket(ws: WebSocketUpgrade, State(handle): State<ShowHandle>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| client(socket, handle))
}

async fn client(socket: WebSocket, handle: ShowHandle) {
    let (mut sink, mut stream) = socket.split();
    let mut updates = handle.subscribe();

    let send = |message: &ServerMessage<'_>| {
        serde_json::to_string(message).map(|text| Message::Text(text.into()))
    };
    let initial = match (handle.snapshot().await, handle.status().await) {
        (Some(snapshot), Some(status)) => [
            send(&ServerMessage::Snapshot {
                snapshot: &snapshot,
            }),
            send(&ServerMessage::Status { status: &status }),
        ],
        _ => return,
    };
    for message in initial.into_iter().flatten() {
        if sink.send(message).await.is_err() {
            return;
        }
    }

    loop {
        let outgoing = tokio::select! {
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    let (id, result) = match serde_json::from_str::<ClientMessage>(text.as_str()) {
                        Ok(ClientMessage::Command { id, command }) => (id, handle.execute(*command).await),
                        Ok(ClientMessage::Save { id }) => (id, handle.save().await),
                        Err(error) => (0, Err(format!("invalid message: {error}"))),
                    };
                    send(&ServerMessage::Result { id, error: result.err() })
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => continue,
            },
            update = updates.recv() => match update {
                Ok(Update::Snapshot(snapshot)) => send(&ServerMessage::Snapshot { snapshot: &snapshot }),
                Ok(Update::Status(status)) => send(&ServerMessage::Status { status: &status }),
                Ok(Update::Events(events)) => send(&ServerMessage::Events { events: &events }),
                Ok(Update::Osc(_)) => continue,
                // Too slow to keep up: resynchronise with a fresh snapshot.
                Err(RecvError::Lagged(_)) => match handle.snapshot().await {
                    Some(snapshot) => send(&ServerMessage::Snapshot { snapshot: &snapshot }),
                    None => break,
                },
                Err(RecvError::Closed) => break,
            },
        };
        match outgoing {
            Ok(message) => {
                if sink.send(message).await.is_err() {
                    break;
                }
            }
            Err(error) => tracing::warn!(%error, "could not serialize a client message"),
        }
    }
}
