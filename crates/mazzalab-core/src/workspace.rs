use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::cue::{Cue, MidiTrigger};
use crate::id::{CueId, WorkspaceId};
use crate::time::seconds;

/// An ordered list of cues with its own playhead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CueList {
    pub id: CueId,
    pub name: String,
    /// A cart: cues called up in no particular order, shown as a grid of
    /// buttons rather than as a list to GO through.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cart: bool,
    #[serde(default)]
    pub cues: Vec<Cue>,
}

impl CueList {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: CueId::new(),
            name: name.into(),
            cart: false,
            cues: Vec::new(),
        }
    }

    pub fn with_cart(mut self, cart: bool) -> Self {
        self.cart = cart;
        self
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

/// Which audio device the show plays on, and how it is opened.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AudioSettings {
    /// Device identifier as listed by the audio backend; `None` follows the
    /// system default output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// Requested sample rate; `None` uses the device's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    /// Requested buffer size in frames; `None` lets the driver choose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buffer_size: Option<u32>,
    /// Master level in dB applied to every output.
    #[serde(default)]
    pub master_db: f64,
    /// Device cues are auditioned on (headphones, a desk monitor), away
    /// from the show's own output; `None` is the system default output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audition_device: Option<String>,
    /// Plays every measured file as loud as this, in LUFS, on top of its
    /// cue's own level; `None` plays files as they are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalize_lufs: Option<f64>,
}

/// The most a loudness normalization moves a file, either way, in dB.
pub const MAX_NORMALIZE_DB: f64 = 20.0;

impl AudioSettings {
    /// What normalization adds to a file of `lufs`, in dB.
    pub fn normalize_gain(&self, lufs: Option<f64>) -> f64 {
        match (self.normalize_lufs, lufs) {
            (Some(target), Some(lufs)) => {
                (target - lufs).clamp(-MAX_NORMALIZE_DB, MAX_NORMALIZE_DB)
            }
            _ => 0.0,
        }
    }
}

/// How a network patch talks to its device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkProtocol {
    /// OSC messages in UDP datagrams.
    #[default]
    OscUdp,
    /// OSC messages over TCP with SLIP framing (OSC 1.1).
    OscTcp,
    /// The message text as it is, in a UDP datagram.
    PlainUdp,
    /// The message text as it is, over a TCP connection.
    PlainTcp,
}

/// A named network destination that Network cues send to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkPatch {
    pub name: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub protocol: NetworkProtocol,
}

/// A named MIDI output that MIDI cues send to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiPatch {
    pub name: String,
    /// Name of the output port, as the system lists it.
    pub port: String,
}

/// MIDI messages that work the show's transport: a GO pedal, a panic pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MidiControls {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub go: Option<MidiTrigger>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_all: Option<MidiTrigger>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panic: Option<MidiTrigger>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playhead_next: Option<MidiTrigger>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playhead_previous: Option<MidiTrigger>,
}

impl MidiControls {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// How DMX leaves the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DmxProtocol {
    /// Art-Net (ArtDmx), broadcast unless an address is given.
    #[default]
    ArtNet,
    /// sACN (ANSI E1.31), multicast unless an address is given.
    Sacn,
}

/// Sends one of the show's DMX universes to the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmxOutput {
    /// The show's universe, counted from 1, as light cues name it.
    pub universe: u16,
    #[serde(default)]
    pub protocol: DmxProtocol,
    /// Address of the node, optionally with a port; empty broadcasts
    /// (Art-Net) or uses the universe's multicast group (sACN).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host: String,
    /// The universe number on the wire: Art-Net counts from 0, sACN from 1.
    pub net_universe: u16,
}

/// A light in the rig, which Light cues call by its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fixture {
    pub name: String,
    /// The show's universe, counted from 1.
    #[serde(default = "first_universe")]
    pub universe: u16,
    /// Its first DMX address, from 1 to 512.
    pub address: u16,
    /// What each of its channels does, in order from the first address:
    /// `intensity`, `red`, `green`, `blue`, `pan`... A dimmer has just
    /// `intensity`.
    #[serde(default = "dimmer", skip_serializing_if = "is_dimmer")]
    pub parameters: Vec<String>,
}

impl Fixture {
    /// A plain dimmer at an address.
    pub fn dimmer(name: impl Into<String>, universe: u16, address: u16) -> Self {
        Self {
            name: name.into(),
            universe,
            address,
            parameters: dimmer(),
        }
    }

    pub fn with_parameters<S: Into<String>>(
        mut self,
        parameters: impl IntoIterator<Item = S>,
    ) -> Self {
        self.parameters = parameters.into_iter().map(Into::into).collect();
        self
    }
}

fn first_universe() -> u16 {
    1
}

fn dimmer() -> Vec<String> {
    vec!["intensity".to_owned()]
}

fn is_dimmer(parameters: &Vec<String>) -> bool {
    *parameters == dimmer()
}

/// Fixtures that Light cues set together, by one name. A group may also
/// hold other groups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureGroup {
    pub name: String,
    /// Names of fixtures or other groups.
    #[serde(default)]
    pub members: Vec<String>,
}

/// A screen the show puts video and text on: any client that opens the
/// stage by name shows it, scaled to fit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage {
    pub name: String,
    /// Its size in pixels, which cue geometry and text sizes are measured
    /// in; usually the projector's resolution.
    #[serde(default = "default_stage_width")]
    pub width: u32,
    #[serde(default = "default_stage_height")]
    pub height: u32,
    /// The display the desktop app puts it on, full screen, by the name
    /// the system gives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<String>,
}

impl Stage {
    /// A stage of the usual size, 1920 by 1080.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            width: default_stage_width(),
            height: default_stage_height(),
            screen: None,
        }
    }
}

fn default_stage_width() -> u32 {
    1920
}

fn default_stage_height() -> u32 {
    1080
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
    #[serde(default)]
    pub audio: AudioSettings,
    /// Destinations for Network cues; a cue without a patch uses the first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network_patches: Vec<NetworkPatch>,
    /// Outputs for MIDI cues; a cue without a patch uses the first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub midi_patches: Vec<MidiPatch>,
    /// Where the universes that Light cues set are sent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dmx_outputs: Vec<DmxOutput>,
    /// The lights of the rig, for Light cues to call by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixtures: Vec<Fixture>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixture_groups: Vec<FixtureGroup>,
    /// The stages Video and Text cues go to; without any, there is one
    /// called "Main".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<Stage>,
    /// Incoming MIDI messages that GO, stop or panic the show.
    #[serde(default, skip_serializing_if = "MidiControls::is_empty")]
    pub midi_controls: MidiControls,
    /// An audio input to read LTC from; `None` listens to none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ltc_input: Option<LtcInput>,
    /// The device ID (0–127) this show answers to for incoming MIDI Show
    /// Control. Without one, only messages to all devices (127) are
    /// answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msc_device_id: Option<u8>,
    /// Cues kept to make others from, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cue_templates: Vec<CueTemplate>,
    /// What clients on other machines may do, by passcode. Without any,
    /// every client may do everything; the machine running the show always
    /// may.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passcodes: Vec<Passcode>,
}

/// A passcode and what it lets a client do. Edit and control include view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Passcode {
    pub code: String,
    /// Who it is for, e.g. "Stage manager".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// See the show and its state.
    #[serde(default)]
    pub view: bool,
    /// Run it: GO, stop, playhead, live values.
    #[serde(default)]
    pub control: bool,
    /// Change it: cues, settings, saving, opening other shows.
    #[serde(default)]
    pub edit: bool,
}

/// Where linear timecode comes in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LtcInput {
    /// Input device identifier as listed by the audio backend; `None` is
    /// the system default input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// The channel the signal is on, from 1.
    #[serde(default = "first_channel")]
    pub channel: u16,
}

fn first_channel() -> u16 {
    1
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            double_go_protection: Duration::ZERO,
            panic_duration: default_panic_duration(),
            audio: AudioSettings::default(),
            network_patches: Vec::new(),
            midi_patches: Vec::new(),
            dmx_outputs: Vec::new(),
            fixtures: Vec::new(),
            fixture_groups: Vec::new(),
            stages: Vec::new(),
            midi_controls: MidiControls::default(),
            ltc_input: None,
            msc_device_id: None,
            cue_templates: Vec::new(),
            passcodes: Vec::new(),
        }
    }
}

/// A cue kept to make others from. Its id and number mean nothing: a new
/// cue made from it gets its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CueTemplate {
    pub name: String,
    pub cue: Cue,
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
