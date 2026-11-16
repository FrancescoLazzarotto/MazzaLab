//! Looking after a show's media: which files the cues name, which of them
//! are missing, gathering the ones that live elsewhere into the show's media
//! folder, and finding missing ones again under a folder.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use mazzalab_core::{Cue, CueId, CueKind, Workspace};
use serde::Serialize;

/// How deep a search for missing files goes below the folder given.
pub const SEARCH_DEPTH: usize = 6;
/// How many directory entries a search looks at before it gives up.
const SEARCH_LIMIT: usize = 200_000;

/// A cue's media file, as the cue names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reference {
    pub cue: CueId,
    pub file: PathBuf,
}

/// The file a cue plays, for the cue kinds that play one.
fn file_of(cue: &Cue) -> Option<&Path> {
    match &cue.kind {
        CueKind::Audio(audio) => audio.file.as_deref(),
        CueKind::Video(video) => video.file.as_deref(),
        CueKind::MidiFile(midi) => midi.file.as_deref(),
        _ => None,
    }
}

/// Every media file a cue names, in the show's order.
pub fn references(workspace: &Workspace) -> Vec<Reference> {
    workspace
        .cues()
        .filter_map(|cue| {
            file_of(cue).map(|file| Reference {
                cue: cue.id,
                file: file.to_path_buf(),
            })
        })
        .collect()
}

/// Where a cue's file is on disk: relative paths are relative to the show.
pub fn resolve(base: &Path, file: &Path) -> PathBuf {
    if file.is_absolute() {
        file.to_path_buf()
    } else {
        base.join(file)
    }
}

/// The references whose files are not there.
pub fn missing(workspace: &Workspace, base: &Path) -> Vec<Reference> {
    references(workspace)
        .into_iter()
        .filter(|reference| !resolve(base, &reference.file).is_file())
        .collect()
}

/// `cue` with its media file replaced; `None` for a cue that has none.
pub fn with_file(cue: &Cue, file: PathBuf) -> Option<Cue> {
    let mut cue = cue.clone();
    match &mut cue.kind {
        CueKind::Audio(audio) => audio.file = Some(file),
        CueKind::Video(video) => video.file = Some(file),
        CueKind::MidiFile(midi) => midi.file = Some(file),
        _ => return None,
    }
    Some(cue)
}

/// A path as cues write them: forward slashes, whatever the system.
pub fn portable(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Whether `path` is inside `dir`, looking through links and `..`. A path
/// that cannot be looked at is taken as outside.
pub fn inside(dir: &Path, path: &Path) -> bool {
    match (dir.canonicalize(), path.canonicalize()) {
        (Ok(dir), Ok(path)) => path.starts_with(dir),
        _ => false,
    }
}

/// A copy to make: the file a cue names now, outside the media folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outside {
    pub cue: CueId,
    /// As the cue names it.
    pub file: PathBuf,
    /// Where it is on disk.
    pub source: PathBuf,
}

/// The cues whose files exist but lie outside `media`.
pub fn outside(workspace: &Workspace, base: &Path, media: &Path) -> Vec<Outside> {
    references(workspace)
        .into_iter()
        .filter_map(|reference| {
            let source = resolve(base, &reference.file);
            (source.is_file() && !inside(media, &source)).then_some(Outside {
                cue: reference.cue,
                file: reference.file,
                source,
            })
        })
        .collect()
}

/// A file found by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
}

/// Looks under `root` for files with these names (compared without regard to
/// case), breadth first so that the nearest one wins, down to
/// [`SEARCH_DEPTH`] levels and a bounded number of entries. Hidden folders
/// are not entered.
pub fn search(root: &Path, names: &HashSet<String>) -> HashMap<String, Found> {
    let mut found: HashMap<String, Found> = HashMap::new();
    let mut queue = VecDeque::from([(root.to_path_buf(), 0_usize)]);
    let mut looked = 0;
    while let Some((dir, depth)) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            looked += 1;
            if looked > SEARCH_LIMIT {
                return found;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if depth < SEARCH_DEPTH && !name.starts_with('.') {
                    queue.push_back((entry.path(), depth + 1));
                }
            } else if kind.is_file() {
                let key = name.to_lowercase();
                if names.contains(&key) {
                    found.entry(key).or_insert(Found { path: entry.path() });
                }
            }
        }
        if found.len() == names.len() {
            break;
        }
    }
    found
}

/// The lower-case file name of a path, as `search` compares them.
pub fn key_of(file: &Path) -> Option<String> {
    // Paths written on another system may use either separator.
    let text = file.to_string_lossy();
    let name = text.rsplit(['/', '\\']).next()?;
    (!name.is_empty()).then(|| name.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazzalab_core::AudioCue;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mazzalab-mm-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sound(file: &str) -> Cue {
        Cue::new(CueKind::Audio(AudioCue {
            file: Some(PathBuf::from(file)),
            ..AudioCue::default()
        }))
    }

    fn show(cues: Vec<Cue>) -> Workspace {
        let mut workspace = Workspace::new("Show");
        workspace.cue_lists[0].cues = cues;
        workspace
    }

    #[test]
    fn references_and_missing_files() {
        let base = dir("missing");
        std::fs::create_dir_all(base.join("media")).unwrap();
        std::fs::write(base.join("media/here.wav"), b"x").unwrap();
        let workspace = show(vec![
            sound("media/here.wav"),
            sound("media/gone.wav"),
            Cue::new(CueKind::Memo),
        ]);
        assert_eq!(references(&workspace).len(), 2);
        let lost = missing(&workspace, &base);
        assert_eq!(lost.len(), 1);
        assert_eq!(lost[0].file, PathBuf::from("media/gone.wav"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_cues_file_can_be_replaced() {
        let cue = sound("a.wav");
        let changed = with_file(&cue, PathBuf::from("media/a.wav")).unwrap();
        assert_eq!(file_of(&changed), Some(Path::new("media/a.wav")));
        assert_eq!(changed.id, cue.id);
        assert!(with_file(&Cue::new(CueKind::Memo), PathBuf::from("x")).is_none());
    }

    #[test]
    fn files_outside_the_media_folder_are_found() {
        let base = dir("outside");
        let media = base.join("media");
        std::fs::create_dir_all(&media).unwrap();
        std::fs::write(media.join("in.wav"), b"1").unwrap();
        let elsewhere = dir("outside-else");
        std::fs::write(elsewhere.join("out.wav"), b"2").unwrap();
        std::fs::write(base.join("loose.wav"), b"3").unwrap();
        let workspace = show(vec![
            sound("media/in.wav"),
            sound(&elsewhere.join("out.wav").to_string_lossy()),
            sound("loose.wav"),
            sound("media/gone.wav"),
        ]);
        let found = outside(&workspace, &base, &media);
        let names: Vec<_> = found
            .iter()
            .map(|item| {
                item.source
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, vec!["out.wav", "loose.wav"]);
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&elsewhere);
    }

    #[test]
    fn search_finds_files_by_name_in_nested_folders_nearest_first() {
        let root = dir("search");
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("a/b/Deep.WAV"), b"x").unwrap();
        std::fs::write(root.join("a/near.wav"), b"x").unwrap();
        std::fs::write(root.join("near.wav"), b"x").unwrap();
        std::fs::write(root.join(".hidden/secret.wav"), b"x").unwrap();
        let names = ["deep.wav", "near.wav", "secret.wav", "nowhere.wav"]
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        let found = search(&root, &names);
        assert_eq!(found["deep.wav"].path, root.join("a/b/Deep.WAV"));
        assert_eq!(found["near.wav"].path, root.join("near.wav"));
        assert!(!found.contains_key("secret.wav"));
        assert!(!found.contains_key("nowhere.wav"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_search_stops_at_its_depth() {
        let root = dir("depth");
        let mut deep = root.clone();
        for level in 0..=SEARCH_DEPTH + 1 {
            deep = deep.join(format!("d{level}"));
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("far.wav"), b"x").unwrap();
        let names = HashSet::from(["far.wav".to_owned()]);
        assert!(search(&root, &names).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn names_are_taken_from_either_kind_of_path() {
        assert_eq!(
            key_of(Path::new("C:\\Music\\Rain.WAV")).as_deref(),
            Some("rain.wav")
        );
        assert_eq!(
            key_of(Path::new("media/Rain.wav")).as_deref(),
            Some("rain.wav")
        );
        assert_eq!(portable(Path::new("media\\a.wav")), "media/a.wav");
    }
}
