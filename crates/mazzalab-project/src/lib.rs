//! mazzalab-project
//!
//! The `.mazzalab` show file: a pretty-printed JSON document with a format
//! marker and a version number, wrapping a [`Workspace`]. Plain text keeps
//! shows reviewable and mergeable with ordinary tools such as git.
//!
//! ```json
//! {
//!   "format": "mazzalab.show",
//!   "version": 1,
//!   "workspace": { ... }
//! }
//! ```

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use mazzalab_core::{CueId, CueKind, Workspace};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const FILE_EXTENSION: &str = "mazzalab";
pub const FORMAT: &str = "mazzalab.show";
pub const FORMAT_VERSION: u32 = 1;
/// Folder next to a show file that holds its safety copies.
pub const BACKUP_FOLDER: &str = "backups";

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
    #[error("not a MazzaLab show file (format is {0:?})")]
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

fn safety_copy(show: &Path, suffix: &str) -> PathBuf {
    let stem = show
        .file_stem()
        .map_or_else(|| "show".into(), |stem| stem.to_string_lossy().into_owned());
    show.parent()
        .unwrap_or(Path::new(""))
        .join(BACKUP_FOLDER)
        .join(format!("{stem}.{suffix}.{FILE_EXTENSION}"))
}

/// Where the `n`th most recent saved version of a show is kept; 1 is the
/// version the last save replaced.
pub fn backup_path(show: &Path, n: usize) -> PathBuf {
    safety_copy(show, &n.to_string())
}

/// Where unsaved changes to a show are written while it is being edited.
pub fn autosave_path(show: &Path) -> PathBuf {
    safety_copy(show, "autosave")
}

/// Saves a show like [`save`], first setting aside the version it replaces:
/// that becomes backup 1, older backups move up, and only `keep` are kept.
/// Saving the same contents again does not use up a backup.
pub fn save_with_backups(
    path: impl AsRef<Path>,
    workspace: &Workspace,
    keep: usize,
) -> Result<(), ProjectError> {
    let path = path.as_ref();
    if keep > 0
        && let Ok(current) = fs::read(path)
        && current != to_string(workspace)?.as_bytes()
    {
        let io_error = |source| ProjectError::Io {
            path: backup_path(path, 1),
            source,
        };
        if let Some(folder) = backup_path(path, 1).parent() {
            fs::create_dir_all(folder).map_err(io_error)?;
        }
        // The oldest falls off the end; a missing one just leaves a gap.
        let _ = fs::remove_file(backup_path(path, keep));
        for n in (1..keep).rev() {
            let _ = fs::rename(backup_path(path, n), backup_path(path, n + 1));
        }
        fs::write(backup_path(path, 1), current).map_err(io_error)?;
    }
    save(path, workspace)
}

/// Writes the current, unsaved state of a show next to it, to be offered
/// back if the program does not get to save it.
pub fn autosave(show: impl AsRef<Path>, workspace: &Workspace) -> Result<(), ProjectError> {
    let target = autosave_path(show.as_ref());
    if let Some(folder) = target.parent() {
        fs::create_dir_all(folder).map_err(|source| ProjectError::Io {
            path: target.clone(),
            source,
        })?;
    }
    save(&target, workspace)
}

/// Removes the autosaved state: it has been saved, or thrown away on purpose.
pub fn discard_autosave(show: impl AsRef<Path>) {
    let _ = fs::remove_file(autosave_path(show.as_ref()));
}

/// The autosaved state of a show, when there is one more recent than the
/// show file itself: changes that were never saved.
pub fn recoverable(show: impl AsRef<Path>) -> Option<Workspace> {
    let show = show.as_ref();
    let autosaved = autosave_path(show);
    let written = fs::metadata(&autosaved)
        .and_then(|meta| meta.modified())
        .ok()?;
    let saved = fs::metadata(show).and_then(|meta| meta.modified()).ok();
    if saved.is_some_and(|saved| saved >= written) {
        return None;
    }
    load(&autosaved).ok()
}

/// Rewrites relative media paths for a show moving from folder `from` to
/// folder `to` (e.g. "save as" elsewhere), so they keep naming the same
/// files. A file inside `to` stays relative; anything else becomes absolute.
/// Both folders should be absolute.
pub fn rebase_media(workspace: &mut Workspace, from: &Path, to: &Path) {
    if from == to {
        return;
    }
    let ids: Vec<CueId> = workspace.cues().map(|cue| cue.id).collect();
    for id in ids {
        let file = match workspace.find_cue_mut(id).map(|cue| &mut cue.kind) {
            Some(CueKind::Audio(audio)) => &mut audio.file,
            Some(CueKind::Video(video)) => &mut video.file,
            _ => continue,
        };
        if let Some(path) = file.as_mut().filter(|path| path.is_relative()) {
            let absolute = from.join(&*path);
            *path = absolute
                .strip_prefix(to)
                .map_or_else(|_| absolute.clone(), Path::to_path_buf);
        }
    }
}
