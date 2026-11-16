//! MIDI Show Control received: the System Exclusive messages of the MSC
//! standard, read into a typed message, and what they ask of a show.
//!
//! A message is `F0 7F <device> 02 <command format> <command> <data> F7`.
//! `device` is `00`..`6F` for one device, `70`..`7E` for a group and `7F`
//! for all. The data of the commands that name a cue is its number and,
//! after a `00`, its list and then its path, as ASCII digits and dots.

use mazzalab_core::{CueId, Workspace};

/// The commands of MIDI Show Control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MscCommand {
    Go,
    Stop,
    Resume,
    TimedGo,
    Load,
    Set,
    Fire,
    AllOff,
    Restore,
    Reset,
    GoOff,
    GoJamClock,
    StandbyPlus,
    StandbyMinus,
    SequencePlus,
    SequenceMinus,
    StartClock,
    StopClock,
    ZeroClock,
    SetClock,
    MtcChaseOn,
    MtcChaseOff,
    OpenCueList,
    CloseCueList,
    OpenCuePath,
    CloseCuePath,
    /// A command this crate does not know, by its code.
    Other(u8),
}

impl MscCommand {
    pub fn from_code(code: u8) -> Self {
        match code {
            0x01 => Self::Go,
            0x02 => Self::Stop,
            0x03 => Self::Resume,
            0x04 => Self::TimedGo,
            0x05 => Self::Load,
            0x06 => Self::Set,
            0x07 => Self::Fire,
            0x08 => Self::AllOff,
            0x09 => Self::Restore,
            0x0A => Self::Reset,
            0x0B => Self::GoOff,
            0x10 => Self::GoJamClock,
            0x11 => Self::StandbyPlus,
            0x12 => Self::StandbyMinus,
            0x13 => Self::SequencePlus,
            0x14 => Self::SequenceMinus,
            0x15 => Self::StartClock,
            0x16 => Self::StopClock,
            0x17 => Self::ZeroClock,
            0x18 => Self::SetClock,
            0x19 => Self::MtcChaseOn,
            0x1A => Self::MtcChaseOff,
            0x1B => Self::OpenCueList,
            0x1C => Self::CloseCueList,
            0x1D => Self::OpenCuePath,
            0x1E => Self::CloseCuePath,
            other => Self::Other(other),
        }
    }

    pub fn code(self) -> u8 {
        match self {
            Self::Go => 0x01,
            Self::Stop => 0x02,
            Self::Resume => 0x03,
            Self::TimedGo => 0x04,
            Self::Load => 0x05,
            Self::Set => 0x06,
            Self::Fire => 0x07,
            Self::AllOff => 0x08,
            Self::Restore => 0x09,
            Self::Reset => 0x0A,
            Self::GoOff => 0x0B,
            Self::GoJamClock => 0x10,
            Self::StandbyPlus => 0x11,
            Self::StandbyMinus => 0x12,
            Self::SequencePlus => 0x13,
            Self::SequenceMinus => 0x14,
            Self::StartClock => 0x15,
            Self::StopClock => 0x16,
            Self::ZeroClock => 0x17,
            Self::SetClock => 0x18,
            Self::MtcChaseOn => 0x19,
            Self::MtcChaseOff => 0x1A,
            Self::OpenCueList => 0x1B,
            Self::CloseCueList => 0x1C,
            Self::OpenCuePath => 0x1D,
            Self::CloseCuePath => 0x1E,
            Self::Other(code) => code,
        }
    }

    /// Whether the command's data starts with a cue number.
    fn names_a_cue(self) -> bool {
        matches!(
            self,
            Self::Go | Self::Stop | Self::Resume | Self::TimedGo | Self::Load | Self::GoOff
        )
    }
}

/// The cue a command names: a number such as `12.5`, in a list and a path
/// when the message says so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MscCue {
    pub number: String,
    pub list: Option<String>,
    pub path: Option<String>,
}

/// A MIDI Show Control message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MscMessage {
    /// `00`..`6F` one device, `70`..`7E` a group, `7F` every device.
    pub device_id: u8,
    /// What kind of equipment it is for (`01` lighting general, `7F` all...).
    pub command_format: u8,
    pub command: MscCommand,
    /// The cue the command names, if it names one.
    pub cue: Option<MscCue>,
    /// The time of a Timed GO: hours, minutes, seconds, frames, status.
    pub time: Option<[u8; 5]>,
    /// The data bytes as they came, for commands this crate does not read.
    pub data: Vec<u8>,
}

impl MscMessage {
    /// Whether the message is for the device with this ID: its own, or all.
    pub fn is_for(&self, device_id: u8) -> bool {
        self.device_id == 0x7F || self.device_id == device_id
    }
}

/// Reads a whole System Exclusive message, `F0` to `F7`, as MIDI Show
/// Control. `None` when it is anything else, or damaged.
pub fn parse_msc(bytes: &[u8]) -> Option<MscMessage> {
    let [
        0xF0,
        0x7F,
        device_id,
        0x02,
        command_format,
        command,
        data @ ..,
        0xF7,
    ] = bytes
    else {
        return None;
    };
    if [*device_id, *command_format, *command, 0]
        .iter()
        .chain(data)
        .any(|byte| *byte >= 0x80)
    {
        return None;
    }
    let command = MscCommand::from_code(*command);
    let mut rest = data;
    let mut time = None;
    if command == MscCommand::TimedGo
        && let Some((clock, after)) = rest.split_first_chunk::<5>()
    {
        time = Some(*clock);
        rest = after;
    }
    let cue = if command.names_a_cue() {
        parse_cue(rest)
    } else {
        None
    };
    Some(MscMessage {
        device_id: *device_id,
        command_format: *command_format,
        command,
        cue,
        time,
        data: data.to_vec(),
    })
}

/// `number [00 list [00 path]]`, each part digits and dots.
fn parse_cue(data: &[u8]) -> Option<MscCue> {
    let mut parts = data.split(|byte| *byte == 0x00);
    let text = |part: &[u8]| -> Option<String> {
        (!part.is_empty()
            && part
                .iter()
                .all(|byte| byte.is_ascii_digit() || *byte == b'.'))
        .then(|| part.iter().map(|byte| *byte as char).collect())
    };
    let number = text(parts.next()?)?;
    let list = parts.next().and_then(text);
    let path = parts.next().and_then(text);
    Some(MscCue { number, list, path })
}

/// What a received MSC message asks of the show, with cue numbers resolved
/// to the show's cues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MscAction {
    /// GO on the current list.
    Go,
    Start(CueId),
    Pause(CueId),
    PauseAll,
    Resume(CueId),
    ResumeAll,
    StopAll,
    Stop(CueId),
    Reset,
    /// Stand by on a cue.
    SetPlayhead(CueId),
    PlayheadNext,
    PlayheadPrevious,
}

/// What the show does for `message`, if it is for `device_id` and is a
/// command a show can follow; `None` otherwise, and for a cue number the
/// show does not have.
///
/// STOP halts a cue and RESUME lets it go on, as the standard has them, so
/// they pause and resume; ALL OFF and GO OFF end cues. LOAD stands by on a
/// cue, and the standby and sequence commands move the playhead. Cue lists
/// and paths named in the message are not told apart: a number is looked up
/// in the whole show.
pub fn action_for(message: &MscMessage, device_id: u8, workspace: &Workspace) -> Option<MscAction> {
    if !message.is_for(device_id) {
        return None;
    }
    let named = || -> Option<CueId> {
        let cue = message.cue.as_ref()?;
        workspace.find_by_number(&cue.number).map(|cue| cue.id)
    };
    // A command that names a cue the show lacks does nothing, rather than
    // doing the same to every cue.
    let names_one = message.cue.is_some();
    Some(match message.command {
        MscCommand::Go | MscCommand::TimedGo => {
            if names_one {
                MscAction::Start(named()?)
            } else {
                MscAction::Go
            }
        }
        MscCommand::Stop => {
            if names_one {
                MscAction::Pause(named()?)
            } else {
                MscAction::PauseAll
            }
        }
        MscCommand::Resume => {
            if names_one {
                MscAction::Resume(named()?)
            } else {
                MscAction::ResumeAll
            }
        }
        MscCommand::GoOff => MscAction::Stop(named()?),
        MscCommand::AllOff => MscAction::StopAll,
        MscCommand::Reset => MscAction::Reset,
        MscCommand::Load => MscAction::SetPlayhead(named()?),
        MscCommand::StandbyPlus | MscCommand::SequencePlus => MscAction::PlayheadNext,
        MscCommand::StandbyMinus | MscCommand::SequenceMinus => MscAction::PlayheadPrevious,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazzalab_core::{Cue, CueKind};

    fn go(device: u8, data: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0xF0, 0x7F, device, 0x02, 0x01, 0x01];
        bytes.extend_from_slice(data);
        bytes.push(0xF7);
        bytes
    }

    #[test]
    fn a_bare_go_names_no_cue() {
        let message = parse_msc(&go(5, &[])).unwrap();
        assert_eq!(message.device_id, 5);
        assert_eq!(message.command_format, 0x01);
        assert_eq!(message.command, MscCommand::Go);
        assert_eq!(message.cue, None);
    }

    #[test]
    fn go_with_a_cue_number_list_and_path() {
        let message = parse_msc(&go(0x7F, b"12.5\x002\x003.1")).unwrap();
        assert_eq!(
            message.cue,
            Some(MscCue {
                number: "12.5".into(),
                list: Some("2".into()),
                path: Some("3.1".into())
            })
        );
        let only_number = parse_msc(&go(1, b"7")).unwrap();
        assert_eq!(only_number.cue.unwrap().list, None);
    }

    #[test]
    fn commands_are_read_by_their_codes() {
        for code in 0x00..0x80u8 {
            assert_eq!(MscCommand::from_code(code).code(), code);
        }
        let stop = parse_msc(&[0xF0, 0x7F, 0x7F, 0x02, 0x7F, 0x02, 0xF7]).unwrap();
        assert_eq!(stop.command, MscCommand::Stop);
        assert_eq!(stop.command_format, 0x7F);
        let all_off = parse_msc(&[0xF0, 0x7F, 0, 0x02, 0x01, 0x08, 0xF7]).unwrap();
        assert_eq!(all_off.command, MscCommand::AllOff);
        let odd = parse_msc(&[0xF0, 0x7F, 0, 0x02, 0x01, 0x55, 0xF7]).unwrap();
        assert_eq!(odd.command, MscCommand::Other(0x55));
    }

    #[test]
    fn a_timed_go_carries_its_time_before_the_cue() {
        let mut data = vec![1, 2, 3, 4, 0x02];
        data.extend_from_slice(b"9");
        let message =
            parse_msc(&[&[0xF0, 0x7F, 1, 0x02, 0x01, 0x04][..], &data, &[0xF7]].concat()).unwrap();
        assert_eq!(message.command, MscCommand::TimedGo);
        assert_eq!(message.time, Some([1, 2, 3, 4, 2]));
        assert_eq!(message.cue.unwrap().number, "9");
    }

    #[test]
    fn what_is_not_msc_is_refused() {
        // Not real time, not show control (MTC full frame), no end, no command,
        // a data byte with the top bit set.
        assert_eq!(parse_msc(&[0xF0, 0x7E, 0, 0x02, 1, 1, 0xF7]), None);
        assert_eq!(
            parse_msc(&[0xF0, 0x7F, 0x7F, 0x01, 0x01, 0x01, 0, 0, 0, 0, 0xF7]),
            None
        );
        assert_eq!(parse_msc(&[0xF0, 0x7F, 0, 0x02, 1, 1]), None);
        assert_eq!(parse_msc(&[0xF0, 0x7F, 0, 0x02, 0xF7]), None);
        assert_eq!(parse_msc(&go(0, &[0x90])), None);
        assert_eq!(parse_msc(&[]), None);
    }

    #[test]
    fn a_cue_number_that_is_not_digits_is_not_a_cue() {
        let message = parse_msc(&go(1, b"A1")).unwrap();
        assert_eq!(message.cue, None);
        assert_eq!(message.data, b"A1");
    }

    #[test]
    fn it_reads_what_the_encoder_writes() {
        use crate::encode;
        use mazzalab_core::MidiMessage;
        let bytes = encode(&MidiMessage::Msc {
            device_id: 5,
            command_format: 0x01,
            command: 0x01,
            q_number: Some("12.5".into()),
            q_list: Some("2".into()),
        })
        .unwrap();
        let message = parse_msc(&bytes).unwrap();
        assert_eq!(message.cue.unwrap().number, "12.5");
    }

    fn show() -> (Workspace, CueId, CueId) {
        let mut workspace = Workspace::new("Show");
        let list = workspace.cue_lists[0].id;
        let (a, b) = (
            Cue::new(CueKind::Memo).with_number("1"),
            Cue::new(CueKind::Memo).with_number("2.5"),
        );
        let (a_id, b_id) = (a.id, b.id);
        workspace
            .cue_lists
            .iter_mut()
            .find(|l| l.id == list)
            .unwrap()
            .cues
            .extend([a, b]);
        (workspace, a_id, b_id)
    }

    #[test]
    fn commands_become_what_a_show_does() {
        let (workspace, one, two_five) = show();
        let act =
            |bytes: &[u8], device: u8| action_for(&parse_msc(bytes).unwrap(), device, &workspace);
        assert_eq!(act(&go(1, &[]), 1), Some(MscAction::Go));
        assert_eq!(act(&go(1, b"2.5"), 1), Some(MscAction::Start(two_five)));
        // Someone else's device, or all of them.
        assert_eq!(act(&go(2, b"1"), 1), None);
        assert_eq!(act(&go(0x7F, b"1"), 1), Some(MscAction::Start(one)));
        // A cue the show lacks is not turned into "every cue".
        assert_eq!(act(&go(1, b"99"), 1), None);
        let command = |code: u8, data: &[u8]| {
            let mut bytes = vec![0xF0, 0x7F, 1, 0x02, 0x01, code];
            bytes.extend_from_slice(data);
            bytes.push(0xF7);
            act(&bytes, 1)
        };
        assert_eq!(command(0x02, &[]), Some(MscAction::PauseAll));
        assert_eq!(command(0x02, b"1"), Some(MscAction::Pause(one)));
        assert_eq!(command(0x03, &[]), Some(MscAction::ResumeAll));
        assert_eq!(command(0x03, b"2.5"), Some(MscAction::Resume(two_five)));
        assert_eq!(command(0x08, &[]), Some(MscAction::StopAll));
        assert_eq!(command(0x0A, &[]), Some(MscAction::Reset));
        assert_eq!(
            command(0x05, b"2.5"),
            Some(MscAction::SetPlayhead(two_five))
        );
        assert_eq!(command(0x0B, b"1"), Some(MscAction::Stop(one)));
        assert_eq!(command(0x11, &[]), Some(MscAction::PlayheadNext));
        assert_eq!(command(0x14, &[]), Some(MscAction::PlayheadPrevious));
        // Clocks, macros and the like are not for a cue list.
        assert_eq!(command(0x15, &[]), None);
        assert_eq!(command(0x07, &[1]), None);
    }
}
