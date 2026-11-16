//! Who may do what: passcodes from the show's settings, checked for web
//! clients and OSC alike.
//!
//! A show without passcodes is open to everyone, as before passcodes
//! existed. The machine running the show always has full access: the
//! operator at the desktop app never types a passcode.

use mazzalab_core::Passcode;
use mazzalab_osc::{CueMethod, PropertyOp};
use mazzalab_show::Command;
use serde::Serialize;

/// What a client may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Access {
    pub view: bool,
    pub control: bool,
    pub edit: bool,
}

/// What a request needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Need {
    /// Nothing: asking what one may do, connecting, keep-alives.
    Nothing,
    View,
    Control,
    Edit,
}

impl Access {
    pub const FULL: Self = Self {
        view: true,
        control: true,
        edit: true,
    };
    pub const NONE: Self = Self {
        view: false,
        control: false,
        edit: false,
    };

    /// What `passcodes` grant a client that gave `code`. A local client,
    /// and anyone when there are no passcodes, may do everything.
    pub fn grant(passcodes: &[Passcode], code: Option<&str>, local: bool) -> Self {
        if local || passcodes.is_empty() {
            return Self::FULL;
        }
        let Some(code) = code else {
            return Self::NONE;
        };
        passcodes
            .iter()
            .filter(|passcode| same(passcode.code.as_bytes(), code.as_bytes()))
            .fold(Self::NONE, |access, passcode| Self {
                view: access.view || passcode.view || passcode.control || passcode.edit,
                control: access.control || passcode.control,
                edit: access.edit || passcode.edit,
            })
    }

    pub fn allows(self, need: Need) -> bool {
        match need {
            Need::Nothing => true,
            Need::View => self.view,
            Need::Control => self.control,
            Need::Edit => self.edit,
        }
    }

    /// The permissions as QLab's `/connect` reply lists them, e.g.
    /// `view|control`.
    pub fn describe(self) -> String {
        [
            (self.view, "view"),
            (self.edit, "edit"),
            (self.control, "control"),
        ]
        .into_iter()
        .filter_map(|(granted, name)| granted.then_some(name))
        .collect::<Vec<_>>()
        .join("|")
    }
}

/// Compares two passcodes in time that depends only on their lengths.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// What a show command needs: running the show is control, changing it
/// is edit.
pub fn command_need(command: &Command) -> Need {
    match command {
        Command::Go { .. }
        | Command::Start { .. }
        | Command::Stop { .. }
        | Command::Pause { .. }
        | Command::Resume { .. }
        | Command::Load { .. }
        | Command::StopAll
        | Command::PauseAll
        | Command::ResumeAll
        | Command::Panic
        | Command::Reset
        | Command::SetPlayhead { .. }
        | Command::PlayheadNext { .. }
        | Command::PlayheadPrevious { .. }
        | Command::Select { .. }
        | Command::SetCurrentList { .. } => Need::Control,
        Command::UpdateCue { .. }
        | Command::InsertCue { .. }
        | Command::DeleteCue { .. }
        | Command::MoveCue { .. }
        | Command::AddCueList { .. }
        | Command::RenameCueList { .. }
        | Command::DeleteCueList { .. }
        | Command::RenameWorkspace { .. }
        | Command::UpdateSettings { .. }
        | Command::SetNumbers { .. }
        | Command::Batch { .. }
        | Command::Undo
        | Command::Redo => Need::Edit,
    }
}

/// What an OSC message needs, in QLab's terms.
pub fn osc_need(command: &mazzalab_osc::Command) -> Need {
    use mazzalab_osc::Command as Osc;
    match command {
        Osc::Connect { .. } | Osc::Disconnect | Osc::Thump | Osc::Version => Need::Nothing,
        Osc::Updates(_) | Osc::AlwaysReply(_) | Osc::Workspaces | Osc::CueLists => Need::View,
        Osc::Go { .. }
        | Osc::Stop
        | Osc::HardStop
        | Osc::Pause
        | Osc::Resume
        | Osc::Panic
        | Osc::Reset
        | Osc::PlayheadNext
        | Osc::PlayheadPrevious
        | Osc::PlayheadNextSequence
        | Osc::PlayheadPreviousSequence
        | Osc::Playhead(_)
        | Osc::SelectNext
        | Osc::SelectPrevious
        | Osc::Select(_) => Need::Control,
        Osc::Cue { method, .. } => match method {
            CueMethod::Property {
                op: PropertyOp::Get,
                ..
            } => Need::View,
            CueMethod::Property { live: true, .. } => Need::Control,
            CueMethod::Property { .. } => Need::Edit,
            CueMethod::Start
            | CueMethod::Stop
            | CueMethod::HardStop
            | CueMethod::Pause
            | CueMethod::Resume
            | CueMethod::Load
            | CueMethod::Preview
            | CueMethod::Panic
            | CueMethod::Reset => Need::Control,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passcode(code: &str, view: bool, control: bool, edit: bool) -> Passcode {
        Passcode {
            code: code.into(),
            name: String::new(),
            view,
            control,
            edit,
        }
    }

    #[test]
    fn without_passcodes_everyone_may_do_everything() {
        assert_eq!(Access::grant(&[], None, false), Access::FULL);
    }

    #[test]
    fn passcodes_grant_what_they_say_and_the_local_machine_everything() {
        let codes = [
            passcode("1111", true, false, false),
            passcode("2222", false, true, false),
            passcode("3333", true, true, true),
        ];
        assert_eq!(Access::grant(&codes, None, false), Access::NONE);
        assert_eq!(Access::grant(&codes, Some("9999"), false), Access::NONE);
        assert_eq!(Access::grant(&codes, Some("111"), false), Access::NONE);
        let view = Access::grant(&codes, Some("1111"), false);
        assert!(view.allows(Need::View) && !view.allows(Need::Control));
        // Control alone still lets one see what one runs.
        let control = Access::grant(&codes, Some("2222"), false);
        assert!(control.view && control.control && !control.edit);
        assert_eq!(control.describe(), "view|control");
        assert_eq!(Access::grant(&codes, Some("3333"), false), Access::FULL);
        assert_eq!(Access::grant(&codes, None, true), Access::FULL);
        assert!(Access::NONE.allows(Need::Nothing));
    }

    #[test]
    fn running_the_show_is_control_and_changing_it_edit() {
        assert_eq!(command_need(&Command::Go { list: None }), Need::Control);
        assert_eq!(command_need(&Command::Panic), Need::Control);
        assert_eq!(command_need(&Command::Undo), Need::Edit);
        assert_eq!(
            command_need(&Command::RenameWorkspace { name: "x".into() }),
            Need::Edit
        );

        let osc = |address: &str, args: &[mazzalab_osc::Arg]| {
            osc_need(&mazzalab_osc::parse(address, args).unwrap().command)
        };
        assert_eq!(osc("/go", &[]), Need::Control);
        assert_eq!(osc("/cue/1/name", &[]), Need::View);
        assert_eq!(
            osc("/cue/1/name", &[mazzalab_osc::Arg::String("Door".into())]),
            Need::Edit
        );
        assert_eq!(osc("/connect", &[]), Need::Nothing);
        assert_eq!(osc("/updates", &[mazzalab_osc::Arg::Int(1)]), Need::View);
    }
}
