//! The real-time mixer. Everything here runs inside the audio callback, so
//! it never allocates, locks or blocks: voices arrive pre-built through a
//! lock-free queue and leave through another one to be dropped elsewhere.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use mazzalab_core::{CueId, FadeCurve};
use rtrb::{Consumer, Producer};

/// Maximum number of channels read from a source.
pub(crate) const MAX_CHANNELS: usize = 16;
/// Levels at or below this are treated as silence.
pub(crate) const SILENT_DB: f32 = -90.0;
/// Short fade applied when a voice is stopped, to avoid clicks.
const DECLICK_SECONDS: f32 = 0.005;

pub(crate) fn db_to_gain(db: f32) -> f32 {
    if db <= SILENT_DB {
        0.0
    } else {
        10_f32.powf(db / 20.0)
    }
}

#[derive(Debug, Clone, Copy)]
struct Ramp {
    from_db: f32,
    to_db: f32,
    total: u32,
    done: u32,
    curve: FadeCurve,
}

/// A level in dB that can glide to a new value. Fades are computed in the
/// dB domain, which sounds even to the ear.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Level {
    db: f32,
    ramp: Option<Ramp>,
}

impl Level {
    pub(crate) fn new(db: f32) -> Self {
        Self { db, ramp: None }
    }

    pub(crate) fn fade_to(&mut self, to_db: f32, frames: u32, curve: FadeCurve) {
        if frames == 0 {
            self.db = to_db;
            self.ramp = None;
        } else {
            self.ramp = Some(Ramp {
                from_db: self.db.max(SILENT_DB),
                to_db,
                total: frames,
                done: 0,
                curve,
            });
        }
    }

    fn is_settled_silent(&self) -> bool {
        self.ramp.is_none() && self.db <= SILENT_DB
    }

    /// Advances one frame and returns the linear gain for it.
    fn next_gain(&mut self) -> f32 {
        if let Some(ramp) = &mut self.ramp {
            ramp.done += 1;
            let t = ramp.done as f32 / ramp.total as f32;
            let shaped = match ramp.curve {
                FadeCurve::Linear => t,
                FadeCurve::SCurve => t * t * (3.0 - 2.0 * t),
            };
            self.db = ramp.from_db + (ramp.to_db - ramp.from_db) * shaped;
            if ramp.done >= ramp.total {
                self.db = ramp.to_db;
                self.ramp = None;
            }
        }
        db_to_gain(self.db)
    }
}

/// Cubic Hermite interpolation between `y1` and `y2`.
fn hermite(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * t + c2) * t + c1) * t + y1
}

enum Pull {
    Frame,
    /// The source has ended; a silent frame was appended.
    Drained,
    /// The decoder has not caught up yet.
    Starved,
}

/// One playing sound: reads interleaved samples from its decoder's ring
/// buffer, resamples them to the output rate and applies its level.
pub struct Voice {
    pub(crate) cue: CueId,
    samples: Consumer<f32>,
    ended: Arc<AtomicBool>,
    channels: usize,
    /// Source frames per output frame (sample-rate ratio times playback rate).
    step: f64,
    /// Fractional read position between `history[1]` and `history[2]`.
    position: f64,
    history: [[f32; MAX_CHANNELS]; 4],
    drained: u8,
    level: Level,
    paused: bool,
    stopping: bool,
    output_rate: u32,
}

impl std::fmt::Debug for Voice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Voice")
            .field("cue", &self.cue)
            .field("channels", &self.channels)
            .field("step", &self.step)
            .finish_non_exhaustive()
    }
}

impl Voice {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        cue: CueId,
        samples: Consumer<f32>,
        ended: Arc<AtomicBool>,
        channels: usize,
        source_rate: u32,
        output_rate: u32,
        rate: f64,
        level_db: f32,
    ) -> Self {
        let rate = if rate.is_finite() && rate > 0.0 {
            rate
        } else {
            1.0
        };
        Self {
            cue,
            samples,
            ended,
            channels: channels.clamp(1, MAX_CHANNELS),
            step: f64::from(source_rate) * rate / f64::from(output_rate.max(1)),
            // Pull three frames before the first output frame.
            position: 3.0,
            history: [[0.0; MAX_CHANNELS]; 4],
            drained: 0,
            level: Level::new(level_db),
            paused: false,
            stopping: false,
            output_rate,
        }
    }

    fn stop(&mut self) {
        if !self.stopping {
            self.stopping = true;
            self.paused = false;
            let frames = (DECLICK_SECONDS * self.output_rate as f32) as u32;
            self.level
                .fade_to(SILENT_DB, frames.max(1), FadeCurve::Linear);
        }
    }

    fn shift(&mut self, frame: [f32; MAX_CHANNELS]) {
        self.history.rotate_left(1);
        self.history[3] = frame;
    }

    fn pull(&mut self) -> Pull {
        if self.samples.slots() >= self.channels {
            let mut frame = [0.0; MAX_CHANNELS];
            for sample in frame.iter_mut().take(self.channels) {
                *sample = self.samples.pop().unwrap_or(0.0);
            }
            self.shift(frame);
            return Pull::Frame;
        }
        // The decoder pushes its last samples before raising `ended`, so
        // look at the buffer again once the flag is seen.
        if self.ended.load(Ordering::Acquire) {
            if self.samples.slots() >= self.channels {
                return self.pull();
            }
            self.shift([0.0; MAX_CHANNELS]);
            self.drained = self.drained.saturating_add(1);
            return Pull::Drained;
        }
        Pull::Starved
    }

    /// Adds this voice into `out` (interleaved). Returns `false` once the
    /// voice has finished and can be removed.
    fn mix(&mut self, out: &mut [f32], out_channels: usize) -> bool {
        for frame in out.chunks_exact_mut(out_channels) {
            if self.paused {
                continue;
            }
            let mut starved = false;
            while self.position >= 1.0 {
                match self.pull() {
                    Pull::Frame => {}
                    // Three padding frames flush the interpolation history.
                    Pull::Drained if self.drained >= 3 => return false,
                    Pull::Drained => {}
                    Pull::Starved => {
                        starved = true;
                        break;
                    }
                }
                self.position -= 1.0;
            }
            if starved {
                continue;
            }

            let gain = self.level.next_gain();
            if self.stopping && self.level.is_settled_silent() {
                return false;
            }
            let t = self.position as f32;
            let [h0, h1, h2, h3] = &self.history;
            if self.channels == 1 {
                let value = hermite(h0[0], h1[0], h2[0], h3[0], t) * gain;
                for sample in frame.iter_mut().take(2) {
                    *sample += value;
                }
            } else {
                for (channel, sample) in frame.iter_mut().enumerate().take(self.channels) {
                    *sample +=
                        hermite(h0[channel], h1[channel], h2[channel], h3[channel], t) * gain;
                }
            }
            self.position += self.step;
        }
        true
    }
}

fn find(voices: &mut [Box<Voice>], cue: CueId) -> impl Iterator<Item = &mut Box<Voice>> {
    voices.iter_mut().filter(move |voice| voice.cue == cue)
}

pub(crate) enum MixerCommand {
    Add(Box<Voice>),
    Stop(CueId),
    StopAll,
    Fade {
        cue: CueId,
        to_db: f32,
        frames: u32,
        curve: FadeCurve,
    },
    FadeAll {
        frames: u32,
    },
    Pause(CueId),
    Resume(CueId),
    Master(f32),
}

pub(crate) enum MixerEvent {
    /// A voice left the mix; `natural` is true when it played to its end
    /// rather than being stopped. Dropped outside the audio thread.
    Done { voice: Box<Voice>, natural: bool },
}

pub(crate) struct Mixer {
    // Boxed on purpose: voices cross the lock-free queues as pointers, so the
    // audio thread never copies or allocates them.
    #[allow(clippy::vec_box)]
    voices: Vec<Box<Voice>>,
    commands: Consumer<MixerCommand>,
    events: Producer<MixerEvent>,
    channels: usize,
    master: Level,
}

impl Mixer {
    pub(crate) fn new(
        commands: Consumer<MixerCommand>,
        events: Producer<MixerEvent>,
        channels: usize,
    ) -> Self {
        Self {
            voices: Vec::with_capacity(256),
            commands,
            events,
            channels: channels.max(1),
            master: Level::new(0.0),
        }
    }

    fn apply(&mut self, command: MixerCommand) {
        match command {
            MixerCommand::Add(voice) => {
                for old in find(&mut self.voices, voice.cue) {
                    old.stop();
                }
                self.voices.push(voice);
            }
            MixerCommand::Stop(cue) => {
                for voice in find(&mut self.voices, cue) {
                    voice.stop();
                }
            }
            MixerCommand::StopAll => {
                for voice in &mut self.voices {
                    voice.stop();
                }
            }
            MixerCommand::Fade {
                cue,
                to_db,
                frames,
                curve,
            } => {
                for voice in find(&mut self.voices, cue).filter(|voice| !voice.stopping) {
                    voice.level.fade_to(to_db, frames, curve);
                }
            }
            MixerCommand::FadeAll { frames } => {
                for voice in self.voices.iter_mut().filter(|voice| !voice.stopping) {
                    voice.level.fade_to(SILENT_DB, frames, FadeCurve::Linear);
                }
            }
            MixerCommand::Pause(cue) => {
                for voice in find(&mut self.voices, cue) {
                    voice.paused = true;
                }
            }
            MixerCommand::Resume(cue) => {
                for voice in find(&mut self.voices, cue) {
                    voice.paused = false;
                }
            }
            MixerCommand::Master(db) => self.master.fade_to(db, 0, FadeCurve::Linear),
        }
    }

    /// Renders one block of interleaved output.
    pub(crate) fn process(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        while let Ok(command) = self.commands.pop() {
            self.apply(command);
        }
        let mut index = 0;
        while index < self.voices.len() {
            if self.voices[index].mix(out, self.channels) {
                index += 1;
            } else {
                let voice = self.voices.swap_remove(index);
                let natural = !voice.stopping;
                // If the queue is full the voice is dropped here; it is rare
                // and only costs a deallocation.
                let _ = self.events.push(MixerEvent::Done { voice, natural });
            }
        }
        for frame in out.chunks_exact_mut(self.channels) {
            let gain = self.master.next_gain();
            for sample in frame {
                *sample *= gain;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtrb::RingBuffer;

    const RATE: u32 = 1000;

    struct Rig {
        mixer: Mixer,
        commands: Producer<MixerCommand>,
        events: Consumer<MixerEvent>,
    }

    fn rig(channels: usize) -> Rig {
        let (commands, command_rx) = RingBuffer::new(64);
        let (event_tx, events) = RingBuffer::new(64);
        Rig {
            mixer: Mixer::new(command_rx, event_tx, channels),
            commands,
            events,
        }
    }

    /// A voice whose source is already fully decoded.
    fn voice(
        cue: u128,
        samples: &[f32],
        channels: usize,
        source_rate: u32,
        level_db: f32,
    ) -> Box<Voice> {
        let (mut producer, consumer) = RingBuffer::new(samples.len().max(1));
        for sample in samples {
            producer.push(*sample).unwrap();
        }
        let ended = Arc::new(AtomicBool::new(true));
        Box::new(Voice::new(
            CueId::from_u128(cue),
            consumer,
            ended,
            channels,
            source_rate,
            RATE,
            1.0,
            level_db,
        ))
    }

    fn render(rig: &mut Rig, frames: usize, channels: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * channels];
        rig.mixer.process(&mut out);
        out
    }

    fn done(rig: &mut Rig) -> Vec<(CueId, bool)> {
        let mut out = Vec::new();
        while let Ok(MixerEvent::Done { voice, natural }) = rig.events.pop() {
            out.push((voice.cue, natural));
        }
        out
    }

    #[test]
    fn plays_a_source_through_and_reports_the_end() {
        let mut rig = rig(2);
        let source: Vec<f32> = (0..100).flat_map(|i| [i as f32, -(i as f32)]).collect();
        rig.commands
            .push(MixerCommand::Add(voice(1, &source, 2, RATE, 0.0)))
            .unwrap();

        let out = render(&mut rig, 200, 2);
        // Same rate: samples come out unchanged and in order.
        assert_eq!(&out[..6], &[0.0, 0.0, 1.0, -1.0, 2.0, -2.0]);
        assert_eq!(out[2 * 99], 99.0);
        assert!(out[2 * 150..].iter().all(|s| *s == 0.0));
        assert_eq!(done(&mut rig), vec![(CueId::from_u128(1), true)]);
    }

    #[test]
    fn mono_goes_to_both_front_channels_with_gain() {
        let mut rig = rig(4);
        rig.commands
            .push(MixerCommand::Add(voice(1, &[1.0; 50], 1, RATE, -6.0)))
            .unwrap();
        let out = render(&mut rig, 10, 4);
        let gain = db_to_gain(-6.0);
        assert!((out[0] - gain).abs() < 1e-6);
        assert!((out[1] - gain).abs() < 1e-6);
        assert_eq!(out[2], 0.0);
        assert_eq!(out[3], 0.0);
    }

    #[test]
    fn resampling_changes_the_number_of_frames() {
        let mut rig = rig(1);
        // 100 frames at twice the output rate play in about 50 output frames.
        rig.commands
            .push(MixerCommand::Add(voice(1, &[1.0; 100], 1, RATE * 2, 0.0)))
            .unwrap();
        let out = render(&mut rig, 100, 1);
        let audible = out.iter().filter(|s| s.abs() > 0.5).count();
        assert!((48..=52).contains(&audible), "{audible}");
    }

    #[test]
    fn stop_fades_quickly_and_is_not_a_natural_end() {
        let mut rig = rig(1);
        rig.commands
            .push(MixerCommand::Add(voice(1, &[1.0; 1000], 1, RATE, 0.0)))
            .unwrap();
        render(&mut rig, 10, 1);
        rig.commands
            .push(MixerCommand::Stop(CueId::from_u128(1)))
            .unwrap();
        let out = render(&mut rig, 20, 1);
        assert!(out[0] < 1.0);
        assert!(out[10..].iter().all(|s| *s == 0.0));
        assert_eq!(done(&mut rig), vec![(CueId::from_u128(1), false)]);
    }

    #[test]
    fn fades_reach_their_target() {
        let mut rig = rig(1);
        rig.commands
            .push(MixerCommand::Add(voice(1, &[1.0; 1000], 1, RATE, 0.0)))
            .unwrap();
        rig.commands
            .push(MixerCommand::Fade {
                cue: CueId::from_u128(1),
                to_db: -20.0,
                frames: 100,
                curve: FadeCurve::SCurve,
            })
            .unwrap();
        let out = render(&mut rig, 200, 1);
        assert!(out[50] < 1.0 && out[50] > 0.1);
        assert!((out[150] - 0.1).abs() < 1e-3);
    }

    #[test]
    fn pause_holds_the_position() {
        let mut rig = rig(1);
        let source: Vec<f32> = (1..=100).map(|i| i as f32).collect();
        rig.commands
            .push(MixerCommand::Add(voice(1, &source, 1, RATE, 0.0)))
            .unwrap();
        render(&mut rig, 10, 1);
        rig.commands
            .push(MixerCommand::Pause(CueId::from_u128(1)))
            .unwrap();
        assert!(render(&mut rig, 10, 1).iter().all(|s| *s == 0.0));
        rig.commands
            .push(MixerCommand::Resume(CueId::from_u128(1)))
            .unwrap();
        assert_eq!(render(&mut rig, 1, 1), vec![11.0]);
    }

    #[test]
    fn starved_voice_waits_for_data() {
        let mut rig = rig(1);
        let (mut producer, consumer) = RingBuffer::new(16);
        let ended = Arc::new(AtomicBool::new(false));
        rig.commands
            .push(MixerCommand::Add(Box::new(Voice::new(
                CueId::from_u128(1),
                consumer,
                ended.clone(),
                1,
                RATE,
                RATE,
                1.0,
                0.0,
            ))))
            .unwrap();
        assert!(render(&mut rig, 4, 1).iter().all(|s| *s == 0.0));
        for value in [5.0, 6.0, 7.0, 8.0] {
            producer.push(value).unwrap();
        }
        ended.store(true, Ordering::Release);
        let out = render(&mut rig, 8, 1);
        assert_eq!(&out[..4], &[5.0, 6.0, 7.0, 8.0]);
        assert!(done(&mut rig).len() == 1);
    }
}
