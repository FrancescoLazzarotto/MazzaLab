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
            // Start from 30 fps: 24 and 25 still tell longs from shorts.
            bit_length: f64::from(sample_rate) / (30.0 * BITS as f64),
            half: false,
            window: 0,
        }
    }

    /// Reads samples; returns the frames whose sync word ended in them.
    pub fn feed(&mut self, samples: &[f32]) -> Vec<Frame> {
        let mut frames = Vec::new();
        for sample in samples {
            self.since += 1.0;
            let positive = *sample > 0.0;
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
