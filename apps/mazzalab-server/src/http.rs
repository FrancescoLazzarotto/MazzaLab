//! HTTP + WebSocket API for web clients, and static hosting of the web UI.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, DefaultBodyLimit, Query, Request, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use futures_util::{SinkExt, StreamExt};
use mazzalab_core::CueId;
use mazzalab_show::{Command, Snapshot};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast::error::RecvError;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::access::{Access, Need, command_need};
use crate::actor::{NoticeLevel, ShowHandle, Update};
use crate::media_manage;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::screens::ScreensHandle;

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
    screens: Option<ScreensHandle>,
}

pub fn router(handle: ShowHandle, ui_dir: Option<PathBuf>) -> Router {
    router_with_screens(handle, ui_dir, None)
}

/// The API, also offering the machine's displays to put stages on.
pub fn router_with_screens(
    handle: ShowHandle,
    ui_dir: Option<PathBuf>,
    screens: Option<ScreensHandle>,
) -> Router {
    let state = AppState {
        handle,
        waveforms: Arc::default(),
        screens,
    };
    let api = Router::new()
        .route("/api/access", get(access))
        .route("/api/snapshot", get(snapshot))
        .route("/api/status", get(status))
        .route("/api/diagnostics", get(diagnostics))
        .route("/api/command", post(command))
        .route("/api/save", post(save))
        .route("/api/show", get(show_file))
        .route("/api/show/files", get(show_files))
        .route("/api/show/open", post(open_show))
        .route("/api/show/new", post(new_show))
        .route("/api/show/save-as", post(save_show_as))
        .route("/api/audio", get(audio))
        .route("/api/midi", get(midi))
        .route("/api/midi/in", post(midi_in))
        .route("/api/timecode/in", post(timecode_in))
        .route("/api/waveform", get(waveform))
        .route("/api/cue-media", get(cue_media))
        .route("/api/preflight", get(preflight))
        .route("/api/media/missing", get(media_missing))
        .route("/api/media/consolidate", post(consolidate_media))
        .route("/api/media/relink", post(relink_media))
        .route("/api/screens", get(list_screens).post(show_on_screens))
        .route(
            "/api/media",
            get(list_media)
                .post(upload_media)
                // Media files are large and streamed straight to disk.
                .layer(DefaultBodyLimit::disable()),
        )
        .route("/ws", get(websocket))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            authorize,
        ))
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

/// The passcode a request carries: `Authorization: Bearer`, an
/// `X-MazzaLab-Passcode` header, or `?passcode=` (all a WebSocket from a
/// browser can send).
fn passcode(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    header("authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .or_else(|| header("x-mazzalab-passcode"))
        .map(str::to_owned)
        .or_else(|| {
            query?
                .split('&')
                .filter_map(|pair| pair.split_once('='))
                .find(|(key, _)| *key == "passcode")
                .map(|(_, value)| decode_query(value))
        })
        .filter(|code| !code.is_empty())
}

/// Undoes the percent-encoding of a query value.
fn decode_query(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = |at: usize| bytes.get(at).and_then(|byte| (*byte as char).to_digit(16));
        match bytes[index] {
            b'+' => out.push(b' '),
            b'%' if hex(index + 1).is_some() && hex(index + 2).is_some() => {
                let high = hex(index + 1).unwrap_or_default();
                let low = hex(index + 2).unwrap_or_default();
                out.push((high * 16 + low) as u8);
                index += 2;
            }
            byte => out.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What a request for `path` needs before its handler looks closer.
fn route_need(method: &Method, path: &str) -> Need {
    match (method, path) {
        (_, "/api/access") => Need::Nothing,
        (&Method::GET, _) => Need::View,
        // Each command is checked on its own.
        (_, "/api/command") => Need::View,
        (_, "/api/midi/in" | "/api/timecode/in" | "/api/screens") => Need::Control,
        _ => Need::Edit,
    }
}

/// Checks every API request against the show's passcodes and hands the
/// handler what the client may do. The UI's own files are open to all, so
/// a browser can ask for a passcode.
async fn authorize(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();
    if !(path.starts_with("/api/") || path == "/ws") {
        return next.run(request).await;
    }
    // Requests built in-process (tests, the desktop app) have no peer.
    let local = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .is_none_or(|ConnectInfo(peer)| peer.ip().is_loopback());
    let code = passcode(request.headers(), request.uri().query());
    let access = state.handle.access(code.as_deref(), local);
    if !access.allows(route_need(request.method(), &path)) {
        return denied(access);
    }
    request.extensions_mut().insert(access);
    request.extensions_mut().insert(Passcode(code, local));
    next.run(request).await
}

/// The passcode a client gave and whether it is local, so a long-lived
/// connection is checked again as the show's passcodes change.
#[derive(Debug, Clone)]
struct Passcode(Option<String>, bool);

/// 401 to a client that has no access at all (no passcode, or a wrong
/// one), 403 to one that may not do this.
fn denied(access: Access) -> Response {
    if access.view {
        failure(StatusCode::FORBIDDEN, "not allowed with this passcode").into_response()
    } else {
        failure(StatusCode::UNAUTHORIZED, "a passcode is needed").into_response()
    }
}

/// What the client may do, and whether the show asks for a passcode.
async fn access(
    State(state): State<AppState>,
    Extension(access): Extension<Access>,
) -> Json<Value> {
    Json(json!({
        "required": state.handle.has_passcodes(),
        "view": access.view,
        "control": access.control,
        "edit": access.edit,
    }))
}

/// The snapshot as a client may see it: passcodes are for editors only.
fn visible(snapshot: &Snapshot, access: Access) -> std::borrow::Cow<'_, Snapshot> {
    if access.edit || snapshot.workspace.settings.passcodes.is_empty() {
        std::borrow::Cow::Borrowed(snapshot)
    } else {
        let mut snapshot = snapshot.clone();
        snapshot.workspace.settings.passcodes.clear();
        std::borrow::Cow::Owned(snapshot)
    }
}

async fn placeholder() -> Html<&'static str> {
    Html(
        "<!doctype html><title>MazzaLab</title><h1>MazzaLab server is running</h1>\
         <p>The web UI is not being served; start the server with <code>--ui-dir</code>.</p>",
    )
}

async fn snapshot(
    State(state): State<AppState>,
    Extension(access): Extension<Access>,
) -> impl IntoResponse {
    match state.handle.snapshot().await {
        Some(snapshot) => {
            Json(serde_json::to_value(visible(&snapshot, access)).unwrap_or(Value::Null))
                .into_response()
        }
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

/// How the server is doing: uptime, clients, audio load and dropouts, the
/// input record.
async fn diagnostics(State(state): State<AppState>) -> Response {
    match state.handle.diagnostics().await {
        Some(diagnostics) => Json(diagnostics).into_response(),
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
    Extension(access): Extension<Access>,
    Json(command): Json<Command>,
) -> Response {
    if !access.allows(command_need(&command)) {
        return denied(access);
    }
    outcome(state.handle.execute(command).await).into_response()
}

async fn save(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    outcome(state.handle.save().await)
}

/// What would not work if GO were pressed now: the pre-show check of the
/// open show, with the media looked for next to the show file.
async fn preflight(State(state): State<AppState>) -> Response {
    let Some(snapshot) = state.handle.snapshot().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let base = state
        .handle
        .base_dir()
        .map_or_else(|| PathBuf::from("."), |base| base.to_path_buf());
    let issues = tokio::task::spawn_blocking(move || {
        mazzalab_show::preflight::check(&snapshot.workspace, &base)
    })
    .await
    .unwrap_or_default();
    Json(json!({ "issues": issues })).into_response()
}

/// The open show's file and its folder.
async fn show_file(State(state): State<AppState>) -> Json<Value> {
    let path = state.handle.path();
    Json(json!({
        "path": path.as_deref().map(|path| path.display().to_string()),
        "folder": path
            .as_deref()
            .and_then(Path::parent)
            .map(|folder| folder.display().to_string()),
    }))
}

#[derive(Debug, Deserialize)]
struct FilesQuery {
    dir: Option<PathBuf>,
}

/// A folder of the server's machine, to pick a show in: its subfolders and
/// show files, by name. Without `dir`, the open show's folder.
async fn show_files(State(state): State<AppState>, Query(query): Query<FilesQuery>) -> Response {
    let dir = query
        .dir
        .or_else(|| state.handle.base_dir().map(|base| base.to_path_buf()))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    match tokio::task::spawn_blocking(move || list_folder(&dir)).await {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(error)) => failure(StatusCode::NOT_FOUND, error.to_string()).into_response(),
        Err(error) => failure(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

fn list_folder(dir: &Path) -> std::io::Result<Value> {
    let dir = std::path::absolute(dir)?;
    let mut folders = Vec::new();
    let mut shows = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            folders.push(name);
        } else if Path::new(&name)
            .extension()
            .is_some_and(|extension| extension == mazzalab_project::FILE_EXTENSION)
        {
            shows.push(name);
        }
    }
    let order = |a: &String, b: &String| a.to_lowercase().cmp(&b.to_lowercase());
    folders.sort_by(order);
    shows.sort_by(order);
    Ok(json!({
        "dir": dir.display().to_string(),
        "parent": dir.parent().map(|parent| parent.display().to_string()),
        "separator": std::path::MAIN_SEPARATOR.to_string(),
        "folders": folders,
        "shows": shows,
    }))
}

#[derive(Debug, Deserialize)]
struct OpenRequest {
    path: PathBuf,
    /// Throw the open show's unsaved changes away; without it, a show with
    /// unsaved changes is not replaced.
    #[serde(default)]
    discard: bool,
}

/// Opens another show in place of the running one, on the same sockets.
async fn open_show(
    State(state): State<AppState>,
    Json(request): Json<OpenRequest>,
) -> (StatusCode, Json<Value>) {
    if !request.path.is_file() {
        return failure(
            StatusCode::NOT_FOUND,
            format!("no show at {}", request.path.display()),
        );
    }
    replace_show(&state.handle, request, false).await
}

/// Creates an empty show file and opens it.
async fn new_show(
    State(state): State<AppState>,
    Json(mut request): Json<OpenRequest>,
) -> (StatusCode, Json<Value>) {
    if request.path.extension().is_none() {
        request.path.set_extension(mazzalab_project::FILE_EXTENSION);
    }
    if request.path.exists() {
        return failure(
            StatusCode::CONFLICT,
            format!("{} already exists", request.path.display()),
        );
    }
    replace_show(&state.handle, request, true).await
}

async fn replace_show(
    handle: &ShowHandle,
    request: OpenRequest,
    create: bool,
) -> (StatusCode, Json<Value>) {
    let dirty = handle
        .snapshot()
        .await
        .is_some_and(|snapshot| snapshot.dirty);
    if dirty && !request.discard {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "the show has unsaved changes", "unsaved": true })),
        );
    }
    let path = request.path;
    let read = tokio::task::spawn_blocking(move || {
        if create {
            let workspace = mazzalab_core::Workspace::new(crate::show_name(&path));
            mazzalab_project::save(&path, &workspace).map_err(|error| error.to_string())?;
        }
        crate::read_show(&path).map(|read| (path, read))
    })
    .await;
    match read {
        Ok(Ok((path, (workspace, recovered)))) => {
            if dirty && let Some(previous) = handle.path() {
                // The changes were thrown away: their autosave goes too.
                mazzalab_project::discard_autosave(&previous);
            }
            handle.open(workspace, Some(path), recovered).await;
            (StatusCode::OK, Json(json!({ "ok": true })))
        }
        Ok(Err(error)) => failure(StatusCode::UNPROCESSABLE_ENTITY, error),
        Err(error) => failure(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
struct SaveAsRequest {
    path: PathBuf,
}

/// Saves the show to another file, which becomes its file.
async fn save_show_as(
    State(state): State<AppState>,
    Json(request): Json<SaveAsRequest>,
) -> (StatusCode, Json<Value>) {
    let mut path = request.path;
    if path.extension().is_none() {
        path.set_extension(mazzalab_project::FILE_EXTENSION);
    }
    outcome(state.handle.save_as(path).await)
}

/// The audio output in use and the devices the show could use instead.
async fn audio(State(state): State<AppState>) -> Json<Value> {
    let status = state.handle.audio_status().await;
    // Asking the drivers can take a moment: keep it off the async threads.
    let (devices, inputs) = tokio::task::spawn_blocking(|| {
        (
            mazzalab_audio::output_devices(),
            mazzalab_audio::input_devices(),
        )
    })
    .await
    .unwrap_or_default();
    let inputs: Vec<Value> = inputs
        .into_iter()
        .map(|device| {
            json!({
                "id": device.id,
                "name": device.name,
                "host": device.host,
                "is_default": device.is_default,
                "max_channels": device.max_channels,
            })
        })
        .collect();
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
    Json(json!({ "status": status, "devices": devices, "inputs": inputs }))
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

/// The machine's displays and whether stages are up on them; a server
/// without any says it has none to offer.
async fn list_screens(State(state): State<AppState>) -> Json<Value> {
    match state.screens {
        Some(ScreensHandle(screens)) => {
            let listed = tokio::task::spawn_blocking(move || (screens.list(), screens.shown()))
                .await
                .unwrap_or_default();
            Json(json!({ "available": true, "screens": listed.0, "shown": listed.1 }))
        }
        None => Json(json!({ "available": false, "screens": [], "shown": false })),
    }
}

#[derive(Deserialize)]
struct ShowOnScreens {
    shown: bool,
}

/// Puts the stages up on the screens they name, or takes them down.
async fn show_on_screens(
    State(state): State<AppState>,
    Json(request): Json<ShowOnScreens>,
) -> Response {
    match state.screens {
        Some(ScreensHandle(screens)) => {
            screens.set_shown(request.shown);
            Json(json!({ "shown": screens.shown() })).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
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
struct TimecodeBody {
    timecode: String,
    #[serde(default)]
    format: mazzalab_core::SmpteFormat,
}

/// Feeds the show timecode as if it had come in, to try timecode triggers
/// without a source.
async fn timecode_in(
    State(state): State<AppState>,
    Json(body): Json<TimecodeBody>,
) -> (StatusCode, Json<Value>) {
    match mazzalab_timecode::Timecode::parse(&body.timecode) {
        Ok(timecode) => {
            state.handle.timecode_in(timecode, body.format.into()).await;
            outcome(Ok(()))
        }
        Err(error) => outcome(Err(error.to_string())),
    }
}

#[derive(Deserialize)]
struct CueQuery {
    cue: CueId,
}

/// The media file of a cue, with range requests, for stages to play.
async fn cue_media(
    State(state): State<AppState>,
    Query(query): Query<CueQuery>,
    request: axum::extract::Request,
) -> Response {
    let Some(path) = state.handle.cue_file(query.cue).await else {
        return failure(StatusCode::NOT_FOUND, "the cue has no media file").into_response();
    };
    match tower_http::services::ServeFile::new(path)
        .try_call(request)
        .await
    {
        Ok(response) => response.into_response(),
        Err(error) => failure(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
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

/// A cue and the media file it names, for the answers below.
fn file_entry(cue: CueId, file: &Path) -> Value {
    json!({ "cue": cue, "file": media_manage::portable(file) })
}

/// The cues whose media files are not there.
async fn media_missing(State(state): State<AppState>) -> Response {
    let Some(base) = state.handle.base_dir() else {
        return media_dir(&state.handle).err().map_or_else(
            || StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            IntoResponse::into_response,
        );
    };
    let Some(snapshot) = state.handle.snapshot().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let lost =
        tokio::task::spawn_blocking(move || media_manage::missing(&snapshot.workspace, &base))
            .await
            .unwrap_or_default();
    let missing: Vec<Value> = lost
        .iter()
        .map(|lost| file_entry(lost.cue, &lost.file))
        .collect();
    Json(json!({ "missing": missing })).into_response()
}

/// Changes the media files of cues in one step, so that one undo puts them
/// back. `changes` holds the cue and its new path.
async fn repoint(
    handle: &ShowHandle,
    workspace: &mazzalab_core::Workspace,
    changes: &[(CueId, PathBuf)],
) -> Result<(), (StatusCode, Json<Value>)> {
    let commands: Vec<Command> = changes
        .iter()
        .filter_map(|(id, file)| {
            let cue = workspace.find_cue(*id)?;
            media_manage::with_file(cue, file.clone()).map(|cue| Command::UpdateCue { cue })
        })
        .collect();
    if commands.is_empty() {
        return Ok(());
    }
    handle
        .execute(Command::Batch { commands })
        .await
        .map_err(|error| failure(StatusCode::INTERNAL_SERVER_ERROR, error))
}

/// Copies every media file that lives outside the show's media folder into
/// it, under a numbered name if the name is taken, and points the cues at
/// the copies.
async fn consolidate_media(State(state): State<AppState>) -> Response {
    let media = match media_dir(&state.handle) {
        Ok(media) => media,
        Err(error) => return error.into_response(),
    };
    let Some(base) = state.handle.base_dir() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Some(snapshot) = state.handle.snapshot().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let workspace = snapshot.workspace;
    let to_copy = {
        let (workspace, base, media) = (workspace.clone(), base.clone(), media.clone());
        move || -> std::io::Result<Vec<(CueId, PathBuf, PathBuf)>> {
            std::fs::create_dir_all(&media)?;
            let mut copies: std::collections::HashMap<PathBuf, String> =
                std::collections::HashMap::new();
            let mut done = Vec::new();
            for (serial, item) in media_manage::outside(&workspace, &base, &media)
                .into_iter()
                .enumerate()
            {
                let name = match copies.get(&item.source) {
                    Some(name) => name.clone(),
                    None => {
                        let wanted = item
                            .source
                            .file_name()
                            .and_then(|name| safe_file_name(&name.to_string_lossy()))
                            .unwrap_or_else(|| "media".to_owned());
                        let temp = media
                            .join(format!(".consolidate-{}-{serial}.part", std::process::id()));
                        std::fs::copy(&item.source, &temp)?;
                        let name = place_upload(&media, &temp, &wanted)?;
                        copies.insert(item.source.clone(), name.clone());
                        name
                    }
                };
                done.push((
                    item.cue,
                    item.file,
                    PathBuf::from(format!("{MEDIA_FOLDER}/{name}")),
                ));
            }
            Ok(done)
        }
    };
    let done = match tokio::task::spawn_blocking(to_copy).await {
        Ok(Ok(done)) => done,
        Ok(Err(error)) => {
            return failure(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
        Err(error) => {
            return failure(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    let changes: Vec<(CueId, PathBuf)> =
        done.iter().map(|(cue, _, to)| (*cue, to.clone())).collect();
    if let Err(error) = repoint(&state.handle, &workspace, &changes).await {
        return error.into_response();
    }
    let copied: Vec<Value> = done
        .iter()
        .map(|(cue, from, to)| json!({ "cue": cue, "from": media_manage::portable(from), "file": media_manage::portable(to) }))
        .collect();
    Json(json!({ "copied": copied })).into_response()
}

#[derive(Deserialize)]
struct RelinkBody {
    /// A folder of the server's machine to look in, below it too.
    #[serde(alias = "from_dir")]
    search_dir: PathBuf,
}

/// Looks for the files cues miss, by name, under a folder, and points the
/// cues at what it finds.
async fn relink_media(State(state): State<AppState>, Json(body): Json<RelinkBody>) -> Response {
    let Some(base) = state.handle.base_dir() else {
        return media_dir(&state.handle).err().map_or_else(
            || StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            IntoResponse::into_response,
        );
    };
    if !body.search_dir.is_dir() {
        return failure(StatusCode::NOT_FOUND, "that folder cannot be read").into_response();
    }
    let Some(snapshot) = state.handle.snapshot().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let workspace = snapshot.workspace;
    let looked = {
        let (workspace, base, root) = (workspace.clone(), base.clone(), body.search_dir);
        move || {
            let lost = media_manage::missing(&workspace, &base);
            let names = lost
                .iter()
                .filter_map(|lost| media_manage::key_of(&lost.file))
                .collect();
            let found = media_manage::search(&root, &names);
            let base_real = base.canonicalize().ok();
            let mut relinked = Vec::new();
            let mut not_found = Vec::new();
            for lost in lost {
                let hit = media_manage::key_of(&lost.file).and_then(|key| found.get(&key));
                match hit {
                    Some(hit) => {
                        // Inside the show's folder, a path relative to it; elsewhere, the full one.
                        let inside = hit
                            .path
                            .canonicalize()
                            .ok()
                            .zip(base_real.as_ref())
                            .and_then(|(path, base)| {
                                path.strip_prefix(base).ok().map(Path::to_path_buf)
                            });
                        relinked.push((
                            lost.cue,
                            lost.file,
                            inside.unwrap_or_else(|| hit.path.clone()),
                        ));
                    }
                    None => not_found.push((lost.cue, lost.file)),
                }
            }
            (relinked, not_found)
        }
    };
    let Ok((relinked, not_found)) = tokio::task::spawn_blocking(looked).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let changes: Vec<(CueId, PathBuf)> = relinked
        .iter()
        .map(|(cue, _, to)| (*cue, to.clone()))
        .collect();
    if let Err(error) = repoint(&state.handle, &workspace, &changes).await {
        return error.into_response();
    }
    Json(json!({
        "relinked": relinked
            .iter()
            .map(|(cue, from, to)| json!({ "cue": cue, "from": media_manage::portable(from), "file": media_manage::portable(to) }))
            .collect::<Vec<Value>>(),
        "not_found": not_found.iter().map(|(cue, file)| file_entry(*cue, file)).collect::<Vec<Value>>(),
    }))
    .into_response()
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

async fn websocket(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Extension(passcode): Extension<Passcode>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| client(socket, state.handle, passcode))
}

/// What a WebSocket message needs.
fn message_need(message: &ClientMessage) -> Need {
    match message {
        ClientMessage::Command { command, .. } => command_need(command),
        ClientMessage::Save { .. } | ClientMessage::Audition { .. } => Need::Edit,
        ClientMessage::StageEnded { .. } => Need::View,
    }
}

async fn client(socket: WebSocket, handle: ShowHandle, Passcode(code, local): Passcode) {
    // Checked again on every message: the show's passcodes may change.
    let access = || handle.access(code.as_deref(), local);
    let _connected = handle.client_connected();
    let (mut sink, mut stream) = socket.split();
    let mut updates = handle.subscribe();

    let send = |message: &ServerMessage<'_>| {
        serde_json::to_string(message).map(|text| Message::Text(text.into()))
    };
    let audio = handle.audio_status().await;
    let initial = match (handle.snapshot().await, handle.status().await) {
        (Some(snapshot), Some(status)) => vec![
            send(&ServerMessage::Snapshot {
                snapshot: &visible(&snapshot, access()),
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
    let stage = handle.stage().await;
    let stage = stage
        .as_ref()
        .map(|stage| send(&ServerMessage::Stage { stage }));
    for message in initial
        .into_iter()
        .chain(audio)
        .chain(dmx)
        .chain(stage)
        .flatten()
    {
        if sink.send(message).await.is_err() {
            return;
        }
    }

    loop {
        let outgoing = tokio::select! {
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    let parsed = serde_json::from_str::<ClientMessage>(text.as_str());
                    let allowed = parsed.as_ref().map_or(true, |message| access().allows(message_need(message)));
                    let (id, result) = match parsed {
                        Ok(message) if !allowed => (message.id(), Err("not allowed with this passcode".to_owned())),
                        Ok(ClientMessage::Command { id, command }) => (id, handle.execute(*command).await),
                        Ok(ClientMessage::Save { id }) => (id, handle.save().await),
                        Ok(ClientMessage::Audition { id, cue }) => (id, handle.audition(cue).await),
                        Ok(ClientMessage::StageEnded { id, cue }) => {
                            handle.media_ended(cue).await;
                            (id, Ok(()))
                        }
                        Err(error) => (0, Err(format!("invalid message: {error}"))),
                    };
                    send(&ServerMessage::Result { id, error: result.err() })
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => continue,
            },
            update = updates.recv() => match update {
                Ok(Update::Snapshot(snapshot)) => send(&ServerMessage::Snapshot { snapshot: &visible(&snapshot, access()) }),
                Ok(Update::Status(status)) => send(&ServerMessage::Status { status: &status }),
                Ok(Update::Events(events)) => send(&ServerMessage::Events { events: &events }),
                Ok(Update::Meters(peaks)) => send(&ServerMessage::Meters { peaks: &peaks }),
                Ok(Update::Audio(audio)) => send(&ServerMessage::Audio { audio: &audio }),
                Ok(Update::Notice(notice)) => send(&ServerMessage::Notice { notice: &notice }),
                Ok(Update::Dmx(levels)) => send(&ServerMessage::Dmx { levels: &levels }),
                Ok(Update::MidiIn(event)) => send(&ServerMessage::MidiIn { event }),
                Ok(Update::Timecode(timecode)) => send(&ServerMessage::Timecode { timecode: &timecode }),
                Ok(Update::Stage(stage)) => send(&ServerMessage::Stage { stage: &stage }),
                Ok(Update::Osc(_)) => continue,
                // Too slow to keep up: resynchronise with a fresh snapshot.
                Err(RecvError::Lagged(_)) => match handle.snapshot().await {
                    Some(snapshot) => send(&ServerMessage::Snapshot { snapshot: &visible(&snapshot, access()) }),
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
