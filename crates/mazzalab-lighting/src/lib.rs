//! mazzalab-lighting
//!
//! Light cues, from text to the wire:
//!
//! * [`Patch`] reads console-style lines such as `1 thru 10 @ 50` or
//!   `front wash + cyc @ 75`, naming DMX addresses or the show's fixtures
//!   and groups;
//! * [`Rig`] holds the level of every DMX slot and fades between looks,
//!   deterministically, with no I/O;
//! * [`artnet_dmx`] and [`sacn_data`] build Art-Net and sACN (E1.31)
//!   packets;
//! * [`Sender`] runs a rig on a background thread and keeps the configured
//!   outputs fed.

mod command;
mod protocol;
mod rig;
mod sender;

pub use command::{Assignment, LightError, Patch, Slot, parse_command};
pub use protocol::{ARTNET_PORT, SACN_PORT, artnet_dmx, sacn_data, sacn_multicast};
pub use rig::Rig;
pub use sender::{Levels, Sender};

/// Slots in a DMX universe.
pub const UNIVERSE_SIZE: usize = 512;
