//! The real-time mixer. Everything here runs inside the audio callback, so
//! it never allocates, locks or blocks: voices arrive pre-built through a
//! lock-free queue and leave through another one to be dropped elsewhere.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use mazzalab_core::{CueId, FadeCurve};
use rtrb::{Consumer, Producer};

/// Maximum number of channels read from a source.
pub(crate) const MAX_CHANNELS: usize = 16;
/// Maximum number of output channels a voice can be routed to.
pub(crate) const MAX_OUTPUTS: usize = 64;
/// Maximum number of crosspoints one voice can feed.
pub(crate) const MAX_ROUTES: usize = 256;
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

/// A linear gain that glides along a quarter sine, used for the fades built
/// into a cue (fade in at the start, fade out before the end). It multiplies
/// the cue's [`Level`], so fade cues and built-in fades do not fight.
#[derive(Debug, Clone, Copy)]
struct Envelope {
    gain: f32,
    from: f32,
    to: f32,
    total: u32,
    done: u32,
}

impl Envelope {
    fn new(gain: f32) -> Self {
        Self {
            gain,
            from: gain,
            to: gain,
            total: 0,
            done: 0,
        }
    }

    fn fade_to(&mut self, to: f32, frames: u32) {
        if frames == 0 {
            *self = Self::new(to);
        } else {
            self.from = self.gain;
            self.to = to;
            self.total = frames;
            self.done = 0;
        }
    }

    fn next_gain(&mut self) -> f32 {
        if self.done < self.total {
            self.done += 1;
            let t = self.done as f32 / self.total as f32;
            // Equal-power shape: quick to become audible, smooth at the top.
            let shaped = if self.to >= self.from {
                (t * std::f32::consts::FRAC_PI_2).sin()
            } else {
                1.0 - ((1.0 - t) * std::f32::consts::FRAC_PI_2).sin()
            };
            self.gain = self.from + (self.to - self.from) * shaped;
            if self.done >= self.total {
                self.gain = self.to;
            }
        }
        self.gain
    }
}

/// One crosspoint: a source channel feeding an output channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Route {
    pub(crate) input: u8,
    pub(crate) output: u8,
    pub(crate) gain: f32,
}

/// The crosspoints of a voice, in a fixed array so that voices never
/// allocate on the audio thread.
#[derive(Clone, Copy)]
pub(crate) struct Routes {
    count: usize,
    items: [Route; MAX_ROUTES],
}

impl std::fmt::Debug for Routes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

impl Routes {
    pub(crate) fn empty() -> Self {
        Self {
            count: 0,
            items: [Route {
                input: 0,
                output: 0,
                gain: 0.0,
            }; MAX_ROUTES],
        }
    }

    fn push(&mut self, input: usize, output: usize, gain: f32) {
        if self.count < MAX_ROUTES && gain != 0.0 {
            self.items[self.count] = Route {
                input: input as u8,
                output: output as u8,
                gain,
            };
            self.count += 1;
        }
    }

    pub(crate) fn as_slice(&self) -> &[Route] {
        &self.items[..self.count]
    }

    /// The routing used when a cue has no matrix: mono goes to the first two
    /// outputs, anything else goes channel for channel. A single output
    /// receives an even mix of every channel.
    pub(crate) fn default_for(inputs: usize, outputs: usize) -> Self {
        let inputs = inputs.clamp(1, MAX_CHANNELS);
        let mut routes = Self::empty();
        if outputs == 1 {
            for input in 0..inputs {
                routes.push(input, 0, 1.0 / inputs as f32);
            }
        } else if inputs == 1 {
            routes.push(0, 0, 1.0);
            routes.push(0, 1, 1.0);
        } else {
            for channel in 0..inputs.min(outputs) {
                routes.push(channel, channel, 1.0);
            }
        }
        routes
    }

    /// Crosspoint levels in dB, `matrix[input][output]`; missing and `None`
    /// entries are silent, as are crosspoints outside the device.
    pub(crate) fn from_matrix(matrix: &[Vec<Option<f64>>], inputs: usize, outputs: usize) -> Self {
        let mut routes = Self::empty();
        for (input, row) in matrix.iter().enumerate().take(inputs.min(MAX_CHANNELS)) {
            for (output, level) in row.iter().enumerate().take(outputs.min(MAX_OUTPUTS)) {
                if let Some(db) = level {
                    routes.push(input, output, db_to_gain(*db as f32));
                }
            }
        }
        routes
    }
}

/// Peak level of every output channel since the last read. Written by the
/// audio thread and read by anyone, without locks.
#[derive(Debug)]
pub struct Meters {
    peaks: Vec<AtomicU32>,
}

impl Meters {
    pub(crate) fn new(channels: usize) -> Self {
        Self {
            peaks: (0..channels.min(MAX_OUTPUTS))
                .map(|_| AtomicU32::new(0))
                .collect(),
        }
    }

    /// Bit patterns of non-negative floats order like the floats themselves,
    /// so an integer maximum is a float maximum.
    fn record(&self, channel: usize, peak: f32) {
        if let Some(slot) = self.peaks.get(channel) {
            slot.fetch_max(peak.to_bits(), Ordering::Relaxed);
        }
    }

    /// Linear peaks since the previous call; resets them.
    pub fn take(&self) -> Vec<f32> {
        self.peaks
            .iter()
            .map(|slot| f32::from_bits(slot.swap(0, Ordering::Relaxed)))
            .collect()
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
    /// Tells voices of the same cue apart.
    pub(crate) serial: u64,
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
    envelope: Envelope,
    routes: Routes,
    /// Output frames rendered so far.
    played: u64,
    /// Output frame at which the built-in fade out starts.
    fade_out_at: Option<u64>,
    fade_out_frames: u32,
    /// Source frames read so far.
    pulled: u64,
    /// Source frames in the first pass and in each pass after it.
    passes: Option<(u64, u64)>,
    /// Source frame at which the voice ends, once devamped.
    stop_at: Option<u64>,
    paused: bool,
    stopping: bool,
    output_rate: u32,
}

/// Everything needed to build a [`Voice`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct VoiceConfig {
    pub cue: CueId,
    pub serial: u64,
    /// Channels in the source's ring buffer.
    pub channels: usize,
    pub source_rate: u32,
    pub output_rate: u32,
    /// Playback rate (1.0 is normal speed).
    pub rate: f64,
    pub level_db: f32,
    pub routes: Routes,
    /// Length of the fade from silence at the start, in output frames.
    pub fade_in_frames: u32,
    /// Where the fade to silence starts and how long it lasts, in output
    /// frames.
    pub fade_out: Option<(u64, u32)>,
    /// Source frames in the first pass and in each pass after it, when the
    /// source loops and its length is known; needed to devamp.
    pub passes: Option<(u64, u64)>,
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
    pub(crate) fn new(config: VoiceConfig, samples: Consumer<f32>, ended: Arc<AtomicBool>) -> Self {
        let rate = if config.rate.is_finite() && config.rate > 0.0 {
            config.rate
        } else {
            1.0
        };
        let mut envelope = Envelope::new(1.0);
        if config.fade_in_frames > 0 {
            envelope = Envelope::new(0.0);
            envelope.fade_to(1.0, config.fade_in_frames);
        }
        Self {
            cue: config.cue,
            serial: config.serial,
            samples,
            ended,
            channels: config.channels.clamp(1, MAX_CHANNELS),
            step: f64::from(config.source_rate) * rate / f64::from(config.output_rate.max(1)),
            // Pull three frames before the first output frame.
            position: 3.0,
            history: [[0.0; MAX_CHANNELS]; 4],
            drained: 0,
            level: Level::new(config.level_db),
            envelope,
            routes: config.routes,
            played: 0,
            fade_out_at: config.fade_out.map(|(at, _)| at),
            fade_out_frames: config.fade_out.map_or(0, |(_, frames)| frames),
            pulled: 0,
            passes: config.passes,
            stop_at: None,
            paused: false,
            stopping: false,
            output_rate: config.output_rate,
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

    /// Stops looping: the voice plays the pass it is in to the end and
    /// finishes there, as if the source had ended.
    fn devamp(&mut self) {
        let Some((first, pass)) = self.passes else {
            return;
        };
        if pass == 0 || self.stop_at.is_some() {
            return;
        }
        self.stop_at = Some(if self.pulled < first {
            first
        } else {
            first + ((self.pulled - first) / pass + 1) * pass
        });
    }

    fn shift(&mut self, frame: [f32; MAX_CHANNELS]) {
        self.history.rotate_left(1);
        self.history[3] = frame;
    }

    fn pull(&mut self) -> Pull {
        if self.stop_at.is_some_and(|at| self.pulled >= at) {
            self.shift([0.0; MAX_CHANNELS]);
            self.drained = self.drained.saturating_add(1);
            return Pull::Drained;
        }
        if self.samples.slots() >= self.channels {
            let mut frame = [0.0; MAX_CHANNELS];
            for sample in frame.iter_mut().take(self.channels) {
                *sample = self.samples.pop().unwrap_or(0.0);
            }
            self.shift(frame);
            self.pulled += 1;
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

            if self.fade_out_at == Some(self.played) {
                self.envelope.fade_to(0.0, self.fade_out_frames);
            }
            self.played += 1;
            let gain = self.level.next_gain() * self.envelope.next_gain();
            if self.stopping && self.level.is_settled_silent() {
                return false;
            }
            let t = self.position as f32;
            let [h0, h1, h2, h3] = &self.history;
            let mut inputs = [0.0_f32; MAX_CHANNELS];
            for (channel, value) in inputs.iter_mut().enumerate().take(self.channels) {
                *value = hermite(h0[channel], h1[channel], h2[channel], h3[channel], t) * gain;
            }
            for route in self.routes.as_slice() {
                if let Some(sample) = frame.get_mut(usize::from(route.output)) {
                    *sample += inputs[usize::from(route.input)] * route.gain;
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
    /// Ends a looping cue at the end of the pass it is in.
    Devamp(CueId),
    /// Replaces the crosspoints of a playing cue.
    Route {
        cue: CueId,
        routes: Box<Routes>,
    },
    Master(f32),
}

pub(crate) enum MixerEvent {
    /// A voice left the mix; `natural` is true when it played to its end
    /// rather than being stopped. Dropped outside the audio thread.
    Done { voice: Box<Voice>, natural: bool },
    /// Memory to free outside the audio thread.
    Garbage(Box<Routes>),
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
    meters: Arc<Meters>,
}

impl Mixer {
    pub(crate) fn new(
        commands: Consumer<MixerCommand>,
        events: Producer<MixerEvent>,
        channels: usize,
        meters: Arc<Meters>,
    ) -> Self {
        Self {
            voices: Vec::with_capacity(256),
            commands,
            events,
            channels: channels.max(1),
            master: Level::new(0.0),
            meters,
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
            MixerCommand::Devamp(cue) => {
                for voice in find(&mut self.voices, cue) {
                    voice.devamp();
                }
            }
            MixerCommand::Route { cue, routes } => {
                for voice in find(&mut self.voices, cue) {
                    voice.routes = *routes;
                }
                // The box is freed away from the audio thread.
                let _ = self.events.push(MixerEvent::Garbage(routes));
            }
            // A short glide keeps master moves free of zipper noise.
            MixerCommand::Master(db) => self.master.fade_to(db, 512, FadeCurve::Linear),
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
        let mut peaks = [0.0_f32; MAX_OUTPUTS];
        for frame in out.chunks_exact_mut(self.channels) {
            let gain = self.master.next_gain();
            for (channel, sample) in frame.iter_mut().enumerate() {
                *sample *= gain;
                if let Some(peak) = peaks.get_mut(channel) {
                    *peak = peak.max(sample.abs());
                }
            }
        }
        for (channel, peak) in peaks.iter().enumerate().take(self.channels) {
            if *peak > 0.0 {
                self.meters.record(channel, *peak);
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
        meters: Arc<Meters>,
        channels: usize,
    }

    fn rig(channels: usize) -> Rig {
        let (commands, command_rx) = RingBuffer::new(64);
        let (event_tx, events) = RingBuffer::new(64);
        let meters = Arc::new(Meters::new(channels));
        Rig {
            mixer: Mixer::new(command_rx, event_tx, channels, meters.clone()),
            commands,
            events,
            meters,
            channels,
        }
    }

    fn config(
        cue: u128,
        channels: usize,
        source_rate: u32,
        level_db: f32,
        outputs: usize,
    ) -> VoiceConfig {
        VoiceConfig {
            cue: CueId::from_u128(cue),
            serial: 0,
            channels,
            source_rate,
            output_rate: RATE,
            rate: 1.0,
            level_db,
            routes: Routes::default_for(channels, outputs),
            fade_in_frames: 0,
            fade_out: None,
            passes: None,
        }
    }

    fn loaded(samples: &[f32]) -> (Consumer<f32>, Arc<AtomicBool>) {
        let (mut producer, consumer) = RingBuffer::new(samples.len().max(1));
        for sample in samples {
            producer.push(*sample).unwrap();
        }
        (consumer, Arc::new(AtomicBool::new(true)))
    }

    fn voice_with(config: VoiceConfig, samples: &[f32]) -> Box<Voice> {
        let (consumer, ended) = loaded(samples);
        Box::new(Voice::new(config, consumer, ended))
    }

    /// A voice whose source is already fully decoded, with default routing
    /// to a device that has at least as many outputs as the tests use.
    fn voice(
        cue: u128,
        samples: &[f32],
        channels: usize,
        source_rate: u32,
        level_db: f32,
    ) -> Box<Voice> {
        // Routing to a single output is exercised separately.
        let outputs = if channels == 1 { 2 } else { channels };
        voice_with(
            config(cue, channels, source_rate, level_db, outputs),
            samples,
        )
    }

    fn render(rig: &mut Rig, frames: usize, channels: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * channels];
        rig.mixer.process(&mut out);
        out
    }

    fn done(rig: &mut Rig) -> Vec<(CueId, bool)> {
        let mut out = Vec::new();
        while let Ok(event) = rig.events.pop() {
            if let MixerEvent::Done { voice, natural } = event {
                out.push((voice.cue, natural));
            }
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
                config(1, 1, RATE, 0.0, 1),
                consumer,
                ended.clone(),
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

    #[test]
    fn matrix_routes_channels_to_any_output() {
        let mut rig = rig(4);
        let source: Vec<f32> = (0..50).flat_map(|_| [1.0, 0.5]).collect();
        let mut config = config(1, 2, RATE, 0.0, 4);
        // Left to output 4 at full level, right to output 1 at -6 dB.
        config.routes = Routes::from_matrix(
            &[
                vec![None, None, None, Some(0.0)],
                vec![Some(-6.0), None, None, None],
            ],
            2,
            4,
        );
        rig.commands
            .push(MixerCommand::Add(voice_with(config, &source)))
            .unwrap();
        let out = render(&mut rig, 10, 4);
        let frame = &out[4 * 5..4 * 6];
        assert!((frame[0] - 0.5 * db_to_gain(-6.0)).abs() < 1e-6);
        assert_eq!(frame[1], 0.0);
        assert_eq!(frame[2], 0.0);
        assert!((frame[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn matrix_ignores_outputs_the_device_does_not_have() {
        let routes = Routes::from_matrix(&[vec![Some(0.0), None, Some(0.0)]], 1, 2);
        assert_eq!(routes.as_slice().len(), 1);
        assert_eq!(routes.as_slice()[0].output, 0);
    }

    #[test]
    fn stereo_is_mixed_down_on_a_single_output() {
        let mut rig = rig(1);
        let source: Vec<f32> = (0..50).flat_map(|_| [1.0, 0.5]).collect();
        rig.commands
            .push(MixerCommand::Add(voice_with(
                config(1, 2, RATE, 0.0, 1),
                &source,
            )))
            .unwrap();
        let out = render(&mut rig, 10, 1);
        assert!((out[5] - 0.75).abs() < 1e-6);
    }

    #[test]
    fn built_in_fades_shape_the_start_and_the_end() {
        let mut rig = rig(1);
        let mut config = config(1, 1, RATE, 0.0, 1);
        config.fade_in_frames = 100;
        config.fade_out = Some((400, 100));
        rig.commands
            .push(MixerCommand::Add(voice_with(config, &[1.0; 500])))
            .unwrap();
        let out = render(&mut rig, 600, 1);
        assert!(out[0] < 0.05, "starts silent: {}", out[0]);
        assert!(out[50] > 0.5 && out[50] < 1.0, "rising: {}", out[50]);
        assert!((out[200] - 1.0).abs() < 1e-6, "full level in the middle");
        assert!(out[450] < 1.0 && out[450] > 0.0, "falling: {}", out[450]);
        assert!(out[499] < 0.05, "ends silent: {}", out[499]);
        assert_eq!(done(&mut rig), vec![(CueId::from_u128(1), true)]);
    }

    #[test]
    fn a_fade_cue_does_not_cancel_the_built_in_fade_in() {
        let mut rig = rig(1);
        let mut config = config(1, 1, RATE, 0.0, 1);
        config.fade_in_frames = 200;
        rig.commands
            .push(MixerCommand::Add(voice_with(config, &[1.0; 1000])))
            .unwrap();
        rig.commands
            .push(MixerCommand::Fade {
                cue: CueId::from_u128(1),
                to_db: -20.0,
                frames: 0,
                curve: FadeCurve::Linear,
            })
            .unwrap();
        let out = render(&mut rig, 400, 1);
        assert!(out[0] < 0.01);
        assert!((out[300] - 0.1).abs() < 1e-3);
    }

    #[test]
    fn route_command_repatches_a_playing_voice() {
        let mut rig = rig(2);
        rig.commands
            .push(MixerCommand::Add(voice(1, &[1.0; 100], 1, RATE, 0.0)))
            .unwrap();
        assert_eq!(&render(&mut rig, 1, 2), &[1.0, 1.0]);
        let routes = Routes::from_matrix(&[vec![None, Some(0.0)]], 1, 2);
        rig.commands
            .push(MixerCommand::Route {
                cue: CueId::from_u128(1),
                routes: Box::new(routes),
            })
            .unwrap();
        assert_eq!(&render(&mut rig, 1, 2), &[0.0, 1.0]);
    }

    #[test]
    fn meters_report_the_peak_of_each_output_once() {
        let mut rig = rig(2);
        let source: Vec<f32> = (0..50).flat_map(|_| [0.5, -0.25]).collect();
        rig.commands
            .push(MixerCommand::Add(voice(1, &source, 2, RATE, 0.0)))
            .unwrap();
        let channels = rig.channels;
        render(&mut rig, 20, channels);
        assert_eq!(rig.meters.take(), vec![0.5, 0.25]);
        assert_eq!(rig.meters.take(), vec![0.0, 0.0]);
    }

    #[test]
    fn devamp_ends_the_voice_at_the_end_of_the_current_pass() {
        let mut rig = rig(1);
        // A source that would loop five times over ten frames, the first
        // pass starting four frames in (a cue loaded to a time).
        let source: Vec<f32> = (0..46).map(|i| (i + 1) as f32).collect();
        let mut config = config(1, 1, RATE, 0.0, 1);
        config.passes = Some((6, 10));
        rig.commands
            .push(MixerCommand::Add(voice_with(config, &source)))
            .unwrap();
        // Eight frames out: two into the second pass.
        assert_eq!(render(&mut rig, 8, 1)[7], 8.0);
        rig.commands
            .push(MixerCommand::Devamp(CueId::from_u128(1)))
            .unwrap();
        let out = render(&mut rig, 40, 1);
        // The second pass ends after sixteen frames in all: eight more.
        assert_eq!(out[7], 16.0);
        assert!(
            out[8..].iter().all(|sample| *sample == 0.0),
            "{:?}",
            &out[..12]
        );
        // It ended on its own account, so the cue completes.
        assert_eq!(done(&mut rig), vec![(CueId::from_u128(1), true)]);
    }

    #[test]
    fn devamp_in_the_first_pass_and_without_known_passes() {
        let mut rig = rig(1);
        let source: Vec<f32> = (0..30).map(|i| (i + 1) as f32).collect();
        let mut first = config(1, 1, RATE, 0.0, 1);
        first.passes = Some((6, 10));
        rig.commands
            .push(MixerCommand::Add(voice_with(first, &source)))
            .unwrap();
        // A voice that does not know its passes plays on.
        rig.commands
            .push(MixerCommand::Add(voice_with(
                config(2, 1, RATE, 0.0, 1),
                &[0.0; 20],
            )))
            .unwrap();
        render(&mut rig, 2, 1);
        for cue in [1, 2] {
            rig.commands
                .push(MixerCommand::Devamp(CueId::from_u128(cue)))
                .unwrap();
        }
        let out = render(&mut rig, 10, 1);
        assert_eq!(out[3], 6.0);
        assert!(out[4..].iter().all(|sample| *sample == 0.0));
        assert_eq!(done(&mut rig), vec![(CueId::from_u128(1), true)]);
    }
}
