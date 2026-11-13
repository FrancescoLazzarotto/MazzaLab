//! The text of light cues: `selection @ level`.

use mazzalab_core::{Fixture, FixtureGroup, WorkspaceSettings};
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
    #[error("{0:?} is not a level; use 0 to 100, \"full\", \"out\" or a colour like #ff8000")]
    BadLevel(String),
    #[error("{0:?} is not a channel; use an address from 1 to 512, or universe/address")]
    BadChannel(String),
    #[error("{0:?} is neither a fixture nor a group of the patch")]
    UnknownName(String),
    #[error("{target:?} has no {parameter:?}")]
    NoParameter { target: String, parameter: String },
    #[error("{0:?} has no red, green and blue (or cyan, magenta and yellow) to mix a colour")]
    NoColour(String),
    #[error("{0:?} is not a fixture or group: only fixtures take a colour")]
    ColourNeedsFixtures(String),
    #[error("fixture {0:?} does not fit in its universe: its channels run past 512")]
    OutOfUniverse(String),
    #[error("a range runs from a lower channel to a higher one in the same universe, unlike {0:?}")]
    BadRange(String),
    #[error("no channels before the level")]
    NoChannels,
}

/// What a command sets its selection to.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Value {
    Level(f32),
    /// Red, green and blue, each from 0.0 to 1.0.
    Colour([f32; 3]),
}

fn read_value(text: &str) -> Result<Value, LightError> {
    let text = text.trim();
    let bad = || LightError::BadLevel(text.to_owned());
    if let Some(hex) = text.strip_prefix('#') {
        if hex.len() != 6 || !hex.is_ascii() {
            return Err(bad());
        }
        let mut colour = [0.0; 3];
        for (component, digits) in colour.iter_mut().zip([&hex[0..2], &hex[2..4], &hex[4..6]]) {
            *component = f32::from(u8::from_str_radix(digits, 16).map_err(|_| bad())?) / 255.0;
        }
        return Ok(Value::Colour(colour));
    }
    match text.to_ascii_lowercase().as_str() {
        "full" | "ff" | "fl" => return Ok(Value::Level(1.0)),
        "out" | "off" => return Ok(Value::Level(0.0)),
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
    Ok(Value::Level(percent / 100.0))
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

/// A term made of addresses: a slot, a range, or a whole universe.
fn addresses(text: &str, slots: &mut Vec<Slot>) -> Result<(), LightError> {
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

/// Names are matched ignoring case and runs of spaces.
fn normalize(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// What one term of a selection names.
enum Term<'a> {
    Slots(Vec<Slot>),
    Fixtures {
        /// As written, for messages.
        name: String,
        fixtures: Vec<&'a Fixture>,
        parameter: Option<String>,
    },
}

/// The fixtures and groups that Light cues can call by name, on top of
/// plain DMX addresses.
#[derive(Debug, Clone, Copy, Default)]
pub struct Patch<'a> {
    pub fixtures: &'a [Fixture],
    pub groups: &'a [FixtureGroup],
}

impl<'a> Patch<'a> {
    pub fn new(fixtures: &'a [Fixture], groups: &'a [FixtureGroup]) -> Self {
        Self { fixtures, groups }
    }

    /// The show's own patch.
    pub fn of(settings: &'a WorkspaceSettings) -> Self {
        Self::new(&settings.fixtures, &settings.fixture_groups)
    }

    /// The fixtures a name stands for: the fixture itself, or every fixture
    /// of a group and of the groups in it. `None` if nothing has that name.
    pub fn resolve(&self, name: &str) -> Result<Option<Vec<&'a Fixture>>, LightError> {
        let key = normalize(name);
        if let Some(fixture) = self.fixture(&key) {
            return Ok(Some(vec![fixture]));
        }
        if self.group(&key).is_none() {
            return Ok(None);
        }
        let mut fixtures = Vec::new();
        let mut seen = Vec::new();
        self.collect(&key, &mut fixtures, &mut seen)?;
        Ok(Some(fixtures))
    }

    fn fixture(&self, key: &str) -> Option<&'a Fixture> {
        self.fixtures
            .iter()
            .find(|fixture| normalize(&fixture.name) == key)
    }

    fn group(&self, key: &str) -> Option<&'a FixtureGroup> {
        self.groups
            .iter()
            .find(|group| normalize(&group.name) == key)
    }

    fn collect(
        &self,
        key: &str,
        fixtures: &mut Vec<&'a Fixture>,
        seen: &mut Vec<String>,
    ) -> Result<(), LightError> {
        if seen.iter().any(|visited| visited == key) {
            return Ok(());
        }
        seen.push(key.to_owned());
        if let Some(fixture) = self.fixture(key) {
            if !fixtures.iter().any(|known| std::ptr::eq(*known, fixture)) {
                fixtures.push(fixture);
            }
            return Ok(());
        }
        let group = self
            .group(key)
            .ok_or_else(|| LightError::UnknownName(key.to_owned()))?;
        for member in &group.members {
            self.collect(&normalize(member), fixtures, seen)?;
        }
        Ok(())
    }

    fn term(&self, text: &str) -> Result<Term<'a>, LightError> {
        let text = text.trim();
        if let Some(fixtures) = self.resolve(text)? {
            return Ok(Term::Fixtures {
                name: text.to_owned(),
                fixtures,
                parameter: None,
            });
        }
        // `front wash.red`: a parameter of a fixture or of a group.
        if let Some((name, parameter)) = text.rsplit_once('.')
            && let Some(fixtures) = self.resolve(name)?
        {
            return Ok(Term::Fixtures {
                name: name.trim().to_owned(),
                fixtures,
                parameter: Some(normalize(parameter)),
            });
        }
        let mut slots = Vec::new();
        match addresses(text, &mut slots) {
            Ok(()) => Ok(Term::Slots(slots)),
            // Words that are neither a name nor an address are a name that
            // is missing from the patch.
            Err(LightError::BadChannel(_))
                if text.starts_with(|first: char| first.is_alphabetic()) =>
            {
                Err(LightError::UnknownName(text.to_owned()))
            }
            Err(error) => Err(error),
        }
    }

    /// Reads one command: channels, fixtures and groups joined by `+` (or
    /// `and`), addresses single or as ranges with `thru`, then `@` (or `=`)
    /// and a level in percent, `full`, `out`, or a colour as `#rrggbb`.
    ///
    /// A fixture or group on its own sets its `intensity` (or, without one,
    /// all its channels); `name.parameter` sets one parameter.
    ///
    /// ```
    /// use mazzalab_core::{Fixture, FixtureGroup};
    /// use mazzalab_lighting::Patch;
    /// let fixtures = [
    ///     Fixture::dimmer("Front left", 1, 1),
    ///     Fixture::dimmer("Front right", 1, 2),
    ///     Fixture::dimmer("Back", 1, 3).with_parameters(["red", "green", "blue"]),
    /// ];
    /// let groups = [FixtureGroup { name: "Front".into(), members: vec!["Front left".into(), "Front right".into()] }];
    /// let patch = Patch::new(&fixtures, &groups);
    /// assert_eq!(patch.parse("front + 10 @ 75").unwrap()[0].slots.len(), 3);
    /// assert_eq!(patch.parse("back @ #ff8000").unwrap().len(), 3);
    /// ```
    pub fn parse(&self, line: &str) -> Result<Vec<Assignment>, LightError> {
        let (selection, value) = line
            .split_once('@')
            .or_else(|| line.split_once('='))
            .ok_or(LightError::NoLevel)?;
        let value = read_value(value)?;
        let mut terms = Vec::new();
        for part in selection.split('+').flat_map(split_and) {
            if part.trim().is_empty() {
                return Err(LightError::NoChannels);
            }
            terms.push(self.term(part)?);
        }
        match value {
            Value::Level(level) => {
                let mut slots = Vec::new();
                for term in terms {
                    match term {
                        Term::Slots(more) => slots.extend(more),
                        Term::Fixtures {
                            name,
                            fixtures,
                            parameter,
                        } => slots.extend(parameter_slots(&name, &fixtures, parameter.as_deref())?),
                    }
                }
                slots.sort_unstable();
                slots.dedup();
                Ok(vec![Assignment { slots, level }])
            }
            Value::Colour(colour) => {
                let mut assignments = Vec::new();
                for term in terms {
                    let Term::Fixtures {
                        name,
                        fixtures,
                        parameter: None,
                    } = term
                    else {
                        return Err(LightError::ColourNeedsFixtures(selection.trim().to_owned()));
                    };
                    let before = assignments.len();
                    for fixture in fixtures {
                        assignments.extend(colour_assignments(fixture, colour)?);
                    }
                    if assignments.len() == before {
                        return Err(LightError::NoColour(name));
                    }
                }
                Ok(assignments)
            }
        }
    }
}

/// `+` and the word `and` both join terms.
fn split_and(part: &str) -> impl Iterator<Item = &str> {
    let mut rest = part;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let lower = rest.to_ascii_lowercase();
        match lower.find(" and ") {
            Some(at) => {
                let (term, tail) = rest.split_at(at);
                rest = &tail[" and ".len()..];
                Some(term)
            }
            None => Some(std::mem::take(&mut rest)),
        }
    })
}

/// The slot of each of a fixture's channels, with its parameter's name.
fn channels(fixture: &Fixture) -> Result<Vec<(String, Slot)>, LightError> {
    let last = usize::from(fixture.address) + fixture.parameters.len().max(1) - 1;
    if fixture.universe == 0 || fixture.address == 0 || last > UNIVERSE_SIZE {
        return Err(LightError::OutOfUniverse(fixture.name.clone()));
    }
    Ok(fixture
        .parameters
        .iter()
        .zip(fixture.address..)
        .map(|(parameter, address)| {
            (
                normalize(parameter),
                Slot {
                    universe: fixture.universe,
                    address,
                },
            )
        })
        .collect())
}

/// Which channels a level goes to on some fixtures: the named parameter, or
/// the intensity, or, on fixtures that have none, every channel.
fn parameter_slots(
    name: &str,
    fixtures: &[&Fixture],
    parameter: Option<&str>,
) -> Result<Vec<Slot>, LightError> {
    let mut slots = Vec::new();
    for fixture in fixtures {
        let channels = channels(fixture)?;
        match parameter {
            Some(parameter) => slots.extend(
                channels
                    .iter()
                    .filter(|(name, _)| name == parameter)
                    .map(|(_, slot)| *slot),
            ),
            None => {
                let intensity: Vec<Slot> = channels
                    .iter()
                    .filter(|(name, _)| name == "intensity" || name == "dimmer")
                    .map(|(_, slot)| *slot)
                    .collect();
                if intensity.is_empty() {
                    slots.extend(channels.iter().map(|(_, slot)| *slot));
                } else {
                    slots.extend(intensity);
                }
            }
        }
    }
    if let (Some(parameter), true) = (parameter, slots.is_empty()) {
        return Err(LightError::NoParameter {
            target: name.to_owned(),
            parameter: parameter.to_owned(),
        });
    }
    Ok(slots)
}

/// A colour on one fixture: red, green and blue as they are, cyan, magenta
/// and yellow as what they take away. Nothing for a fixture with neither.
fn colour_assignments(fixture: &Fixture, colour: [f32; 3]) -> Result<Vec<Assignment>, LightError> {
    let mut assignments = Vec::new();
    for (parameter, slot) in channels(fixture)? {
        let level = match parameter.as_str() {
            "red" => colour[0],
            "green" => colour[1],
            "blue" => colour[2],
            "cyan" => 1.0 - colour[0],
            "magenta" => 1.0 - colour[1],
            "yellow" => 1.0 - colour[2],
            _ => continue,
        };
        assignments.push(Assignment {
            slots: vec![slot],
            level,
        });
    }
    Ok(assignments)
}

/// Reads one command made of DMX addresses only, with no patch: channels
/// joined by `+` (or `and`), single or as ranges with `thru`, then `@` (or
/// `=`) and a level in percent, `full` or `out`.
///
/// ```
/// use mazzalab_lighting::parse_command;
/// let look = parse_command("1 thru 4 + 2/10 @ 75").unwrap();
/// assert_eq!(look.slots.len(), 5);
/// assert_eq!(look.level, 0.75);
/// ```
pub fn parse_command(line: &str) -> Result<Assignment, LightError> {
    let mut assignments = Patch::default().parse(line)?;
    // Without fixtures there is no colour, so there is a single level.
    Ok(assignments.remove(0))
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
        assert_eq!(addresses("5 AND 6 @ full"), [(1, 5), (1, 6)]);
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
            parse_command("1 @ #12345"),
            Err(LightError::BadLevel("#12345".into()))
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
            Err(LightError::UnknownName("front wash".into()))
        );
        assert_eq!(
            parse_command("1 @ #ff0000"),
            Err(LightError::ColourNeedsFixtures("1".into()))
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

    /// Two dimmers on stage, an LED bar with a dimmer and a moving head
    /// with CMY on universe 2.
    fn rig() -> (Vec<Fixture>, Vec<FixtureGroup>) {
        let fixtures = vec![
            Fixture::dimmer("Front left", 1, 1),
            Fixture::dimmer("Front right", 1, 2),
            Fixture::dimmer("Cyc", 1, 10).with_parameters(["intensity", "red", "green", "blue"]),
            Fixture::dimmer("Spot", 2, 101).with_parameters([
                "pan",
                "tilt",
                "intensity",
                "cyan",
                "magenta",
                "yellow",
            ]),
            Fixture::dimmer("Par", 1, 20).with_parameters(["red", "green", "blue"]),
        ];
        let group = |name: &str, members: &[&str]| FixtureGroup {
            name: name.into(),
            members: members.iter().map(|member| (*member).to_owned()).collect(),
        };
        let groups = vec![
            group("Front wash", &["Front left", "Front right"]),
            group("Colour", &["Cyc", "Spot", "Par"]),
            // Groups in groups, even in a loop, and a fixture named twice.
            group(
                "Everything",
                &["front wash", "Colour", "Front left", "Everything"],
            ),
        ];
        (fixtures, groups)
    }

    fn set(patch: &Patch, line: &str) -> Vec<(u16, u16, f32)> {
        let mut out: Vec<(u16, u16, f32)> = patch
            .parse(line)
            .unwrap()
            .into_iter()
            .flat_map(|assignment| {
                assignment
                    .slots
                    .into_iter()
                    .map(move |slot| (slot.universe, slot.address, assignment.level))
            })
            .collect();
        out.sort_by_key(|(universe, address, _)| (*universe, *address));
        out
    }

    #[test]
    fn fixtures_and_groups_by_name() {
        let (fixtures, groups) = rig();
        let patch = Patch::new(&fixtures, &groups);
        assert_eq!(set(&patch, "front left @ 50"), [(1, 1, 0.5)]);
        assert_eq!(
            set(&patch, "  FRONT   wash @ full"),
            [(1, 1, 1.0), (1, 2, 1.0)]
        );
        // The intensity, wherever it is among the channels.
        assert_eq!(set(&patch, "spot @ 40"), [(2, 103, 0.4)]);
        // Without one, every channel.
        assert_eq!(
            set(&patch, "par @ 20"),
            [(1, 20, 0.2), (1, 21, 0.2), (1, 22, 0.2)]
        );
        // Mixed with addresses.
        assert_eq!(
            set(&patch, "front right + 2/1 and cyc @ 30"),
            [(1, 2, 0.3), (1, 10, 0.3), (2, 1, 0.3)]
        );
        // Groups of groups, each fixture once.
        assert_eq!(set(&patch, "everything @ out").len(), 2 + 1 + 1 + 3);
    }

    #[test]
    fn parameters() {
        let (fixtures, groups) = rig();
        let patch = Patch::new(&fixtures, &groups);
        assert_eq!(set(&patch, "spot.pan @ 50"), [(2, 101, 0.5)]);
        assert_eq!(
            set(&patch, "Front wash.Intensity @ 10"),
            [(1, 1, 0.1), (1, 2, 0.1)]
        );
        // Fixtures of the group that lack the parameter are left alone.
        assert_eq!(
            set(&patch, "colour.red @ full"),
            [(1, 11, 1.0), (1, 20, 1.0)]
        );
        assert_eq!(
            patch.parse("front wash.pan @ 50"),
            Err(LightError::NoParameter {
                target: "front wash".into(),
                parameter: "pan".into()
            })
        );
        assert_eq!(
            patch.parse("ghost.red @ 50"),
            Err(LightError::UnknownName("ghost.red".into()))
        );
    }

    #[test]
    fn colours() {
        let (fixtures, groups) = rig();
        let patch = Patch::new(&fixtures, &groups);
        assert_eq!(
            set(&patch, "cyc @ #ff8000"),
            [(1, 11, 1.0), (1, 12, 128.0 / 255.0), (1, 13, 0.0)]
        );
        // Subtractive mixing takes away what is not wanted.
        assert_eq!(
            set(&patch, "spot @ #FF0000"),
            [(2, 104, 0.0), (2, 105, 1.0), (2, 106, 1.0)]
        );
        // A group colours those of its fixtures that can.
        assert_eq!(set(&patch, "everything @ #ffffff").len(), 9);
        assert_eq!(
            patch.parse("front wash @ #ffffff"),
            Err(LightError::NoColour("front wash".into()))
        );
        assert_eq!(
            patch.parse("cyc.red @ #ffffff"),
            Err(LightError::ColourNeedsFixtures("cyc.red".into()))
        );
    }

    #[test]
    fn a_broken_patch_is_reported() {
        let fixtures =
            vec![Fixture::dimmer("Edge", 1, 511).with_parameters(["red", "green", "blue"])];
        let groups = vec![FixtureGroup {
            name: "Stray".into(),
            members: vec!["Nobody".into()],
        }];
        let patch = Patch::new(&fixtures, &groups);
        assert_eq!(
            patch.parse("edge @ 50"),
            Err(LightError::OutOfUniverse("Edge".into()))
        );
        assert_eq!(
            patch.parse("stray @ 50"),
            Err(LightError::UnknownName("nobody".into()))
        );
    }
}
