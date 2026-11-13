//! mazzalab-timecode
//!
//! SMPTE timecode, with no I/O:
//!
//! * [`Timecode`] and [`Rate`]: `HH:MM:SS:FF` values, frame counts and
//!   seconds at 24, 25 and 30 fps, 23.976 and 29.97, drop frame included;
//! * [`mtc`]: MIDI Time Code quarter frames and full frames, and a decoder
//!   that follows them;
//! * [`ltc`]: linear timecode as audio samples, and a decoder for it.

pub mod ltc;
pub mod mtc;

use std::fmt;
use std::str::FromStr;

use mazzalab_core::SmpteFormat;
use thiserror::Error;

/// A timecode value. Frames count from 0 up to the rate's frames per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Timecode {
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
    pub frames: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TimecodeError {
    #[error("{0:?} is not a timecode; write it as HH:MM:SS:FF")]
    Format(String),
    #[error("{0:?} is out of range: up to 23:59:59 and the frames of the rate")]
    Range(String),
}

impl Timecode {
    pub const fn new(hours: u8, minutes: u8, seconds: u8, frames: u8) -> Self {
        Self {
            hours,
            minutes,
            seconds,
            frames,
        }
    }

    /// Reads `HH:MM:SS:FF`; `;` or `.` before the frames (as drop-frame
    /// timecode is often written) is accepted too. Fewer fields count from
    /// the right: `5:00` is five seconds.
    pub fn parse(text: &str) -> Result<Self, TimecodeError> {
        let bad = || TimecodeError::Format(text.to_owned());
        let fields: Vec<&str> = text.trim().split([':', ';', '.']).collect();
        if fields.is_empty() || fields.len() > 4 || fields.iter().any(|field| field.is_empty()) {
            return Err(bad());
        }
        let mut values = [0_u8; 4];
        let offset = 4 - fields.len();
        for (index, field) in fields.iter().enumerate() {
            if field.len() > 2 || !field.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(bad());
            }
            values[offset + index] = field.parse().map_err(|_| bad())?;
        }
        let [hours, minutes, seconds, frames] = values;
        if hours > 23 || minutes > 59 || seconds > 59 || frames > 29 {
            return Err(TimecodeError::Range(text.to_owned()));
        }
        Ok(Self::new(hours, minutes, seconds, frames))
    }
}

impl FromStr for Timecode {
    type Err = TimecodeError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl fmt::Display for Timecode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02}:{:02}:{:02}:{:02}",
            self.hours, self.minutes, self.seconds, self.frames
        )
    }
}

/// A timecode frame rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rate {
    /// Frames counted per second of timecode: 24, 25 or 30.
    pub base: u8,
    /// Runs 1000/1001 slower than its base (23.976, 29.97).
    pub pulled_down: bool,
    /// Skips frame numbers to stay with the clock (29.97 drop frame).
    pub drop_frame: bool,
}

/// Frames in 24 hours of timecode, before drop frame.
const DAY_SECONDS: u64 = 24 * 60 * 60;

impl Rate {
    pub const FPS_24: Self = Self::new(24, false, false);
    pub const FPS_25: Self = Self::new(25, false, false);
    pub const FPS_30: Self = Self::new(30, false, false);
    pub const FPS_29_97_DROP: Self = Self::new(30, true, true);

    pub const fn new(base: u8, pulled_down: bool, drop_frame: bool) -> Self {
        Self {
            base,
            pulled_down,
            drop_frame,
        }
    }

    /// Frames per second of real time.
    pub fn fps(&self) -> f64 {
        if self.pulled_down {
            f64::from(self.base) * 1000.0 / 1001.0
        } else {
            f64::from(self.base)
        }
    }

    /// How many frame numbers there are in a day.
    pub fn frames_per_day(&self) -> u64 {
        let nominal = DAY_SECONDS * u64::from(self.base);
        if self.drop_frame {
            // Two numbers dropped every minute but each tenth.
            nominal - 2 * (24 * 60 - 24 * 6)
        } else {
            nominal
        }
    }

    /// The frame number of a timecode, counted from midnight.
    pub fn to_frames(&self, timecode: Timecode) -> u64 {
        let base = u64::from(self.base);
        let seconds = u64::from(timecode.hours) * 3600
            + u64::from(timecode.minutes) * 60
            + u64::from(timecode.seconds);
        let frames = seconds * base + u64::from(timecode.frames.min(self.base - 1));
        if self.drop_frame {
            let minutes = u64::from(timecode.hours) * 60 + u64::from(timecode.minutes);
            frames.saturating_sub(2 * (minutes - minutes / 10))
        } else {
            frames
        }
    }

    /// The timecode of a frame number; past a day it wraps around midnight.
    pub fn from_frames(&self, frames: u64) -> Timecode {
        let base = u64::from(self.base);
        let mut number = frames % self.frames_per_day();
        if self.drop_frame {
            // 17982 frames in ten minutes, 1798 in each minute after the
            // first of them.
            let tens = number / 17_982;
            let rest = number % 17_982;
            number += 18 * tens + if rest > 1 { 2 * ((rest - 2) / 1798) } else { 0 };
        }
        Timecode::new(
            (number / (base * 3600) % 24) as u8,
            (number / (base * 60) % 60) as u8,
            (number / base % 60) as u8,
            (number % base) as u8,
        )
    }

    /// Whether a timecode exists at this rate (drop frame skips some).
    pub fn is_valid(&self, timecode: Timecode) -> bool {
        timecode.frames < self.base
            && !(self.drop_frame
                && timecode.seconds == 0
                && timecode.frames < 2
                && !timecode.minutes.is_multiple_of(10))
    }

    /// The timecode `frames` frames after `timecode`.
    pub fn add(&self, timecode: Timecode, frames: i64) -> Timecode {
        let day = self.frames_per_day() as i64;
        let number = (self.to_frames(timecode) as i64 + frames).rem_euclid(day);
        self.from_frames(number as u64)
    }

    /// Seconds of real time from midnight to a timecode.
    pub fn seconds(&self, timecode: Timecode) -> f64 {
        self.to_frames(timecode) as f64 / self.fps()
    }
}

impl From<SmpteFormat> for Rate {
    fn from(format: SmpteFormat) -> Self {
        match format {
            SmpteFormat::Fps24 => Self::FPS_24,
            SmpteFormat::Fps25 => Self::FPS_25,
            SmpteFormat::Fps30NonDrop => Self::FPS_30,
            SmpteFormat::Fps30Drop => Self::new(30, false, true),
            SmpteFormat::Fps23_976 => Self::new(24, true, false),
            SmpteFormat::Fps29_97NonDrop => Self::new(30, true, false),
            SmpteFormat::Fps29_97Drop => Self::FPS_29_97_DROP,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_and_printing() {
        assert_eq!(
            Timecode::parse("01:02:03:04"),
            Ok(Timecode::new(1, 2, 3, 4))
        );
        assert_eq!(
            Timecode::parse(" 1:00:00;12 "),
            Ok(Timecode::new(1, 0, 0, 12))
        );
        assert_eq!(Timecode::parse("5:10"), Ok(Timecode::new(0, 0, 5, 10)));
        assert_eq!(Timecode::new(10, 0, 5, 9).to_string(), "10:00:05:09");
        assert!(matches!(
            Timecode::parse("1:2:3:4:5"),
            Err(TimecodeError::Format(_))
        ));
        assert!(matches!(
            Timecode::parse("aa:00:00:00"),
            Err(TimecodeError::Format(_))
        ));
        assert!(matches!(
            Timecode::parse("24:00:00:00"),
            Err(TimecodeError::Range(_))
        ));
        assert!(matches!(
            Timecode::parse("00:60:00:00"),
            Err(TimecodeError::Range(_))
        ));
    }

    #[test]
    fn frames_without_drop() {
        let rate = Rate::FPS_25;
        let one_hour = Timecode::new(1, 0, 0, 0);
        assert_eq!(rate.to_frames(one_hour), 90_000);
        assert_eq!(rate.from_frames(90_000 + 24), Timecode::new(1, 0, 0, 24));
        assert_eq!(
            rate.add(Timecode::new(0, 0, 0, 24), 1),
            Timecode::new(0, 0, 1, 0)
        );
        // Before midnight is the day before.
        assert_eq!(
            rate.add(Timecode::default(), -1),
            Timecode::new(23, 59, 59, 24)
        );
        assert_eq!(rate.seconds(one_hour), 3600.0);
    }

    #[test]
    fn drop_frame_skips_two_numbers_a_minute_but_every_tenth() {
        let rate = Rate::FPS_29_97_DROP;
        assert_eq!(rate.from_frames(1799), Timecode::new(0, 0, 59, 29));
        assert_eq!(rate.from_frames(1800), Timecode::new(0, 1, 0, 2));
        assert_eq!(rate.from_frames(17_982), Timecode::new(0, 10, 0, 0));
        assert_eq!(rate.frames_per_day(), 2_589_408);
        for frames in [
            0, 1, 2, 1799, 1800, 1801, 17_981, 17_982, 17_984, 107_892, 2_589_407,
        ] {
            assert_eq!(rate.to_frames(rate.from_frames(frames)), frames, "{frames}");
        }
        assert!(!rate.is_valid(Timecode::new(0, 1, 0, 0)));
        assert!(rate.is_valid(Timecode::new(0, 10, 0, 0)));
        // An hour of 29.97 drop frame is an hour of the clock, to the frame.
        let hour = Timecode::new(1, 0, 0, 0);
        assert!((rate.seconds(hour) - 3600.0).abs() < 1.0 / rate.fps());
    }

    #[test]
    fn rates_of_the_show_formats() {
        assert_eq!(Rate::from(SmpteFormat::Fps25).fps(), 25.0);
        assert!((Rate::from(SmpteFormat::Fps23_976).fps() - 23.976).abs() < 0.001);
        assert!(Rate::from(SmpteFormat::Fps29_97Drop).drop_frame);
    }
}
