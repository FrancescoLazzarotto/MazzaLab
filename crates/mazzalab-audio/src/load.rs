//! How hard the audio thread works, measured from inside its callback with
//! atomics only: no locks and no allocation on the audio thread.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

/// A callback that comes more than this many buffers after the previous
/// one most likely left a gap in the sound.
const LATE_FACTOR: f64 = 2.5;

/// What the audio thread reports.
#[derive(Debug)]
pub(crate) struct CallbackLoad {
    epoch: Instant,
    callbacks: AtomicU64,
    late: AtomicU64,
    /// When the previous callback started, in nanoseconds since `epoch`
    /// (0 before the first), and how long its buffer lasts.
    last_start: AtomicU64,
    last_budget: AtomicU64,
    /// Since the last [`take`](Self::take): time spent, time of audio
    /// rendered, and the highest load of one callback (f32 bits).
    busy: AtomicU64,
    rendered: AtomicU64,
    peak: AtomicU32,
}

/// The audio thread's work since the previous reading.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AudioLoad {
    /// Time spent rendering over the length of audio rendered, 0 to 1 (and
    /// above when it cannot keep up).
    pub load: f32,
    /// The highest load of a single callback.
    pub peak: f32,
    /// Callbacks since the device opened.
    pub callbacks: u64,
    /// Callbacks that came too late since the device opened: each most
    /// likely a dropout heard in the room.
    pub late: u64,
}

impl CallbackLoad {
    pub(crate) fn new() -> Self {
        Self {
            epoch: Instant::now(),
            callbacks: AtomicU64::new(0),
            late: AtomicU64::new(0),
            last_start: AtomicU64::new(0),
            last_budget: AtomicU64::new(0),
            busy: AtomicU64::new(0),
            rendered: AtomicU64::new(0),
            peak: AtomicU32::new(0),
        }
    }

    /// Called at the start of a callback; hand the result to [`end`].
    pub(crate) fn start(&self) -> Instant {
        Instant::now()
    }

    /// Called at the end of a callback that rendered `frames` at `rate`.
    pub(crate) fn end(&self, started: Instant, frames: usize, rate: u32) {
        let budget = (frames as f64 * 1e9 / f64::from(rate.max(1))) as u64;
        let busy = started.elapsed().as_nanos() as u64;
        let since = started.saturating_duration_since(self.epoch).as_nanos() as u64 + 1;
        let previous = self.last_start.swap(since, Ordering::Relaxed);
        let previous_budget = self.last_budget.swap(budget, Ordering::Relaxed);
        if previous != 0
            && previous_budget != 0
            && (since - previous.min(since)) as f64 > previous_budget as f64 * LATE_FACTOR
        {
            self.late.fetch_add(1, Ordering::Relaxed);
        }
        self.callbacks.fetch_add(1, Ordering::Relaxed);
        self.busy.fetch_add(busy, Ordering::Relaxed);
        self.rendered.fetch_add(budget, Ordering::Relaxed);
        if budget > 0 {
            let load = busy as f32 / budget as f32;
            // Bit patterns of non-negative floats order like the floats.
            self.peak.fetch_max(load.to_bits(), Ordering::Relaxed);
        }
    }

    /// The load since the previous call; the counters keep counting.
    pub(crate) fn take(&self) -> AudioLoad {
        let busy = self.busy.swap(0, Ordering::Relaxed);
        let rendered = self.rendered.swap(0, Ordering::Relaxed);
        AudioLoad {
            load: if rendered == 0 {
                0.0
            } else {
                busy as f32 / rendered as f32
            },
            peak: f32::from_bits(self.peak.swap(0, Ordering::Relaxed)),
            callbacks: self.callbacks.load(Ordering::Relaxed),
            late: self.late.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn load_is_time_spent_over_audio_rendered_and_gaps_are_late() {
        let load = CallbackLoad::new();
        // 480 frames at 48 kHz: 10 ms of audio.
        for _ in 0..3 {
            let started = load.start();
            std::thread::sleep(Duration::from_millis(2));
            load.end(started, 480, 48_000);
            std::thread::sleep(Duration::from_millis(8));
        }
        let reading = load.take();
        assert_eq!(reading.callbacks, 3);
        assert!(reading.load > 0.1 && reading.load < 1.0, "{reading:?}");
        assert!(reading.peak >= reading.load);
        assert_eq!(reading.late, 0, "{reading:?}");

        // A callback 50 ms after one that lasted 10 ms: too late.
        std::thread::sleep(Duration::from_millis(50));
        let started = load.start();
        load.end(started, 480, 48_000);
        assert_eq!(load.take().late, 1);
        // Read again with nothing rendered: no load.
        assert_eq!(load.take().load, 0.0);
    }
}
