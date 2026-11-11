//! DMX over the network: Art-Net (ArtDmx) and sACN (ANSI E1.31) packets.

use std::net::Ipv4Addr;

use crate::UNIVERSE_SIZE;

/// UDP port Art-Net nodes listen on.
pub const ARTNET_PORT: u16 = 6454;
/// UDP port sACN receivers listen on.
pub const SACN_PORT: u16 = 5568;

/// An ArtDmx packet carrying one universe. `universe` is the 15-bit
/// Art-Net port address (net, sub-net and universe), which counts from 0.
pub fn artnet_dmx(universe: u16, sequence: u8, data: &[u8; UNIVERSE_SIZE]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(18 + UNIVERSE_SIZE);
    packet.extend_from_slice(b"Art-Net\0");
    packet.extend_from_slice(&0x5000_u16.to_le_bytes()); // OpDmx
    packet.extend_from_slice(&14_u16.to_be_bytes()); // protocol version
    packet.push(sequence);
    packet.push(0); // physical port, informational
    packet.extend_from_slice(&(universe & 0x7FFF).to_le_bytes());
    packet.extend_from_slice(&(UNIVERSE_SIZE as u16).to_be_bytes());
    packet.extend_from_slice(data);
    packet
}

/// The multicast group sACN receivers of a universe listen on.
pub fn sacn_multicast(universe: u16) -> Ipv4Addr {
    let [high, low] = universe.to_be_bytes();
    Ipv4Addr::new(239, 255, high, low)
}

/// "Flags and length" of an E1.31 layer: the top nibble is always 0x7.
fn flags_and_length(length: usize) -> [u8; 2] {
    (0x7000 | (length as u16 & 0x0FFF)).to_be_bytes()
}

/// An E1.31 data packet carrying one universe (which counts from 1).
/// `cid` identifies the sender and must stay the same for its lifetime.
pub fn sacn_data(
    cid: [u8; 16],
    source_name: &str,
    universe: u16,
    sequence: u8,
    priority: u8,
    data: &[u8; UNIVERSE_SIZE],
) -> Vec<u8> {
    const TOTAL: usize = 126 + UNIVERSE_SIZE;
    let mut packet = Vec::with_capacity(TOTAL);
    // Root layer.
    packet.extend_from_slice(&0x0010_u16.to_be_bytes()); // preamble size
    packet.extend_from_slice(&0x0000_u16.to_be_bytes()); // postamble size
    packet.extend_from_slice(b"ASC-E1.17\0\0\0");
    packet.extend_from_slice(&flags_and_length(TOTAL - 16));
    packet.extend_from_slice(&0x0000_0004_u32.to_be_bytes()); // E1.31 data
    packet.extend_from_slice(&cid);
    // Framing layer.
    packet.extend_from_slice(&flags_and_length(TOTAL - 38));
    packet.extend_from_slice(&0x0000_0002_u32.to_be_bytes()); // DMP wrapper
    let mut name = [0_u8; 64];
    // Truncated on a character boundary, leaving the terminating zero.
    let mut end = source_name.len().min(63);
    while !source_name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].copy_from_slice(&source_name.as_bytes()[..end]);
    packet.extend_from_slice(&name);
    packet.push(priority.min(200));
    packet.extend_from_slice(&0_u16.to_be_bytes()); // no synchronization
    packet.push(sequence);
    packet.push(0); // options
    packet.extend_from_slice(&universe.to_be_bytes());
    // DMP layer.
    packet.extend_from_slice(&flags_and_length(TOTAL - 115));
    packet.push(0x02); // set property
    packet.push(0xA1); // address and data type
    packet.extend_from_slice(&0_u16.to_be_bytes()); // first property address
    packet.extend_from_slice(&1_u16.to_be_bytes()); // address increment
    packet.extend_from_slice(&(UNIVERSE_SIZE as u16 + 1).to_be_bytes());
    packet.push(0x00); // DMX start code
    packet.extend_from_slice(data);
    debug_assert_eq!(packet.len(), TOTAL);
    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> [u8; UNIVERSE_SIZE] {
        std::array::from_fn(|index| index as u8)
    }

    #[test]
    fn artnet_packet_layout() {
        let packet = artnet_dmx(0x0102, 7, &ramp());
        assert_eq!(packet.len(), 530);
        assert_eq!(&packet[..8], b"Art-Net\0");
        assert_eq!(packet[8..10], [0x00, 0x50]); // OpDmx, low byte first
        assert_eq!(packet[10..12], [0, 14]);
        assert_eq!(packet[12], 7);
        assert_eq!(packet[14..16], [0x02, 0x01]); // SubUni, then Net
        assert_eq!(packet[16..18], [0x02, 0x00]); // 512, high byte first
        assert_eq!(packet[18..21], [0, 1, 2]);
        assert_eq!(packet[529], 255);
    }

    #[test]
    fn sacn_packet_layout() {
        let cid = [0xAB; 16];
        let packet = sacn_data(cid, "MazzaLab", 258, 9, 100, &ramp());
        assert_eq!(packet.len(), 638);
        assert_eq!(packet[..4], [0x00, 0x10, 0x00, 0x00]);
        assert_eq!(&packet[4..16], b"ASC-E1.17\0\0\0");
        assert_eq!(packet[16..18], [0x72, 0x6E]); // 0x7000 | 622
        assert_eq!(packet[18..22], [0, 0, 0, 4]);
        assert_eq!(packet[22..38], cid);
        assert_eq!(packet[38..40], [0x72, 0x58]); // 0x7000 | 600
        assert_eq!(packet[40..44], [0, 0, 0, 2]);
        assert_eq!(&packet[44..52], b"MazzaLab");
        assert_eq!(packet[52], 0);
        assert_eq!(packet[108], 100); // priority
        assert_eq!(packet[111], 9); // sequence
        assert_eq!(packet[113..115], [0x01, 0x02]); // universe 258
        assert_eq!(packet[115..117], [0x72, 0x0B]); // 0x7000 | 523
        assert_eq!(packet[117..119], [0x02, 0xA1]);
        assert_eq!(packet[123..125], [0x02, 0x01]); // 513 values
        assert_eq!(packet[125], 0); // start code
        assert_eq!(packet[126..129], [0, 1, 2]);
        assert_eq!(packet[637], 255);
    }

    #[test]
    fn sacn_details() {
        assert_eq!(sacn_multicast(1), Ipv4Addr::new(239, 255, 0, 1));
        assert_eq!(sacn_multicast(258), Ipv4Addr::new(239, 255, 1, 2));
        // Long names are cut without splitting a character.
        let long = "è".repeat(40);
        let packet = sacn_data([0; 16], &long, 1, 0, 255, &[0; UNIVERSE_SIZE]);
        assert_eq!(packet[107], 0, "the name keeps its terminating zero");
        assert!(std::str::from_utf8(&packet[44..106]).is_ok());
        assert_eq!(packet[108], 200, "priority is capped");
    }
}
