//! File decoding with symphonia: probing and a streaming reader thread that
//! feeds a voice's ring buffer.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use rtrb::Producer;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase};

use crate::AudioError;
use crate::mixer::MAX_CHANNELS;

/// What a media file contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaInfo {
    /// Total length, when the container declares it.
    pub duration: Option<Duration>,
    pub channels: usize,
    pub sample_rate: u32,
}

/// An opened file, ready to be decoded.
pub(crate) struct Source {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    pub(crate) info: MediaInfo,
}

pub(crate) fn open(path: &Path) -> Result<Source, AudioError> {
    let error = |message: String| AudioError::Media {
        path: path.to_owned(),
        message,
    };
    let file = File::open(path).map_err(|e| error(e.to_string()))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }
    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| error(e.to_string()))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| error("no audio track".into()))?;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or_else(|| error("no audio codec parameters".into()))?;
    let decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| error(e.to_string()))?;
    let sample_rate = params
        .sample_rate
        .ok_or_else(|| error("unknown sample rate".into()))?;
    let channels = params.channels.as_ref().map_or(0, |c| c.count());
    let duration = track
        .num_frames
        .map(|frames| Duration::from_secs_f64(frames as f64 / f64::from(sample_rate)));
    let track_id = track.id;
    let time_base = track.time_base;
    Ok(Source {
        format,
        decoder,
        track_id,
        time_base,
        info: MediaInfo {
            duration,
            channels: channels.clamp(1, MAX_CHANNELS),
            sample_rate,
        },
    })
}

/// Reads a file's format information without decoding it.
pub fn probe(path: &Path) -> Result<MediaInfo, AudioError> {
    open(path).map(|source| source.info)
}

/// Which part of the file to play and how often.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Playback {
    pub start: Duration,
    pub end: Option<Duration>,
    /// Number of passes; `None` loops forever.
    pub passes: Option<u32>,
}

/// Spawns the reader thread. It decodes ahead into `producer`, sets
/// `ended` when everything has been written, and quits early when the voice
/// is dropped.
pub(crate) fn spawn_reader(
    source: Source,
    playback: Playback,
    producer: Producer<f32>,
    ended: Arc<AtomicBool>,
) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("mazzalab-audio-reader".into())
        .spawn(move || {
            let mut reader = Reader {
                source,
                producer,
                interleaved: Vec::new(),
            };
            reader.run(playback);
            ended.store(true, Ordering::Release);
        })
}

struct Reader {
    source: Source,
    producer: Producer<f32>,
    interleaved: Vec<f32>,
}

enum Flow {
    Continue,
    /// The voice was dropped: stop decoding.
    Abandoned,
}

impl Reader {
    fn run(&mut self, playback: Playback) {
        let rate = f64::from(self.source.info.sample_rate);
        let limit = playback
            .end
            .map(|end| (end.saturating_sub(playback.start).as_secs_f64() * rate).round() as u64);
        let mut pass = 0_u32;
        loop {
            let skip = if pass == 0 && playback.start.is_zero() {
                Some(0)
            } else {
                self.seek(playback.start)
            };
            let Some(skip) = skip else {
                tracing::warn!("cannot seek; stopping playback");
                return;
            };
            if let Flow::Abandoned = self.pass(skip, limit) {
                return;
            }
            pass += 1;
            if playback.passes.is_some_and(|passes| pass >= passes) {
                return;
            }
        }
    }

    /// Seeks to `start` and returns how many decoded frames to drop to land
    /// exactly on it.
    fn seek(&mut self, start: Duration) -> Option<u64> {
        let time = Time::try_from_secs_f64(start.as_secs_f64())?;
        let seeked = self
            .source
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(self.source.track_id),
                },
            )
            .ok()?;
        self.source.decoder.reset();
        let ticks = (seeked.required_ts.get() - seeked.actual_ts.get()).max(0) as f64;
        let seconds = self.source.time_base.map_or(0.0, |base| {
            ticks * f64::from(base.numer.get()) / f64::from(base.denom.get())
        });
        Some((seconds * f64::from(self.source.info.sample_rate)).round() as u64)
    }

    /// Decodes one pass, dropping `skip` frames first and stopping after
    /// `limit` frames.
    fn pass(&mut self, mut skip: u64, limit: Option<u64>) -> Flow {
        let channels = self.source.info.channels;
        let mut written = 0_u64;
        loop {
            let packet = match self.source.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Flow::Continue,
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(error) => {
                    tracing::warn!(%error, "audio file read error");
                    return Flow::Continue;
                }
            };
            if packet.track_id != self.source.track_id {
                continue;
            }
            let decoded = match self.source.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(SymphoniaError::DecodeError(_) | SymphoniaError::IoError(_)) => continue,
                Err(error) => {
                    tracing::warn!(%error, "audio decode error");
                    return Flow::Continue;
                }
            };
            let source_channels = decoded.spec().channels().count().max(1);
            self.interleaved.resize(decoded.samples_interleaved(), 0.0);
            decoded.copy_to_slice_interleaved(&mut self.interleaved[..]);

            let frames = self.interleaved.len() / source_channels;
            let mut first = 0;
            if skip > 0 {
                let dropped = skip.min(frames as u64);
                skip -= dropped;
                first = dropped as usize;
            }
            let mut last = frames;
            if let Some(limit) = limit {
                let remaining = limit.saturating_sub(written);
                last = last.min(first + remaining as usize);
            }
            for frame in first..last {
                let start = frame * source_channels;
                let samples = &self.interleaved[start..start + source_channels.min(channels)];
                if let Flow::Abandoned = Self::push_frame(&mut self.producer, samples, channels) {
                    return Flow::Abandoned;
                }
            }
            written += (last - first) as u64;
            if limit.is_some_and(|limit| written >= limit) {
                return Flow::Continue;
            }
        }
    }

    fn push_frame(producer: &mut Producer<f32>, samples: &[f32], channels: usize) -> Flow {
        while producer.slots() < channels {
            if producer.is_abandoned() {
                return Flow::Abandoned;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        for channel in 0..channels {
            // A file with fewer channels than announced is padded with silence.
            let _ = producer.push(samples.get(channel).copied().unwrap_or(0.0));
        }
        Flow::Continue
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rtrb::RingBuffer;

    /// Writes a 16-bit PCM WAV whose sample `i` of every channel is `i`.
    pub(crate) fn write_ramp_wav(path: &Path, rate: u32, channels: u16, frames: u32) {
        let data_len = frames * u32::from(channels) * 2;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
        bytes.extend_from_slice(&(channels * 2).to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for frame in 0..frames {
            for _ in 0..channels {
                bytes.extend_from_slice(&(frame as i16).to_le_bytes());
            }
        }
        std::fs::write(path, bytes).unwrap();
    }

    pub(crate) fn temp_wav(
        name: &str,
        rate: u32,
        channels: u16,
        frames: u32,
    ) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mazzalab-audio-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        write_ramp_wav(&path, rate, channels, frames);
        path
    }

    /// Decodes with `playback` and returns the first channel as frame indices.
    fn read(path: &Path, playback: Playback) -> Vec<i32> {
        let source = open(path).unwrap();
        let channels = source.info.channels;
        let (producer, mut consumer) = RingBuffer::new(1 << 20);
        let ended = Arc::new(AtomicBool::new(false));
        spawn_reader(source, playback, producer, ended.clone())
            .unwrap()
            .join()
            .unwrap();
        assert!(ended.load(Ordering::Acquire));
        let mut samples = Vec::new();
        while let Ok(sample) = consumer.pop() {
            samples.push(sample);
        }
        samples
            .chunks(channels)
            .map(|frame| (frame[0] * 32768.0).round() as i32)
            .collect()
    }

    #[test]
    fn probe_reports_length_and_format() {
        let path = temp_wav("probe.wav", 8000, 2, 4000);
        let info = probe(&path).unwrap();
        assert_eq!(info.channels, 2);
        assert_eq!(info.sample_rate, 8000);
        assert_eq!(info.duration, Some(Duration::from_millis(500)));
        assert!(probe(Path::new("missing.wav")).is_err());
    }

    #[test]
    fn reads_whole_file() {
        let path = temp_wav("whole.wav", 8000, 1, 1000);
        let frames = read(
            &path,
            Playback {
                start: Duration::ZERO,
                end: None,
                passes: Some(1),
            },
        );
        assert_eq!(frames.len(), 1000);
        assert_eq!(frames[0], 0);
        assert_eq!(frames[999], 999);
    }

    #[test]
    fn trims_start_and_end_and_loops() {
        let path = temp_wav("trim.wav", 8000, 2, 2000);
        let frames = read(
            &path,
            Playback {
                start: Duration::from_millis(50),
                end: Some(Duration::from_millis(100)),
                passes: Some(2),
            },
        );
        // 50 ms at 8 kHz is 400 frames; two passes over frames 400..800.
        assert_eq!(frames.len(), 800);
        assert_eq!(frames[0], 400);
        assert_eq!(frames[399], 799);
        assert_eq!(frames[400], 400);
        assert_eq!(frames[799], 799);
    }
}
