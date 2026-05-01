use std::net::{SocketAddr, UdpSocket};

use super::{OutputDriver, OutputError};
use crate::engine::DMX_CHANNELS;

pub const ARTNET_PORT: u16 = 6454;
const HEADER_LEN: usize = 18;
pub const PACKET_LEN: usize = HEADER_LEN + DMX_CHANNELS;

pub struct ArtNetDriver {
    socket: UdpSocket,
    target: SocketAddr,
    sequence: u8,
}

impl ArtNetDriver {
    pub fn new(target: SocketAddr) -> Result<Self, OutputError> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_broadcast(true)?;
        Ok(Self {
            socket,
            target,
            sequence: 0,
        })
    }
}

impl OutputDriver for ArtNetDriver {
    fn name(&self) -> &'static str {
        "art-net"
    }

    fn send(&mut self, universe: u16, data: &[u8; DMX_CHANNELS]) -> Result<(), OutputError> {
        let mut buf = [0u8; PACKET_LEN];
        write_packet(&mut buf, universe, self.sequence, data);
        self.sequence = self.sequence.wrapping_add(1);
        if self.sequence == 0 {
            // 0 means "sequence disabled" per spec; skip it on wraparound.
            self.sequence = 1;
        }
        self.socket.send_to(&buf, self.target)?;
        Ok(())
    }
}

/// Build an ArtDMX packet (opcode 0x5000) into `buf`.
///
/// Layout (Art-Net 4 spec):
/// ```text
/// 0..8   "Art-Net\0"
/// 8..10  OpCode (LE) = 0x5000
/// 10..12 ProtVer (BE) = 14
/// 12     Sequence (0 = disabled)
/// 13     Physical
/// 14     SubUni (universe & 0xFF)
/// 15     Net    ((universe >> 8) & 0x7F)
/// 16..18 Length (BE) = 512
/// 18..   Data
/// ```
pub fn write_packet(
    buf: &mut [u8; PACKET_LEN],
    universe: u16,
    sequence: u8,
    data: &[u8; DMX_CHANNELS],
) {
    buf[0..8].copy_from_slice(b"Art-Net\0");
    buf[8..10].copy_from_slice(&0x5000u16.to_le_bytes());
    buf[10] = 0; // ProtVerHi
    buf[11] = 14; // ProtVerLo
    buf[12] = sequence;
    buf[13] = 0; // physical
    buf[14] = (universe & 0xFF) as u8;
    buf[15] = ((universe >> 8) & 0x7F) as u8;
    buf[16..18].copy_from_slice(&(DMX_CHANNELS as u16).to_be_bytes());
    buf[18..].copy_from_slice(data);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_for_universe_zero_all_zero_data() {
        let mut buf = [0u8; PACKET_LEN];
        let data = [0u8; DMX_CHANNELS];
        write_packet(&mut buf, 0, 0, &data);

        // ID
        assert_eq!(&buf[0..8], b"Art-Net\0");
        // OpCode 0x5000 little-endian
        assert_eq!(buf[8], 0x00);
        assert_eq!(buf[9], 0x50);
        // ProtVer = 14
        assert_eq!(buf[10], 0);
        assert_eq!(buf[11], 14);
        // Sequence + Physical
        assert_eq!(buf[12], 0);
        assert_eq!(buf[13], 0);
        // SubUni / Net
        assert_eq!(buf[14], 0);
        assert_eq!(buf[15], 0);
        // Length 512 big-endian
        assert_eq!(buf[16], 0x02);
        assert_eq!(buf[17], 0x00);
        // Data zeroed
        assert!(buf[18..].iter().all(|&b| b == 0));
    }

    #[test]
    fn universe_split_into_subuni_and_net() {
        let mut buf = [0u8; PACKET_LEN];
        let data = [0u8; DMX_CHANNELS];
        // Universe 0x0107 → net=1, subuni=7
        write_packet(&mut buf, 0x0107, 0, &data);
        assert_eq!(buf[14], 0x07);
        assert_eq!(buf[15], 0x01);
    }

    #[test]
    fn sequence_byte_is_written() {
        let mut buf = [0u8; PACKET_LEN];
        let data = [0u8; DMX_CHANNELS];
        write_packet(&mut buf, 0, 42, &data);
        assert_eq!(buf[12], 42);
    }

    #[test]
    fn data_bytes_are_copied_in_order() {
        let mut buf = [0u8; PACKET_LEN];
        let mut data = [0u8; DMX_CHANNELS];
        data[0] = 1;
        data[1] = 128;
        data[511] = 255;
        write_packet(&mut buf, 0, 0, &data);
        assert_eq!(buf[18], 1);
        assert_eq!(buf[19], 128);
        assert_eq!(buf[18 + 511], 255);
    }

    #[test]
    fn packet_total_length_is_530() {
        assert_eq!(PACKET_LEN, 530);
    }
}
