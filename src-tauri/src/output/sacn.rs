use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

use super::{OutputDriver, OutputError};
use crate::engine::DMX_CHANNELS;

pub const SACN_PORT: u16 = 5568;
const ROOT_LEN: usize = 38;
const FRAMING_LEN: usize = 77;
const DMP_LEN: usize = 523;
pub const PACKET_LEN: usize = ROOT_LEN + FRAMING_LEN + DMP_LEN; // 638

const ACN_PACKET_IDENTIFIER: &[u8; 12] = b"ASC-E1.17\0\0\0";
const VECTOR_ROOT_E131_DATA: u32 = 0x00000004;
const VECTOR_E131_DATA_PACKET: u32 = 0x00000002;
const VECTOR_DMP_SET_PROPERTY: u8 = 0x02;
const PDU_FLAGS: u16 = 0x7000;

/// E1.31 (sACN) UDP multicast driver.
///
/// Per spec, multicast group for universe `u` is `239.255.<hi>.<lo>` on port 5568.
pub struct SacnDriver {
    socket: UdpSocket,
    cid: [u8; 16],
    source_name: String,
    priority: u8,
    sequence: u8,
}

impl SacnDriver {
    pub fn new(cid: [u8; 16], source_name: &str, priority: u8) -> Result<Self, OutputError> {
        if !(1..=200).contains(&priority) {
            return Err(OutputError::Config(format!(
                "sACN priority must be 1..=200 (got {priority})"
            )));
        }
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_multicast_loop_v4(true)?;
        Ok(Self {
            socket,
            cid,
            source_name: source_name.to_string(),
            priority,
            sequence: 0,
        })
    }

    fn target(universe: u16) -> SocketAddr {
        let hi = ((universe >> 8) & 0xFF) as u8;
        let lo = (universe & 0xFF) as u8;
        SocketAddr::new(Ipv4Addr::new(239, 255, hi, lo).into(), SACN_PORT)
    }
}

impl OutputDriver for SacnDriver {
    fn name(&self) -> &'static str {
        "sacn"
    }

    fn send(&mut self, universe: u16, data: &[u8; DMX_CHANNELS]) -> Result<(), OutputError> {
        let mut buf = [0u8; PACKET_LEN];
        write_packet(
            &mut buf,
            &self.cid,
            &self.source_name,
            self.priority,
            self.sequence,
            universe,
            data,
        );
        self.sequence = self.sequence.wrapping_add(1);
        self.socket.send_to(&buf, Self::target(universe))?;
        Ok(())
    }
}

/// Build an E1.31 ArtDMX-equivalent packet (DMP set property).
///
/// PDU layout (ANSI E1.31-2018):
/// ```text
/// Root layer (38 bytes):
///   0..2   Preamble Size (BE) = 0x0010
///   2..4   Postamble Size = 0x0000
///   4..16  ACN Packet Identifier "ASC-E1.17\0\0\0"
///   16..18 Flags+Length = 0x7000 | (PDU_LEN & 0x0FFF) where PDU_LEN = total - 16
///   18..22 Vector (BE u32) = 0x00000004
///   22..38 CID
///
/// E1.31 Framing layer (77 bytes):
///   38..40  Flags+Length = 0x7000 | framing_len (= total - 38)
///   40..44  Vector = 0x00000002
///   44..108 Source Name (UTF-8, NUL terminated, 64 bytes)
///   108     Priority
///   109..111 Synchronization Address (BE) = 0
///   111     Sequence
///   112     Options
///   113..115 Universe (BE)
///
/// DMP layer (523 bytes):
///   115..117 Flags+Length = 0x7000 | dmp_len (= total - 115)
///   117      Vector = 0x02
///   118      Address Type & Data Type = 0xa1
///   119..121 First Property Address = 0x0000
///   121..123 Address Increment = 0x0001
///   123..125 Property Value Count = 513 (start code + 512 channels)
///   125      DMX Start Code = 0x00
///   126..638 DMX data (512)
/// ```
#[allow(clippy::too_many_arguments)]
pub fn write_packet(
    buf: &mut [u8; PACKET_LEN],
    cid: &[u8; 16],
    source_name: &str,
    priority: u8,
    sequence: u8,
    universe: u16,
    data: &[u8; DMX_CHANNELS],
) {
    // ---- Root layer ----
    buf[0..2].copy_from_slice(&0x0010u16.to_be_bytes());
    buf[2..4].copy_from_slice(&0x0000u16.to_be_bytes());
    buf[4..16].copy_from_slice(ACN_PACKET_IDENTIFIER);
    let root_pdu_len = (PACKET_LEN - 16) as u16;
    buf[16..18].copy_from_slice(&(PDU_FLAGS | (root_pdu_len & 0x0FFF)).to_be_bytes());
    buf[18..22].copy_from_slice(&VECTOR_ROOT_E131_DATA.to_be_bytes());
    buf[22..38].copy_from_slice(cid);

    // ---- Framing layer ----
    let framing_pdu_len = (PACKET_LEN - 38) as u16;
    buf[38..40].copy_from_slice(&(PDU_FLAGS | (framing_pdu_len & 0x0FFF)).to_be_bytes());
    buf[40..44].copy_from_slice(&VECTOR_E131_DATA_PACKET.to_be_bytes());
    // Source name, truncated and NUL-terminated within 64 bytes.
    let bytes = source_name.as_bytes();
    let max = bytes.len().min(63);
    buf[44..44 + max].copy_from_slice(&bytes[..max]);
    for b in &mut buf[44 + max..108] {
        *b = 0;
    }
    buf[108] = priority;
    buf[109..111].copy_from_slice(&0u16.to_be_bytes()); // sync addr
    buf[111] = sequence;
    buf[112] = 0; // options
    buf[113..115].copy_from_slice(&universe.to_be_bytes());

    // ---- DMP layer ----
    let dmp_pdu_len = (PACKET_LEN - 115) as u16;
    buf[115..117].copy_from_slice(&(PDU_FLAGS | (dmp_pdu_len & 0x0FFF)).to_be_bytes());
    buf[117] = VECTOR_DMP_SET_PROPERTY;
    buf[118] = 0xa1;
    buf[119..121].copy_from_slice(&0u16.to_be_bytes()); // first addr
    buf[121..123].copy_from_slice(&1u16.to_be_bytes()); // increment
    buf[123..125].copy_from_slice(&((DMX_CHANNELS as u16) + 1).to_be_bytes());
    buf[125] = 0; // start code
    buf[126..].copy_from_slice(data);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cid() -> [u8; 16] {
        *b"dmx-control-tst1"
    }

    #[test]
    fn packet_total_length_is_638() {
        assert_eq!(PACKET_LEN, 638);
    }

    #[test]
    fn root_layer_has_acn_id_and_correct_length() {
        let mut buf = [0u8; PACKET_LEN];
        write_packet(&mut buf, &cid(), "src", 100, 0, 1, &[0u8; DMX_CHANNELS]);
        assert_eq!(&buf[0..2], &0x0010u16.to_be_bytes());
        assert_eq!(&buf[4..16], ACN_PACKET_IDENTIFIER);
        let expected = (0x7000u16 | ((PACKET_LEN - 16) as u16 & 0x0FFF)).to_be_bytes();
        assert_eq!(&buf[16..18], &expected);
        assert_eq!(&buf[18..22], &VECTOR_ROOT_E131_DATA.to_be_bytes());
        assert_eq!(&buf[22..38], &cid());
    }

    #[test]
    fn framing_layer_has_priority_universe_and_sequence() {
        let mut buf = [0u8; PACKET_LEN];
        write_packet(
            &mut buf,
            &cid(),
            "Juliere DMX",
            150,
            42,
            0x0107,
            &[0u8; DMX_CHANNELS],
        );
        assert_eq!(&buf[40..44], &VECTOR_E131_DATA_PACKET.to_be_bytes());
        assert_eq!(&buf[44..55], b"Juliere DMX");
        assert_eq!(buf[55], 0); // NUL-terminated
        assert_eq!(buf[108], 150);
        assert_eq!(buf[111], 42);
        assert_eq!(&buf[113..115], &0x0107u16.to_be_bytes());
    }

    #[test]
    fn dmp_layer_has_start_code_and_dmx_data() {
        let mut buf = [0u8; PACKET_LEN];
        let mut data = [0u8; DMX_CHANNELS];
        data[0] = 1;
        data[1] = 128;
        data[511] = 255;
        write_packet(&mut buf, &cid(), "src", 100, 0, 1, &data);
        let expected_dmp_len = ((PACKET_LEN - 115) as u16 & 0x0FFF) | 0x7000;
        assert_eq!(&buf[115..117], &expected_dmp_len.to_be_bytes());
        assert_eq!(buf[117], VECTOR_DMP_SET_PROPERTY);
        assert_eq!(buf[118], 0xa1);
        assert_eq!(&buf[123..125], &(513u16).to_be_bytes());
        assert_eq!(buf[125], 0); // start code
        assert_eq!(buf[126], 1);
        assert_eq!(buf[127], 128);
        assert_eq!(buf[126 + 511], 255);
    }

    #[test]
    fn priority_out_of_range_errors() {
        assert!(SacnDriver::new(cid(), "x", 0).is_err());
        assert!(SacnDriver::new(cid(), "x", 201).is_err());
    }

    #[test]
    fn multicast_target_for_universe_1() {
        let addr = SacnDriver::target(1);
        assert_eq!(addr.ip().to_string(), "239.255.0.1");
        assert_eq!(addr.port(), 5568);
    }

    #[test]
    fn multicast_target_for_high_universe() {
        let addr = SacnDriver::target(0x0107);
        assert_eq!(addr.ip().to_string(), "239.255.1.7");
    }
}
