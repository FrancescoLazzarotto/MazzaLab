//! Linear timecode (SMPTE 12M): 80 bits a frame, biphase-mark coded as an
//! audio signal, so that it can be sent down a cable like sound.

use crate::{Rate, Timecode};

/// Bits in an LTC frame.
const BITS: usize = 80;
/// The sync word that closes every frame, bits 64 to 79 read as a number
/// with bit 64 lowest.
const SYNC: u128 = 0xBFFC;

/// The 80 bits of a frame, bit 0 first.
pub fn frame_bits(timecode: Timecode, rate: Rate) -> [bool; BITS] {
    let mut bits = [false; BITS];
    let mut put = |start: usize, value: u8, width: usize| {
        for offset in 0..width {
            bits[start + offset] = (value >> offset) & 1 == 1;
        }
    };
    put(0, timecode.frames % 10, 4);
    put(8, timecode.frames / 10, 2);
    put(16, timecode.seconds % 10, 4);
    put(24, timecode.seconds / 10, 3);
    put(32, timecode.minutes % 10, 4);
    put(40, timecode.minutes / 10, 3);
    put(48, timecode.hours % 10, 4);
    put(56, timecode.hours / 10, 2);
    bits[10] = rate.drop_frame;
    for (offset, bit) in bits[64..].iter_mut().enumerate() {
        *bit = (SYNC >> offset) & 1 == 1;
    }
    // Every frame holds an even number of ones, so that each one starts on
    // the same polarity: the correction bit makes it so.
    let correction = if rate.base == 25 { 59 } else { 27 };
    if bits.iter().filter(|bit| **bit).count() % 2 == 1 {
        bits[correction] = true;
    }
    bits
}

/// Turns timecode into samples, frame after frame from a start.
#[derive(Debug, Clone)]
pub struct Encoder {
    rate: Rate,
    level: f32,
    samples_per_bit: f64,
    timecode: Timecode,
    bits: [bool; BITS],
    bit: usize,
    /// Samples into the current bit.
    position: f64,
    /// The mid-bit transition of a one has been made.
    halfway: bool,
    high: bool,
}

impl Encoder {
    /// Starts at `start`; `level` is the peak sample value.
    pub fn new(start: Timecode, rate: Rate, sample_rate: u32, level: f32) -> Self {
        Self {
            rate,
            level,
            samples_per_bit: f64::from(sample_rate) / (rate.fps() * BITS as f64),
            timecode: start,
            bits: frame_bits(start, rate),
            bit: 0,
            position: 0.0,
            halfway: false,
            // The transition that opens the first bit has just been made.
            high: true,
        }
    }

    /// The frame being sent.
    pub fn timecode(&self) -> Timecode {
        self.timecode
    }

    /// Writes the next samples of the signal, mono.
    pub fn fill(&mut self, out: &mut [f32]) {
        for sample in out {
            if self.position >= self.samples_per_bit {
                self.position -= self.samples_per_bit;
                self.bit += 1;
                if self.bit == BITS {
                    self.bit = 0;
                    self.timecode = self.rate.add(self.timecode, 1);
                    self.bits = frame_bits(self.timecode, self.rate);
                }
                // Every bit opens with a transition...
                self.high = !self.high;
                self.halfway = false;
            }
            // ...and a one has another halfway through.
            if self.bits[self.bit] && !self.halfway && self.position >= self.samples_per_bit / 2.0 {
                self.high = !self.high;
                self.halfway = true;
            }
            *sample = if self.high { self.level } else { -self.level };
            self.position += 1.0;
        }
    }
}

/// A frame read back from the signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub timecode: Timecode,
    pub drop_frame: bool,
}

/// Reads timecode out of samples, at any of the frame rates and with the
/// level and speed drifting a little.
#[derive(Debug, Clone)]
pub struct Decoder {
    /// Samples since the last zero crossing.
    since: f64,
    positive: bool,
    /// The signal's DC offset, followed slowly and taken out.
    offset: f32,
    /// The signal's level, as its recent peaks: a crossing counts once the
    /// signal is a fair way past zero, so that noise around zero is not
    /// read as transitions.
    envelope: f32,
    /// The length of a bit, as measured so far.
    bit_length: f64,
    /// A short interval came: the first half of a one.
    half: bool,
    /// The last 80 bits, bit 0 of a frame lowest once it is in place.
    window: u128,
}

impl Decoder {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            since: 0.0,
            positive: false,
            offset: 0.0,
            envelope: 0.0,
            // Start from 30 fps: 24 and 25 still tell longs from shorts.
            bit_length: f64::from(sample_rate) / (30.0 * BITS as f64),
            half: false,
            window: 0,
        }
    }

    /// Reads samples; returns the frames whose sync word ended in them.
    pub fn feed(&mut self, samples: &[f32]) -> Vec<Frame> {
        let mut frames = Vec::new();
        for &sample in samples {
            self.since += 1.0;
            self.offset += (sample - self.offset) * 0.001;
            let sample = sample - self.offset;
            self.envelope = (self.envelope * 0.9995).max(sample.abs());
            let threshold = (self.envelope * 0.3).max(0.001);
            let positive = if sample > threshold {
                true
            } else if sample < -threshold {
                false
            } else {
                self.positive
            };
            if positive == self.positive {
                continue;
            }
            self.positive = positive;
            let interval = std::mem::take(&mut self.since);
            if interval > self.bit_length * 0.75 {
                // A whole bit without a transition halfway: a zero.
                self.half = false;
                self.bit_length = self.bit_length * 0.9 + interval * 0.1;
                if let Some(frame) = self.push(false) {
                    frames.push(frame);
                }
            } else if self.half {
                self.half = false;
                self.bit_length = self.bit_length * 0.9 + interval * 2.0 * 0.1;
                if let Some(frame) = self.push(true) {
                    frames.push(frame);
                }
            } else {
                self.half = true;
            }
        }
        frames
    }

    fn push(&mut self, bit: bool) -> Option<Frame> {
        self.window = (self.window >> 1) | (u128::from(bit) << (BITS - 1));
        if (self.window >> 64) & 0xFFFF != SYNC {
            return None;
        }
        let field = |start: u32, width: u32| ((self.window >> start) & ((1 << width) - 1)) as u8;
        let timecode = Timecode::new(
            field(56, 2) * 10 + field(48, 4),
            field(40, 3) * 10 + field(32, 4),
            field(24, 3) * 10 + field(16, 4),
            field(8, 2) * 10 + field(0, 4),
        );
        Some(Frame {
            timecode,
            drop_frame: field(10, 1) == 1,
        })
    }
}

/// Reads timecode from a signal and tells its rate as well: how many
/// frames come a second, measured from the samples between them, and
/// whether they drop frame numbers, from the frames' own flag.
#[derive(Debug, Clone)]
pub struct Reader {
    decoder: Decoder,
    sample_rate: f64,
    /// Samples read so far.
    samples: u64,
    /// Where the last frame ended, in samples.
    last: Option<u64>,
    /// Samples a frame takes, smoothed.
    frame_length: Option<f64>,
}

impl Reader {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            decoder: Decoder::new(sample_rate),
            sample_rate: f64::from(sample_rate.max(1)),
            samples: 0,
            last: None,
            frame_length: None,
        }
    }

    /// Reads samples; returns the frames whose end came in them, with the
    /// rate they run at. The first frame waits for the second, which tells
    /// the rate.
    pub fn feed(&mut self, samples: &[f32]) -> Vec<(Timecode, Rate)> {
        let mut read = Vec::new();
        // Where a frame ends is known to the block it ended in: blocks of
        // 64 samples tell 24 frames a second from 25 well enough.
        let mut start = 0;
        while start < samples.len() {
            let end = (start + 64).min(samples.len());
            let frames = self.decoder.feed(&samples[start..end]);
            self.samples += (end - start) as u64;
            for frame in frames {
                if let Some(rate) = self.measure(frame.drop_frame) {
                    read.push((frame.timecode, rate));
                }
            }
            start = end;
        }
        read
    }

    /// Notes a frame ending about now and returns the rate, once known.
    fn measure(&mut self, drop_frame: bool) -> Option<Rate> {
        let now = self.samples;
        let previous = self.last.replace(now);
        if let Some(previous) = previous {
            let length = (now - previous) as f64;
            // A gap (the signal stopped, or skipped) starts the measure afresh.
            let plausible = (self.sample_rate / 31.0..=self.sample_rate / 23.0).contains(&length);
            if plausible {
                self.frame_length = Some(match self.frame_length {
                    Some(smoothed) => smoothed * 0.8 + length * 0.2,
                    None => length,
                });
            } else {
                self.frame_length = None;
            }
        }
        let fps = self.sample_rate / self.frame_length?;
        let base = [24_u8, 25, 30]
            .into_iter()
            .min_by(|a, b| {
                (f64::from(*a) - fps)
                    .abs()
                    .total_cmp(&(f64::from(*b) - fps).abs())
            })
            .unwrap_or(30);
        // Drop frame only exists at 29.97; anything else is read as it counts.
        Some(if drop_frame && base == 30 {
            Rate::FPS_29_97_DROP
        } else {
            Rate::new(base, false, false)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(rate: Rate, sample_rate: u32, start: Timecode, seconds: f64) -> Vec<Frame> {
        let mut encoder = Encoder::new(start, rate, sample_rate, 0.5);
        let mut decoder = Decoder::new(sample_rate);
        let mut frames = Vec::new();
        let mut block = vec![0.0; 512];
        let total = (seconds * f64::from(sample_rate)) as usize;
        let mut done = 0;
        while done < total {
            encoder.fill(&mut block);
            frames.extend(decoder.feed(&block));
            done += block.len();
        }
        frames
    }

    #[test]
    fn frames_hold_an_even_number_of_ones_and_end_in_the_sync_word() {
        for rate in [Rate::FPS_24, Rate::FPS_25, Rate::FPS_29_97_DROP] {
            for frame in [
                Timecode::new(0, 0, 0, 0),
                Timecode::new(23, 59, 59, 23),
                Timecode::new(12, 34, 56, 7),
            ] {
                let bits = frame_bits(frame, rate);
                assert_eq!(bits.iter().filter(|bit| **bit).count() % 2, 0);
                assert_eq!(&bits[64..68], &[false, false, true, true]);
                assert_eq!(&bits[76..], &[true, true, false, true]);
            }
        }
    }

    #[test]
    fn what_is_sent_is_read_back_frame_after_frame() {
        for (rate, sample_rate) in [
            (Rate::FPS_25, 48_000),
            (Rate::FPS_24, 44_100),
            (Rate::FPS_30, 48_000),
            (Rate::FPS_29_97_DROP, 44_100),
        ] {
            let start = Timecode::new(1, 9, 59, 20);
            let frames = round_trip(rate, sample_rate, start, 2.0);
            let expected_count = (2.0 * rate.fps()) as usize;
            assert!(
                frames.len() + 2 >= expected_count,
                "{rate:?}: {} frames",
                frames.len()
            );
            // Each frame follows the one before, across the minute.
            for pair in frames.windows(2) {
                assert_eq!(rate.add(pair[0].timecode, 1), pair[1].timecode, "{rate:?}");
            }
            assert!(
                frames
                    .iter()
                    .all(|frame| frame.drop_frame == rate.drop_frame)
            );
            // The very first frame is read too.
            assert_eq!(frames[0].timecode, start, "{rate:?}");
        }
    }

    #[test]
    fn a_quiet_noisy_offset_and_dull_signal_still_reads() {
        // What comes off a long cable or a microphone: low, off centre,
        // rounded edges and hiss.
        let rate = Rate::FPS_25;
        let start = Timecode::new(2, 0, 0, 0);
        let mut encoder = Encoder::new(start, rate, 48_000, 1.0);
        let mut decoder = Decoder::new(48_000);
        let mut block = vec![0.0; 480];
        let mut smooth = 0.0_f32;
        let mut noise = 12345_u32;
        let mut frames = Vec::new();
        for _ in 0..100 {
            encoder.fill(&mut block);
            for sample in &mut block {
                smooth += (*sample - smooth) * 0.25;
                noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let hiss = (noise >> 8) as f32 / (1 << 24) as f32 - 0.5;
                *sample = 0.05 * smooth + 0.2 + 0.01 * hiss;
            }
            frames.extend(decoder.feed(&block));
        }
        assert!(frames.len() >= 20, "{} frames", frames.len());
        for pair in frames.windows(2) {
            assert_eq!(rate.add(pair[0].timecode, 1), pair[1].timecode);
        }
    }

    #[test]
    fn the_reader_tells_the_rate_from_the_signal() {
        for (rate, sample_rate) in [
            (Rate::FPS_24, 48_000),
            (Rate::FPS_25, 44_100),
            (Rate::FPS_30, 48_000),
            (Rate::FPS_29_97_DROP, 48_000),
        ] {
            let start = Timecode::new(10, 0, 0, 0);
            let mut encoder = Encoder::new(start, rate, sample_rate, 0.5);
            let mut reader = Reader::new(sample_rate);
            let mut block = vec![0.0; 480];
            let mut read = Vec::new();
            for _ in 0..(sample_rate / 480) {
                encoder.fill(&mut block);
                read.extend(reader.feed(&block));
            }
            assert!(
                read.len() as f64 >= rate.fps() - 3.0,
                "{rate:?}: {}",
                read.len()
            );
            assert!(
                read.iter().all(|(_, read_rate)| *read_rate == rate),
                "{rate:?}"
            );
            // The first frame only told where the measure starts.
            assert_eq!(read[0].0, rate.add(start, 1), "{rate:?}");
        }
    }

    #[test]
    fn the_encoder_keeps_time() {
        let rate = Rate::FPS_25;
        let mut encoder = Encoder::new(Timecode::new(10, 0, 0, 0), rate, 48_000, 0.5);
        let mut block = vec![0.0; 48_010];
        encoder.fill(&mut block);
        // Just past a second of samples: 25 frames sent, the 26th under way.
        assert_eq!(encoder.timecode(), Timecode::new(10, 0, 1, 0));
        assert!(block.iter().all(|sample| sample.abs() == 0.5));
    }
}
