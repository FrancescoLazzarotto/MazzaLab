//! The pre-show check: what in a show would not work if GO were pressed now.
//!
//! [`check`] looks at the workspace and at the media folder and returns a
//! list of [`Issue`]s: files that are missing, cues aimed at nothing or at
//! the wrong thing, patches and stages that do not exist, light commands
//! that cannot be read, empty groups, numbers and triggers used twice. Each
//! issue carries a `key` that front ends turn into a sentence in the
//! operator's language, and a `detail` with the specifics (a path, a name).

use std::collections::HashMap;
use std::path::Path;

use mazzalab_core::{Cue, CueId, CueKind, MidiTrigger, Workspace};
use mazzalab_lighting::Patch;
use serde::Serialize;

/// How much an issue matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The cue cannot do what it is for.
    Error,
    /// The cue works, but probably not as intended.
    Warning,
    /// Worth knowing.
    Info,
}

/// One thing to look at before the show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub severity: Severity,
    /// The cue it is about; none for the show as a whole.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cue: Option<CueId>,
    /// What kind of issue, in words for programs: `missing_media`,
    /// `no_target`, `unknown_patch`...
    pub key: &'static str,
    /// The specifics: the file, the patch, the line, the other cues.
    pub detail: String,
}

/// Checks `workspace`, looking for media in `base_dir` (the folder of the
/// show file) when a cue names its file relative to it.
pub fn check(workspace: &Workspace, base_dir: &Path) -> Vec<Issue> {
    check_with(workspace, &|file| {
        let path = if file.is_absolute() {
            file.to_path_buf()
        } else {
            base_dir.join(file)
        };
        path.is_file()
    })
}

/// [`check`] with the question "is there such a file?" answered by `exists`.
pub fn check_with(workspace: &Workspace, exists: &dyn Fn(&Path) -> bool) -> Vec<Issue> {
    let mut issues = Vec::new();
    let settings = &workspace.settings;
    let patch = Patch::new(&settings.fixtures, &settings.fixture_groups);
    let cues: Vec<&Cue> = workspace.cues().collect();

    for cue in &cues {
        let mut add = |severity, key, detail: String| {
            issues.push(Issue {
                severity,
                cue: Some(cue.id),
                key,
                detail,
            });
        };
        match &cue.kind {
            CueKind::Audio(audio) => media(audio.file.as_deref(), exists, &mut add),
            CueKind::Video(video) => {
                media(video.file.as_deref(), exists, &mut add);
                stage(video.stage.as_deref(), workspace, &mut add);
            }
            CueKind::Text(text) => stage(text.stage.as_deref(), workspace, &mut add),
            CueKind::Network(network) => {
                let known: Vec<&str> = settings
                    .network_patches
                    .iter()
                    .map(|patch| patch.name.as_str())
                    .collect();
                patch_named(network.patch.as_deref(), &known, &mut add);
                if network.message.trim().is_empty() {
                    add(Severity::Error, "no_message", String::new());
                }
            }
            CueKind::Midi(midi) => {
                let known: Vec<&str> = settings
                    .midi_patches
                    .iter()
                    .map(|patch| patch.name.as_str())
                    .collect();
                patch_named(midi.patch.as_deref(), &known, &mut add);
            }
            CueKind::Timecode(timecode) => {
                if timecode.signal == mazzalab_core::TimecodeSignal::Mtc {
                    let known: Vec<&str> = settings
                        .midi_patches
                        .iter()
                        .map(|patch| patch.name.as_str())
                        .collect();
                    patch_named(timecode.patch.as_deref(), &known, &mut add);
                }
            }
            CueKind::Light(light) => {
                for line in light.commands.iter().filter(|line| !line.trim().is_empty()) {
                    if let Err(error) = patch.parse(line) {
                        add(
                            Severity::Error,
                            "bad_light_command",
                            format!("{line}: {error}"),
                        );
                    }
                }
                if light.commands.iter().all(|line| line.trim().is_empty()) {
                    add(Severity::Error, "no_commands", String::new());
                }
            }
            CueKind::Group(group) if group.children.is_empty() => {
                add(Severity::Warning, "empty_group", String::new());
            }
            CueKind::Mic(_) | CueKind::Camera(_) | CueKind::MidiFile(_) | CueKind::Script(_) => {
                add(Severity::Info, "not_supported", cue.type_name().to_owned());
            }
            _ => {}
        }
        targets(cue, workspace, &mut add);
    }

    numbers(&cues, &mut issues);
    triggers(&cues, workspace, &mut issues);
    // Errors first, then warnings, then notes; each in the show's order.
    issues.sort_by_key(|issue| issue.severity);
    issues
}

/// An Audio, Video or MIDI File cue's file: named, and there.
fn media(
    file: Option<&Path>,
    exists: &dyn Fn(&Path) -> bool,
    add: &mut impl FnMut(Severity, &'static str, String),
) {
    match file {
        None => add(Severity::Error, "no_file", String::new()),
        Some(file) if !exists(file) => {
            add(Severity::Error, "missing_media", file.display().to_string())
        }
        Some(_) => {}
    }
}

/// A stage a cue names has to be one of the show's; without any, only
/// `Main` exists.
fn stage(
    name: Option<&str>,
    workspace: &Workspace,
    add: &mut impl FnMut(Severity, &'static str, String),
) {
    let Some(name) = name.filter(|name| !name.is_empty()) else {
        return;
    };
    let stages = &workspace.settings.stages;
    let known = if stages.is_empty() {
        name.eq_ignore_ascii_case("Main")
    } else {
        stages
            .iter()
            .any(|stage| stage.name.eq_ignore_ascii_case(name))
    };
    if !known {
        add(Severity::Warning, "unknown_stage", name.to_owned());
    }
}

/// A patch a cue names has to exist; a cue that names none uses the first,
/// which has to exist too.
fn patch_named(
    name: Option<&str>,
    known: &[&str],
    add: &mut impl FnMut(Severity, &'static str, String),
) {
    match name.filter(|name| !name.is_empty()) {
        Some(name) if !known.iter().any(|patch| patch.eq_ignore_ascii_case(name)) => {
            add(Severity::Error, "unknown_patch", name.to_owned());
        }
        None if known.is_empty() => add(Severity::Error, "no_patches", String::new()),
        _ => {}
    }
}

/// The cues a cue acts on: there, and of a kind it can act on.
fn targets(cue: &Cue, workspace: &Workspace, add: &mut impl FnMut(Severity, &'static str, String)) {
    if let CueKind::Target(target) = &cue.kind {
        if target.assigned_number.is_empty() {
            add(Severity::Error, "no_target", String::new());
        } else if workspace.find_by_number(&target.assigned_number).is_none() {
            add(
                Severity::Error,
                "missing_target",
                target.assigned_number.clone(),
            );
        }
        return;
    }
    if !cue.kind.requires_target() {
        return;
    }
    let Some(id) = cue.target else {
        add(Severity::Error, "no_target", String::new());
        return;
    };
    if id == cue.id {
        add(Severity::Error, "targets_itself", String::new());
        return;
    }
    let Some(target) = workspace.find_cue(id) else {
        add(Severity::Error, "missing_target", id.to_string());
        return;
    };
    let wrong = match &cue.kind {
        // A devamp is for something with slices or a loop to leave.
        CueKind::Devamp => !matches!(target.kind, CueKind::Audio(_)),
        // A fade moves a level, an opacity or a position.
        CueKind::Fade(_) => !matches!(
            target.kind,
            CueKind::Audio(_) | CueKind::Video(_) | CueKind::Text(_) | CueKind::Group(_)
        ),
        _ => false,
    };
    if wrong {
        add(
            Severity::Warning,
            "wrong_target",
            target.type_name().to_owned(),
        );
    }
}

/// Two cues with one number: OSC and the operators cannot tell them apart.
fn numbers(cues: &[&Cue], issues: &mut Vec<Issue>) {
    let mut by_number: HashMap<&str, Vec<&Cue>> = HashMap::new();
    for cue in cues {
        if let Some(number) = cue.number.as_deref().filter(|number| !number.is_empty()) {
            by_number.entry(number).or_default().push(cue);
        }
    }
    for cue in cues {
        let Some(number) = cue.number.as_deref().filter(|number| !number.is_empty()) else {
            continue;
        };
        if by_number[number].len() > 1 {
            issues.push(Issue {
                severity: Severity::Warning,
                cue: Some(cue.id),
                key: "duplicate_number",
                detail: number.to_owned(),
            });
        }
    }
}

/// Whether two MIDI triggers would both fire on some message.
fn overlap(a: &MidiTrigger, b: &MidiTrigger) -> bool {
    a.kind == b.kind
        && a.number == b.number
        && a.channel.is_none_or(|x| b.channel.is_none_or(|y| x == y))
        && a.value.is_none_or(|x| b.value.is_none_or(|y| x == y))
}

/// Triggers that more than one cue (or a transport control) answers to.
fn triggers(cues: &[&Cue], workspace: &Workspace, issues: &mut Vec<Issue>) {
    let label = |cue: &Cue| cue.number.clone().unwrap_or_else(|| cue.display_name());
    let mut report = |cue: &Cue, key: &'static str, others: Vec<String>| {
        if !others.is_empty() {
            issues.push(Issue {
                severity: Severity::Warning,
                cue: Some(cue.id),
                key,
                detail: others.join(", "),
            });
        }
    };
    let controls = &workspace.settings.midi_controls;
    let transport = [
        ("GO", &controls.go),
        ("Stop all", &controls.stop_all),
        ("Panic", &controls.panic),
        ("Playhead next", &controls.playhead_next),
        ("Playhead previous", &controls.playhead_previous),
    ];
    for cue in cues {
        let same_hotkey = |other: &&&Cue| matches!((&cue.triggers.hotkey, &other.triggers.hotkey), (Some(a), Some(b)) if a.eq_ignore_ascii_case(b));
        let hotkeys = cues
            .iter()
            .filter(|other| other.id != cue.id)
            .filter(same_hotkey)
            .map(|other| label(other))
            .collect();
        if cue.triggers.hotkey.is_some() {
            report(cue, "hotkey_collision", hotkeys);
        }

        if let Some(midi) = &cue.triggers.midi {
            let mut others: Vec<String> = cues
                .iter()
                .filter(|other| other.id != cue.id)
                .filter(|other| {
                    other
                        .triggers
                        .midi
                        .as_ref()
                        .is_some_and(|theirs| overlap(midi, theirs))
                })
                .map(|other| label(other))
                .collect();
            others.extend(
                transport
                    .iter()
                    .filter(|(_, control)| {
                        control
                            .as_ref()
                            .is_some_and(|control| overlap(midi, control))
                    })
                    .map(|(name, _)| (*name).to_owned()),
            );
            report(cue, "midi_collision", others);
        }

        if let Some(osc) = &cue.triggers.osc {
            let others = cues
                .iter()
                .filter(|other| other.id != cue.id)
                .filter(|other| other.triggers.osc.as_deref() == Some(osc.as_str()))
                .map(|other| label(other))
                .collect();
            report(cue, "osc_collision", others);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazzalab_core::{
        AudioCue, Fixture, GroupCue, LightCue, MidiTriggerKind, NetworkCue, NetworkPatch, Stage,
        TargetCue, TimecodeSignal, VideoCue,
    };
    use std::path::PathBuf;

    fn show(cues: Vec<Cue>) -> Workspace {
        let mut workspace = Workspace::new("Show");
        workspace.cue_lists[0].cues = cues;
        workspace
    }

    fn found(workspace: &Workspace, there: &[&str]) -> Vec<(Severity, &'static str, String)> {
        check_with(workspace, &|file| {
            there.iter().any(|name| file == Path::new(name))
        })
        .into_iter()
        .map(|issue| (issue.severity, issue.key, issue.detail))
        .collect()
    }

    fn audio(file: Option<&str>) -> Cue {
        Cue::new(CueKind::Audio(AudioCue {
            file: file.map(PathBuf::from),
            ..AudioCue::default()
        }))
    }

    #[test]
    fn a_clean_show_has_nothing_to_say() {
        let workspace = show(vec![
            audio(Some("media/a.wav")).with_number("1"),
            Cue::new(CueKind::Memo),
        ]);
        assert!(found(&workspace, &["media/a.wav"]).is_empty());
    }

    #[test]
    fn media_must_be_named_and_there() {
        let workspace = show(vec![audio(None), audio(Some("media/gone.wav"))]);
        let issues = found(&workspace, &[]);
        assert_eq!(issues[0], (Severity::Error, "no_file", String::new()));
        assert_eq!(
            issues[1],
            (Severity::Error, "missing_media", "media/gone.wav".into())
        );
    }

    #[test]
    fn the_media_folder_is_looked_in_for_relative_paths() {
        let dir = std::env::temp_dir().join(format!("mazzalab-preflight-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("media")).unwrap();
        std::fs::write(dir.join("media/here.wav"), b"x").unwrap();
        let workspace = show(vec![
            audio(Some("media/here.wav")),
            audio(Some("media/not.wav")),
        ]);
        let issues = check(&workspace, &dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].key, "missing_media");
    }

    #[test]
    fn targets_must_exist_and_suit_the_cue() {
        let sound = audio(Some("a.wav"));
        let video = Cue::new(CueKind::Video(VideoCue {
            file: Some("v.mp4".into()),
            ..VideoCue::default()
        }));
        let (sound_id, video_id) = (sound.id, video.id);
        let mut gone = Cue::new(CueKind::Start);
        gone.target = Some(CueId::new());
        let mut itself = Cue::new(CueKind::Stop);
        itself.target = Some(itself.id);
        let workspace = show(vec![
            sound,
            video,
            Cue::new(CueKind::Start),
            gone,
            itself,
            Cue::new(CueKind::Devamp).with_target(video_id),
            Cue::new(CueKind::Devamp).with_target(sound_id),
            Cue::new(CueKind::Target(TargetCue {
                assigned_number: "77".into(),
            })),
        ]);
        let keys: Vec<_> = found(&workspace, &["a.wav", "v.mp4"])
            .into_iter()
            .map(|(s, k, _)| (s, k))
            .collect();
        assert_eq!(
            keys,
            vec![
                (Severity::Error, "no_target"),
                (Severity::Error, "missing_target"),
                (Severity::Error, "targets_itself"),
                (Severity::Error, "missing_target"),
                (Severity::Warning, "wrong_target"),
            ]
        );
    }

    #[test]
    fn patches_and_stages_must_exist() {
        let mut workspace = show(vec![
            Cue::new(CueKind::Network(NetworkCue {
                patch: Some("Desk".into()),
                message: "/go".into(),
                ..NetworkCue::default()
            })),
            Cue::new(CueKind::Network(NetworkCue {
                message: "/go".into(),
                ..NetworkCue::default()
            })),
            Cue::new(CueKind::Video(VideoCue {
                file: Some("v.mp4".into()),
                stage: Some("Side".into()),
                ..VideoCue::default()
            })),
        ]);
        let keys = |workspace: &Workspace| {
            found(workspace, &["v.mp4"])
                .into_iter()
                .map(|(_, k, d)| (k, d))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            keys(&workspace),
            vec![
                ("unknown_patch", "Desk".into()),
                ("no_patches", String::new()),
                ("unknown_stage", "Side".into())
            ]
        );
        workspace.settings.network_patches.push(NetworkPatch {
            name: "desk".into(),
            host: "10.0.0.2".into(),
            port: 9000,
            protocol: Default::default(),
        });
        workspace.settings.stages.push(Stage::new("Side"));
        assert!(keys(&workspace).is_empty());
    }

    #[test]
    fn light_commands_must_be_readable() {
        let mut workspace = show(vec![Cue::new(CueKind::Light(LightCue {
            commands: vec![
                "1 thru 10 @ 50".into(),
                "front @ full".into(),
                "nonsense".into(),
            ],
            ..LightCue::default()
        }))]);
        let issues = found(&workspace, &[]);
        assert_eq!(issues.len(), 2);
        assert!(
            issues
                .iter()
                .all(|(severity, key, _)| *severity == Severity::Error
                    && *key == "bad_light_command")
        );
        workspace
            .settings
            .fixtures
            .push(Fixture::dimmer("Front", 1, 1));
        assert_eq!(found(&workspace, &[]).len(), 1);
        let empty = show(vec![Cue::new(CueKind::Light(LightCue::default()))]);
        assert_eq!(found(&empty, &[])[0].1, "no_commands");
    }

    #[test]
    fn empty_groups_and_cues_that_do_nothing_yet() {
        let workspace = show(vec![
            Cue::new(CueKind::Group(GroupCue::default())),
            Cue::new(CueKind::Group(GroupCue {
                children: vec![Cue::new(CueKind::Memo)],
                ..GroupCue::default()
            })),
            Cue::new(CueKind::Script(Default::default())),
        ]);
        let keys: Vec<_> = found(&workspace, &[])
            .into_iter()
            .map(|(s, k, _)| (s, k))
            .collect();
        assert_eq!(
            keys,
            vec![
                (Severity::Warning, "empty_group"),
                (Severity::Info, "not_supported")
            ]
        );
    }

    #[test]
    fn numbers_used_twice_are_reported_on_both_cues() {
        let workspace = show(vec![
            Cue::new(CueKind::Memo).with_number("1"),
            Cue::new(CueKind::Memo).with_number("1"),
            Cue::new(CueKind::Memo).with_number("2"),
        ]);
        let issues = check_with(&workspace, &|_| true);
        assert_eq!(
            issues
                .iter()
                .filter(|issue| issue.key == "duplicate_number")
                .count(),
            2
        );
        assert!(issues.iter().all(|issue| issue.detail == "1"));
    }

    #[test]
    fn triggers_that_two_cues_answer_to_are_reported() {
        let trigger = |kind, number, channel, value| MidiTrigger {
            kind,
            number,
            channel,
            value,
        };
        let mut a = Cue::new(CueKind::Memo).with_number("1");
        a.triggers.hotkey = Some("Q".into());
        a.triggers.midi = Some(trigger(MidiTriggerKind::NoteOn, 60, Some(1), None));
        a.triggers.osc = Some("/go/1".into());
        let mut b = Cue::new(CueKind::Memo).with_number("2");
        b.triggers.hotkey = Some("q".into());
        // Any channel overlaps channel 1; a different note does not.
        b.triggers.midi = Some(trigger(MidiTriggerKind::NoteOn, 60, None, Some(100)));
        b.triggers.osc = Some("/go/1".into());
        let mut c = Cue::new(CueKind::Memo).with_number("3");
        c.triggers.midi = Some(trigger(MidiTriggerKind::NoteOn, 61, Some(1), None));
        let mut workspace = show(vec![a, b, c]);
        workspace.settings.midi_controls.go =
            Some(trigger(MidiTriggerKind::NoteOn, 61, None, None));
        let issues = check_with(&workspace, &|_| true);
        let of = |key: &str| {
            issues
                .iter()
                .filter(|issue| issue.key == key)
                .map(|issue| issue.detail.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(of("hotkey_collision"), vec!["2", "1"]);
        assert_eq!(of("midi_collision"), vec!["2", "1", "GO"]);
        assert_eq!(of("osc_collision"), vec!["2", "1"]);
    }

    #[test]
    fn mtc_cues_need_a_midi_patch_but_ltc_ones_do_not() {
        use mazzalab_core::TimecodeCue;
        let make = |signal| {
            Cue::new(CueKind::Timecode(TimecodeCue {
                signal,
                start: "01:00:00:00".into(),
                ..TimecodeCue::default()
            }))
        };
        let workspace = show(vec![make(TimecodeSignal::Mtc), make(TimecodeSignal::Ltc)]);
        let issues = found(&workspace, &[]);
        assert_eq!(issues, vec![(Severity::Error, "no_patches", String::new())]);
    }

    #[test]
    fn errors_come_before_warnings_before_notes() {
        let workspace = show(vec![
            Cue::new(CueKind::Script(Default::default())),
            Cue::new(CueKind::Group(GroupCue::default())),
            audio(None),
        ]);
        let severities: Vec<_> = found(&workspace, &[])
            .into_iter()
            .map(|(s, _, _)| s)
            .collect();
        assert_eq!(
            severities,
            vec![Severity::Error, Severity::Warning, Severity::Info]
        );
    }
}
