//! Executes parsed OSC requests (QLab-compatible addresses) on a [`Show`].

use std::time::Duration;

use mazzalab_core::{ActionLength, ContinueMode, Cue, CueColor, CueId};
use mazzalab_engine::RunPhase;
use mazzalab_osc::{Arg, Command as Osc, CueMethod, CueSelector, PropertyOp, ReplyStatus, Request};
use serde_json::{Value, json};

use crate::{Command, Show, ShowError, duration_mut};

/// Per-connection requests the server handles itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionRequest {
    Updates(bool),
    AlwaysReply(bool),
    Connect { passcode: Option<String> },
    Disconnect,
    Thump,
}

/// The result of an OSC request.
#[derive(Debug, Clone, PartialEq)]
pub struct OscOutcome {
    pub status: ReplyStatus,
    pub data: Option<Value>,
    /// The request asked for information: a reply is always sent. Pure
    /// actions are only answered when the client asked for all replies.
    pub is_query: bool,
    pub session: Option<SessionRequest>,
}

impl OscOutcome {
    fn ok() -> Self {
        Self {
            status: ReplyStatus::Ok,
            data: None,
            is_query: false,
            session: None,
        }
    }

    fn data(data: Value) -> Self {
        Self {
            data: Some(data),
            is_query: true,
            ..Self::ok()
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            status: ReplyStatus::Error,
            data: Some(Value::String(message.into())),
            ..Self::ok()
        }
    }

    fn session(request: SessionRequest) -> Self {
        Self {
            session: Some(request),
            ..Self::ok()
        }
    }
}

impl From<Result<(), ShowError>> for OscOutcome {
    fn from(result: Result<(), ShowError>) -> Self {
        match result {
            Ok(()) => Self::ok(),
            Err(error) => Self::error(error.to_string()),
        }
    }
}

impl Show {
    /// Handles one OSC request. Returns `None` when the request is
    /// addressed to a different workspace.
    pub fn handle_osc(&mut self, request: &Request) -> Option<OscOutcome> {
        if let Some(target) = &request.workspace {
            let workspace = self.workspace();
            let matches =
                target.eq_ignore_ascii_case(&workspace.id.to_string()) || *target == workspace.name;
            if !matches {
                return None;
            }
        }
        let list = self.current_list();
        Some(match &request.command {
            Osc::Go { cue: None } => self.execute(Command::Go { list: None }).into(),
            Osc::Go { cue: Some(number) } => match self.workspace().find_by_number(number) {
                Some(cue) => {
                    let cue = cue.id;
                    self.execute(Command::SetPlayhead {
                        list: None,
                        cue: Some(cue),
                    })
                    .and_then(|()| {
                        let list = self.list_of(cue)?;
                        self.execute(Command::Go { list: Some(list) })
                    })
                    .into()
                }
                None => OscOutcome::error(format!("no cue numbered {number:?}")),
            },
            Osc::Stop | Osc::HardStop => self.execute(Command::StopAll).into(),
            Osc::Pause => self.execute(Command::PauseAll).into(),
            Osc::Resume => self.execute(Command::ResumeAll).into(),
            Osc::Panic => self.execute(Command::Panic).into(),
            Osc::Reset => self.execute(Command::Reset).into(),
            Osc::PlayheadNext => self.execute(Command::PlayheadNext { list: None }).into(),
            Osc::PlayheadPrevious => self
                .execute(Command::PlayheadPrevious { list: None })
                .into(),
            Osc::PlayheadNextSequence => self
                .engine
                .playhead_next_sequence(list)
                .map_err(ShowError::from)
                .into(),
            Osc::PlayheadPreviousSequence => self
                .engine
                .playhead_previous_sequence(list)
                .map_err(ShowError::from)
                .into(),
            Osc::Playhead(selector) => match self.resolve(selector).first() {
                Some(&cue) => self
                    .execute(Command::SetPlayhead {
                        list: None,
                        cue: Some(cue),
                    })
                    .into(),
                None => OscOutcome::error("no matching cue"),
            },
            Osc::SelectNext => self.step_selection(1),
            Osc::SelectPrevious => self.step_selection(-1),
            Osc::Select(selector) => match self.resolve(selector).first() {
                Some(&cue) => self.execute(Command::Select { cue: Some(cue) }).into(),
                None => OscOutcome::error("no matching cue"),
            },
            Osc::Cue { cue, method } => self.cue_method(cue, method),
            Osc::Updates(on) => OscOutcome::session(SessionRequest::Updates(*on)),
            Osc::AlwaysReply(on) => OscOutcome::session(SessionRequest::AlwaysReply(*on)),
            Osc::Connect { passcode } => OscOutcome::session(SessionRequest::Connect {
                passcode: passcode.clone(),
            }),
            Osc::Disconnect => OscOutcome::session(SessionRequest::Disconnect),
            Osc::Thump => OscOutcome {
                data: Some(json!("thump")),
                ..OscOutcome::session(SessionRequest::Thump)
            },
            Osc::Version => {
                OscOutcome::data(json!(concat!("MazzaLab ", env!("CARGO_PKG_VERSION"))))
            }
            Osc::Workspaces => {
                let workspace = self.workspace();
                OscOutcome::data(json!([{
                    "uniqueID": workspace.id.to_string(),
                    "displayName": workspace.name,
                    "hasPasscode": false,
                    "version": env!("CARGO_PKG_VERSION"),
                }]))
            }
            Osc::CueLists => OscOutcome::data(self.cue_lists_json()),
        })
    }

    /// Cues matched by a selector, in workspace order.
    fn resolve(&self, selector: &CueSelector) -> Vec<CueId> {
        let workspace = self.workspace();
        match selector {
            CueSelector::Number(pattern) => workspace
                .cues()
                .filter(|cue| {
                    cue.number
                        .as_deref()
                        .is_some_and(|number| osc_match(pattern, number))
                })
                .map(|cue| cue.id)
                .collect(),
            CueSelector::Id(id) => workspace
                .cues()
                .filter(|cue| id.eq_ignore_ascii_case(&cue.id.to_string()))
                .map(|cue| cue.id)
                .collect(),
            CueSelector::Selected => self.selection().into_iter().collect(),
            CueSelector::Active => self.engine.running_cues().collect(),
            CueSelector::Playhead => self
                .engine
                .playhead(self.current_list())
                .into_iter()
                .collect(),
        }
    }

    fn step_selection(&mut self, step: isize) -> OscOutcome {
        let Some(list) = self.workspace().cue_list(self.current_list()) else {
            return OscOutcome::error("no current cue list");
        };
        let mut order = Vec::new();
        flatten(&list.cues, &mut order);
        let position = self
            .selection()
            .and_then(|cue| order.iter().position(|id| *id == cue));
        let next = match position {
            Some(index) => index.checked_add_signed(step).filter(|i| *i < order.len()),
            None => (!order.is_empty()).then_some(0),
        };
        match next {
            Some(index) => self
                .execute(Command::Select {
                    cue: Some(order[index]),
                })
                .into(),
            None => OscOutcome::ok(),
        }
    }

    fn cue_method(&mut self, selector: &CueSelector, method: &CueMethod) -> OscOutcome {
        let cues = self.resolve(selector);
        if cues.is_empty() {
            return OscOutcome::error("no matching cue");
        }
        let action = |cue| match method {
            CueMethod::Start => Some(Command::Start { cue }),
            CueMethod::Stop | CueMethod::HardStop | CueMethod::Panic | CueMethod::Reset => {
                Some(Command::Stop { cue })
            }
            CueMethod::Pause => Some(Command::Pause { cue }),
            CueMethod::Resume => Some(Command::Resume { cue }),
            CueMethod::Load | CueMethod::Preview | CueMethod::Property { .. } => None,
        };
        if let CueMethod::Property { name, op, .. } = method {
            return self.property(&cues, name, op);
        }
        if action(cues[0]).is_none() {
            return OscOutcome::error("not supported yet");
        }
        for cue in cues {
            if let Some(command) = action(cue)
                && let Err(error) = self.execute(command)
            {
                return OscOutcome::error(error.to_string());
            }
        }
        OscOutcome::ok()
    }

    fn property(&mut self, cues: &[CueId], name: &str, op: &PropertyOp) -> OscOutcome {
        if *op == PropertyOp::Get {
            let values: Option<Vec<Value>> = cues
                .iter()
                .map(|cue| self.get_property(*cue, name))
                .collect();
            return match values {
                Some(mut values) if values.len() == 1 => OscOutcome::data(values.remove(0)),
                Some(values) => OscOutcome::data(Value::Array(values)),
                None => OscOutcome::error(format!("unknown property {name:?}")),
            };
        }

        let mut updated = Vec::new();
        for cue in cues {
            let Some(mut cue) = self.workspace().find_cue(*cue).cloned() else {
                continue;
            };
            if let Err(message) = set_property(&mut cue, name, op) {
                return OscOutcome::error(message);
            }
            updated.push(cue);
        }
        let result = self.edit(|workspace| {
            for cue in updated {
                crate::update_cue(workspace, cue)?;
            }
            Ok(())
        });
        result.into()
    }

    fn get_property(&self, id: CueId, name: &str) -> Option<Value> {
        let cue = self.workspace().find_cue(id)?;
        let run = self.engine.run_info(id);
        let elapsed_in = |phase| {
            run.filter(|run| run.phase == phase)
                .map_or(0.0, |run| run.elapsed.as_secs_f64())
        };
        Some(match name {
            "uniqueID" => json!(cue.id.to_string()),
            "number" => json!(cue.number.clone().unwrap_or_default()),
            "name" => json!(cue.name),
            "listName" | "displayName" => json!(cue.display_name()),
            "notes" => json!(cue.notes),
            "type" => json!(cue.type_name()),
            "preWait" => json!(cue.pre_wait.as_secs_f64()),
            "postWait" => json!(cue.post_wait.as_secs_f64()),
            "duration" => json!(match cue.kind.action_length() {
                ActionLength::Fixed(length) => length.as_secs_f64(),
                _ => 0.0,
            }),
            "continueMode" => json!(continue_index(cue.continue_mode)),
            "armed" => json!(cue.armed),
            "flagged" => json!(cue.flagged),
            "colorName" => json!(color_name(cue.color)),
            "isRunning" => json!(run.is_some_and(|run| !run.paused)),
            "isPaused" => json!(run.is_some_and(|run| run.paused)),
            "isBroken" => json!(false),
            "preWaitElapsed" => json!(elapsed_in(RunPhase::PreWait)),
            "actionElapsed" => json!(elapsed_in(RunPhase::Action)),
            _ => return None,
        })
    }

    fn cue_lists_json(&self) -> Value {
        fn cue_json(cue: &Cue) -> Value {
            let mut value = json!({
                "uniqueID": cue.id.to_string(),
                "number": cue.number.clone().unwrap_or_default(),
                "name": cue.name,
                "listName": cue.display_name(),
                "type": cue.type_name(),
                "colorName": color_name(cue.color),
                "flagged": cue.flagged,
                "armed": cue.armed,
            });
            if matches!(cue.kind, mazzalab_core::CueKind::Group(_)) {
                value["cues"] = cue.children().iter().map(cue_json).collect();
            }
            value
        }
        self.workspace()
            .cue_lists
            .iter()
            .map(|list| {
                json!({
                    "uniqueID": list.id.to_string(),
                    "number": "",
                    "name": list.name,
                    "listName": list.name,
                    "type": "Cue List",
                    "colorName": "none",
                    "flagged": false,
                    "armed": true,
                    "cues": list.cues.iter().map(cue_json).collect::<Vec<_>>(),
                })
            })
            .collect()
    }
}

fn flatten(cues: &[Cue], out: &mut Vec<CueId>) {
    for cue in cues {
        out.push(cue.id);
        flatten(cue.children(), out);
    }
}

fn continue_index(mode: ContinueMode) -> i64 {
    match mode {
        ContinueMode::DoNotContinue => 0,
        ContinueMode::AutoContinue => 1,
        ContinueMode::AutoFollow => 2,
    }
}

fn continue_from_index(index: i64) -> ContinueMode {
    match index.rem_euclid(3) {
        1 => ContinueMode::AutoContinue,
        2 => ContinueMode::AutoFollow,
        _ => ContinueMode::DoNotContinue,
    }
}

const COLORS: [(CueColor, &str); 7] = [
    (CueColor::Red, "red"),
    (CueColor::Orange, "orange"),
    (CueColor::Yellow, "yellow"),
    (CueColor::Green, "green"),
    (CueColor::Blue, "blue"),
    (CueColor::Purple, "purple"),
    (CueColor::Grey, "grey"),
];

fn color_name(color: Option<CueColor>) -> &'static str {
    color
        .and_then(|color| COLORS.iter().find(|(c, _)| *c == color))
        .map_or("none", |(_, name)| name)
}

fn arg_number(arg: &Arg) -> Option<f64> {
    match arg {
        Arg::Int(v) => Some(*v as f64),
        Arg::Float(v) => Some(*v),
        Arg::String(s) => s.trim().parse().ok(),
        Arg::Bool(v) => Some(f64::from(u8::from(*v))),
    }
}

fn arg_text(arg: &Arg) -> String {
    match arg {
        Arg::String(s) => s.clone(),
        Arg::Int(v) => v.to_string(),
        Arg::Float(v) if v.fract() == 0.0 && v.abs() < 1e15 => format!("{}", *v as i64),
        Arg::Float(v) => v.to_string(),
        Arg::Bool(v) => v.to_string(),
    }
}

/// Applies a write, increment or decrement to a cue property.
fn set_property(cue: &mut Cue, name: &str, op: &PropertyOp) -> Result<(), String> {
    let number = |current: f64| -> Result<f64, String> {
        match op {
            PropertyOp::Set(arg) => arg_number(arg),
            PropertyOp::Add(arg) => arg_number(arg).map(|delta| current + delta),
            PropertyOp::Subtract(arg) => arg_number(arg).map(|delta| current - delta),
            PropertyOp::Get => None,
        }
        .ok_or_else(|| format!("{name} expects a number"))
    };
    let seconds = |current: Duration| -> Result<Duration, String> {
        let value = number(current.as_secs_f64())?.max(0.0);
        Duration::try_from_secs_f64(value).map_err(|error| error.to_string())
    };
    let set_arg = || match op {
        PropertyOp::Set(arg) => Ok(arg),
        _ => Err(format!("{name} cannot be incremented")),
    };

    match name {
        "preWait" => cue.pre_wait = seconds(cue.pre_wait)?,
        "postWait" => cue.post_wait = seconds(cue.post_wait)?,
        "duration" => {
            let type_name = cue.type_name();
            let duration = duration_mut(&mut cue.kind)
                .ok_or_else(|| format!("the duration of a {type_name} cue is not editable"))?;
            *duration = seconds(*duration)?;
        }
        "continueMode" => {
            let value = number(continue_index(cue.continue_mode) as f64)?;
            cue.continue_mode = continue_from_index(value.round() as i64);
        }
        "number" => {
            let text = arg_text(set_arg()?);
            cue.number = (!text.is_empty()).then_some(text);
        }
        "name" => cue.name = arg_text(set_arg()?),
        "notes" => cue.notes = arg_text(set_arg()?),
        "armed" | "flagged" => {
            let value = set_arg()?
                .as_bool()
                .ok_or_else(|| format!("{name} expects a boolean"))?;
            if name == "armed" {
                cue.armed = value;
            } else {
                cue.flagged = value;
            }
        }
        "colorName" => {
            let text = arg_text(set_arg()?).to_ascii_lowercase();
            cue.color = if text == "none" {
                None
            } else {
                Some(
                    COLORS
                        .iter()
                        .find(|(_, name)| *name == text)
                        .map(|(color, _)| *color)
                        .ok_or_else(|| format!("unknown colour {text:?}"))?,
                )
            };
        }
        _ => return Err(format!("property {name:?} is read-only or unknown")),
    }
    Ok(())
}

/// OSC address pattern matching for cue numbers: `*` and `?` wildcards.
fn osc_match(pattern: &str, text: &str) -> bool {
    fn matches(pattern: &[char], text: &[char]) -> bool {
        match (pattern.split_first(), text.split_first()) {
            (None, None) => true,
            (Some(('*', rest)), _) => {
                matches(rest, text) || (!text.is_empty() && matches(pattern, &text[1..]))
            }
            (Some(('?', rest)), Some((_, text_rest))) => matches(rest, text_rest),
            (Some((p, rest)), Some((t, text_rest))) => p == t && matches(rest, text_rest),
            _ => false,
        }
    }
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    matches(&pattern, &text)
}

#[cfg(test)]
mod tests {
    use super::osc_match;

    #[test]
    fn wildcards() {
        assert!(osc_match("1", "1"));
        assert!(osc_match("*", "12.5"));
        assert!(osc_match("1*", "12"));
        assert!(osc_match("1?", "12"));
        assert!(!osc_match("1?", "1"));
        assert!(!osc_match("2*", "12"));
    }
}
