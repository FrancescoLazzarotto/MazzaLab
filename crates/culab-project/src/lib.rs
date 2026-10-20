//! culab-project
//!
//! The `.culab` show file: a pretty-printed JSON document with a format
//! marker and a version number, wrapping a [`Workspace`]. Plain text keeps
//! shows reviewable and mergeable with ordinary tools such as git.
//!
//! ```json
//! {
//!   "format": "culab.show",
//!   "version": 1,
//!   "workspace": { ... }
//! }
//! ```

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use culab_core::Workspace;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const FILE_EXTENSION: &str = "culab";
pub const FORMAT: &str = "culab.show";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("cannot access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid show file: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not a CuLab show file (format is {0:?})")]
    WrongFormat(String),
    #[error("show file version {found} is newer than this build supports ({FORMAT_VERSION})")]
    UnsupportedVersion { found: u32 },
}

#[derive(Serialize)]
struct ShowFileRef<'a> {
    format: &'static str,
    version: u32,
    workspace: &'a Workspace,
}

#[derive(Deserialize)]
struct Header {
    format: String,
    version: u32,
}

#[derive(Deserialize)]
struct ShowFile {
    workspace: Workspace,
}

/// Serializes a workspace to show file text.
pub fn to_string(workspace: &Workspace) -> Result<String, ProjectError> {
    let mut text = serde_json::to_string_pretty(&ShowFileRef {
        format: FORMAT,
        version: FORMAT_VERSION,
        workspace,
    })?;
    text.push('\n');
    Ok(text)
}

/// Parses show file text, checking the format marker and version first so
/// that a newer file gives a clear error instead of a confusing field error.
pub fn from_str(text: &str) -> Result<Workspace, ProjectError> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    let header = Header::deserialize(&value)?;
    if header.format != FORMAT {
        return Err(ProjectError::WrongFormat(header.format));
    }
    if header.version > FORMAT_VERSION {
        return Err(ProjectError::UnsupportedVersion {
            found: header.version,
        });
    }
    let file: ShowFile = serde_json::from_value(value)?;
    Ok(file.workspace)
}

pub fn load(path: impl AsRef<Path>) -> Result<Workspace, ProjectError> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|source| ProjectError::Io {
        path: path.to_owned(),
        source,
    })?;
    from_str(&text)
}

/// Writes the show file atomically: the new contents go to a sibling
/// temporary file which then replaces the target, so a crash mid-save never
/// leaves a truncated show behind.
pub fn save(path: impl AsRef<Path>, workspace: &Workspace) -> Result<(), ProjectError> {
    let path = path.as_ref();
    let text = to_string(workspace)?;
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    let io_error = |source| ProjectError::Io {
        path: path.to_owned(),
        source,
    };
    fs::write(&temp, text).map_err(io_error)?;
    fs::rename(&temp, path).map_err(io_error)
}
