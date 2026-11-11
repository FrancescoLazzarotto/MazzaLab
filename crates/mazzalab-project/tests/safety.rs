//! Backups made on save, autosave and recovery.

use std::path::PathBuf;

use mazzalab_core::Workspace;
use mazzalab_project::{
    autosave, autosave_path, backup_path, discard_autosave, load, recoverable, save,
    save_with_backups,
};

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mazzalab-safety-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn named(name: &str) -> Workspace {
    let mut workspace = Workspace::new("Show");
    workspace.name = name.to_owned();
    workspace
}

/// The file's modification time moved by `seconds`, so tests do not depend
/// on how fine the file system's clock is.
fn shift_modified(path: &std::path::Path, seconds: i64) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    let now = std::time::SystemTime::now();
    let moved = if seconds >= 0 {
        now + std::time::Duration::from_secs(seconds as u64)
    } else {
        now - std::time::Duration::from_secs(seconds.unsigned_abs())
    };
    file.set_modified(moved).unwrap();
}

#[test]
fn saving_keeps_the_versions_it_replaces() {
    let dir = folder("backups");
    let show = dir.join("Show.mazzalab");

    // The first save has nothing to set aside.
    save_with_backups(&show, &named("v1"), 2).unwrap();
    assert!(!backup_path(&show, 1).exists());

    save_with_backups(&show, &named("v2"), 2).unwrap();
    assert_eq!(load(backup_path(&show, 1)).unwrap().name, "v1");

    save_with_backups(&show, &named("v3"), 2).unwrap();
    assert_eq!(load(&show).unwrap().name, "v3");
    assert_eq!(load(backup_path(&show, 1)).unwrap().name, "v2");
    assert_eq!(load(backup_path(&show, 2)).unwrap().name, "v1");

    // Only two are kept: the oldest falls off.
    let v4 = named("v4");
    save_with_backups(&show, &v4, 2).unwrap();
    assert_eq!(load(backup_path(&show, 1)).unwrap().name, "v3");
    assert_eq!(load(backup_path(&show, 2)).unwrap().name, "v2");
    assert!(!backup_path(&show, 3).exists());

    // Saving again without changes does not push a real version out.
    save_with_backups(&show, &v4, 2).unwrap();
    save_with_backups(&show, &v4, 2).unwrap();
    assert_eq!(load(backup_path(&show, 1)).unwrap().name, "v3");
    assert_eq!(load(backup_path(&show, 2)).unwrap().name, "v2");

    // Keeping none makes no backup folder at all.
    let plain = dir.join("Plain.mazzalab");
    save_with_backups(&plain, &named("a"), 0).unwrap();
    save_with_backups(&plain, &named("b"), 0).unwrap();
    assert!(!backup_path(&plain, 1).exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn autosaved_changes_can_be_recovered_until_saved_or_discarded() {
    let dir = folder("autosave");
    let show = dir.join("Show.mazzalab");
    save(&show, &named("saved")).unwrap();
    assert!(recoverable(&show).is_none());

    autosave(&show, &named("edited")).unwrap();
    shift_modified(&autosave_path(&show), 5);
    assert_eq!(recoverable(&show).unwrap().name, "edited");
    // The show file itself is untouched.
    assert_eq!(load(&show).unwrap().name, "saved");

    // An autosave older than the show file was already saved.
    shift_modified(&autosave_path(&show), -3600);
    assert!(recoverable(&show).is_none());

    shift_modified(&autosave_path(&show), 5);
    discard_autosave(&show);
    assert!(recoverable(&show).is_none());
    // Discarding twice is fine.
    discard_autosave(&show);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_show_that_was_never_saved_can_be_recovered_too() {
    let dir = folder("unsaved");
    let show = dir.join("New.mazzalab");
    autosave(&show, &named("first draft")).unwrap();
    assert!(!show.exists());
    assert_eq!(recoverable(&show).unwrap().name, "first draft");

    // A damaged autosave is not offered.
    std::fs::write(autosave_path(&show), "not a show").unwrap();
    assert!(recoverable(&show).is_none());
    std::fs::remove_dir_all(dir).unwrap();
}
