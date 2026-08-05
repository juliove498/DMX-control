use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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

// ---- Node discovery (ArtPoll / ArtPollReply) ------------------------------
//
// The scanner broadcasts an ArtPoll (opcode 0x2000) and collects the
// ArtPollReply (0x2100) packets every Art-Net node on the LAN is
// required to answer with. Replies are broadcast to port 6454, so the
// scan socket binds that port with SO_REUSEADDR/SO_REUSEPORT — that
// also keeps us compatible with other Art-Net software running on the
// same machine. If the port is genuinely unavailable we fall back to
// an ephemeral port and still catch nodes that unicast their reply.

const OP_POLL: u16 = 0x2000;
const OP_POLL_REPLY: u16 = 0x2100;

/// One discovered Art-Net node, straight out of its ArtPollReply.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct ArtNetNodeInfo {
    /// Node IP as reported in the reply (falls back to the packet's
    /// source address when the field is zeroed).
    pub ip: String,
    pub short_name: String,
    pub long_name: String,
    /// "aa:bb:cc:dd:ee:ff", empty when the node sent a short packet.
    pub mac: String,
    /// Firmware as "hi.lo".
    pub firmware: String,
    /// Style code decoded to a label (Node / Controller / Media / ...).
    pub style: String,
    /// 15-bit port-addresses this node OUTPUTS to DMX (what you patch
    /// a controller at).
    pub output_universes: Vec<u16>,
    /// Port-addresses the node reads as DMX input.
    pub input_universes: Vec<u16>,
    /// Latest node diagnostic string (the "#0001 [ok]"-style report).
    pub node_report: String,
    /// Distinguishes multiple logical nodes behind one IP.
    pub bind_index: u8,
}

/// Build the 14-byte ArtPoll packet. Flags 0: reply once, no diag.
fn build_art_poll() -> [u8; 14] {
    let mut buf = [0u8; 14];
    buf[0..8].copy_from_slice(b"Art-Net\0");
    buf[8..10].copy_from_slice(&OP_POLL.to_le_bytes());
    buf[10] = 0; // ProtVerHi
    buf[11] = 14; // ProtVerLo
    buf[12] = 0; // TalkToMe
    buf[13] = 0; // Priority
    buf
}

/// NUL-terminated fixed-width ASCII field → trimmed String. Returns an
/// empty string when the packet is too short to contain the field.
fn str_field(data: &[u8], start: usize, len: usize) -> String {
    let Some(slice) = data.get(start..start + len) else {
        return String::new();
    };
    let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..end]).trim().to_string()
}

fn style_label(code: u8) -> &'static str {
    match code {
        0x00 => "Node",
        0x01 => "Controller",
        0x02 => "Media",
        0x03 => "Route",
        0x04 => "Backup",
        0x05 => "Config",
        0x06 => "Visual",
        _ => "Unknown",
    }
}

/// Parse an ArtPollReply. Defensive about length — the spec packet is
/// 239 bytes but plenty of hardware sends truncated variants, so every
/// field beyond the opcode degrades to a default instead of bailing.
pub fn parse_art_poll_reply(data: &[u8], from: SocketAddr) -> Option<ArtNetNodeInfo> {
    if data.len() < 14 || &data[0..8] != b"Art-Net\0" {
        return None;
    }
    let opcode = u16::from_le_bytes([data[8], data[9]]);
    if opcode != OP_POLL_REPLY {
        return None;
    }
    let ip = match data.get(10..14) {
        Some(b) if b != [0, 0, 0, 0] => Ipv4Addr::new(b[0], b[1], b[2], b[3]).to_string(),
        _ => match from {
            SocketAddr::V4(v4) => v4.ip().to_string(),
            SocketAddr::V6(v6) => v6.ip().to_string(),
        },
    };
    let firmware = match data.get(16..18) {
        Some(v) => format!("{}.{}", v[0], v[1]),
        None => String::new(),
    };
    let net = data.get(18).copied().unwrap_or(0) & 0x7F;
    let sub = data.get(19).copied().unwrap_or(0) & 0x0F;
    let short_name = str_field(data, 26, 18);
    let long_name = str_field(data, 44, 64);
    let node_report = str_field(data, 108, 64);
    let num_ports = data.get(173).copied().unwrap_or(0).min(4) as usize;
    let mut output_universes = Vec::new();
    let mut input_universes = Vec::new();
    for i in 0..num_ports {
        let port_type = data.get(174 + i).copied().unwrap_or(0);
        let base = ((net as u16) << 8) | ((sub as u16) << 4);
        // Bit 7: the port outputs DMX from Art-Net; bit 6: it inputs.
        if port_type & 0x80 != 0 {
            let sw = data.get(190 + i).copied().unwrap_or(0) & 0x0F;
            output_universes.push(base | sw as u16);
        }
        if port_type & 0x40 != 0 {
            let sw = data.get(186 + i).copied().unwrap_or(0) & 0x0F;
            input_universes.push(base | sw as u16);
        }
    }
    let mac = match data.get(201..207) {
        Some(m) => m
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(":"),
        None => String::new(),
    };
    let style = style_label(data.get(200).copied().unwrap_or(0)).to_string();
    let bind_index = data.get(211).copied().unwrap_or(0);
    Some(ArtNetNodeInfo {
        ip,
        short_name,
        long_name,
        mac,
        firmware,
        style,
        output_universes,
        input_universes,
        node_report,
        bind_index,
    })
}

/// Limited broadcast plus every interface's directed broadcast — Art-Net
/// rigs commonly live on 2.x/10.x networks where 255.255.255.255 only
/// leaves the default interface.
fn broadcast_targets() -> Vec<Ipv4Addr> {
    let mut out = vec![Ipv4Addr::BROADCAST];
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            if let if_addrs::IfAddr::V4(v4) = iface.addr {
                if let Some(b) = v4.broadcast {
                    if !out.contains(&b) {
                        out.push(b);
                    }
                }
            }
        }
    }
    out
}

/// Broadcast ArtPoll and collect replies for `timeout_ms`. Blocking —
/// call from a worker thread (Tauri sync commands already are).
pub fn scan(timeout_ms: u64) -> Result<Vec<ArtNetNodeInfo>, OutputError> {
    use socket2::{Domain, Protocol, Socket, Type};
    let raw = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    raw.set_reuse_address(true)?;
    #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
    raw.set_reuse_port(true)?;
    raw.set_broadcast(true)?;
    let on_artnet_port = raw
        .bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, ARTNET_PORT)).into())
        .is_ok();
    if !on_artnet_port {
        // Port hard-taken (non-reuse listener). Ephemeral still catches
        // nodes that unicast their reply back to the poll's source.
        raw.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)).into())?;
        tracing::warn!("artnet scan: port 6454 unavailable, falling back to ephemeral (broadcast-only nodes may be missed)");
    }
    let socket: UdpSocket = raw.into();
    socket.set_read_timeout(Some(Duration::from_millis(120)))?;

    let poll = build_art_poll();
    for target in broadcast_targets() {
        if let Err(e) = socket.send_to(&poll, (target, ARTNET_PORT)) {
            tracing::debug!(%target, error = %e, "artnet scan: poll send failed");
        }
    }

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut found: Vec<ArtNetNodeInfo> = Vec::new();
    let mut buf = [0u8; 1024];
    while Instant::now() < deadline {
        match socket.recv_from(&mut buf) {
            Ok((n, from)) => {
                if let Some(node) = parse_art_poll_reply(&buf[..n], from) {
                    let dup = found
                        .iter()
                        .any(|f| f.ip == node.ip && f.bind_index == node.bind_index);
                    if !dup {
                        found.push(node);
                    }
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }
    found.sort_by(|a, b| a.ip.cmp(&b.ip).then(a.bind_index.cmp(&b.bind_index)));
    Ok(found)
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

    // ---- discovery parser ------------------------------------------------

    fn from_addr() -> SocketAddr {
        "192.168.1.50:6454".parse().unwrap()
    }

    /// Build a spec-shaped 239-byte ArtPollReply for tests.
    fn sample_reply() -> Vec<u8> {
        let mut p = vec![0u8; 239];
        p[0..8].copy_from_slice(b"Art-Net\0");
        p[8..10].copy_from_slice(&OP_POLL_REPLY.to_le_bytes());
        p[10..14].copy_from_slice(&[2, 0, 0, 10]); // ip 2.0.0.10
        p[16] = 1; // fw hi
        p[17] = 4; // fw lo
        p[18] = 0x01; // net
        p[19] = 0x02; // sub-net
        p[26..26 + 7].copy_from_slice(b"NODE-01");
        p[44..44 + 12].copy_from_slice(b"Test Node 01");
        p[108..108 + 10].copy_from_slice(b"#0001 [ok]");
        p[173] = 2; // two ports
        p[174] = 0x80; // port 0: DMX output
        p[175] = 0xC0; // port 1: output + input
        p[186..190].copy_from_slice(&[0, 5, 0, 0]); // swin
        p[190..194].copy_from_slice(&[3, 4, 0, 0]); // swout
        p[200] = 0x00; // style: node
        p[201..207].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0x01, 0x02, 0x03]);
        p[211] = 1; // bind index
        p
    }

    #[test]
    fn poll_reply_parses_names_ip_mac_and_universes() {
        let node = parse_art_poll_reply(&sample_reply(), from_addr()).expect("parses");
        assert_eq!(node.ip, "2.0.0.10");
        assert_eq!(node.short_name, "NODE-01");
        assert_eq!(node.long_name, "Test Node 01");
        assert_eq!(node.node_report, "#0001 [ok]");
        assert_eq!(node.firmware, "1.4");
        assert_eq!(node.mac, "aa:bb:cc:01:02:03");
        assert_eq!(node.style, "Node");
        assert_eq!(node.bind_index, 1);
        // net=1, sub=2 → base 0x120; swout 3 & 4; swin 5 on port 1.
        assert_eq!(node.output_universes, vec![0x123, 0x124]);
        assert_eq!(node.input_universes, vec![0x125]);
    }

    #[test]
    fn poll_reply_zero_ip_falls_back_to_source() {
        let mut p = sample_reply();
        p[10..14].copy_from_slice(&[0, 0, 0, 0]);
        let node = parse_art_poll_reply(&p, from_addr()).expect("parses");
        assert_eq!(node.ip, "192.168.1.50");
    }

    #[test]
    fn poll_reply_truncated_after_names_still_parses() {
        // Plenty of cheap nodes stop after the long name; the parser
        // must degrade to empty ports/mac rather than reject.
        let p = sample_reply()[..108].to_vec();
        let node = parse_art_poll_reply(&p, from_addr()).expect("parses");
        assert_eq!(node.short_name, "NODE-01");
        assert!(node.output_universes.is_empty());
        assert_eq!(node.mac, "");
    }

    #[test]
    #[ignore = "hits the real network; run manually with -- --ignored"]
    fn manual_scan_smoke() {
        // Proves the socket path (reuse-bind 6454, broadcast send,
        // timed collect) works on this machine. Zero nodes is a valid
        // outcome on a network without Art-Net hardware.
        let nodes = scan(1500).expect("scan should not error");
        eprintln!("artnet scan found {} node(s): {:#?}", nodes.len(), nodes);
    }

    #[test]
    fn non_reply_opcodes_are_ignored() {
        // Our own ArtPoll comes back off the broadcast — must not parse.
        let poll = build_art_poll();
        assert!(parse_art_poll_reply(&poll, from_addr()).is_none());
        let mut junk = sample_reply();
        junk[0] = b'X';
        assert!(parse_art_poll_reply(&junk, from_addr()).is_none());
    }
}
