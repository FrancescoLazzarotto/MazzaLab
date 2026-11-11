//! The state of every DMX slot, and the fades between looks.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::UNIVERSE_SIZE;
use crate::command::{Assignment, Slot};

#[derive(Debug, Clone, Copy)]
struct Fade {
    from: f32,
    to: f32,
    start: Duration,
    length: Duration,
}

#[derive(Debug, Clone, Copy, Default)]
struct Channel {
    level: f32,
    fade: Option<Fade>,
}

/// Every slot a show has touched. A new look takes over the slots it names
/// (latest takes precedence) and leaves the others where they are, so looks
/// build on each other the way they do on a tracking console.
///
/// Time is whatever the caller says it is: the rig does no I/O and gives the
/// same output for the same calls.
#[derive(Debug, Clone, Default)]
pub struct Rig {
    channels: BTreeMap<Slot, Channel>,
}

impl Rig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sends slots to new levels over `fade`, starting at `now` from
    /// wherever each of them is at that moment.
    pub fn apply(&mut self, assignments: &[Assignment], fade: Duration, now: Duration) {
        self.advance(now);
        for assignment in assignments {
            let to = assignment.level.clamp(0.0, 1.0);
            for slot in &assignment.slots {
                let channel = self.channels.entry(*slot).or_default();
                if fade.is_zero() {
                    *channel = Channel {
                        level: to,
                        fade: None,
                    };
                } else {
                    channel.fade = Some(Fade {
                        from: channel.level,
                        to,
                        start: now,
                        length: fade,
                    });
                }
            }
        }
    }

    /// Moves every running fade to where it is at `now`. Returns whether any
    /// level changed.
    pub fn advance(&mut self, now: Duration) -> bool {
        let mut changed = false;
        for channel in self.channels.values_mut() {
            let Some(fade) = channel.fade else {
                continue;
            };
            let elapsed = now.saturating_sub(fade.start);
            let level = if elapsed >= fade.length {
                channel.fade = None;
                fade.to
            } else {
                let t = elapsed.as_secs_f32() / fade.length.as_secs_f32();
                fade.from + (fade.to - fade.from) * t
            };
            changed |= level != channel.level;
            channel.level = level;
        }
        changed
    }

    /// Whether any slot is still on its way to a level.
    pub fn is_fading(&self) -> bool {
        self.channels.values().any(|channel| channel.fade.is_some())
    }

    /// The universes that have ever been given a level, in order.
    pub fn universes(&self) -> Vec<u16> {
        let mut universes: Vec<u16> = self.channels.keys().map(|slot| slot.universe).collect();
        universes.dedup();
        universes
    }

    /// The level of a slot, from 0.0 to 1.0.
    pub fn level(&self, slot: Slot) -> f32 {
        self.channels
            .get(&slot)
            .map_or(0.0, |channel| channel.level)
    }

    /// One universe as the bytes that go on the wire.
    pub fn frame(&self, universe: u16) -> [u8; UNIVERSE_SIZE] {
        let mut frame = [0; UNIVERSE_SIZE];
        let first = Slot {
            universe,
            address: 1,
        };
        let last = Slot {
            universe,
            address: UNIVERSE_SIZE as u16,
        };
        for (slot, channel) in self.channels.range(first..=last) {
            frame[usize::from(slot.address) - 1] = (channel.level * 255.0).round() as u8;
        }
        frame
    }

    /// Every slot that is not at zero, with its level as a DMX value.
    pub fn lit(&self) -> Vec<(Slot, u8)> {
        self.channels
            .iter()
            .map(|(slot, channel)| (*slot, (channel.level * 255.0).round() as u8))
            .filter(|(_, value)| *value > 0)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_command;

    const fn slot(universe: u16, address: u16) -> Slot {
        Slot { universe, address }
    }

    fn secs(value: f32) -> Duration {
        Duration::from_secs_f32(value)
    }

    fn look(lines: &[&str]) -> Vec<Assignment> {
        lines
            .iter()
            .map(|line| parse_command(line).unwrap())
            .collect()
    }

    #[test]
    fn a_look_without_time_snaps() {
        let mut rig = Rig::new();
        rig.apply(
            &look(&["1 thru 2 @ 50", "2/3 @ full"]),
            Duration::ZERO,
            secs(0.0),
        );
        assert!(!rig.is_fading());
        assert_eq!(rig.frame(1)[..3], [128, 128, 0]);
        assert_eq!(rig.frame(2)[2], 255);
        assert_eq!(rig.universes(), [1, 2]);
        assert_eq!(
            rig.lit(),
            [(slot(1, 1), 128), (slot(1, 2), 128), (slot(2, 3), 255)]
        );
    }

    #[test]
    fn fades_run_from_where_each_slot_is() {
        let mut rig = Rig::new();
        rig.apply(&look(&["1 @ full"]), secs(4.0), secs(10.0));
        assert!(rig.is_fading());
        assert!(rig.advance(secs(11.0)));
        assert_eq!(rig.level(slot(1, 1)), 0.25);

        // A new look takes the slot over halfway and fades from there.
        rig.apply(&look(&["1 @ out"]), secs(2.0), secs(12.0));
        assert_eq!(rig.level(slot(1, 1)), 0.5);
        rig.advance(secs(13.0));
        assert_eq!(rig.level(slot(1, 1)), 0.25);
        rig.advance(secs(20.0));
        assert_eq!(rig.level(slot(1, 1)), 0.0);
        assert!(!rig.is_fading());
        // Nothing moves once the fades are done.
        assert!(!rig.advance(secs(21.0)));
    }

    #[test]
    fn looks_build_on_each_other() {
        let mut rig = Rig::new();
        rig.apply(&look(&["1 thru 4 @ 40"]), Duration::ZERO, secs(0.0));
        rig.apply(&look(&["2 @ full", "4 @ out"]), Duration::ZERO, secs(1.0));
        assert_eq!(rig.frame(1)[..4], [102, 255, 102, 0]);
        // Within one look the later line wins.
        rig.apply(&look(&["all @ 10", "1 @ 90"]), Duration::ZERO, secs(2.0));
        assert_eq!(rig.frame(1)[..3], [230, 26, 26]);
        assert_eq!(rig.frame(1)[511], 26);
    }
}
