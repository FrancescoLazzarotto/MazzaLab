//! SLIP framing (RFC 1055) for OSC over TCP, using the "double END"
//! variant required by OSC 1.1: every frame starts and ends with `END`.

const END: u8 = 0xC0;
const ESC: u8 = 0xDB;
const ESC_END: u8 = 0xDC;
const ESC_ESC: u8 = 0xDD;

pub fn encode(packet: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(packet.len() + 2);
    out.push(END);
    for &byte in packet {
        match byte {
            END => out.extend_from_slice(&[ESC, ESC_END]),
            ESC => out.extend_from_slice(&[ESC, ESC_ESC]),
            other => out.push(other),
        }
    }
    out.push(END);
    out
}

/// Incremental decoder for a TCP byte stream.
#[derive(Debug, Default)]
pub struct Decoder {
    frame: Vec<u8>,
    escaped: bool,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds received bytes and returns every frame they complete.
    /// Empty frames (back-to-back `END`s) are skipped.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        for &byte in bytes {
            if self.escaped {
                self.escaped = false;
                self.frame.push(match byte {
                    ESC_END => END,
                    ESC_ESC => ESC,
                    // Protocol violation: keep the byte as-is.
                    other => other,
                });
                continue;
            }
            match byte {
                END if !self.frame.is_empty() => frames.push(std::mem::take(&mut self.frame)),
                END => {}
                ESC => self.escaped = true,
                other => self.frame.push(other),
            }
        }
        frames
    }
}
