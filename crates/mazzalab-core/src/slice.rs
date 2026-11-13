//! Slices: an audio cue's file cut into stretches that each play a number
//! of times, or loop until a Devamp moves the cue on.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::cue::AudioCue;
use crate::time::seconds;

/// A point that cuts an audio cue into slices. The slice that ends at the
/// marker plays `play_count` times; 0 loops it until the cue is devamped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SliceMarker {
    /// Time in the file, between the cue's start and end.
    #[serde(with = "seconds")]
    pub time: Duration,
    #[serde(default = "one")]
    pub play_count: u32,
}

fn one() -> u32 {
    1
}

/// One stretch of the file, played over and over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub from: Duration,
    /// `None` runs to the end of the file.
    pub to: Option<Duration>,
    /// `None` loops until devamped.
    pub passes: Option<u32>,
}

impl Segment {
    /// How long one pass lasts in the file, when it can be told.
    pub fn length(&self, media_end: Option<Duration>) -> Option<Duration> {
        self.to
            .or(media_end)
            .map(|to| to.saturating_sub(self.from))
            .filter(|length| !length.is_zero())
    }
}

/// A point of an audio cue's playback: which segment, which pass of it,
/// and how far into that pass, in file time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub segment: usize,
    pub pass: u32,
    pub within: Duration,
}

/// A play count where 0 means until devamped.
fn passes(count: u32) -> Option<u32> {
    (count > 0).then_some(count)
}

impl AudioCue {
    /// The stretches the cue plays, in order. Without slices it is the
    /// whole region between start and end, with the cue's play count; with
    /// slices each one has its own count and the cue's own loop settings
    /// do not apply.
    pub fn segments(&self) -> Vec<Segment> {
        let end = self.end_time.or(self.media_duration);
        let mut markers: Vec<SliceMarker> = self
            .slices
            .iter()
            .copied()
            .filter(|marker| {
                marker.time > self.start_time && end.is_none_or(|end| marker.time < end)
            })
            .collect();
        markers.sort_by_key(|marker| marker.time);
        markers.dedup_by_key(|marker| marker.time);
        if markers.is_empty() {
            return vec![Segment {
                from: self.start_time,
                to: self.end_time,
                passes: if self.infinite_loop {
                    None
                } else {
                    Some(self.play_count.max(1))
                },
            }];
        }
        let mut segments = Vec::with_capacity(markers.len() + 1);
        let mut from = self.start_time;
        for marker in &markers {
            segments.push(Segment {
                from,
                to: Some(marker.time),
                passes: passes(marker.play_count),
            });
            from = marker.time;
        }
        segments.push(Segment {
            from,
            to: self.end_time,
            passes: passes(self.last_slice_play_count),
        });
        segments
    }

    /// Whether a Devamp has anything to do: the cue loops or has slices.
    pub fn devamps(&self) -> bool {
        let segments = self.segments();
        segments.len() > 1 || segments[0].passes != Some(1)
    }

    /// Where playback is once `travelled` of the file has been played (the
    /// action's elapsed time times the rate). `None` past the end.
    pub fn locate(&self, travelled: Duration) -> Option<Position> {
        let segments = self.segments();
        let mut left = travelled;
        let last = segments.len() - 1;
        for (index, segment) in segments.iter().enumerate() {
            let Some(length) = segment.length(self.media_duration) else {
                // Only the last slice can run to an end not known yet.
                return Some(Position {
                    segment: index,
                    pass: 0,
                    within: left,
                });
            };
            let pass = (left.as_secs_f64() / length.as_secs_f64()) as u32;
            if segment.passes.is_none_or(|passes| pass < passes) {
                let within = left.saturating_sub(length.saturating_mul(pass));
                return Some(Position {
                    segment: index,
                    pass,
                    within,
                });
            }
            let total = length.saturating_mul(segment.passes.unwrap_or(1));
            left = left.saturating_sub(total);
            if index == last {
                return None;
            }
        }
        None
    }

    /// How much of the file is left to play after a Devamp `travelled`
    /// into the action: the rest of the pass it is in, then the slices
    /// after it. `None` when it cannot be told: a length not known yet, or
    /// a later slice that loops until devamped again.
    pub fn left_after_devamp(&self, travelled: Duration) -> Option<Duration> {
        let segments = self.segments();
        let position = self.locate(travelled)?;
        let current = segments[position.segment].length(self.media_duration)?;
        let mut left = current.saturating_sub(position.within);
        for segment in &segments[position.segment + 1..] {
            let length = segment.length(self.media_duration)?;
            left += length.saturating_mul(segment.passes?);
        }
        Some(left)
    }

    /// How long the whole cue plays in the file, or `None` when it loops
    /// until devamped or a length is not known yet.
    pub fn file_length(&self) -> Option<Duration> {
        let mut total = Duration::ZERO;
        for segment in self.segments() {
            total += segment
                .length(self.media_duration)?
                .saturating_mul(segment.passes?);
        }
        Some(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(value: f64) -> Duration {
        Duration::from_secs_f64(value)
    }

    fn marker(time: f64, play_count: u32) -> SliceMarker {
        SliceMarker {
            time: secs(time),
            play_count,
        }
    }

    /// Ten seconds of file cut at 2 and 6: an intro, a vamp that loops, an
    /// ending.
    fn vamp() -> AudioCue {
        AudioCue {
            media_duration: Some(secs(10.0)),
            slices: vec![marker(6.0, 0), marker(2.0, 1)],
            ..AudioCue::default()
        }
    }

    #[test]
    fn markers_cut_the_region_into_segments() {
        let cue = vamp();
        let segments = cue.segments();
        assert_eq!(
            segments,
            [
                Segment {
                    from: secs(0.0),
                    to: Some(secs(2.0)),
                    passes: Some(1)
                },
                Segment {
                    from: secs(2.0),
                    to: Some(secs(6.0)),
                    passes: None
                },
                Segment {
                    from: secs(6.0),
                    to: None,
                    passes: Some(1)
                },
            ]
        );
        assert!(cue.devamps());
        assert_eq!(cue.file_length(), None);

        // Markers outside the region are left out; none at all is one
        // segment with the cue's own loop.
        let trimmed = AudioCue {
            start_time: secs(3.0),
            end_time: Some(secs(5.0)),
            play_count: 2,
            ..vamp()
        };
        assert_eq!(trimmed.segments().len(), 1);
        assert_eq!(trimmed.segments()[0].passes, Some(2));
        assert!(!AudioCue::default().devamps());
    }

    #[test]
    fn locating_a_time() {
        let cue = vamp();
        let at = |time| cue.locate(secs(time)).unwrap();
        assert_eq!((at(1.0).segment, at(1.0).within), (0, secs(1.0)));
        // The vamp loops every four seconds.
        let deep = at(2.0 + 4.0 * 3.0 + 1.5);
        assert_eq!((deep.segment, deep.pass, deep.within), (1, 3, secs(1.5)));

        let counted = AudioCue {
            slices: vec![marker(2.0, 2), marker(6.0, 1)],
            last_slice_play_count: 1,
            ..vamp()
        };
        let at = |time| counted.locate(secs(time)).unwrap();
        assert_eq!((at(3.0).segment, at(3.0).pass), (0, 1));
        assert_eq!((at(4.5).segment, at(4.5).within), (1, secs(0.5)));
        assert_eq!((at(9.0).segment, at(9.0).within), (2, secs(1.0)));
        assert_eq!(counted.locate(secs(13.0)), None);
        assert_eq!(counted.file_length(), Some(secs(4.0 + 4.0 + 4.0)));
    }

    #[test]
    fn what_is_left_after_a_devamp() {
        let cue = vamp();
        // In the vamp: the rest of the pass, then the ending.
        assert_eq!(
            cue.left_after_devamp(secs(2.0 + 4.0 + 1.0)),
            Some(secs(3.0 + 4.0))
        );
        // In the intro, the vamp that follows loops until devamped again.
        assert_eq!(cue.left_after_devamp(secs(1.0)), None);
        // Right at the top of a pass, the whole pass.
        assert_eq!(
            cue.left_after_devamp(secs(2.0 + 4.0)),
            Some(secs(4.0 + 4.0))
        );
        let once = AudioCue {
            slices: vec![marker(2.0, 1), marker(6.0, 3)],
            ..vamp()
        };
        assert_eq!(once.left_after_devamp(secs(9.0)), Some(secs(1.0 + 4.0)));
        // In the last slice, the rest of it.
        assert_eq!(once.left_after_devamp(secs(15.0)), Some(secs(3.0)));

        // A plain loop finishes the pass it is in.
        let looped = AudioCue {
            media_duration: Some(secs(4.0)),
            infinite_loop: true,
            ..AudioCue::default()
        };
        assert_eq!(looped.left_after_devamp(secs(9.0)), Some(secs(3.0)));
    }
}
