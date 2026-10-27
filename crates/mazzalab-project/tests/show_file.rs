use std::time::Duration;

use std::path::{Path, PathBuf};

use mazzalab_core::{AudioCue, ContinueMode, Cue, CueKind, VideoCue, WaitCue, Workspace};
use mazzalab_project::{
    FORMAT, FORMAT_VERSION, ProjectError, from_str, load, rebase_media, save, to_string,
};

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

fn media_files(workspace: &Workspace) -> Vec<Option<PathBuf>> {
    workspace
        .cues()
        .filter_map(|cue| match &cue.kind {
            CueKind::Audio(audio) => Some(audio.file.clone()),
            CueKind::Video(video) => Some(video.file.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn rebasing_media_keeps_files_found_from_the_new_folder() {
    let root = std::env::temp_dir().join("mazzalab-rebase");
    let (from, to) = (root.join("old"), root.join("old").join("copy"));
    let audio = |file: &Path| {
        Cue::new(CueKind::Audio(AudioCue {
            file: Some(file.to_owned()),
            ..AudioCue::default()
        }))
    };
    let elsewhere = root.join("library").join("music.wav");
    let mut workspace = Workspace::new("Rebase");
    workspace.cue_lists[0].cues = vec![
        audio(Path::new("thunder.wav")),
        audio(Path::new("copy/media/rain.wav")),
        audio(&elsewhere),
        Cue::new(CueKind::Video(VideoCue {
            file: Some("clip.mp4".into()),
            ..VideoCue::default()
        })),
        Cue::new(CueKind::Audio(AudioCue::default())),
    ];

    let unchanged = workspace.clone();
    rebase_media(&mut workspace, &from, &from);
    assert_eq!(workspace, unchanged);

    rebase_media(&mut workspace, &from, &to);
    assert_eq!(
        media_files(&workspace),
        vec![
            Some(from.join("thunder.wav")),
            Some(Path::new("media").join("rain.wav")),
            Some(elsewhere),
            Some(from.join("clip.mp4")),
            None,
        ]
    );
}
