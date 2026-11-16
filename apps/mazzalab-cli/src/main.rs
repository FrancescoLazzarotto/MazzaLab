//! mazzalab-cli entry point
//!
//! `mazzalab` works on `.mazzalab` show files without any audio or video
//! hardware: create a demo show, validate a show, print its structure, or
//! simulate playback on the real engine and print the resulting timeline.

mod demo;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use mazzalab_core::{ContinueMode, Cue, CueId, CueList, Workspace, validate};
use mazzalab_engine::{Engine, EventKind};

#[derive(Parser)]
#[command(name = "mazzalab", version, about = "MazzaLab show file tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write a demo show file.
    New {
        path: PathBuf,
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
    },
    /// Check a show file for problems.
    Validate { path: PathBuf },
    /// Print the cue lists of a show file.
    Info { path: PathBuf },
    /// Run a show on the engine (no media output) and print the timeline.
    Simulate {
        path: PathBuf,
        /// Cue list name (defaults to the first list).
        #[arg(long)]
        list: Option<String>,
        /// Number of GOs to press.
        #[arg(long, default_value_t = 1)]
        gos: u32,
        /// Seconds between GOs.
        #[arg(long, default_value_t = 10.0)]
        interval: f64,
        /// Stop simulating after this many seconds.
        #[arg(long, default_value_t = 600.0)]
        until: f64,
    },
    /// Replay a recorded performance (a `.jsonl` from the show's `logs`
    /// folder) and print its timeline.
    Replay {
        path: PathBuf,
        /// Carry on to this many seconds after the last input.
        #[arg(long)]
        until: Option<f64>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<ExitCode, String> {
    match command {
        Command::New { path, force } => {
            if path.exists() && !force {
                return Err(format!(
                    "{} already exists (use --force to overwrite)",
                    path.display()
                ));
            }
            mazzalab_project::save(&path, &demo::show()).map_err(|e| e.to_string())?;
            println!("wrote {}", path.display());
            Ok(ExitCode::SUCCESS)
        }
        Command::Validate { path } => {
            let workspace = load(&path)?;
            let issues = validate(&workspace);
            if issues.is_empty() {
                println!("{}: ok", path.display());
                return Ok(ExitCode::SUCCESS);
            }
            for issue in &issues {
                println!("{}: {issue}", path.display());
            }
            Ok(ExitCode::FAILURE)
        }
        Command::Info { path } => {
            let workspace = load(&path)?;
            println!("{} ({})", workspace.name, workspace.id);
            for list in &workspace.cue_lists {
                println!("\n[{}]", list.name);
                print_cues(&list.cues, 1);
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Simulate {
            path,
            list,
            gos,
            interval,
            until,
        } => {
            let workspace = load(&path)?;
            let list_id = pick_list(&workspace, list.as_deref())?.id;
            let interval = seconds(interval)?;
            let until = seconds(until)?;
            simulate(workspace, list_id, gos, interval, until);
            Ok(ExitCode::SUCCESS)
        }
        Command::Replay { path, until } => {
            let until = until.map(seconds).transpose()?;
            replay(&path, until)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn load(path: &PathBuf) -> Result<Workspace, String> {
    mazzalab_project::load(path).map_err(|e| e.to_string())
}

fn seconds(value: f64) -> Result<Duration, String> {
    Duration::try_from_secs_f64(value).map_err(|_| format!("invalid number of seconds: {value}"))
}

fn pick_list<'a>(workspace: &'a Workspace, name: Option<&str>) -> Result<&'a CueList, String> {
    match name {
        Some(name) => workspace
            .cue_lists
            .iter()
            .find(|list| list.name == name)
            .ok_or_else(|| format!("no cue list named {name:?}")),
        None => workspace
            .cue_lists
            .first()
            .ok_or_else(|| "the show has no cue lists".to_owned()),
    }
}

fn print_cues(cues: &[Cue], depth: usize) {
    for cue in cues {
        let mut timing = Vec::new();
        if !cue.pre_wait.is_zero() {
            timing.push(format!("pre {:.2}s", cue.pre_wait.as_secs_f64()));
        }
        if !cue.post_wait.is_zero() {
            timing.push(format!("post {:.2}s", cue.post_wait.as_secs_f64()));
        }
        match cue.continue_mode {
            ContinueMode::DoNotContinue => {}
            ContinueMode::AutoContinue => timing.push("auto-continue".into()),
            ContinueMode::AutoFollow => timing.push("auto-follow".into()),
        }
        if !cue.armed {
            timing.push("disarmed".into());
        }
        println!(
            "{number:>8}  {indent}{name} [{kind}]{timing}",
            indent = "  ".repeat(depth - 1),
            number = cue.number.as_deref().unwrap_or("-"),
            name = cue.display_name(),
            kind = cue.type_name(),
            timing = if timing.is_empty() {
                String::new()
            } else {
                format!("  ({})", timing.join(", "))
            },
        );
        print_cues(cue.children(), depth + 1);
    }
}

fn simulate(workspace: Workspace, list: CueId, gos: u32, interval: Duration, until: Duration) {
    let mut engine = Engine::new(workspace);
    let print_events = |engine: &mut Engine| {
        for event in engine.take_events() {
            let text = describe(engine.workspace(), event.kind);
            println!("{:>9.3}s  {text}", event.at.as_secs_f64());
        }
    };

    for index in 0..gos {
        let at = interval.saturating_mul(index);
        if at > until {
            break;
        }
        engine.advance_to(at);
        print_events(&mut engine);
        println!("{:>9.3}s  ===== GO", at.as_secs_f64());
        if engine.go(list).ok().flatten().is_none() {
            println!("{:>9.3}s  (nothing at the playhead)", at.as_secs_f64());
        }
        print_events(&mut engine);
    }
    while let Some(deadline) = engine.next_deadline().filter(|deadline| *deadline <= until) {
        engine.advance_to(deadline);
        print_events(&mut engine);
    }

    let still_running: Vec<_> = engine.running_cues().collect();
    if !still_running.is_empty() {
        println!("\nstill running (open-ended or unknown length):");
        for cue in still_running {
            println!("  {}", label(engine.workspace(), cue));
        }
    }
}

/// A cue as the timeline names it: number, name and type.
fn label(workspace: &Workspace, id: CueId) -> String {
    workspace.find_cue(id).map_or_else(
        || id.to_string(),
        |cue| {
            format!(
                "{} {} [{}]",
                cue.number.as_deref().unwrap_or("-"),
                cue.display_name(),
                cue.type_name()
            )
        },
    )
}

/// One line of the timeline for an engine event.
fn describe(workspace: &Workspace, kind: EventKind) -> String {
    match kind {
        EventKind::Triggered(cue) => format!("start    {}", label(workspace, cue)),
        EventKind::Skipped(cue) => format!("skip     {}", label(workspace, cue)),
        EventKind::ActionStarted { cue, armed: true } => {
            format!("action   {}", label(workspace, cue))
        }
        EventKind::ActionStarted { cue, armed: false } => {
            format!("action   {} (disarmed)", label(workspace, cue))
        }
        EventKind::ActionCompleted(cue) => {
            format!("done     {}", label(workspace, cue))
        }
        EventKind::Stopped(cue) => format!("stop     {}", label(workspace, cue)),
        EventKind::Paused(cue) => format!("pause    {}", label(workspace, cue)),
        EventKind::Resumed(cue) => format!("resume   {}", label(workspace, cue)),
        EventKind::Panicking { cue, fade } => format!(
            "panic    {} (fade {:.2}s)",
            label(workspace, cue),
            fade.as_secs_f64()
        ),
        EventKind::PlayheadMoved { cue, .. } => format!(
            "playhead {}",
            cue.map_or_else(|| "(end)".to_owned(), |c| label(workspace, c))
        ),
        EventKind::Loaded { cue, time } => format!(
            "load     {} (to {:.2}s)",
            label(workspace, cue),
            time.as_secs_f64()
        ),
        EventKind::Unloaded(cue) => format!("unload   {}", label(workspace, cue)),
        EventKind::Devamped(cue) => format!("devamp   {}", label(workspace, cue)),
        EventKind::FadingOut { cue, fade } => format!(
            "fadeout  {} over {:.2}s",
            label(workspace, cue),
            fade.as_secs_f64()
        ),
        EventKind::Ducked { cue, by_db, .. } => {
            format!("duck     {} by {by_db:.1} dB", label(workspace, cue))
        }
        EventKind::Unducked { cue, .. } => {
            format!("unduck   {}", label(workspace, cue))
        }
    }
}

/// Replays an input record and prints the timeline it produces.
fn replay(path: &Path, until: Option<Duration>) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut show = mazzalab_show::replay(std::io::BufReader::new(file), |show, entry| {
        let input = match &entry.input {
            mazzalab_show::Input::Command { command } => {
                serde_json::to_string(command).unwrap_or_default()
            }
            mazzalab_show::Input::Osc { request } => format!("OSC {request:?}"),
            mazzalab_show::Input::OscTrigger { address } => format!("OSC trigger {address}"),
            mazzalab_show::Input::ActionFinished { cue } => {
                format!("media ended {}", label(show.workspace(), *cue))
            }
            mazzalab_show::Input::MediaInfo { cue, .. } => {
                format!("media read {}", label(show.workspace(), *cue))
            }
        };
        let events = show.take_events();
        println!("{:>9.3}s  ===== {input}", entry.time().as_secs_f64());
        for event in events {
            println!(
                "{:>9.3}s  {}",
                event.at.as_secs_f64(),
                describe(show.workspace(), event.kind)
            );
        }
    })
    .map_err(|error| error.to_string())?;
    let end = until.unwrap_or_else(|| show.now());
    while let Some(deadline) = show.next_deadline().filter(|deadline| *deadline <= end) {
        show.advance_to(deadline);
        for event in show.take_events() {
            println!(
                "{:>9.3}s  {}",
                event.at.as_secs_f64(),
                describe(show.workspace(), event.kind)
            );
        }
    }
    Ok(())
}
