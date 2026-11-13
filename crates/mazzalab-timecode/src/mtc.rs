//! MIDI Time Code: eight quarter-frame messages spread over two frames
//! carry a timecode; a full-frame System Exclusive message gives one at
//! once, to locate.

use crate::{Rate, Timecode};

/// The status byte of a quarter-frame message.
pub const QUARTER_FRAME: u8 = 0xF1;

/// The rate code MTC carries: it knows 24, 25, 29.97 drop frame and 30.
/// 23.976 travels as 24 and 29.97 non-drop as 30, as is the custom.
fn rate_code(rate: Rate) -> u8 {
    match (rate.base, rate.drop_frame) {
        (24, _) => 0,
        (25, _) => 1,
        (30, true) => 2,
        _ => 3,
    }
}

fn rate_of(code: u8) -> Rate {
    match code & 0b11 {
        0 => Rate::FPS_24,
        1 => Rate::FPS_25,
        2 => Rate::FPS_29_97_DROP,
        _ => Rate::FPS_30,
    }
}

/// The eight quarter frames that carry `timecode`, in the order they are
/// sent: a quarter of a frame apart, starting on that frame.
pub fn quarter_frames(timecode: Timecode, rate: Rate) -> [[u8; 2]; 8] {
    let pieces = [
        timecode.frames & 0x0F,
        (timecode.frames >> 4) & 0x01,
        timecode.seconds & 0x0F,
        (timecode.seconds >> 4) & 0x03,
        timecode.minutes & 0x0F,
        (timecode.minutes >> 4) & 0x03,
        timecode.hours & 0x0F,
        ((timecode.hours >> 4) & 0x01) | (rate_code(rate) << 1),
    ];
    let mut out = [[0; 2]; 8];
    for (index, piece) in pieces.into_iter().enumerate() {
        out[index] = [QUARTER_FRAME, ((index as u8) << 4) | piece];
    }
    out
}

/// The full-frame message that puts a receiver at `timecode` straight away.
pub fn full_frame(timecode: Timecode, rate: Rate) -> [u8; 10] {
    [
        0xF0,
        0x7F,
        0x7F,
        0x01,
        0x01,
        (rate_code(rate) << 5) | (timecode.hours & 0x1F),
        timecode.minutes,
        timecode.seconds,
        timecode.frames,
        0xF7,
    ]
}

/// Follows incoming MTC.
#[derive(Debug, Clone, Default)]
pub struct Decoder {
    pieces: [u8; 8],
    /// Which pieces have arrived since the last piece 0, in order.
    next: u8,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads one message. Returns the timecode it brings the receiver to:
    /// a full frame at once, quarter frames once all eight have come in
    /// order (every two frames, then; the value is the frame being shown
    /// as the last piece arrives). Anything else, or pieces out of order,
    /// give `None`.
    pub fn feed(&mut self, bytes: &[u8]) -> Option<(Timecode, Rate)> {
        match bytes {
            [QUARTER_FRAME, data] => self.quarter(*data),
            [
                0xF0,
                0x7F,
                _,
                0x01,
                0x01,
                hours,
                minutes,
                seconds,
                frames,
                0xF7,
            ] => {
                self.next = 0;
                let rate = rate_of(hours >> 5);
                Some((
                    Timecode::new(hours & 0x1F, *minutes, *seconds, *frames),
                    rate,
                ))
            }
            _ => None,
        }
    }

    fn quarter(&mut self, data: u8) -> Option<(Timecode, Rate)> {
        let index = data >> 4;
        if index != self.next {
            // Lost a piece, or running backwards: wait for the next frame.
            self.next = 0;
            if index != 0 {
                return None;
            }
        }
        self.pieces[usize::from(index)] = data & 0x0F;
        self.next = index + 1;
        if index != 7 {
            return None;
        }
        self.next = 0;
        let piece = |index: usize| self.pieces[index];
        let rate = rate_of(piece(7) >> 1);
        let timecode = Timecode::new(
            piece(6) | ((piece(7) & 0x01) << 4),
            piece(4) | (piece(5) << 4),
            piece(2) | (piece(3) << 4),
            piece(0) | (piece(1) << 4),
        );
        // The pieces describe the frame they started on, two frames ago.
        Some((rate.add(timecode, 2), rate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_frames_carry_the_timecode_and_the_rate() {
        let frames = quarter_frames(Timecode::new(23, 59, 58, 24), Rate::FPS_25);
        assert_eq!(
            frames.map(|[_, data]| data),
            [0x08, 0x11, 0x2A, 0x33, 0x4B, 0x53, 0x67, 0x73]
        );
        assert!(frames.iter().all(|[status, _]| *status == QUARTER_FRAME));
        assert_eq!(
            full_frame(Timecode::new(1, 2, 3, 4), Rate::FPS_29_97_DROP),
            [0xF0, 0x7F, 0x7F, 0x01, 0x01, 0x41, 2, 3, 4, 0xF7]
        );
    }

    #[test]
    fn the_decoder_follows_what_the_encoder_sends() {
        let rate = Rate::FPS_25;
        let mut decoder = Decoder::new();
        let mut seen = Vec::new();
        let mut timecode = Timecode::new(10, 0, 0, 0);
        // Two frames per round of eight pieces, as a sender plays them out.
        for _ in 0..4 {
            for message in quarter_frames(timecode, rate) {
                if let Some(found) = decoder.feed(&message) {
                    seen.push(found);
                }
            }
            timecode = rate.add(timecode, 2);
        }
        assert_eq!(
            seen,
            [2, 4, 6, 8].map(|frame| (Timecode::new(10, 0, 0, frame), rate))
        );
    }

    #[test]
    fn full_frames_locate_and_lost_pieces_are_waited_out() {
        let mut decoder = Decoder::new();
        assert_eq!(
            decoder.feed(&full_frame(Timecode::new(1, 0, 0, 0), Rate::FPS_30)),
            Some((Timecode::new(1, 0, 0, 0), Rate::FPS_30))
        );
        let pieces = quarter_frames(Timecode::new(1, 0, 0, 0), Rate::FPS_24);
        // Joining halfway: nothing until a whole round has come.
        for message in &pieces[4..] {
            assert_eq!(decoder.feed(message), None);
        }
        // A piece goes missing.
        for (index, message) in pieces.iter().enumerate() {
            if index != 3 {
                assert_eq!(decoder.feed(message), None);
            }
        }
        let mut last = None;
        for message in &pieces {
            last = decoder.feed(message);
        }
        assert_eq!(last, Some((Timecode::new(1, 0, 0, 2), Rate::FPS_24)));
        assert_eq!(decoder.feed(&[0x90, 60, 100]), None);
    }
}
