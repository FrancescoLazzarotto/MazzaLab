//! MIDI Time Code out: quarter frames played out in time to a port.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mazzalab_timecode::{Rate, Timecode, mtc};

use crate::Sender;

/// Sends MTC from a start for as long as it lives; dropping it stops.
#[derive(Debug)]
pub struct MtcGenerator {
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MtcGenerator {
    /// Locates the port to `start` with a full frame, then runs from there:
    /// four quarter frames a frame, at the rate's real speed.
    pub fn start(
        sender: Sender,
        port: String,
        start: Timecode,
        rate: Rate,
    ) -> std::io::Result<Self> {
        Self::start_into(
            move |bytes| sender.send_bytes(&port, bytes).is_ok(),
            start,
            rate,
        )
    }

    /// Like [`start`](Self::start), handing each message to `sink`, which
    /// says whether it could take it.
    pub fn start_into(
        mut sink: impl FnMut(Vec<u8>) -> bool + Send + 'static,
        start: Timecode,
        rate: Rate,
    ) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let (stopping, pausing) = (stop.clone(), paused.clone());
        let thread = std::thread::Builder::new()
            .name("mazzalab-mtc".into())
            .spawn(move || run(&mut sink, start, rate, &stopping, &pausing))?;
        Ok(Self {
            stop,
            paused,
            thread: Some(thread),
        })
    }

    /// Holds the timecode where it is; [`resume`](Self::resume) locates the
    /// port there again and runs on.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }
}

impl Drop for MtcGenerator {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(
    sink: &mut impl FnMut(Vec<u8>) -> bool,
    start: Timecode,
    rate: Rate,
    stop: &AtomicBool,
    paused: &AtomicBool,
) {
    let quarter = Duration::from_secs_f64(1.0 / (rate.fps() * 4.0));
    let mut timecode = start;
    let mut pieces = mtc::quarter_frames(timecode, rate);
    let mut piece = 0;
    sink(mtc::full_frame(timecode, rate).to_vec());
    let mut next = Instant::now();
    let mut was_paused = false;
    while !stop.load(Ordering::Relaxed) {
        if paused.load(Ordering::Relaxed) {
            was_paused = true;
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        if was_paused {
            // Back from a pause: locate again, from the top of a frame pair.
            was_paused = false;
            piece = 0;
            sink(mtc::full_frame(timecode, rate).to_vec());
            next = Instant::now();
        }
        let now = Instant::now();
        if now < next {
            std::thread::sleep((next - now).min(Duration::from_millis(5)));
            continue;
        }
        if !sink(pieces[piece].to_vec()) {
            return;
        }
        piece += 1;
        next += quarter;
        if piece == pieces.len() {
            piece = 0;
            timecode = rate.add(timecode, 2);
            pieces = mtc::quarter_frames(timecode, rate);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn quarter_frames_run_in_time_and_pause() {
        type Log = Arc<Mutex<Vec<(Instant, Vec<u8>)>>>;
        let sent: Log = Arc::default();
        let log = sent.clone();
        let start = Timecode::new(1, 0, 0, 0);
        let rate = Rate::FPS_25;
        let began = Instant::now();
        let generator = MtcGenerator::start_into(
            move |bytes| {
                log.lock().unwrap().push((Instant::now(), bytes));
                true
            },
            start,
            rate,
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(410));
        generator.pause();
        std::thread::sleep(Duration::from_millis(30));
        let paused_at = sent.lock().unwrap().len();
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(
            sent.lock().unwrap().len(),
            paused_at,
            "nothing while paused"
        );
        generator.resume();
        std::thread::sleep(Duration::from_millis(60));
        drop(generator);

        let sent = sent.lock().unwrap();
        // A full frame to locate, then a hundred quarter frames a second.
        assert_eq!(sent[0].1, mtc::full_frame(start, rate));
        let quarters = sent[1..paused_at].len();
        assert!(
            (38..=46).contains(&quarters),
            "{quarters} quarter frames in 0.41 s"
        );
        // Read back, they count frames from the start.
        let mut decoder = mtc::Decoder::new();
        let read: Vec<Timecode> = sent[..paused_at]
            .iter()
            .filter_map(|(_, bytes)| decoder.feed(bytes).map(|(timecode, _)| timecode))
            .collect();
        assert_eq!(read[0], start);
        assert_eq!(read[1], Timecode::new(1, 0, 0, 2));
        // Back from the pause it locates again where it stopped.
        assert_eq!(sent[paused_at].1[0], 0xF0);
        assert!(sent[paused_at].0 >= began + Duration::from_millis(590));
    }
}
