//! The text of light cues: `selection @ level`.

use thiserror::Error;

use crate::UNIVERSE_SIZE;

/// One DMX slot: a universe (counted from 1) and an address in it (1–512).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Slot {
    pub universe: u16,
    pub address: u16,
}

/// A level for a set of slots.
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub slots: Vec<Slot>,
    /// From 0.0 (out) to 1.0 (full).
    pub level: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LightError {
    #[error("a light command reads \"channels @ level\", like \"1 thru 10 @ 50\"")]
    NoLevel,
    #[error("{0:?} is not a level; use 0 to 100, \"full\" or \"out\"")]
    BadLevel(String),
    #[error("{0:?} is not a channel; use an address from 1 to 512, or universe/address")]
    BadChannel(String),
    #[error("a range runs from a lower channel to a higher one in the same universe, unlike {0:?}")]
    BadRange(String),
    #[error("no channels before the level")]
    NoChannels,
}

fn level(text: &str) -> Result<f32, LightError> {
    let text = text.trim();
    let bad = || LightError::BadLevel(text.to_owned());
    match text.to_ascii_lowercase().as_str() {
        "full" | "ff" | "fl" => return Ok(1.0),
        "out" | "off" => return Ok(0.0),
        _ => {}
    }
    let percent: f32 = text
        .strip_suffix('%')
        .unwrap_or(text)
        .trim()
        .replace(',', ".")
        .parse()
        .map_err(|_| bad())?;
    if !(0.0..=100.0).contains(&percent) {
        return Err(bad());
    }
    Ok(percent / 100.0)
}

/// `7` is address 7 of universe 1; `2/7` is address 7 of universe 2.
fn slot(text: &str) -> Result<Slot, LightError> {
    let text = text.trim();
    let bad = || LightError::BadChannel(text.to_owned());
    let (universe, address) = match text.split_once('/') {
        Some((universe, address)) => (universe.trim().parse().map_err(|_| bad())?, address.trim()),
        None => (1, text),
    };
    let address: u16 = address.parse().map_err(|_| bad())?;
    if universe == 0 || address == 0 || usize::from(address) > UNIVERSE_SIZE {
        return Err(bad());
    }
    Ok(Slot { universe, address })
}

/// One term of a selection: a slot, a range, or a whole universe.
fn term(text: &str, slots: &mut Vec<Slot>) -> Result<(), LightError> {
    let text = text.trim();
    let lower = text.to_ascii_lowercase();
    // `all` is every slot of universe 1, `2/all` of universe 2.
    if let Some(universe) = lower
        .strip_suffix("all")
        .map(|rest| rest.trim().trim_end_matches('/').trim())
    {
        let universe: u16 = if universe.is_empty() {
            1
        } else {
            universe
                .parse()
                .ok()
                .filter(|universe| *universe > 0)
                .ok_or_else(|| LightError::BadChannel(text.to_owned()))?
        };
        slots.extend((1..=UNIVERSE_SIZE as u16).map(|address| Slot { universe, address }));
        return Ok(());
    }
    let range = lower
        .split_once("thru")
        .or_else(|| lower.split_once('>'))
        .or_else(|| lower.split_once('-'));
    match range {
        Some((first, last)) => {
            let first = slot(first)?;
            // The end of a range may leave the universe out: `2/1 thru 10`.
            let last = if last.contains('/') {
                slot(last)?
            } else {
                Slot {
                    universe: first.universe,
                    ..slot(last)?
                }
            };
            if last.universe != first.universe || last.address < first.address {
                return Err(LightError::BadRange(text.to_owned()));
            }
            slots.extend((first.address..=last.address).map(|address| Slot {
                universe: first.universe,
                address,
            }));
        }
        None => slots.push(slot(&lower)?),
    }
    Ok(())
}

/// Reads one command: channels joined by `+` (or `and`), single or as
/// ranges with `thru`, then `@` (or `=`) and a level in percent, `full` or
/// `out`.
///
/// ```
/// use mazzalab_lighting::parse_command;
/// let look = parse_command("1 thru 4 + 2/10 @ 75").unwrap();
/// assert_eq!(look.slots.len(), 5);
/// assert_eq!(look.level, 0.75);
/// ```
pub fn parse_command(line: &str) -> Result<Assignment, LightError> {
    let (selection, value) = line
        .split_once('@')
        .or_else(|| line.split_once('='))
        .ok_or(LightError::NoLevel)?;
    let level = level(value)?;
    let mut slots = Vec::new();
    let lower = selection.to_ascii_lowercase();
    for part in lower.split('+').flat_map(|part| part.split(" and ")) {
        if part.trim().is_empty() {
            return Err(LightError::NoChannels);
        }
        term(part, &mut slots)?;
    }
    slots.sort_unstable();
    slots.dedup();
    Ok(Assignment { slots, level })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addresses(line: &str) -> Vec<(u16, u16)> {
        parse_command(line)
            .unwrap()
            .slots
            .iter()
            .map(|slot| (slot.universe, slot.address))
            .collect()
    }

    #[test]
    fn single_channels_ranges_and_sums() {
        assert_eq!(addresses("7 @ 50"), [(1, 7)]);
        assert_eq!(addresses("1 thru 3 @ 50"), [(1, 1), (1, 2), (1, 3)]);
        assert_eq!(addresses("1-3 @ 50"), [(1, 1), (1, 2), (1, 3)]);
        assert_eq!(
            addresses("5 + 1 THRU 2 and 9 @ full"),
            [(1, 1), (1, 2), (1, 5), (1, 9)]
        );
        // Named twice, set once.
        assert_eq!(addresses("1 thru 2 + 2 @ 10"), [(1, 1), (1, 2)]);
    }

    #[test]
    fn other_universes() {
        assert_eq!(addresses("2/7 @ 50"), [(2, 7)]);
        assert_eq!(addresses("2/1 thru 3 @ 50"), [(2, 1), (2, 2), (2, 3)]);
        assert_eq!(addresses("2/1 thru 2/2 @ 50"), [(2, 1), (2, 2)]);
        assert_eq!(parse_command("all @ out").unwrap().slots.len(), 512);
        let second = parse_command("3/all @ 0").unwrap();
        assert_eq!(second.slots.len(), 512);
        assert!(second.slots.iter().all(|slot| slot.universe == 3));
    }

    #[test]
    fn levels() {
        let level = |line| parse_command(line).unwrap().level;
        assert_eq!(level("1 @ 0"), 0.0);
        assert_eq!(level("1 @ 50"), 0.5);
        assert_eq!(level("1 @ 12,5 %"), 0.125);
        assert_eq!(level("1 @ Full"), 1.0);
        assert_eq!(level("1 @ out"), 0.0);
        assert_eq!(level("1 = 75"), 0.75);
    }

    #[test]
    fn mistakes_are_explained() {
        assert_eq!(parse_command("1 thru 10"), Err(LightError::NoLevel));
        assert_eq!(
            parse_command("1 @ bright"),
            Err(LightError::BadLevel("bright".into()))
        );
        assert_eq!(
            parse_command("1 @ 101"),
            Err(LightError::BadLevel("101".into()))
        );
        assert_eq!(
            parse_command("0 @ 50"),
            Err(LightError::BadChannel("0".into()))
        );
        assert_eq!(
            parse_command("513 @ 50"),
            Err(LightError::BadChannel("513".into()))
        );
        assert_eq!(
            parse_command("front wash @ 50"),
            Err(LightError::BadChannel("front wash".into()))
        );
        assert!(matches!(
            parse_command("10 thru 1 @ 50"),
            Err(LightError::BadRange(_))
        ));
        assert!(matches!(
            parse_command("1/5 thru 2/6 @ 50"),
            Err(LightError::BadRange(_))
        ));
        assert_eq!(parse_command(" @ 50"), Err(LightError::NoChannels));
        assert_eq!(parse_command("1 + @ 50"), Err(LightError::NoChannels));
    }
}
