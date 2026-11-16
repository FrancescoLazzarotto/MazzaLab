//! Stages full screen on the machine's displays.
//!
//! A stage that names a `screen` gets a borderless, full-screen window on
//! that display, showing the web UI's stage page, while the operator has
//! asked for stages to be up (the Video menu, or the web UI). The windows
//! follow the show: renaming a stage, moving it to another display or
//! opening another show puts them where they now belong.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, PoisonError};

use mazzalab_core::Stage;
use mazzalab_server::{Output, Screen, Screens};
use mazzalab_show::Show;
use tauri::menu::MenuItemKind;
use tauri::{AppHandle, Manager, Monitor, Url, WebviewUrl, WebviewWindowBuilder};

use crate::MAIN_WINDOW;

/// Labels of stage windows start with this.
const STAGE_WINDOW: &str = "stage-";
/// The id of the Video menu's toggle.
pub const MENU_SCREENS: &str = "screens";

/// Arguments for every webview: the defaults Tauri sets on Windows, plus
/// letting stages play video with sound without a click first.
#[cfg(windows)]
pub const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

#[derive(Debug, Default)]
struct State {
    shown: bool,
    stages: Vec<Stage>,
    /// The address of the server whose stages are shown.
    base: Option<String>,
}

/// The machine's displays, and the stage windows on them.
pub struct DesktopScreens {
    app: AppHandle,
    state: Mutex<State>,
    /// Held while windows are being put right, so that changes coming
    /// quickly one after another are applied in turn.
    applying: tokio::sync::Mutex<()>,
}

impl DesktopScreens {
    pub fn new(app: AppHandle) -> Arc<Self> {
        Arc::new(Self {
            app,
            state: Mutex::default(),
            applying: tokio::sync::Mutex::new(()),
        })
    }

    /// The server stages are shown from; `None` takes them down while a
    /// show is being swapped.
    pub fn set_base(self: &Arc<Self>, base: Option<String>) {
        self.change(|state| {
            let changed = state.base != base;
            state.base = base;
            changed
        });
    }

    /// Flips whether stages are up, as the Video menu does.
    pub fn toggle(self: &Arc<Self>) {
        self.change(|state| {
            state.shown = !state.shown;
            true
        });
    }

    fn change(self: &Arc<Self>, edit: impl FnOnce(&mut State) -> bool) {
        let changed = edit(&mut self.state.lock().unwrap_or_else(PoisonError::into_inner));
        if changed {
            let screens = Arc::clone(self);
            tauri::async_runtime::spawn(async move { screens.apply().await });
        }
    }

    /// Puts the windows where the latest state says.
    async fn apply(&self) {
        let _turn = self.applying.lock().await;
        let (shown, stages, base) = {
            let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            (state.shown, state.stages.clone(), state.base.clone())
        };
        if let Some(MenuItemKind::Check(item)) =
            self.app.menu().and_then(|menu| menu.get(MENU_SCREENS))
        {
            let _ = item.set_checked(shown);
        }
        let monitors = self.app.available_monitors().unwrap_or_default();
        let wanted: Vec<(String, Url, &Stage, &Monitor)> = match base.as_deref() {
            Some(base) if shown => stages
                .iter()
                .filter_map(|stage| {
                    let screen = stage.screen.as_deref()?;
                    let monitor = monitors
                        .iter()
                        .find(|monitor| monitor.name().map(String::as_str) == Some(screen))?;
                    let mut url = Url::parse(base).ok()?;
                    url.query_pairs_mut().append_pair("stage", &stage.name);
                    Some((label(base, &stage.name, screen), url, stage, monitor))
                })
                .collect(),
            _ => Vec::new(),
        };

        for (label, window) in self.app.webview_windows() {
            if label.starts_with(STAGE_WINDOW)
                && !wanted.iter().any(|(wanted, ..)| *wanted == label)
            {
                let _ = window.destroy();
            }
        }
        for (label, url, stage, monitor) in wanted {
            if self.app.get_webview_window(&label).is_some() {
                continue;
            }
            if let Err(error) = open(&self.app, &label, url, stage, monitor) {
                tracing::error!(%error, stage = %stage.name, "could not open the stage window");
            }
        }
    }
}

/// A window label for a stage on a display of a server: a new one when
/// any of them changes, so that the window is opened afresh.
fn label(base: &str, stage: &str, screen: &str) -> String {
    let mut hasher = DefaultHasher::new();
    (base, stage, screen).hash(&mut hasher);
    format!("{STAGE_WINDOW}{:016x}", hasher.finish())
}

/// A borderless window covering `monitor`, showing the stage.
fn open(
    app: &AppHandle,
    label: &str,
    url: Url,
    stage: &Stage,
    monitor: &Monitor,
) -> tauri::Result<()> {
    // The builder places windows in logical pixels, which the display's
    // own scale turns into its physical ones: a window sized in physical
    // pixels on a scaled display would spill past it.
    let scale = monitor.scale_factor();
    let position = monitor.position().to_logical::<f64>(scale);
    let size = monitor.size().to_logical::<f64>(scale);
    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::External(url))
        .title(format!("{} · MazzaLab", stage.name))
        .decorations(false)
        .skip_taskbar(true)
        .focused(false)
        .position(position.x, position.y)
        .inner_size(size.width, size.height)
        .fullscreen(true);
    #[cfg(windows)]
    let builder = builder.additional_browser_args(BROWSER_ARGS);
    builder.build()?;
    Ok(())
}

impl Screens for DesktopScreens {
    fn list(&self) -> Vec<Screen> {
        let main = self
            .app
            .get_webview_window(MAIN_WINDOW)
            .and_then(|window| window.current_monitor().ok().flatten())
            .and_then(|monitor| monitor.name().cloned());
        self.app
            .available_monitors()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|monitor| {
                let name = monitor.name()?.clone();
                Some(Screen {
                    main: main.as_ref() == Some(&name),
                    name,
                    width: monitor.size().width,
                    height: monitor.size().height,
                    x: monitor.position().x,
                    y: monitor.position().y,
                })
            })
            .collect()
    }

    fn shown(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .shown
    }

    fn set_shown(&self, shown: bool) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.shown == shown {
            return;
        }
        state.shown = shown;
        drop(state);
        // `self` is behind the server's Arc: find the app's to spawn with.
        let screens = Arc::clone(&self.app.state::<Arc<DesktopScreens>>());
        tauri::async_runtime::spawn(async move { screens.apply().await });
    }
}

/// Tells the screens about the show's stages as they change.
pub struct StageScreens(pub Arc<DesktopScreens>);

impl Output for StageScreens {
    fn handle(&mut self, _show: &Show, _events: &[mazzalab_engine::Event]) {}

    fn sync(&mut self, show: &Show) {
        let stages = show.workspace().settings.stages.clone();
        self.0.change(|state| {
            let changed = state.stages != stages;
            state.stages = stages;
            changed
        });
    }
}
