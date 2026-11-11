//! Reduces an audio file to a fixed number of peaks, for drawing waveforms.

use std::path::Path;

use crate::AudioError;
use crate::decode;

/// A file's outline: for each of `peaks.len() / 2` equal slices of the
/// file, the lowest and the highest sample of the mono mix.
#[derive(Debug, Clone, PartialEq)]
pub struct Waveform {
    /// Length in seconds, counted from the decoded audio.
    pub duration: f64,
    pub channels: usize,
    pub sample_rate: u32,
    /// `[min, max, min, max, ...]`, values in `-1.0..=1.0`.
    pub peaks: Vec<f32>,
}

/// Longest run of frames reduced to one intermediate peak.
const MAX_CHUNK: u64 = 4096;

/// Decodes the whole file and returns `buckets` peaks (fewer for files
/// shorter than that many frames).
pub fn waveform(path: &Path, buckets: usize) -> Result<Waveform, AudioError> {
    let buckets = buckets.clamp(1, 16_384);
    let mut source = decode::open(path)?;
    let info = source.info;

    // Peaks are first taken over small chunks, then merged into the buckets,
    // so the length of the file does not have to be known up front.
    let chunk = info.duration.map_or(256, |duration| {
        let frames = duration.as_secs_f64() * f64::from(info.sample_rate);
        ((frames / (buckets as f64 * 4.0)) as u64).clamp(1, MAX_CHUNK)
    });
    let mut chunks: Vec<(f32, f32)> = Vec::new();
    let mut current = (f32::MAX, f32::MIN);
    let mut in_chunk = 0_u64;
    let mut frames = 0_u64;
    source.decode_all(|samples, channels| {
        for frame in samples.chunks_exact(channels) {
            let value = frame.iter().sum::<f32>() / channels as f32;
            current = (current.0.min(value), current.1.max(value));
            in_chunk += 1;
            frames += 1;
            if in_chunk == chunk {
                chunks.push(current);
                current = (f32::MAX, f32::MIN);
                in_chunk = 0;
            }
        }
    });
    if in_chunk > 0 {
        chunks.push(current);
    }

    let buckets = buckets.min(chunks.len());
    let mut peaks = Vec::with_capacity(buckets * 2);
    for bucket in 0..buckets {
        let first = bucket * chunks.len() / buckets;
        let last = ((bucket + 1) * chunks.len() / buckets).max(first + 1);
        let (low, high) = chunks[first..last]
            .iter()
            .fold((f32::MAX, f32::MIN), |acc, chunk| {
                (acc.0.min(chunk.0), acc.1.max(chunk.1))
            });
        peaks.push(low.clamp(-1.0, 1.0));
        peaks.push(high.clamp(-1.0, 1.0));
    }
    Ok(Waveform {
        duration: frames as f64 / f64::from(info.sample_rate.max(1)),
        channels: info.channels,
        sample_rate: info.sample_rate,
        peaks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::tests::temp_wav;

    #[test]
    fn peaks_follow_the_file() {
        // A ramp from 0 to 7999 (as 16-bit samples) over one second.
        let path = temp_wav("waveform.wav", 8000, 2, 8000);
        let waveform = waveform(&path, 10).unwrap();
        assert_eq!(waveform.channels, 2);
        assert_eq!(waveform.sample_rate, 8000);
        assert!((waveform.duration - 1.0).abs() < 1e-9);
        assert_eq!(waveform.peaks.len(), 20);
        let scale = 32768.0;
        // First slice covers frames 0..800, last one 7200..8000.
        assert!(waveform.peaks[0].abs() < 1e-6);
        assert!((waveform.peaks[1] - 799.0 / scale).abs() < 1e-4);
        assert!((waveform.peaks[18] - 7200.0 / scale).abs() < 1e-4);
        assert!((waveform.peaks[19] - 7999.0 / scale).abs() < 1e-4);
    }

    #[test]
    fn short_files_give_fewer_peaks() {
        let path = temp_wav("waveform-short.wav", 8000, 1, 5);
        let waveform = waveform(&path, 100).unwrap();
        assert_eq!(waveform.peaks.len(), 10);
    }

    #[test]
    fn missing_file_is_an_error() {
        assert!(waveform(Path::new("no-such-file.wav"), 10).is_err());
    }
}
