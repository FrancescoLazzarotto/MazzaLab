use std::time::Duration;

use mazzalab_core::{ContinueMode, Cue, CueKind, WaitCue, Workspace};
use mazzalab_project::{FORMAT, FORMAT_VERSION, ProjectError, from_str, load, save, to_string};

fn workspace() -> Workspace {
    let mut workspace = Workspace::new("Hamlet");
    workspace.cue_lists[0].cues = vec![
        Cue::new(CueKind::Memo)
            .with_number("0")
            .with_name("House open"),
        Cue::new(CueKind::Wait(WaitCue {
            duration: Duration::from_secs(3),
        }))
        .with_number("1")
        .with_continue(ContinueMode::AutoFollow),
    ];
    workspace
}

#[test]
fn text_round_trip() {
    let workspace = workspace();
    let text = to_string(&workspace).unwrap();
    assert!(text.contains(&format!("\"format\": \"{FORMAT}\"")));
    assert!(text.ends_with('\n'));
    assert_eq!(from_str(&text).unwrap(), workspace);
}

#[test]
fn rejects_other_formats() {
    let text = r#"{"format":"something.else","version":1,"workspace":{}}"#;
    assert!(matches!(from_str(text), Err(ProjectError::WrongFormat(f)) if f == "something.else"));
}

#[test]
fn rejects_newer_versions_before_parsing_the_body() {
    let text = format!(
        r#"{{"format":"{FORMAT}","version":{},"workspace":"from the future"}}"#,
        FORMAT_VERSION + 1
    );
    assert!(matches!(
        from_str(&text),
        Err(ProjectError::UnsupportedVersion { found }) if found == FORMAT_VERSION + 1
    ));
}

#[test]
fn save_and_load_file() {
    let dir = std::env::temp_dir().join(format!("mazzalab-project-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("show.mazzalab");

    let workspace = workspace();
    save(&path, &workspace).unwrap();
    // Saving again replaces the file in place.
    save(&path, &workspace).unwrap();
    assert_eq!(load(&path).unwrap(), workspace);
    assert!(!dir.join("show.mazzalab.tmp").exists());

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn missing_file_reports_its_path() {
    let error = load("does/not/exist.mazzalab").unwrap_err();
    assert!(error.to_string().contains("exist.mazzalab"));
}
