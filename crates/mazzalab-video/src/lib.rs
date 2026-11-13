//! mazzalab-video
//!
//! What the show's stages show. A stage is any screen that renders the
//! [`StageState`]: a browser window on a projector, a monitor in the
//! operator's UI. [`Stages`] follows the engine's events and keeps, for
//! every Video and Text cue that is up, where it plays, on which layer, at
//! what opacity and from which time, so that a stage that connects late, or
//! again, shows the same thing as the others. It does no I/O and keeps no
//! clock of its own: callers say what time it is.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use mazzalab_core::{CueId, CueKind, Workspace};
use mazzalab_engine::{Event, EventKind};
use serde::Serialize;

/// The stage a cue goes to when it names none.
pub const DEFAULT_STAGE: &str = "Main";

/// What is on one layer of a stage.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Content {
    Video {
        /// Trims of the file, in seconds.
        start_time: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        end_time: Option<f64>,
        /// Plays the region over and over until the cue is stopped.
        looped: bool,
        /// Keeps the last frame up when the file ends.
        hold: bool,
        rate: f64,
    },
    Text {
        text: String,
    },
}

/// An opacity on its way to another.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct OpacityFade {
    pub from: f64,
    /// When it started, on the callers' clock, in seconds.
    pub started: f64,
    pub duration: f64,
}

/// A cue that is up on a stage.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StageItem {
    pub cue: CueId,
    pub stage: String,
    pub layer: i32,
    /// The opacity it has, or is fading to.
    pub opacity: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fade: Option<OpacityFade>,
    #[serde(flatten)]
    pub content: Content,
    /// When it started or last resumed, on the callers' clock, in seconds.
    pub started: f64,
    /// How far into its action it was at `started`, in seconds.
    pub offset: f64,
    pub paused: bool,
}

impl StageItem {
    /// How far into its action the cue is at `now`.
    pub fn position(&self, now: f64) -> f64 {
        if self.paused {
            self.offset
        } else {
            self.offset + (now - self.started).max(0.0)
        }
    }

    /// Its opacity at `now`, part way through a fade.
    pub fn opacity_at(&self, now: f64) -> f64 {
        match self.fade {
            Some(fade) if fade.duration > 0.0 && now < fade.started + fade.duration => {
                let t = ((now - fade.started) / fade.duration).clamp(0.0, 1.0);
                fade.from + (self.opacity - fade.from) * t
            }
            _ => self.opacity,
        }
    }
}

/// Everything that is up, on every stage, at a moment.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StageState {
    /// The callers' clock when this was taken, so that stages can line
    /// their own clock up with it.
    pub now: f64,
    /// Stages the show names, for clients to offer.
    pub stages: Vec<String>,
    /// Lowest layer first.
    pub items: Vec<StageItem>,
}

/// Follows the engine and keeps what the stages show.
#[derive(Debug, Clone, Default)]
pub struct Stages {
    items: BTreeMap<CueId, StageItem>,
    /// Cues loaded to a time, from the start of their pre-wait.
    loaded: HashMap<CueId, Duration>,
}

fn stage_name(stage: Option<&String>) -> String {
    stage
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .unwrap_or(DEFAULT_STAGE)
        .to_owned()
}

/// What a cue puts on a stage, and where, if it is a Video or Text cue.
fn placement(kind: &CueKind) -> Option<(String, i32, f64, Content)> {
    match kind {
        CueKind::Video(video) if video.file.is_some() => Some((
            stage_name(video.stage.as_ref()),
            video.layer,
            video.opacity.clamp(0.0, 1.0),
            Content::Video {
                start_time: video.start_time.as_secs_f64(),
                end_time: video.end_time.map(|end| end.as_secs_f64()),
                looped: video.infinite_loop || video.play_count > 1,
                hold: video.hold_at_end,
                rate: if video.rate.is_finite() && video.rate > 0.0 {
                    video.rate
                } else {
                    1.0
                },
            },
        )),
        CueKind::Text(text) => Some((
            stage_name(text.stage.as_ref()),
            text.layer,
            1.0,
            Content::Text {
                text: text.text.clone(),
            },
        )),
        _ => None,
    }
}

impl Stages {
    pub fn new() -> Self {
        Self::default()
    }

    /// Follows a batch of engine events, at `now` seconds. Returns whether
    /// anything on the stages changed.
    pub fn handle(&mut self, workspace: &Workspace, events: &[Event], now: f64) -> bool {
        let mut changed = false;
        for event in events {
            match event.kind {
                EventKind::ActionStarted { cue, armed: true } => {
                    let Some(definition) = workspace.find_cue(cue) else {
                        continue;
                    };
                    if let CueKind::Fade(fade) = &definition.kind {
                        if let (Some(target), Some(opacity)) = (definition.target, fade.opacity) {
                            changed |= self.fade(target, opacity, fade.duration, now);
                        }
                        continue;
                    }
                    let Some((stage, layer, opacity, content)) = placement(&definition.kind) else {
                        continue;
                    };
                    let into = self.loaded.remove(&cue).map_or(Duration::ZERO, |time| {
                        time.saturating_sub(definition.pre_wait)
                    });
                    self.items.insert(
                        cue,
                        StageItem {
                            cue,
                            stage,
                            layer,
                            opacity,
                            fade: None,
                            content,
                            started: now,
                            offset: into.as_secs_f64(),
                            paused: false,
                        },
                    );
                    changed = true;
                }
                EventKind::Stopped(cue) | EventKind::ActionCompleted(cue) => {
                    changed |= self.items.remove(&cue).is_some();
                }
                EventKind::Paused(cue) => {
                    if let Some(item) = self.items.get_mut(&cue).filter(|item| !item.paused) {
                        item.offset = item.position(now);
                        item.paused = true;
                        changed = true;
                    }
                }
                EventKind::Resumed(cue) => {
                    if let Some(item) = self.items.get_mut(&cue).filter(|item| item.paused) {
                        item.started = now;
                        item.paused = false;
                        changed = true;
                    }
                }
                EventKind::Loaded { cue, time } => {
                    self.loaded.insert(cue, time);
                }
                EventKind::Unloaded(cue) => {
                    self.loaded.remove(&cue);
                }
                _ => {}
            }
        }
        changed
    }

    /// Takes `cue` to `opacity` over `duration`, from where it is now.
    fn fade(&mut self, cue: CueId, opacity: f64, duration: Duration, now: f64) -> bool {
        let Some(item) = self.items.get_mut(&cue) else {
            return false;
        };
        let from = item.opacity_at(now);
        item.opacity = opacity.clamp(0.0, 1.0);
        item.fade = (!duration.is_zero()).then_some(OpacityFade {
            from,
            started: now,
            duration: duration.as_secs_f64(),
        });
        true
    }

    /// Follows edits made to cues that are up: another stage, layer,
    /// opacity or text shows at once. Returns whether anything changed.
    pub fn sync(&mut self, workspace: &Workspace) -> bool {
        let mut changed = false;
        self.items.retain(|cue, item| {
            let Some((stage, layer, opacity, content)) = workspace
                .find_cue(*cue)
                .and_then(|definition| placement(&definition.kind))
            else {
                // Deleted, or no longer something a stage shows.
                changed = true;
                return false;
            };
            if item.stage != stage || item.layer != layer || item.content != content {
                item.stage = stage;
                item.layer = layer;
                item.content = content;
                changed = true;
            }
            // An opacity being faded belongs to the fade.
            if item.fade.is_none() && item.opacity != opacity {
                item.opacity = opacity;
                changed = true;
            }
            true
        });
        changed
    }

    /// What is up, lowest layer first.
    pub fn state(&self, workspace: &Workspace, now: f64) -> StageState {
        let mut items: Vec<StageItem> = self.items.values().cloned().collect();
        items.sort_by_key(|item| item.layer);
        let mut stages: Vec<String> = workspace
            .settings
            .stages
            .iter()
            .map(|stage| stage.name.clone())
            .collect();
        if stages.is_empty() {
            stages.push(DEFAULT_STAGE.to_owned());
        }
        StageState { now, stages, items }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazzalab_core::{Cue, FadeCue, TextCue, VideoCue};
    use mazzalab_engine::Engine;

    fn secs(value: f64) -> Duration {
        Duration::from_secs_f64(value)
    }

    fn show() -> Engine {
        let video = Cue::new(CueKind::Video(VideoCue {
            file: Some("clip.mp4".into()),
            layer: 2,
            media_duration: Some(secs(10.0)),
            ..VideoCue::default()
        }))
        .with_id(CueId::from_u128(1));
        let title = Cue::new(CueKind::Text(TextCue {
            text: "Atto primo".into(),
            stage: Some("Side".into()),
            layer: 5,
        }))
        .with_id(CueId::from_u128(2));
        let fade = Cue::new(CueKind::Fade(FadeCue {
            duration: secs(2.0),
            opacity: Some(0.0),
            ..FadeCue::default()
        }))
        .with_id(CueId::from_u128(3))
        .with_target(CueId::from_u128(1));
        let mut workspace = Workspace::new("Video");
        workspace.cue_lists[0].cues = vec![video, title, fade];
        Engine::new(workspace)
    }

    /// Runs the engine to `at` and hands its events to the stages.
    fn step(engine: &mut Engine, stages: &mut Stages, at: f64) -> bool {
        engine.advance_to(secs(at));
        let events = engine.take_events();
        stages.handle(engine.workspace(), &events, at)
    }

    #[test]
    fn video_and_text_go_up_on_their_stage_and_layer() {
        let mut engine = show();
        let mut stages = Stages::new();
        engine.start(CueId::from_u128(1)).unwrap();
        engine.start(CueId::from_u128(2)).unwrap();
        assert!(step(&mut engine, &mut stages, 0.0));
        let state = stages.state(engine.workspace(), 1.0);
        assert_eq!(state.stages, [DEFAULT_STAGE]);
        assert_eq!(state.items.len(), 2);
        let video = &state.items[0];
        assert_eq!(
            (video.stage.as_str(), video.layer, video.opacity),
            (DEFAULT_STAGE, 2, 1.0)
        );
        assert_eq!(video.position(3.5), 3.5);
        assert_eq!(state.items[1].stage, "Side");
        assert_eq!(
            state.items[1].content,
            Content::Text {
                text: "Atto primo".into()
            }
        );

        // The file plays out and comes down.
        assert!(step(&mut engine, &mut stages, 10.0));
        assert_eq!(stages.state(engine.workspace(), 10.0).items.len(), 1);
    }

    #[test]
    fn pausing_holds_the_picture_and_loading_starts_it_further_on() {
        let mut engine = show();
        let mut stages = Stages::new();
        engine.load(CueId::from_u128(1), secs(4.0)).unwrap();
        step(&mut engine, &mut stages, 0.0);
        engine.start(CueId::from_u128(1)).unwrap();
        step(&mut engine, &mut stages, 1.0);
        let item = |stages: &Stages, engine: &Engine| {
            stages.state(engine.workspace(), 0.0).items[0].clone()
        };
        assert_eq!(item(&stages, &engine).position(2.0), 5.0);

        engine.advance_to(secs(3.0));
        engine.pause(CueId::from_u128(1)).unwrap();
        step(&mut engine, &mut stages, 3.0);
        let paused = item(&stages, &engine);
        assert!(paused.paused);
        assert_eq!(paused.position(9.0), 6.0);
        engine.resume(CueId::from_u128(1)).unwrap();
        step(&mut engine, &mut stages, 5.0);
        assert_eq!(item(&stages, &engine).position(6.0), 7.0);
    }

    #[test]
    fn a_fade_cue_takes_the_opacity_down_from_where_it_is() {
        let mut engine = show();
        let mut stages = Stages::new();
        engine.start(CueId::from_u128(1)).unwrap();
        step(&mut engine, &mut stages, 0.0);
        engine.advance_to(secs(1.0));
        engine.start(CueId::from_u128(3)).unwrap();
        step(&mut engine, &mut stages, 1.0);
        let item = stages.state(engine.workspace(), 1.0).items[0].clone();
        assert_eq!(item.opacity, 0.0);
        assert_eq!(item.opacity_at(2.0), 0.5);
        assert_eq!(item.opacity_at(3.5), 0.0);
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["kind"], "video");
        assert_eq!(json["fade"]["from"], 1.0);
    }

    #[test]
    fn edits_show_at_once_and_a_deleted_cue_comes_down() {
        let mut engine = show();
        let mut stages = Stages::new();
        engine.start(CueId::from_u128(2)).unwrap();
        step(&mut engine, &mut stages, 0.0);
        let mut workspace = engine.workspace().clone();
        if let Some(CueKind::Text(text)) = workspace
            .find_cue_mut(CueId::from_u128(2))
            .map(|cue| &mut cue.kind)
        {
            text.text = "Atto secondo".into();
            text.layer = 1;
        }
        assert!(stages.sync(&workspace));
        let item = &stages.state(&workspace, 0.0).items[0];
        assert_eq!(
            (item.layer, &item.content),
            (
                1,
                &Content::Text {
                    text: "Atto secondo".into()
                }
            )
        );
        assert!(!stages.sync(&workspace));
        workspace.cue_lists[0]
            .cues
            .retain(|cue| cue.id != CueId::from_u128(2));
        assert!(stages.sync(&workspace));
        assert!(stages.is_empty());
    }
}
