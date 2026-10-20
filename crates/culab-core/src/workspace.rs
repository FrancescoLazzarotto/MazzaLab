use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::cue::Cue;
use crate::id::{CueId, WorkspaceId};
use crate::time::seconds;

/// An ordered list of cues with its own playhead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CueList {
    pub id: CueId,
    pub name: String,
    #[serde(default)]
    pub cues: Vec<Cue>,
}

impl CueList {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: CueId::new(),
            name: name.into(),
            cues: Vec::new(),
        }
    }

    pub fn with_id(mut self, id: CueId) -> Self {
        self.id = id;
        self
    }

    pub fn with_cues(mut self, cues: Vec<Cue>) -> Self {
        self.cues = cues;
        self
    }
}

fn default_panic_duration() -> Duration {
    Duration::from_secs(1)
}

/// Workspace-wide behaviour settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSettings {
    /// GO commands arriving within this window after a previous GO are ignored.
    #[serde(default, with = "seconds")]
    pub double_go_protection: Duration,
    /// How long a panic fades everything out before stopping.
    #[serde(default = "default_panic_duration", with = "seconds")]
    pub panic_duration: Duration,
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            double_go_protection: Duration::ZERO,
            panic_duration: default_panic_duration(),
        }
    }
}

/// One show document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    #[serde(default)]
    pub settings: WorkspaceSettings,
    #[serde(default)]
    pub cue_lists: Vec<CueList>,
}

impl Workspace {
    /// Creates a workspace with a single empty cue list.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: WorkspaceId::new(),
            name: name.into(),
            settings: WorkspaceSettings::default(),
            cue_lists: vec![CueList::new("Main Cue List")],
        }
    }

    pub fn cue_list(&self, id: CueId) -> Option<&CueList> {
        self.cue_lists.iter().find(|list| list.id == id)
    }

    /// Every cue in the workspace, depth first, in list order.
    pub fn cues(&self) -> impl Iterator<Item = &Cue> {
        let mut out = Vec::new();
        for list in &self.cue_lists {
            collect(&list.cues, &mut out);
        }
        out.into_iter()
    }

    pub fn find_cue(&self, id: CueId) -> Option<&Cue> {
        self.cues().find(|cue| cue.id == id)
    }

    pub fn find_cue_mut(&mut self, id: CueId) -> Option<&mut Cue> {
        self.cue_lists
            .iter_mut()
            .find_map(|list| find_mut(&mut list.cues, id))
    }

    pub fn find_by_number(&self, number: &str) -> Option<&Cue> {
        self.cues()
            .find(|cue| cue.number.as_deref() == Some(number))
    }
}

fn collect<'a>(cues: &'a [Cue], out: &mut Vec<&'a Cue>) {
    for cue in cues {
        out.push(cue);
        collect(cue.children(), out);
    }
}

fn find_mut(cues: &mut [Cue], id: CueId) -> Option<&mut Cue> {
    for cue in cues {
        if cue.id == id {
            return Some(cue);
        }
        if let Some(found) = cue
            .children_mut()
            .and_then(|children| find_mut(children, id))
        {
            return Some(found);
        }
    }
    None
}
