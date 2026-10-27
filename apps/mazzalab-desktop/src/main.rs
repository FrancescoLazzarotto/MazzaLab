//! mazzalab-desktop entry point
//!
//! The desktop app runs a full show server in-process (engine, audio, OSC
//! and the web API, reachable by tablets on the network) and shows the
//! operator UI in a native window pointed at that server. The File menu
//! creates, opens and saves shows; opening another show restarts the server
//! on it and points the window at the new server.
//!
//! Usage: `mazzalab-desktop [show.mazzalab]`. Without a path the show is
//! `Documents/MazzaLab/Untitled.mazzalab`.

// Release builds on Windows should not open a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use mazzalab_core::Workspace;
use mazzalab_server::{AudioBackend, Completion, LogOutput, Output, RunningServer, ServerConfig};
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{
    App, AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent, Wry,
};
use tauri_plugin_dialog::{
    DialogExt, FileDialogBuilder, FilePath, MessageDialogButtons, MessageDialogKind,
    MessageDialogResult,
};
use tokio::sync::{Mutex, oneshot};
use tracing_subscriber::EnvFilter;

const MAIN_WINDOW: &str = "main";

const MENU_NEW: &str = "new";
const MENU_OPEN: &str = "open";
const MENU_SAVE: &str = "save";
const MENU_SAVE_AS: &str = "save_as";
const MENU_QUIT: &str = "quit";

/// App-wide state.
struct Desktop {
    lang: Lang,
    ui_dir: Option<PathBuf>,
    /// The show being run. File actions hold the lock from start to finish,
    /// so a second action started meanwhile (e.g. a double-clicked menu) is
    /// ignored instead of interleaving dialogs.
    session: Mutex<Option<Session>>,
    /// Set once closing has been confirmed, so the close goes through.
    closing: AtomicBool,
}

/// A show file and the server running it.
struct Session {
    show: PathBuf,
    server: RunningServer,
}

impl Session {
    fn name(&self) -> String {
        self.show.file_stem().map_or_else(
            || self.show.display().to_string(),
            |stem| stem.to_string_lossy().into_owned(),
        )
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.server.http.port())
    }

    fn title(&self) -> String {
        format!("MazzaLab — {}", self.show.display())
    }
}

/// Language of the native menus and dialogs, from the system locale (the web
/// UI picks its own the same way).
#[derive(Debug, Clone, Copy)]
enum Lang {
    En,
    It,
}

impl Lang {
    fn detect() -> Self {
        match sys_locale::get_locale() {
            Some(locale) if locale.to_lowercase().starts_with("it") => Self::It,
            _ => Self::En,
        }
    }

    fn t(self, en: &'static str, it: &'static str) -> &'static str {
        match self {
            Self::En => en,
            Self::It => it,
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(setup)
        .on_menu_event(|app, event| {
            let app = app.clone();
            match event.id().as_ref() {
                MENU_NEW => spawn(new_show(app)),
                MENU_OPEN => spawn(open_show(app)),
                MENU_SAVE => spawn(save_show(app)),
                MENU_SAVE_AS => spawn(save_show_as(app)),
                MENU_QUIT => {
                    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                        let _ = window.close();
                    }
                }
                _ => {}
            }
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle().clone();
                if !app.state::<Desktop>().closing.load(Ordering::SeqCst) {
                    api.prevent_close();
                    spawn(close(app));
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("MazzaLab failed to start");
}

fn setup(app: &mut App) -> Result<(), Box<dyn Error>> {
    let lang = Lang::detect();
    let ui_dir = ui_dir(app);
    let show = initial_show(app)?;
    let workspace = load(&show)?;
    let server = tauri::async_runtime::block_on(start_server(&show, workspace, ui_dir.clone()))?;
    tracing::info!(show = %show.display(), osc = %server.osc_udp, http = %server.http, "show server running");
    let session = Session { show, server };

    app.set_menu(menu(app.handle(), lang)?)?;
    WebviewWindowBuilder::new(
        app,
        MAIN_WINDOW,
        WebviewUrl::External(session.url().parse()?),
    )
    .title(session.title())
    .inner_size(1440.0, 900.0)
    .min_inner_size(900.0, 600.0)
    .build()?;
    app.manage(Desktop {
        lang,
        ui_dir,
        session: Mutex::new(Some(session)),
        closing: AtomicBool::new(false),
    });
    Ok(())
}

fn spawn(task: impl Future<Output = ()> + Send + 'static) {
    tauri::async_runtime::spawn(task);
}

fn menu(app: &AppHandle, lang: Lang) -> tauri::Result<Menu<Wry>> {
    let item = |id: &str, text: &str, accelerator: &str| {
        MenuItemBuilder::with_id(id, text)
            .accelerator(accelerator)
            .build(app)
    };
    let quit = item(
        MENU_QUIT,
        lang.t("Quit MazzaLab", "Esci da MazzaLab"),
        "CmdOrCtrl+Q",
    )?;

    let file = SubmenuBuilder::new(app, lang.t("File", "File"))
        .item(&item(
            MENU_NEW,
            lang.t("New Show…", "Nuovo show…"),
            "CmdOrCtrl+N",
        )?)
        .item(&item(MENU_OPEN, lang.t("Open…", "Apri…"), "CmdOrCtrl+O")?)
        .separator()
        .item(&item(MENU_SAVE, lang.t("Save", "Salva"), "CmdOrCtrl+S")?)
        .item(&item(
            MENU_SAVE_AS,
            lang.t("Save As…", "Salva con nome…"),
            "CmdOrCtrl+Shift+S",
        )?);

    // On macOS Quit lives in the application menu, and text fields only get
    // the clipboard shortcuts through an Edit menu. Undo and redo are left
    // out: the web UI handles them for the show itself.
    #[cfg(target_os = "macos")]
    {
        use tauri::menu::PredefinedMenuItem;
        let application = SubmenuBuilder::new(app, "MazzaLab")
            .about(None)
            .separator()
            .services()
            .separator()
            .hide()
            .hide_others()
            .show_all()
            .separator()
            .item(&quit)
            .build()?;
        let edit = SubmenuBuilder::new(app, lang.t("Edit", "Modifica"))
            .cut()
            .copy()
            .paste()
            .item(&PredefinedMenuItem::select_all(app, None)?)
            .build()?;
        MenuBuilder::new(app)
            .items(&[&application, &file.build()?, &edit])
            .build()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let file = file.separator().item(&quit).build()?;
        MenuBuilder::new(app).item(&file).build()
    }
}

/// File > New: asks where to create the show, then opens it empty.
async fn new_show(app: AppHandle) {
    let desktop = app.state::<Desktop>();
    let Ok(mut session) = desktop.session.try_lock() else {
        return;
    };
    let lang = desktop.lang;
    if !confirm_discard(&app, session.as_ref(), lang).await {
        return;
    }
    let dialog = show_dialog(&app, lang)
        .set_title(lang.t("New Show", "Nuovo show"))
        .set_file_name(format!("Untitled.{}", mazzalab_project::FILE_EXTENSION));
    let Some(path) = pick(dialog, Pick::Save).await else {
        return;
    };
    let workspace = Workspace::new(show_name(&path));
    if let Err(error) = mazzalab_project::save(&path, &workspace) {
        return report(
            &app,
            lang.t(
                "The show could not be created.",
                "Impossibile creare lo show.",
            ),
            &error.to_string(),
        );
    }
    switch_to(&app, &mut session, path, workspace).await;
}

/// File > Open.
async fn open_show(app: AppHandle) {
    let desktop = app.state::<Desktop>();
    let Ok(mut session) = desktop.session.try_lock() else {
        return;
    };
    let lang = desktop.lang;
    if !confirm_discard(&app, session.as_ref(), lang).await {
        return;
    }
    let dialog = show_dialog(&app, lang).set_title(lang.t("Open Show", "Apri show"));
    let Some(path) = pick(dialog, Pick::Open).await else {
        return;
    };
    match load(&path) {
        Ok(workspace) => switch_to(&app, &mut session, path, workspace).await,
        Err(error) => report(
            &app,
            lang.t(
                "The show could not be opened.",
                "Impossibile aprire lo show.",
            ),
            &error,
        ),
    }
}

/// File > Save.
async fn save_show(app: AppHandle) {
    let desktop = app.state::<Desktop>();
    let Ok(session) = desktop.session.try_lock() else {
        return;
    };
    if let Some(session) = session.as_ref()
        && let Err(error) = session.server.handle.save().await
    {
        report(
            &app,
            desktop.lang.t(
                "The show could not be saved.",
                "Impossibile salvare lo show.",
            ),
            &error,
        );
    }
}

/// File > Save As: writes the show to a new file, which then becomes the
/// open show. The original file keeps its last saved state.
async fn save_show_as(app: AppHandle) {
    let desktop = app.state::<Desktop>();
    let Ok(mut session) = desktop.session.try_lock() else {
        return;
    };
    let lang = desktop.lang;
    let Some(current) = session.as_ref() else {
        return;
    };
    let mut dialog = show_dialog(&app, lang)
        .set_title(lang.t("Save Show As", "Salva show con nome"))
        .set_file_name(format!(
            "{}.{}",
            current.name(),
            mazzalab_project::FILE_EXTENSION
        ));
    if let Some(folder) = current.show.parent().filter(|folder| folder.is_dir()) {
        dialog = dialog.set_directory(folder);
    }
    let Some(path) = pick(dialog, Pick::Save).await else {
        return;
    };
    let saved = current.server.handle.save_copy(path.clone()).await;
    match saved.and_then(|()| load(&path)) {
        Ok(workspace) => switch_to(&app, &mut session, path, workspace).await,
        Err(error) => report(
            &app,
            lang.t(
                "The show could not be saved.",
                "Impossibile salvare lo show.",
            ),
            &error,
        ),
    }
}

/// Window close (or Quit): asks about unsaved changes first.
async fn close(app: AppHandle) {
    let desktop = app.state::<Desktop>();
    let Ok(session) = desktop.session.try_lock() else {
        return;
    };
    if confirm_discard(&app, session.as_ref(), desktop.lang).await {
        desktop.closing.store(true, Ordering::SeqCst);
        if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
            let _ = window.close();
        }
    }
}

/// Whether the current show may be replaced or closed. With unsaved
/// changes the operator chooses to save them, throw them away or cancel.
async fn confirm_discard(app: &AppHandle, session: Option<&Session>, lang: Lang) -> bool {
    let Some(session) = session else {
        return true;
    };
    let dirty = session
        .server
        .handle
        .snapshot()
        .await
        .is_some_and(|snapshot| snapshot.dirty);
    if !dirty {
        return true;
    }

    let save = lang.t("Save", "Salva");
    let message = lang
        .t(
            "“{show}” has unsaved changes. Do you want to save them?",
            "Lo show “{show}” ha modifiche non salvate. Vuoi salvarle?",
        )
        .replace("{show}", &session.name());
    let dialog = app
        .dialog()
        .message(message)
        .title("MazzaLab")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::YesNoCancelCustom(
            save.to_owned(),
            lang.t("Don't Save", "Non salvare").to_owned(),
            lang.t("Cancel", "Annulla").to_owned(),
        ));
    let dialog = match main_window(app) {
        Some(window) => dialog.parent(&window),
        None => dialog,
    };
    let (reply, answer) = oneshot::channel();
    dialog.show_with_result(move |result| {
        let _ = reply.send(result);
    });
    // Custom buttons come back as their label on some platforms and as
    // Yes/No on others.
    match answer.await.unwrap_or_default() {
        MessageDialogResult::Yes => {}
        MessageDialogResult::Custom(label) if label == save => {}
        MessageDialogResult::No => return true,
        MessageDialogResult::Custom(label) if label == lang.t("Don't Save", "Non salvare") => {
            return true;
        }
        _ => return false,
    }
    match session.server.handle.save().await {
        Ok(()) => true,
        Err(error) => {
            report(
                app,
                lang.t(
                    "The show could not be saved.",
                    "Impossibile salvare lo show.",
                ),
                &error,
            );
            false
        }
    }
}

/// Stops the current server, starts one for `show` and points the window
/// at it.
async fn switch_to(
    app: &AppHandle,
    session: &mut Option<Session>,
    show: PathBuf,
    workspace: Workspace,
) {
    let desktop = app.state::<Desktop>();
    if let Some(previous) = session.take() {
        // Waits for the ports to be released so the new server gets them.
        previous.server.stop().await;
    }
    let server = match start_server(&show, workspace, desktop.ui_dir.clone()).await {
        Ok(server) => server,
        Err(error) => {
            return report(
                app,
                desktop.lang.t(
                    "The show could not be opened.",
                    "Impossibile aprire lo show.",
                ),
                &error.to_string(),
            );
        }
    };
    tracing::info!(show = %show.display(), osc = %server.osc_udp, http = %server.http, "show server running");
    let next = Session { show, server };
    if let Some(window) = main_window(app) {
        let _ = window.set_title(&next.title());
        match next.url().parse() {
            Ok(url) => {
                let _ = window.navigate(url);
            }
            Err(error) => tracing::error!(%error, "invalid server URL"),
        }
    }
    *session = Some(next);
}

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN_WINDOW)
}

/// A file dialog for show files, attached to the main window.
fn show_dialog(app: &AppHandle, lang: Lang) -> FileDialogBuilder<Wry> {
    let dialog = app.dialog().file().add_filter(
        lang.t("MazzaLab show", "Show MazzaLab"),
        &[mazzalab_project::FILE_EXTENSION],
    );
    match main_window(app) {
        Some(window) => dialog.set_parent(&window),
        None => dialog,
    }
}

#[derive(Debug, Clone, Copy)]
enum Pick {
    Open,
    Save,
}

/// Shows a file dialog; `None` when it was cancelled. Saved shows always get
/// the show file extension.
async fn pick(dialog: FileDialogBuilder<Wry>, mode: Pick) -> Option<PathBuf> {
    let (reply, answer) = oneshot::channel();
    let reply = move |path: Option<FilePath>| {
        let _ = reply.send(path);
    };
    match mode {
        Pick::Open => dialog.pick_file(reply),
        Pick::Save => dialog.save_file(reply),
    }
    let mut path = answer.await.ok().flatten()?.into_path().ok()?;
    if matches!(mode, Pick::Save) && path.extension().is_none() {
        path.set_extension(mazzalab_project::FILE_EXTENSION);
    }
    Some(path)
}

/// Shows an error. Non-blocking: the caller just stops what it was doing.
fn report(app: &AppHandle, what: &str, error: &str) {
    tracing::error!(%error, "{what}");
    let dialog = app
        .dialog()
        .message(format!("{what}\n\n{error}"))
        .title("MazzaLab")
        .kind(MessageDialogKind::Error);
    match main_window(app) {
        Some(window) => dialog.parent(&window),
        None => dialog,
    }
    .show(|_| {});
}

fn initial_show(app: &App) -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = std::env::args().skip(1).find(|arg| !arg.starts_with('-')) {
        return Ok(PathBuf::from(path));
    }
    let folder = app.path().document_dir()?.join("MazzaLab");
    std::fs::create_dir_all(&folder)?;
    Ok(folder.join("Untitled.mazzalab"))
}

fn show_name(path: &Path) -> String {
    path.file_stem().map_or_else(
        || "Untitled".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    )
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

/// Folder that media paths in `show` are relative to.
fn base_dir(show: &Path) -> PathBuf {
    show.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// Reads a show (an empty one when the file does not exist yet) and fills in
/// the lengths of its audio files.
fn load(show: &Path) -> Result<Workspace, String> {
    let mut workspace = mazzalab_server::load_or_create(show)?;
    for (cue, problem) in mazzalab_audio::probe_workspace(&mut workspace, &base_dir(show)) {
        tracing::warn!(%cue, %problem, "audio file unavailable");
    }
    Ok(workspace)
}

async fn start_server(
    show: &Path,
    workspace: Workspace,
    ui_dir: Option<PathBuf>,
) -> Result<RunningServer, Box<dyn Error>> {
    let base_dir = base_dir(show);
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
