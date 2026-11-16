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

use mazzalab_core::{CueId, CueKind, FadeCue, FadeCurve, Geometry, Stage, TextStyle, Workspace};
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
        style: TextStyle,
    },
}

/// An opacity and a geometry on their way to others.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Fade {
    /// The opacity it started from.
    pub from: f64,
    pub from_geometry: Geometry,
    /// When it started, on the callers' clock, in seconds.
    pub started: f64,
    pub duration: f64,
    pub curve: FadeCurve,
}

impl Fade {
    /// How far through it is at `now`, 0 to 1, along its curve.
    pub fn progress(&self, now: f64) -> f64 {
        if self.duration <= 0.0 {
            return 1.0;
        }
        let t = ((now - self.started) / self.duration).clamp(0.0, 1.0);
        match self.curve {
            FadeCurve::Linear => t,
            FadeCurve::SCurve => t * t * (3.0 - 2.0 * t),
        }
    }
}

/// A cue that is up on a stage.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StageItem {
    pub cue: CueId,
    pub stage: String,
    pub layer: i32,
    /// The opacity it has, or is fading to.
    pub opacity: f64,
    /// Where it sits, or is moving to.
    pub geometry: Geometry,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fade: Option<Fade>,
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
            Some(fade) => fade.from + (self.opacity - fade.from) * fade.progress(now),
            None => self.opacity,
        }
    }

    /// Its geometry at `now`, part way through a fade.
    pub fn geometry_at(&self, now: f64) -> Geometry {
        match self.fade {
            Some(fade) => fade.from_geometry.lerp(&self.geometry, fade.progress(now)),
            None => self.geometry,
        }
    }
}

/// Everything that is up, on every stage, at a moment.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StageState {
    /// The callers' clock when this was taken, so that stages can line
    /// their own clock up with it.
    pub now: f64,
    /// The show's stages, with their sizes.
    pub stages: Vec<Stage>,
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

/// The stage a cue goes to: the one it names, or the show's first.
fn stage_name(stage: Option<&String>, default: &str) -> String {
    stage
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .unwrap_or(default)
        .to_owned()
}

/// The show's first stage, where cues that name none go.
fn default_stage(workspace: &Workspace) -> &str {
    workspace
        .settings
        .stages
        .first()
        .map_or(DEFAULT_STAGE, |stage| stage.name.as_str())
}

/// What a Video or Text cue puts on a stage, and where.
struct Placement {
    stage: String,
    layer: i32,
    opacity: f64,
    geometry: Geometry,
    content: Content,
}

/// An opacity a stage can show.
fn opacity(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// What a cue puts on a stage, and where, if it is a Video or Text cue.
fn placement(kind: &CueKind, default: &str) -> Option<Placement> {
    match kind {
        CueKind::Video(video) if video.file.is_some() => Some(Placement {
            stage: stage_name(video.stage.as_ref(), default),
            layer: video.layer,
            opacity: opacity(video.opacity),
            geometry: video.geometry.sanitized(),
            content: Content::Video {
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
        }),
        CueKind::Text(text) => Some(Placement {
            stage: stage_name(text.stage.as_ref(), default),
            layer: text.layer,
            opacity: opacity(text.opacity),
            geometry: text.geometry.sanitized(),
            content: Content::Text {
                text: text.text.clone(),
                style: text.style.clone(),
            },
        }),
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
                        if let Some(target) = definition.target
                            && (fade.opacity.is_some() || fade.geometry.is_some())
                        {
                            changed |= self.fade(target, fade, now);
                        }
                        continue;
                    }
                    let Some(Placement {
                        stage,
                        layer,
                        opacity,
                        geometry,
                        content,
                    }) = placement(&definition.kind, default_stage(workspace))
                    else {
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
                            geometry,
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
                // Another cue fades this one away; the engine stops it after.
                EventKind::FadingOut { cue, fade } => {
                    let out = FadeCue {
                        duration: fade,
                        curve: FadeCurve::Linear,
                        opacity: Some(0.0),
                        ..FadeCue::default()
                    };
                    changed |= self.fade(cue, &out, now);
                }
                _ => {}
            }
        }
        changed
    }

    /// Takes `cue` to the opacity and geometry a Fade cue sets, over its
    /// duration, from where it is now. What the fade does not set stays.
    fn fade(&mut self, cue: CueId, fade: &FadeCue, now: f64) -> bool {
        let Some(item) = self.items.get_mut(&cue) else {
            return false;
        };
        let from = item.opacity_at(now);
        let from_geometry = item.geometry_at(now);
        item.opacity = fade.opacity.map_or(from, opacity);
        item.geometry = fade.geometry.map_or(from_geometry, |to| Geometry {
            // A fade moves the picture; how it fills the stage stays.
            fit: from_geometry.fit,
            ..to.sanitized()
        });
        item.fade = (!fade.duration.is_zero()).then_some(Fade {
            from,
            from_geometry,
            started: now,
            duration: fade.duration.as_secs_f64(),
            curve: fade.curve,
        });
        true
    }

    /// Follows edits made to cues that are up: another stage, layer,
    /// opacity or text shows at once. Returns whether anything changed.
    pub fn sync(&mut self, workspace: &Workspace) -> bool {
        let mut changed = false;
        self.items.retain(|cue, item| {
            let Some(Placement {
                stage,
                layer,
                opacity,
                geometry,
                content,
            }) = workspace
                .find_cue(*cue)
                .and_then(|definition| placement(&definition.kind, default_stage(workspace)))
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
            // An opacity and a geometry that were faded belong to the fade.
            if item.fade.is_none() && (item.opacity != opacity || item.geometry != geometry) {
                item.opacity = opacity;
                item.geometry = geometry;
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
        let mut stages = workspace.settings.stages.clone();
        if stages.is_empty() {
            stages.push(Stage::new(DEFAULT_STAGE));
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
    use mazzalab_core::{Cue, Fit, TextCue, VideoCue};
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
            ..TextCue::default()
        }))
        .with_id(CueId::from_u128(2));
        let fade = Cue::new(CueKind::Fade(FadeCue {
            duration: secs(2.0),
            curve: FadeCurve::Linear,
            opacity: Some(0.0),
            ..FadeCue::default()
        }))
        .with_id(CueId::from_u128(3))
        .with_target(CueId::from_u128(1));
        let slide = Cue::new(CueKind::Fade(FadeCue {
            duration: secs(4.0),
            curve: FadeCurve::SCurve,
            geometry: Some(Geometry {
                fit: Fit::Stretch,
                x: 400.0,
                scale_x: 0.5,
                scale_y: 0.5,
                ..Geometry::default()
            }),
            ..FadeCue::default()
        }))
        .with_id(CueId::from_u128(4))
        .with_target(CueId::from_u128(1));
        let mut workspace = Workspace::new("Video");
        workspace.cue_lists[0].cues = vec![video, title, fade, slide];
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
        assert_eq!(state.stages, [Stage::new(DEFAULT_STAGE)]);
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
                text: "Atto primo".into(),
                style: TextStyle::default(),
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
        // An opacity fade leaves the picture where it is.
        assert_eq!(item.geometry, Geometry::default());
    }

    #[test]
    fn a_fade_cue_moves_the_picture_along_its_curve() {
        let mut engine = show();
        let mut stages = Stages::new();
        engine.start(CueId::from_u128(1)).unwrap();
        step(&mut engine, &mut stages, 0.0);
        engine.start(CueId::from_u128(4)).unwrap();
        step(&mut engine, &mut stages, 0.0);
        let item = stages.state(engine.workspace(), 0.0).items[0].clone();
        // How the picture fills the stage is not something a fade moves.
        assert_eq!(item.geometry.fit, Fit::Fit);
        assert_eq!((item.geometry.x, item.geometry.scale_x), (400.0, 0.5));
        assert_eq!(item.opacity, 1.0);
        let half = item.geometry_at(2.0);
        assert_eq!((half.x, half.scale_y), (200.0, 0.75));
        // An S-curve starts slowly.
        assert!(item.geometry_at(1.0).x < 100.0);
        assert_eq!(item.geometry_at(9.0), item.geometry);

        // A fade that starts part way through another starts from where that one got to.
        engine.advance_to(secs(2.0));
        engine.start(CueId::from_u128(3)).unwrap();
        step(&mut engine, &mut stages, 2.0);
        let item = stages.state(engine.workspace(), 2.0).items[0].clone();
        assert_eq!(item.geometry, half);
        assert_eq!(item.fade.unwrap().from_geometry, half);
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
                    text: "Atto secondo".into(),
                    style: TextStyle::default(),
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
