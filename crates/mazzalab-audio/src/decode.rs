//! File decoding with symphonia: probing and a streaming reader thread that
//! feeds a voice's ring buffer.

use std::collections::VecDeque;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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

use mazzalab_core::{Position, Segment};

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

impl Source {
    /// Decodes the whole file from the current position, handing each block
    /// of interleaved samples to `sink` together with its channel count.
    pub(crate) fn decode_all(&mut self, mut sink: impl FnMut(&[f32], usize)) {
        let mut interleaved = Vec::new();
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return,
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(error) => {
                    tracing::warn!(%error, "audio file read error");
                    return;
                }
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(SymphoniaError::DecodeError(_) | SymphoniaError::IoError(_)) => continue,
                Err(error) => {
                    tracing::warn!(%error, "audio decode error");
                    return;
                }
            };
            let channels = decoded.spec().channels().count().max(1);
            interleaved.resize(decoded.samples_interleaved(), 0.0);
            decoded.copy_to_slice_interleaved(&mut interleaved[..]);
            sink(&interleaved, channels);
        }
    }
}

/// Reads a file's format information without decoding it.
pub fn probe(path: &Path) -> Result<MediaInfo, AudioError> {
    open(path).map(|source| source.info)
}

/// Which parts of the file to play and how often.
#[derive(Debug, Clone)]
pub(crate) struct Playback {
    /// The stretches of the file, in order: the whole region, or slices.
    pub segments: Vec<Segment>,
    /// Where playing begins: a cue loaded to a time begins partway through.
    pub start: Position,
}

/// Frames a devamp answer says to drop when the voice must end instead.
pub(crate) const DEVAMP_END: u64 = u64::MAX;

/// How a voice asks its reader to devamp, and hears back where.
///
/// The reader decodes ahead, so only it knows where each pass ended in the
/// stream it wrote and how much it wrote past that point. The voice says
/// how far it has read; the reader answers with the end of that pass and
/// either the frames it wrote beyond it (which the voice drops when it gets
/// there, the next slice following straight after) or [`DEVAMP_END`].
#[derive(Debug, Default)]
pub(crate) struct DevampControl {
    /// 1 + the frames the voice had read when devamped; 0 for none.
    request: AtomicU64,
    /// 1 + the frame of the stream where the pass ends; 0 until answered.
    answer_at: AtomicU64,
    /// Written before `answer_at`.
    answer_skip: AtomicU64,
}

impl DevampControl {
    /// From the voice: devamp, having read `read` frames.
    pub(crate) fn request(&self, read: u64) {
        self.request.store(read + 1, Ordering::Release);
    }

    /// From the voice: the answer, once there is one: where, and how many
    /// frames to drop there (or [`DEVAMP_END`]).
    pub(crate) fn take_answer(&self) -> Option<(u64, u64)> {
        let at = self.answer_at.swap(0, Ordering::Acquire);
        (at > 0).then(|| (at - 1, self.answer_skip.load(Ordering::Acquire)))
    }

    /// What the voice asked for, if not answered yet.
    #[cfg(test)]
    pub(crate) fn requested(&self) -> Option<u64> {
        let request = self.request.load(Ordering::Acquire);
        (request > 0).then(|| request - 1)
    }

    pub(crate) fn answer(&self, at: u64, skip: u64) {
        self.answer_skip.store(skip, Ordering::Release);
        self.answer_at.store(at + 1, Ordering::Release);
    }
}

/// Spawns the reader thread. It decodes ahead into `producer`, sets
/// `ended` when everything has been written, answers devamps from
/// `control`, and quits once the voice is dropped.
pub(crate) fn spawn_reader(
    source: Source,
    playback: Playback,
    producer: Producer<f32>,
    ended: Arc<AtomicBool>,
    control: Arc<DevampControl>,
) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("mazzalab-audio-reader".into())
        .spawn(move || {
            let mut reader = Reader {
                source,
                producer,
                interleaved: Vec::new(),
                ended,
                control,
                segments: playback.segments.len(),
                written: 0,
                boundaries: VecDeque::new(),
                fresh: true,
            };
            reader.run(&playback);
        })
}

/// Pass ends remembered, to answer a devamp from a voice that is behind.
const BOUNDARIES_KEPT: usize = 4096;

struct Reader {
    source: Source,
    producer: Producer<f32>,
    interleaved: Vec<f32>,
    ended: Arc<AtomicBool>,
    control: Arc<DevampControl>,
    /// How many segments the playback has.
    segments: usize,
    /// Frames written to the stream so far.
    written: u64,
    /// Where recent passes ended in the stream, with their segment.
    boundaries: VecDeque<(u64, usize)>,
    /// Nothing decoded yet: the file is at its start without seeking.
    fresh: bool,
}

enum Flow {
    Continue,
    /// The voice was dropped: stop decoding.
    Abandoned,
    /// Devamped: carry on from the start of this segment.
    Jump(usize),
    /// Devamped in the last segment: nothing more to decode.
    End,
}

impl Reader {
    fn run(&mut self, playback: &Playback) {
        let mut from = Some(playback.start);
        loop {
            if let Some(start) = from.take() {
                match self.play(&playback.segments, start) {
                    Flow::Abandoned => return,
                    Flow::Jump(segment) => {
                        from = Some(Position {
                            segment,
                            pass: 0,
                            within: Duration::ZERO,
                        });
                        continue;
                    }
                    Flow::Continue | Flow::End => {}
                }
            }
            // All written: the voice plays it out and may still devamp.
            self.ended.store(true, Ordering::Release);
            loop {
                if self.producer.is_abandoned() {
                    return;
                }
                if let Some(Flow::Jump(segment)) = self.poll_devamp() {
                    from = Some(Position {
                        segment,
                        pass: 0,
                        within: Duration::ZERO,
                    });
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    /// Decodes the segments from `start` on.
    fn play(&mut self, segments: &[Segment], start: Position) -> Flow {
        let rate = f64::from(self.source.info.sample_rate);
        for (index, segment) in segments.iter().enumerate().skip(start.segment) {
            let mut pass = if index == start.segment {
                start.pass
            } else {
                0
            };
            while segment.passes.is_none_or(|passes| pass < passes) {
                let from = if index == start.segment && pass == start.pass {
                    segment.from + start.within
                } else {
                    segment.from
                };
                let limit = segment
                    .to
                    .map(|end| (end.saturating_sub(from).as_secs_f64() * rate).round() as u64);
                let skip = if self.fresh && from.is_zero() {
                    Some(0)
                } else {
                    self.seek(from)
                };
                self.fresh = false;
                let Some(skip) = skip else {
                    tracing::warn!("cannot seek; stopping playback");
                    return Flow::Continue;
                };
                match self.pass(skip, limit) {
                    Flow::Continue => {}
                    other => return other,
                }
                if self.boundaries.len() == BOUNDARIES_KEPT {
                    self.boundaries.pop_front();
                }
                self.boundaries.push_back((self.written, index));
                // A devamp asked for during this pass takes effect here.
                if let Some(flow) = self.poll_devamp() {
                    return flow;
                }
                pass += 1;
            }
        }
        Flow::Continue
    }

    /// Answers a devamp, once the end of the pass the voice is in has been
    /// written.
    fn poll_devamp(&mut self) -> Option<Flow> {
        let request = self.control.request.load(Ordering::Acquire);
        if request == 0 {
            return None;
        }
        let read = request - 1;
        let (at, segment) = self.boundaries.iter().copied().find(|(at, _)| *at > read)?;
        self.control.request.store(0, Ordering::Relaxed);
        if segment + 1 >= self.segments {
            self.control.answer(at, DEVAMP_END);
            return Some(Flow::End);
        }
        // What was written past the end of that pass is dropped by the
        // voice; the stream goes on from there with the next segment.
        let stale = self.written - at;
        self.written = at;
        self.boundaries.retain(|(end, _)| *end <= at);
        self.ended.store(false, Ordering::Release);
        self.control.answer(at, stale);
        Some(Flow::Jump(segment + 1))
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
                while self.producer.slots() < channels {
                    if self.producer.is_abandoned() {
                        return Flow::Abandoned;
                    }
                    // Waiting is when a voice that is behind gets its answer.
                    if let Some(flow) = self.poll_devamp() {
                        return flow;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                let start = frame * source_channels;
                for channel in 0..channels {
                    // A file with fewer channels than announced is padded
                    // with silence.
                    let sample = if channel < source_channels {
                        self.interleaved[start + channel]
                    } else {
                        0.0
                    };
                    let _ = self.producer.push(sample);
                }
                written += 1;
                self.written += 1;
            }
            if limit.is_some_and(|limit| written >= limit) {
                return Flow::Continue;
            }
        }
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

    fn segment(from_ms: u64, to_ms: Option<u64>, passes: Option<u32>) -> Segment {
        Segment {
            from: Duration::from_millis(from_ms),
            to: to_ms.map(Duration::from_millis),
            passes,
        }
    }

    fn from_start(segments: Vec<Segment>) -> Playback {
        Playback {
            segments,
            start: Position {
                segment: 0,
                pass: 0,
                within: Duration::ZERO,
            },
        }
    }

    /// Plays the stream out the way a voice would, through a ring buffer of
    /// `capacity` frames: devamps once `devamp_after` frames have been read
    /// and follows the answer. Returns the first channel as frame indices.
    fn play_out(
        path: &Path,
        playback: Playback,
        capacity: usize,
        devamp_after: Option<u64>,
    ) -> Vec<i32> {
        let source = open(path).unwrap();
        let channels = source.info.channels;
        let (producer, mut consumer) = RingBuffer::new(capacity * channels);
        let ended = Arc::new(AtomicBool::new(false));
        let control = Arc::new(DevampControl::default());
        let reader =
            spawn_reader(source, playback, producer, ended.clone(), control.clone()).unwrap();
        let mut out = Vec::new();
        let mut read = 0_u64;
        let mut skip_at = None;
        let mut stop_at = None;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            assert!(
                std::time::Instant::now() < deadline,
                "the stream never ended"
            );
            if let Some((at, skip)) = control.take_answer() {
                if skip == DEVAMP_END {
                    stop_at = Some(at);
                } else {
                    skip_at = Some((at, skip));
                }
            }
            if stop_at == Some(read) {
                break;
            }
            if let Some((at, skip)) = skip_at
                && read == at
            {
                // Everything to drop was written before the answer.
                for _ in 0..skip * channels as u64 {
                    consumer.pop().unwrap();
                }
                skip_at = None;
            }
            if consumer.slots() >= channels {
                let frame: Vec<f32> = (0..channels).map(|_| consumer.pop().unwrap()).collect();
                out.push((frame[0] * 32768.0).round() as i32);
                read += 1;
                if devamp_after == Some(read) {
                    control.request(read);
                    // A reader that is ahead answers at once; give it time
                    // to, as the audio callback would.
                    let until = std::time::Instant::now() + Duration::from_secs(2);
                    while std::time::Instant::now() < until {
                        if let Some((at, skip)) = control.take_answer() {
                            if skip == DEVAMP_END {
                                stop_at = Some(at);
                            } else {
                                skip_at = Some((at, skip));
                            }
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            } else if ended.load(Ordering::Acquire) && consumer.slots() == 0 {
                break;
            } else {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        drop(consumer);
        reader.join().unwrap();
        out
    }

    fn read(path: &Path, playback: Playback) -> Vec<i32> {
        play_out(path, playback, 1 << 16, None)
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
        let frames = read(&path, from_start(vec![segment(0, None, Some(1))]));
        assert_eq!(frames.len(), 1000);
        assert_eq!(frames[0], 0);
        assert_eq!(frames[999], 999);
    }

    #[test]
    fn trims_start_and_end_and_loops() {
        let path = temp_wav("trim.wav", 8000, 2, 2000);
        let frames = read(&path, from_start(vec![segment(50, Some(100), Some(2))]));
        // 50 ms at 8 kHz is 400 frames; two passes over frames 400..800.
        assert_eq!(frames.len(), 800);
        assert_eq!(frames[0], 400);
        assert_eq!(frames[399], 799);
        assert_eq!(frames[400], 400);
        assert_eq!(frames[799], 799);
    }

    #[test]
    fn the_first_pass_can_start_partway_through() {
        let path = temp_wav("first-pass.wav", 8000, 1, 2000);
        let frames = read(
            &path,
            Playback {
                segments: vec![segment(50, Some(100), Some(2))],
                start: Position {
                    segment: 0,
                    pass: 0,
                    within: Duration::from_millis(25),
                },
            },
        );
        // A short first pass over frames 600..800, then the whole loop.
        assert_eq!(frames.len(), 600);
        assert_eq!((frames[0], frames[199]), (600, 799));
        assert_eq!((frames[200], frames[599]), (400, 799));
    }

    /// Frames 0..800 an intro, 800..1200 a vamp, 1200..2000 an ending.
    fn vamp(vamp_passes: Option<u32>) -> Vec<Segment> {
        vec![
            segment(0, Some(100), Some(1)),
            segment(100, Some(150), vamp_passes),
            segment(150, None, Some(1)),
        ]
    }

    #[test]
    fn slices_play_in_order_each_its_number_of_times() {
        let path = temp_wav("slices.wav", 8000, 1, 2000);
        let frames = read(&path, from_start(vamp(Some(2))));
        assert_eq!(frames.len(), 800 + 2 * 400 + 800);
        assert_eq!((frames[799], frames[800]), (799, 800));
        assert_eq!((frames[1199], frames[1200]), (1199, 800));
        assert_eq!((frames[1599], frames[1600]), (1199, 1200));
        assert_eq!(frames[2399], 1999);

        // Loaded into the second pass of the vamp.
        let frames = read(
            &path,
            Playback {
                segments: vamp(Some(2)),
                start: Position {
                    segment: 1,
                    pass: 1,
                    within: Duration::from_millis(30),
                },
            },
        );
        assert_eq!(frames.len(), 160 + 800);
        assert_eq!((frames[0], frames[160]), (1040, 1200));
    }

    #[test]
    fn devamp_leaves_the_vamp_at_the_end_of_the_pass_without_a_gap() {
        let path = temp_wav("devamp.wav", 8000, 1, 2000);
        // The reader far ahead of the voice, then barely ahead of it.
        for capacity in [1 << 14, 64] {
            // Read 1000 frames: 200 into the vamp's first pass.
            let frames = play_out(&path, from_start(vamp(None)), capacity, Some(1000));
            assert_eq!(frames.len(), 1200 + 800, "capacity {capacity}");
            let expected: Vec<i32> = (0..2000).collect();
            assert_eq!(frames, expected, "capacity {capacity}");
        }
        // Devamped deep into the vamp: it finishes the pass it is in.
        let frames = play_out(
            &path,
            from_start(vamp(None)),
            1 << 14,
            Some(800 + 3 * 400 + 10),
        );
        assert_eq!(frames.len(), 800 + 4 * 400 + 800);
        assert_eq!((frames[2399], frames[2400]), (1199, 1200));
    }

    #[test]
    fn devamp_in_the_last_segment_ends_there() {
        let path = temp_wav("devamp-end.wav", 8000, 1, 2000);
        // A plain loop of 400 frames, devamped in its second pass.
        let frames = play_out(
            &path,
            from_start(vec![segment(0, Some(50), None)]),
            1 << 14,
            Some(500),
        );
        assert_eq!(frames.len(), 800);
        assert_eq!(frames[799], 399);
        // Once everything was written, a devamp still gets its answer.
        let frames = play_out(
            &path,
            from_start(vec![segment(0, Some(50), Some(3))]),
            1 << 14,
            Some(10),
        );
        assert_eq!(frames.len(), 400);
    }
}
