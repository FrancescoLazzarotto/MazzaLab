use std::collections::{HashMap, HashSet};

use thiserror::Error;

use crate::cue::CueKind;
use crate::id::CueId;
use crate::workspace::Workspace;

/// A problem found by [`validate`]. None of these stop a show from loading,
/// but each one is something an operator should fix before opening night.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Issue {
    #[error("id {0} is used by more than one cue or cue list")]
    DuplicateId(CueId),
    #[error("cue number {0:?} is used by more than one cue")]
    DuplicateNumber(String),
    #[error("{kind} cue {cue} has no target")]
    MissingTarget { cue: CueId, kind: &'static str },
    #[error("cue {cue} targets {target}, which does not exist")]
    DanglingTarget { cue: CueId, target: CueId },
    #[error("cue {0} targets itself")]
    TargetsItself(CueId),
    #[error("cue {0} has a playback rate that is not a positive number")]
    InvalidRate(CueId),
}

/// Checks a workspace for structural problems.
pub fn validate(workspace: &Workspace) -> Vec<Issue> {
    let mut issues = Vec::new();

    let mut seen_ids = HashSet::new();
    for id in workspace
        .cue_lists
        .iter()
        .map(|list| list.id)
        .chain(workspace.cues().map(|cue| cue.id))
    {
        if !seen_ids.insert(id) {
            issues.push(Issue::DuplicateId(id));
        }
    }

    let mut numbers: HashMap<&str, usize> = HashMap::new();
    for number in workspace.cues().filter_map(|cue| cue.number.as_deref()) {
        *numbers.entry(number).or_default() += 1;
    }
    let mut duplicates: Vec<_> = numbers
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(number, _)| number.to_owned())
        .collect();
    duplicates.sort();
    issues.extend(duplicates.into_iter().map(Issue::DuplicateNumber));

    for cue in workspace.cues() {
        match cue.target {
            None if cue.kind.requires_target() => issues.push(Issue::MissingTarget {
                cue: cue.id,
                kind: cue.type_name(),
            }),
            Some(target) if target == cue.id => issues.push(Issue::TargetsItself(cue.id)),
            Some(target) if !seen_ids.contains(&target) => {
                issues.push(Issue::DanglingTarget {
                    cue: cue.id,
                    target,
                });
            }
            _ => {}
        }

        let rate = match &cue.kind {
            CueKind::Audio(audio) => Some(audio.rate),
            CueKind::Video(video) => Some(video.rate),
            CueKind::MidiFile(midi) => Some(midi.rate),
            _ => None,
        };
        if rate.is_some_and(|rate| !(rate.is_finite() && rate > 0.0)) {
            issues.push(Issue::InvalidRate(cue.id));
        }
    }

    issues
}
