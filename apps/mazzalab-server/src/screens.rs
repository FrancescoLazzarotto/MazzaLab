//! The screens of the machine the server runs on, when it has some to put
//! stages on: the desktop app provides them, a headless server has none.

use std::fmt;
use std::sync::Arc;

use serde::Serialize;

/// A display attached to the machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Screen {
    /// What the system calls it; a stage's `screen` names it.
    pub name: String,
    /// Its resolution, in physical pixels.
    pub width: u32,
    pub height: u32,
    /// Where it sits on the desktop, in physical pixels.
    pub x: i32,
    pub y: i32,
    /// The one the operator's window is on.
    pub main: bool,
}

/// Opens the show's stages full screen on the displays they name.
pub trait Screens: Send + Sync {
    /// The displays attached now.
    fn list(&self) -> Vec<Screen>;
    /// Whether stages are up on their screens.
    fn shown(&self) -> bool;
    /// Puts the stages up on their screens, or takes them down.
    fn set_shown(&self, shown: bool);
}

/// A shared [`Screens`], for the server's configuration.
#[derive(Clone)]
pub struct ScreensHandle(pub Arc<dyn Screens>);

impl fmt::Debug for ScreensHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ScreensHandle")
    }
}
