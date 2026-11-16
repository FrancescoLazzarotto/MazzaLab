//! Integrated loudness of a file, as ITU-R BS.1770-4 defines it (LUFS):
//! K-weighted, in 400 ms blocks overlapping by 75%, gated at -70 LUFS and
//! then 10 LU under the ungated loudness.

use std::path::Path;

use crate::AudioError;
use crate::decode;

const ABSOLUTE_GATE: f64 = -70.0;
const RELATIVE_GATE: f64 = -10.0;

/// A second-order section, direct form I.
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
}

impl Biquad {
    fn new(b: [f64; 3], a0: f64, a: [f64; 2]) -> Self {
        Self {
            b: b.map(|value| value / a0),
            a: a.map(|value| value / a0),
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    fn process(&mut self, input: f64) -> f64 {
        let output = self.b[0] * input + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [input, self.x[0]];
        self.y = [output, self.y[0]];
        output
    }
}

/// The two stages of the K-weighting filter at `rate`: a high shelf that
/// models the head, then a high pass. Derived by bilinear transform from the
/// standard's analogue prototypes, which gives its published coefficients
/// at 48 kHz and the same response at other rates.
fn k_weighting(rate: u32) -> [Biquad; 2] {
    let rate = f64::from(rate);
    let shelf = {
        let (gain, q, frequency) = (
            3.999_843_853_973_347,
            0.707_175_236_955_419_6,
            1_681.974_450_955_533,
        );
        let k = (std::f64::consts::PI * frequency / rate).tan();
        let vh = 10_f64.powf(gain / 20.0);
        let vb = vh.powf(0.499_666_774_154_541_6);
        let a0 = 1.0 + k / q + k * k;
        Biquad::new(
            [
                vh + vb * k / q + k * k,
                2.0 * (k * k - vh),
                vh - vb * k / q + k * k,
            ],
            a0,
            [2.0 * (k * k - 1.0), 1.0 - k / q + k * k],
        )
    };
    let high_pass = {
        let (q, frequency) = (0.500_327_037_323_877_3, 38.135_470_876_024_44);
        let k = (std::f64::consts::PI * frequency / rate).tan();
        let a0 = 1.0 + k / q + k * k;
        Biquad {
            b: [1.0, -2.0, 1.0],
            a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
            x: [0.0; 2],
            y: [0.0; 2],
        }
    };
    [shelf, high_pass]
}

/// How much each channel counts: 5.1 leaves out the LFE and gives the
/// surrounds +1.5 dB; otherwise every channel counts once.
fn weights(channels: usize) -> Vec<f64> {
    if channels == 6 {
        vec![1.0, 1.0, 1.0, 0.0, 1.41, 1.41]
    } else {
        vec![1.0; channels]
    }
}

/// Measures interleaved audio as it arrives.
pub(crate) struct Meter {
    filters: Vec<[Biquad; 2]>,
    weights: Vec<f64>,
    /// Frames in a 100 ms step.
    step: usize,
    in_step: usize,
    /// Per channel, the sum of squares of the step being filled.
    sums: Vec<f64>,
    /// Weighted mean square of each finished step.
    steps: Vec<f64>,
}

impl Meter {
    pub(crate) fn new(channels: usize, rate: u32) -> Self {
        let channels = channels.max(1);
        Self {
            filters: vec![k_weighting(rate); channels],
            weights: weights(channels),
            step: (rate as usize / 10).max(1),
            in_step: 0,
            sums: vec![0.0; channels],
            steps: Vec::new(),
        }
    }

    pub(crate) fn feed(&mut self, samples: &[f32]) {
        let channels = self.filters.len();
        for frame in samples.chunks_exact(channels) {
            for (channel, sample) in frame.iter().enumerate() {
                let [shelf, high_pass] = &mut self.filters[channel];
                let filtered = high_pass.process(shelf.process(f64::from(*sample)));
                self.sums[channel] += filtered * filtered;
            }
            self.in_step += 1;
            if self.in_step == self.step {
                let step = self.step as f64;
                let power = self
                    .sums
                    .iter()
                    .zip(&self.weights)
                    .map(|(sum, weight)| weight * sum / step)
                    .sum();
                self.steps.push(power);
                self.sums.fill(0.0);
                self.in_step = 0;
            }
        }
    }

    /// The integrated loudness in LUFS; `None` for audio shorter than a
    /// block or silent throughout.
    pub(crate) fn loudness(&self) -> Option<f64> {
        let blocks: Vec<f64> = self
            .steps
            .windows(4)
            .map(|window| window.iter().sum::<f64>() / 4.0)
            .collect();
        let lufs = |power: f64| -0.691 + 10.0 * power.log10();
        let mean = |powers: &[f64]| powers.iter().sum::<f64>() / powers.len() as f64;
        let audible: Vec<f64> = blocks
            .into_iter()
            .filter(|power| *power > 0.0 && lufs(*power) > ABSOLUTE_GATE)
            .collect();
        if audible.is_empty() {
            return None;
        }
        let threshold = lufs(mean(&audible)) + RELATIVE_GATE;
        let gated: Vec<f64> = audible
            .iter()
            .copied()
            .filter(|power| lufs(*power) > threshold)
            .collect();
        (!gated.is_empty()).then(|| lufs(mean(&gated)))
    }
}

/// Decodes the whole file and returns its integrated loudness in LUFS;
/// `None` when it is too short or silent.
pub fn loudness(path: &Path) -> Result<Option<f64>, AudioError> {
    let mut source = decode::open(path)?;
    let info = source.info;
    let mut meter: Option<Meter> = None;
    source.decode_all(|samples, channels| {
        meter
            .get_or_insert_with(|| Meter::new(channels, info.sample_rate))
            .feed(samples);
    });
    Ok(meter.and_then(|meter| meter.loudness()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(channels: usize, amplitude: f32, seconds: f32, rate: u32) -> Vec<f32> {
        let frames = (seconds * rate as f32) as usize;
        (0..frames)
            .flat_map(|frame| {
                let value = amplitude
                    * (std::f32::consts::TAU * 1_000.0 * frame as f32 / rate as f32).sin();
                std::iter::repeat_n(value, channels)
            })
            .collect()
    }

    fn measure(channels: usize, samples: &[f32], rate: u32) -> Option<f64> {
        let mut meter = Meter::new(channels, rate);
        meter.feed(samples);
        meter.loudness()
    }

    #[test]
    fn the_k_filter_matches_the_standard_at_48k() {
        let [shelf, high_pass] = k_weighting(48_000);
        let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
        assert!(close(shelf.b[0], 1.535_124_859_586_97));
        assert!(close(shelf.b[1], -2.691_696_189_406_38));
        assert!(close(shelf.a[0], -1.690_659_293_182_41));
        assert!(close(high_pass.a[0], -1.990_047_454_833_98));
        assert!(close(high_pass.a[1], 0.990_072_250_366_21));
    }

    #[test]
    fn a_full_scale_tone_reads_as_the_standard_says() {
        // A 1 kHz sine at 0 dBFS in both channels: about 0 LUFS.
        let stereo = measure(2, &sine(2, 1.0, 5.0, 48_000), 48_000).unwrap();
        assert!(stereo.abs() < 0.2, "{stereo}");
        // One channel of it: 3 dB less.
        let mono = measure(1, &sine(1, 1.0, 5.0, 48_000), 48_000).unwrap();
        assert!((mono + 3.01).abs() < 0.2, "{mono}");
        // 20 dB down reads 20 LU less, at another rate too.
        let quiet = measure(2, &sine(2, 0.1, 5.0, 44_100), 44_100).unwrap();
        assert!((quiet + 20.0).abs() < 0.2, "{quiet}");
    }

    #[test]
    fn silence_and_tiny_files_have_no_loudness() {
        assert_eq!(measure(2, &vec![0.0; 96_000], 48_000), None);
        assert_eq!(measure(2, &sine(2, 1.0, 0.2, 48_000), 48_000), None);
    }

    #[test]
    fn quiet_passages_are_gated_out() {
        // Five seconds of tone, then five of near-silence: the silence does
        // not drag the reading down.
        let mut samples = sine(2, 1.0, 5.0, 48_000);
        samples.extend(sine(2, 0.0001, 5.0, 48_000));
        let gated = measure(2, &samples, 48_000).unwrap();
        assert!(gated.abs() < 0.3, "{gated}");
    }
}
