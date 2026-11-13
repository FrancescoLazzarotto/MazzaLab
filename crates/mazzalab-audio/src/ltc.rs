//! LTC as a source for the mixer: a thread encodes timecode into a ring
//! buffer the way the file reader decodes audio into one.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use mazzalab_core::TimecodeCue;
use mazzalab_timecode::{Rate, Timecode, ltc::Encoder};
use rtrb::{Consumer, Producer, RingBuffer};

/// How much signal is encoded ahead of the mixer.
const AHEAD_SECONDS: f64 = 0.25;
/// Samples encoded at a time.
const BLOCK: usize = 256;

/// Starts encoding a Timecode cue's LTC from `into` its action, at the
/// device's rate. `None` when its start is not a timecode.
pub(crate) fn start(
    cue: &TimecodeCue,
    into: Duration,
    sample_rate: u32,
) -> Option<(Consumer<f32>, Arc<AtomicBool>)> {
    let rate = Rate::from(cue.format);
    let start = Timecode::parse(&cue.start)
        .map_err(|error| tracing::warn!(%error, "LTC not sent"))
        .ok()?;
    // Started partway through, the timecode is that much further on.
    let start = rate.add(start, (into.as_secs_f64() * rate.fps()).floor() as i64);
    let mut encoder = Encoder::new(start, rate, sample_rate, 1.0);
    let mut left = cue.duration.map(|duration| {
        (duration.saturating_sub(into).as_secs_f64() * f64::from(sample_rate)) as u64
    });

    let capacity = ((AHEAD_SECONDS * f64::from(sample_rate)) as usize).max(BLOCK * 4);
    let (mut producer, consumer) = RingBuffer::new(capacity);
    let ended = Arc::new(AtomicBool::new(false));
    // The first stretch is there before the voice reaches the mix.
    let mut block = vec![0.0_f32; BLOCK];
    while producer.slots() >= BLOCK && left.is_none_or(|left| left > 0) {
        write(&mut encoder, &mut producer, &mut block, &mut left);
    }
    let done = ended.clone();
    let spawned = std::thread::Builder::new()
        .name("mazzalab-ltc".into())
        .spawn(move || {
            loop {
                if producer.is_abandoned() {
                    return;
                }
                if left == Some(0) {
                    done.store(true, Ordering::Release);
                    return;
                }
                if producer.slots() >= BLOCK {
                    write(&mut encoder, &mut producer, &mut block, &mut left);
                } else {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "cannot start the LTC encoder");
        return None;
    }
    Some((consumer, ended))
}

/// Encodes one block, or what is left of the cue's duration.
fn write(
    encoder: &mut Encoder,
    producer: &mut Producer<f32>,
    block: &mut [f32],
    left: &mut Option<u64>,
) {
    let length = left.map_or(block.len(), |left| left.min(block.len() as u64) as usize);
    encoder.fill(&mut block[..length]);
    for sample in &block[..length] {
        let _ = producer.push(*sample);
    }
    if let Some(left) = left {
        *left -= length as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazzalab_core::{SmpteFormat, TimecodeSignal};
    use mazzalab_timecode::ltc::Decoder;

    #[test]
    fn a_timecode_cue_encodes_from_its_start_for_its_duration() {
        let cue = TimecodeCue {
            signal: TimecodeSignal::Ltc,
            format: SmpteFormat::Fps25,
            start: "10:00:00:00".into(),
            duration: Some(Duration::from_secs(1)),
            ..TimecodeCue::default()
        };
        let (mut consumer, ended) = start(&cue, Duration::ZERO, 48_000).unwrap();
        let mut samples = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !(ended.load(Ordering::Acquire) && consumer.is_empty()) {
            assert!(std::time::Instant::now() < deadline);
            match consumer.pop() {
                Ok(sample) => samples.push(sample),
                Err(_) => std::thread::sleep(Duration::from_millis(1)),
            }
        }
        assert_eq!(samples.len(), 48_000);
        let frames = Decoder::new(48_000).feed(&samples);
        // The last bit of the last frame is only told apart by a transition
        // that never comes: 24 of the 25 frames read back.
        assert_eq!(frames.len(), 24);
        assert_eq!(frames[0].timecode, Timecode::new(10, 0, 0, 0));
        assert_eq!(frames[23].timecode, Timecode::new(10, 0, 0, 23));

        // Loaded 0.4 s in: ten frames on.
        let (mut consumer, _) = start(&cue, Duration::from_millis(400), 48_000).unwrap();
        let mut samples = vec![0.0; 4000];
        for sample in &mut samples {
            *sample = consumer.pop().unwrap();
        }
        let frames = Decoder::new(48_000).feed(&samples);
        assert_eq!(frames[0].timecode, Timecode::new(10, 0, 0, 10));
        assert!(
            start(
                &TimecodeCue {
                    start: "later".into(),
                    ..cue
                },
                Duration::ZERO,
                48_000
            )
            .is_none()
        );
    }
}
