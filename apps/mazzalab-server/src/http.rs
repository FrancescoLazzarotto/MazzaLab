//! HTTP + WebSocket API for web clients, and static hosting of the web UI.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use mazzalab_core::CueId;
use mazzalab_show::Command;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast::error::RecvError;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::actor::{NoticeLevel, ShowHandle, Update};
use crate::protocol::{ClientMessage, ServerMessage};

/// Folder, next to the show file, that uploaded media is stored in.
pub const MEDIA_FOLDER: &str = "media";
/// Waveforms kept in memory, most recently computed last.
const WAVEFORM_CACHE: usize = 64;

type WaveformKey = (PathBuf, Option<SystemTime>, usize);
type WaveformCache = Vec<(WaveformKey, Arc<Value>)>;

#[derive(Clone)]
struct AppState {
    handle: ShowHandle,
    waveforms: Arc<Mutex<WaveformCache>>,
}

pub fn router(handle: ShowHandle, ui_dir: Option<PathBuf>) -> Router {
    let state = AppState {
        handle,
        waveforms: Arc::default(),
    };
    let api = Router::new()
        .route("/api/snapshot", get(snapshot))
        .route("/api/status", get(status))
        .route("/api/command", post(command))
        .route("/api/save", post(save))
        .route("/api/audio", get(audio))
        .route("/api/midi", get(midi))
        .route("/api/midi/in", post(midi_in))
        .route("/api/waveform", get(waveform))
        .route(
            "/api/media",
            get(list_media)
                .post(upload_media)
                // Media files are large and streamed straight to disk.
                .layer(DefaultBodyLimit::disable()),
        )
        .route("/ws", get(websocket))
        .with_state(state);
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

async fn snapshot(State(state): State<AppState>) -> impl IntoResponse {
    match state.handle.snapshot().await {
        Some(snapshot) => {
            Json(serde_json::to_value(snapshot).unwrap_or(Value::Null)).into_response()
        }
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

async fn status(State(state): State<AppState>) -> impl IntoResponse {
    match state.handle.status().await {
        Some(status) => Json(serde_json::to_value(status).unwrap_or(Value::Null)).into_response(),
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn outcome(result: Result<(), String>) -> (StatusCode, Json<Value>) {
    match result {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))),
        Err(error) => failure(StatusCode::BAD_REQUEST, error),
    }
}

fn failure(status: StatusCode, error: impl Into<String>) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": error.into() })))
}

async fn command(
    State(state): State<AppState>,
    Json(command): Json<Command>,
) -> (StatusCode, Json<Value>) {
    outcome(state.handle.execute(command).await)
}

async fn save(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    outcome(state.handle.save().await)
}

/// The audio output in use and the devices the show could use instead.
async fn audio(State(state): State<AppState>) -> Json<Value> {
    let status = state.handle.audio_status().await;
    // Asking the drivers can take a moment: keep it off the async threads.
    let devices = tokio::task::spawn_blocking(mazzalab_audio::output_devices)
        .await
        .unwrap_or_default();
    let devices: Vec<Value> = devices
        .into_iter()
        .map(|device| {
            json!({
                "id": device.id,
                "name": device.name,
                "host": device.host,
                "is_default": device.is_default,
                "max_channels": device.max_channels,
                "sample_rates": device.sample_rates,
                "default_sample_rate": device.default_sample_rate,
            })
        })
        .collect();
    Json(json!({ "status": status, "devices": devices }))
}

/// The MIDI ports on this machine.
async fn midi() -> Json<Value> {
    // Asking the drivers can take a moment: keep it off the async threads.
    let ports = tokio::task::spawn_blocking(|| {
        (mazzalab_midi::output_ports(), mazzalab_midi::input_ports())
    })
    .await
    .unwrap_or_default();
    Json(json!({ "outputs": ports.0, "inputs": ports.1 }))
}

/// Feeds the show a MIDI message as if it had arrived on an input, to try
/// triggers out without the controller.
async fn midi_in(
    State(state): State<AppState>,
    Json(event): Json<mazzalab_midi::MidiEvent>,
) -> (StatusCode, Json<Value>) {
    state.handle.midi_in(event).await;
    outcome(Ok(()))
}

#[derive(Deserialize)]
struct WaveformQuery {
    cue: CueId,
    #[serde(default)]
    buckets: Option<usize>,
}

/// The outline of an audio cue's file: `peaks` holds a minimum and a maximum
/// (from -127 to 127) for each slice of the file.
async fn waveform(State(state): State<AppState>, Query(query): Query<WaveformQuery>) -> Response {
    let Some(path) = state.handle.cue_file(query.cue).await else {
        return failure(StatusCode::NOT_FOUND, "the cue has no media file").into_response();
    };
    let buckets = query.buckets.unwrap_or(1200).clamp(16, 8192);
    let modified = std::fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .ok();
    let key: WaveformKey = (path.clone(), modified, buckets);
    let cached = lock(&state.waveforms)
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, value)| value.clone());
    if let Some(value) = cached {
        return Json(value.as_ref().clone()).into_response();
    }

    let computed =
        tokio::task::spawn_blocking(move || mazzalab_audio::waveform(&path, buckets)).await;
    match computed {
        Ok(Ok(waveform)) => {
            let peaks: Vec<i8> = waveform
                .peaks
                .iter()
                .map(|peak| (peak * 127.0).round() as i8)
                .collect();
            let value = Arc::new(json!({
                "duration": waveform.duration,
                "channels": waveform.channels,
                "sample_rate": waveform.sample_rate,
                "peaks": peaks,
            }));
            let mut cache = lock(&state.waveforms);
            cache.retain(|(candidate, _)| *candidate != key);
            cache.push((key, value.clone()));
            if cache.len() > WAVEFORM_CACHE {
                cache.remove(0);
            }
            drop(cache);
            Json(value.as_ref().clone()).into_response()
        }
        Ok(Err(error)) => {
            failure(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()).into_response()
        }
        Err(error) => failure(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The show's media folder, which only exists for a show with a file.
fn media_dir(handle: &ShowHandle) -> Result<PathBuf, (StatusCode, Json<Value>)> {
    handle
        .base_dir()
        .map(|base| base.join(MEDIA_FOLDER))
        .ok_or_else(|| {
            failure(
                StatusCode::CONFLICT,
                "save the show first: media is stored next to the show file",
            )
        })
}

/// The files in the show's media folder, as paths relative to the show.
async fn list_media(State(state): State<AppState>) -> Response {
    let dir = match media_dir(&state.handle) {
        Ok(dir) => dir,
        Err(error) => return error.into_response(),
    };
    let mut files = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            if meta.is_file() && !name.starts_with('.') {
                files.push((name, meta.len()));
            }
        }
    }
    files.sort();
    let files: Vec<Value> = files
        .into_iter()
        .map(|(name, size)| json!({ "file": format!("{MEDIA_FOLDER}/{name}"), "name": name, "size": size }))
        .collect();
    Json(json!({ "files": files })).into_response()
}

/// Keeps the last component of an uploaded name and removes what file
/// systems refuse. `None` when nothing usable is left.
pub fn safe_file_name(name: &str) -> Option<String> {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = last
        .chars()
        .map(|c| match c {
            ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim().to_owned();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// `name.ext`, `name (2).ext`, `name (3).ext`...
fn numbered(name: &str, n: u32) -> String {
    if n <= 1 {
        return name.to_owned();
    }
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => format!("{stem} ({n}).{extension}"),
        _ => format!("{name} ({n})"),
    }
}

/// Whether two files hold the same bytes.
fn same_contents(a: &Path, b: &Path) -> std::io::Result<bool> {
    use std::io::Read;
    if std::fs::metadata(a)?.len() != std::fs::metadata(b)?.len() {
        return Ok(false);
    }
    let mut a = std::io::BufReader::new(std::fs::File::open(a)?);
    let mut b = std::io::BufReader::new(std::fs::File::open(b)?);
    let mut block_a = vec![0_u8; 1 << 16];
    let mut block_b = vec![0_u8; 1 << 16];
    loop {
        let read = a.read(&mut block_a)?;
        if read == 0 {
            return Ok(true);
        }
        b.read_exact(&mut block_b[..read])?;
        if block_a[..read] != block_b[..read] {
            return Ok(false);
        }
    }
}

/// Moves a finished upload into place. A file already there with the same
/// contents is reused; a different one keeps its name and the upload gets a
/// numbered one. Returns the final file name.
fn place_upload(dir: &Path, temp: &Path, name: &str) -> std::io::Result<String> {
    for n in 1.. {
        let candidate = numbered(name, n);
        let target = dir.join(&candidate);
        if !target.exists() {
            std::fs::rename(temp, &target)?;
            return Ok(candidate);
        }
        if same_contents(temp, &target)? {
            std::fs::remove_file(temp)?;
            return Ok(candidate);
        }
    }
    unreachable!("the loop only ends by returning")
}

#[derive(Deserialize)]
struct UploadQuery {
    name: String,
}

/// Stores the request body as a file in the show's media folder and answers
/// with its path relative to the show, ready to be used as a cue's file.
/// This is how browsers and tablets, which cannot name files on the server's
/// disk, bring media into a show.
async fn upload_media(
    State(state): State<AppState>,
    Query(query): Query<UploadQuery>,
    body: Body,
) -> (StatusCode, Json<Value>) {
    let dir = match media_dir(&state.handle) {
        Ok(dir) => dir,
        Err(error) => return error,
    };
    let Some(name) = safe_file_name(&query.name) else {
        return failure(StatusCode::BAD_REQUEST, "invalid file name");
    };
    let server_error =
        |error: std::io::Error| failure(StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
    if let Err(error) = tokio::fs::create_dir_all(&dir).await {
        return server_error(error);
    }

    static UPLOADS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let serial = UPLOADS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temp = dir.join(format!(".upload-{}-{serial}.part", std::process::id()));
    let written = async {
        let mut file = tokio::fs::File::create(&temp).await?;
        let mut stream = body.into_data_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(std::io::Error::other)?;
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        file.sync_all().await
    }
    .await;
    if let Err(error) = written {
        let _ = tokio::fs::remove_file(&temp).await;
        return server_error(error);
    }

    let placed = {
        let (dir, temp, name) = (dir.clone(), temp.clone(), name.clone());
        tokio::task::spawn_blocking(move || place_upload(&dir, &temp, &name)).await
    };
    match placed {
        Ok(Ok(name)) => (
            StatusCode::OK,
            Json(json!({ "file": format!("{MEDIA_FOLDER}/{name}"), "name": name })),
        ),
        Ok(Err(error)) => {
            let _ = tokio::fs::remove_file(&temp).await;
            state
                .handle
                .notify(NoticeLevel::Error, format!("{name}: {error}"));
            server_error(error)
        }
        Err(error) => failure(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

async fn websocket(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| client(socket, state.handle))
}

async fn client(socket: WebSocket, handle: ShowHandle) {
    let (mut sink, mut stream) = socket.split();
    let mut updates = handle.subscribe();

    let send = |message: &ServerMessage<'_>| {
        serde_json::to_string(message).map(|text| Message::Text(text.into()))
    };
    let audio = handle.audio_status().await;
    let initial = match (handle.snapshot().await, handle.status().await) {
        (Some(snapshot), Some(status)) => vec![
            send(&ServerMessage::Snapshot {
                snapshot: &snapshot,
            }),
            send(&ServerMessage::Status { status: &status }),
        ],
        _ => return,
    };
    let audio = audio
        .as_ref()
        .map(|audio| send(&ServerMessage::Audio { audio }));
    let dmx = handle.dmx_levels().await;
    let dmx = dmx
        .as_ref()
        .map(|levels| send(&ServerMessage::Dmx { levels }));
    for message in initial.into_iter().chain(audio).chain(dmx).flatten() {
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
                        Ok(ClientMessage::Audition { id, cue }) => (id, handle.audition(cue).await),
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
                Ok(Update::Meters(peaks)) => send(&ServerMessage::Meters { peaks: &peaks }),
                Ok(Update::Audio(audio)) => send(&ServerMessage::Audio { audio: &audio }),
                Ok(Update::Notice(notice)) => send(&ServerMessage::Notice { notice: &notice }),
                Ok(Update::Dmx(levels)) => send(&ServerMessage::Dmx { levels: &levels }),
                Ok(Update::MidiIn(event)) => send(&ServerMessage::MidiIn { event }),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_names_are_made_safe() {
        assert_eq!(
            safe_file_name("thunder.wav").as_deref(),
            Some("thunder.wav")
        );
        assert_eq!(
            safe_file_name(r"C:\Users\me\Music\rain 01.wav").as_deref(),
            Some("rain 01.wav")
        );
        assert_eq!(
            safe_file_name("../../etc/passwd").as_deref(),
            Some("passwd")
        );
        assert_eq!(safe_file_name("a:b*c?.mp3").as_deref(), Some("a_b_c_.mp3"));
        assert_eq!(safe_file_name(".hidden").as_deref(), Some("hidden"));
        assert_eq!(
            safe_file_name("città è.flac").as_deref(),
            Some("città è.flac")
        );
        assert_eq!(safe_file_name(".."), None);
        assert_eq!(safe_file_name("dir/"), None);
        assert_eq!(safe_file_name("   "), None);
    }

    #[test]
    fn numbered_names_keep_the_extension() {
        assert_eq!(numbered("a.wav", 1), "a.wav");
        assert_eq!(numbered("a.wav", 2), "a (2).wav");
        assert_eq!(numbered("a.b.wav", 3), "a.b (3).wav");
        assert_eq!(numbered("noext", 2), "noext (2)");
    }
}
