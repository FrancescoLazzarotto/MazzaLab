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

    pub fn cue_list_mut(&mut self, id: CueId) -> Option<&mut CueList> {
        self.cue_lists.iter_mut().find(|list| list.id == id)
    }

    /// The cue list that contains `cue` (at any depth).
    pub fn list_of(&self, cue: CueId) -> Option<&CueList> {
        self.cue_lists.iter().find(|list| contains(&list.cues, cue))
    }

    /// Where a cue sits: its container and its index in it.
    pub fn locate(&self, cue: CueId) -> Option<(Container, usize)> {
        self.cue_lists
            .iter()
            .find_map(|list| locate_in(&list.cues, cue, Container::List(list.id)))
    }

    pub fn children_mut(&mut self, container: Container) -> Option<&mut Vec<Cue>> {
        match container {
            Container::List(id) => self.cue_list_mut(id).map(|list| &mut list.cues),
            Container::Group(id) => self.find_cue_mut(id)?.children_mut(),
        }
    }

    /// Removes a cue (with its children) and returns it with its old place.
    pub fn remove_cue(&mut self, cue: CueId) -> Option<(Container, usize, Cue)> {
        let (container, index) = self.locate(cue)?;
        let removed = self.children_mut(container)?.remove(index);
        Some((container, index, removed))
    }

    /// Inserts a cue into a container at `index` (appending when `None` or
    /// past the end). Gives the cue back if the container does not exist.
    pub fn insert_cue(
        &mut self,
        container: Container,
        index: Option<usize>,
        cue: Cue,
    ) -> Result<(), Box<Cue>> {
        let Some(children) = self.children_mut(container) else {
            return Err(Box::new(cue));
        };
        let index = index.unwrap_or(children.len()).min(children.len());
        children.insert(index, cue);
        Ok(())
    }
}

/// Something that holds an ordered list of cues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Container {
    List(CueId),
    Group(CueId),
}

fn contains(cues: &[Cue], id: CueId) -> bool {
    cues.iter()
        .any(|cue| cue.id == id || contains(cue.children(), id))
}

fn locate_in(cues: &[Cue], id: CueId, container: Container) -> Option<(Container, usize)> {
    cues.iter().enumerate().find_map(|(index, cue)| {
        if cue.id == id {
            Some((container, index))
        } else {
            locate_in(cue.children(), id, Container::Group(cue.id))
        }
    })
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
