//! RDM (ANSI E1.20) discovery over the FTDI D2xx / libusb output path.
//!
//! What this does: pauses the DMX output thread, takes over the FTDI
//! dongle, walks the DISC_UNIQUE_BRANCH binary search to enumerate
//! every RDM responder on the line, mutes each one as it's found, then
//! GETs a few descriptive PIDs (device info, model, manufacturer,
//! label, software version) — and hands the port back.
//!
//! Hardware honesty: RDM needs the interface to RECEIVE on the DMX
//! line during the response window. The FT232 chip always can; whether
//! the *dongle* can depends on its RS-485 transceiver wiring. Plenty of
//! "Open DMX" clones are transmit-only — on those, discovery runs
//! cleanly and simply finds nothing. The UI copy says as much.
//!
//! Protocol notes (E1.20):
//! - Request frames: BREAK + MAB, start code 0xCC, sub start 0x01,
//!   message length = 24 + PDL, 16-bit additive checksum.
//! - DISC_UNIQUE_BRANCH responses come back **without** a break as
//!   0–7 × 0xFE preamble, 0xAA separator, then the 6-byte UID and a
//!   16-bit checksum, each byte doubled as (b | 0xAA, b | 0x55).
//! - The FT232 prepends 2 modem-status bytes to every 64-byte bulk IN
//!   packet; `read_raw` strips them.

use std::time::{Duration, Instant};

use rusb::GlobalContext;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::output::d2xx::{
    find_out_endpoint, ftdi_control, modem_value, spin_for, BAUD_DIVISOR_250K, FTDI_REQ_RESET,
    FTDI_REQ_SET_BAUD_RATE, FTDI_REQ_SET_FLOW_CTRL, FTDI_REQ_SET_LATENCY, FTDI_REQ_SET_LINE_PROP,
    FTDI_REQ_SET_MODEM_CTRL, FTDI_RESET_PURGE_RX, FTDI_RESET_PURGE_TX, FTDI_RESET_SIO, FTDI_VID,
    LINE_8N2, LINE_8N2_BREAK,
};

// Command classes.
pub const CC_DISCOVERY: u8 = 0x10;
pub const CC_DISCOVERY_RESPONSE: u8 = 0x11;
pub const CC_GET: u8 = 0x20;
pub const CC_GET_RESPONSE: u8 = 0x21;

// PIDs we use.
pub const PID_DISC_UNIQUE_BRANCH: u16 = 0x0001;
pub const PID_DISC_MUTE: u16 = 0x0002;
pub const PID_DISC_UN_MUTE: u16 = 0x0003;
pub const PID_DEVICE_INFO: u16 = 0x0060;
pub const PID_DEVICE_MODEL_DESCRIPTION: u16 = 0x0080;
pub const PID_MANUFACTURER_LABEL: u16 = 0x0081;
pub const PID_DEVICE_LABEL: u16 = 0x0082;
pub const PID_SOFTWARE_VERSION_LABEL: u16 = 0x00C0;

pub const RESPONSE_TYPE_ACK: u8 = 0x00;

/// Our controller UID: 0x7FF0 is inside ESTA's prototyping range.
pub const CONTROLLER_UID: Uid = Uid {
    manufacturer: 0x7FF0,
    device: 0x0000_0001,
};

pub const BROADCAST_ALL: Uid = Uid {
    manufacturer: 0xFFFF,
    device: 0xFFFF_FFFF,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Uid {
    pub manufacturer: u16,
    pub device: u32,
}

impl Uid {
    pub fn as_u64(&self) -> u64 {
        ((self.manufacturer as u64) << 32) | self.device as u64
    }

    pub fn from_u64(v: u64) -> Self {
        Self {
            manufacturer: ((v >> 32) & 0xFFFF) as u16,
            device: (v & 0xFFFF_FFFF) as u32,
        }
    }

    pub fn to_bytes(self) -> [u8; 6] {
        let m = self.manufacturer.to_be_bytes();
        let d = self.device.to_be_bytes();
        [m[0], m[1], d[0], d[1], d[2], d[3]]
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < 6 {
            return None;
        }
        Some(Self {
            manufacturer: u16::from_be_bytes([b[0], b[1]]),
            device: u32::from_be_bytes([b[2], b[3], b[4], b[5]]),
        })
    }
}

impl std::fmt::Display for Uid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04X}:{:08X}", self.manufacturer, self.device)
    }
}

/// One discovered responder, decorated with the descriptive PIDs we
/// managed to GET (all best-effort — a device that ignores
/// DEVICE_LABEL still shows up with its UID).
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../bindings/")]
pub struct RdmDeviceInfo {
    /// "MMMM:DDDDDDDD" hex UID.
    pub uid: String,
    pub model: String,
    pub manufacturer: String,
    pub label: String,
    pub software_version: String,
    /// 1-based DMX start address (0 = unknown / footprint 0).
    pub dmx_start_address: u16,
    /// Channels consumed by the current personality.
    pub footprint: u16,
    pub personality: String,
}

// ---- Frame building / parsing (pure, unit-tested) --------------------------

/// Build a complete RDM request frame (start code through checksum).
pub fn build_request(
    dest: Uid,
    src: Uid,
    tn: u8,
    port_id: u8,
    cc: u8,
    pid: u16,
    pd: &[u8],
) -> Vec<u8> {
    let ml = 24 + pd.len();
    let mut f = Vec::with_capacity(ml + 2);
    f.push(0xCC); // START code
    f.push(0x01); // sub START
    f.push(ml as u8);
    f.extend_from_slice(&dest.to_bytes());
    f.extend_from_slice(&src.to_bytes());
    f.push(tn);
    f.push(port_id);
    f.push(0); // message count
    f.extend_from_slice(&0u16.to_be_bytes()); // sub-device: root
    f.push(cc);
    f.extend_from_slice(&pid.to_be_bytes());
    f.push(pd.len() as u8);
    f.extend_from_slice(pd);
    let sum: u16 = f.iter().map(|&b| b as u16).fold(0, u16::wrapping_add);
    f.extend_from_slice(&sum.to_be_bytes());
    f
}

pub fn build_disc_unique_branch(lower: Uid, upper: Uid, src: Uid, tn: u8) -> Vec<u8> {
    let mut pd = Vec::with_capacity(12);
    pd.extend_from_slice(&lower.to_bytes());
    pd.extend_from_slice(&upper.to_bytes());
    build_request(
        BROADCAST_ALL,
        src,
        tn,
        1,
        CC_DISCOVERY,
        PID_DISC_UNIQUE_BRANCH,
        &pd,
    )
}

/// Outcome of decoding a DISC_UNIQUE_BRANCH response window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscOutcome {
    /// Nothing answered in this branch.
    Silence,
    /// Exactly one responder — its UID decoded and checksum-verified.
    Single(Uid),
    /// Garbled data: two or more responders talked over each other
    /// (or line noise). Split the branch and recurse.
    Collision,
}

/// Decode a raw DUB response buffer (preamble + EUID + checksum).
pub fn decode_disc_response(buf: &[u8]) -> DiscOutcome {
    if buf.is_empty() {
        return DiscOutcome::Silence;
    }
    // Skip up to 7 preamble bytes (0xFE) then require the 0xAA separator.
    let mut i = 0;
    while i < buf.len() && buf[i] == 0xFE && i < 8 {
        i += 1;
    }
    if i >= buf.len() || buf[i] != 0xAA {
        return DiscOutcome::Collision;
    }
    i += 1;
    let rest = &buf[i..];
    if rest.len() < 16 {
        return DiscOutcome::Collision;
    }
    // Each real byte arrives twice: (b | 0xAA) then (b | 0x55). AND-ing
    // the pair reconstructs it; any bit disagreement = collision.
    let mut decoded = [0u8; 8];
    for (k, d) in decoded.iter_mut().enumerate() {
        let hi = rest[k * 2];
        let lo = rest[k * 2 + 1];
        if hi & 0xAA != 0xAA || lo & 0x55 != 0x55 {
            return DiscOutcome::Collision;
        }
        *d = hi & lo;
    }
    let uid = Uid::from_bytes(&decoded[0..6]).expect("6 bytes");
    let cs_expect = u16::from_be_bytes([decoded[6], decoded[7]]);
    let cs_actual: u16 = rest[..12]
        .iter()
        .map(|&b| b as u16)
        .fold(0, u16::wrapping_add);
    if cs_expect != cs_actual {
        return DiscOutcome::Collision;
    }
    DiscOutcome::Single(uid)
}

#[derive(Debug, Clone)]
pub struct RdmResponse {
    pub response_type: u8,
    pub cc: u8,
    pub pid: u16,
    pub pd: Vec<u8>,
}

/// Locate and validate a normal (break-framed) RDM response inside a
/// raw read buffer. Scans for the 0xCC 0x01 signature so leading noise
/// or a stripped break don't matter.
pub fn parse_response(buf: &[u8]) -> Option<RdmResponse> {
    for start in 0..buf.len().saturating_sub(25) {
        if buf[start] != 0xCC || buf[start + 1] != 0x01 {
            continue;
        }
        let ml = buf[start + 2] as usize;
        if ml < 24 || start + ml + 2 > buf.len() {
            continue;
        }
        let frame = &buf[start..start + ml + 2];
        let sum: u16 = frame[..ml]
            .iter()
            .map(|&b| b as u16)
            .fold(0, u16::wrapping_add);
        let expect = u16::from_be_bytes([frame[ml], frame[ml + 1]]);
        if sum != expect {
            continue;
        }
        let pdl = frame[23] as usize;
        if 24 + pdl != ml {
            continue;
        }
        return Some(RdmResponse {
            response_type: frame[16],
            cc: frame[20],
            pid: u16::from_be_bytes([frame[21], frame[22]]),
            pd: frame[24..24 + pdl].to_vec(),
        });
    }
    None
}

fn ascii_field(pd: &[u8]) -> String {
    let end = pd.iter().position(|&b| b == 0).unwrap_or(pd.len());
    String::from_utf8_lossy(&pd[..end]).trim().to_string()
}

// ---- FTDI transport ---------------------------------------------------------

const USB_TIMEOUT: Duration = Duration::from_millis(60);
const BREAK_DURATION: Duration = Duration::from_micros(176);
const MAB_DURATION: Duration = Duration::from_micros(16);

struct FtdiRdmPort {
    handle: rusb::DeviceHandle<GlobalContext>,
    out_ep: u8,
    in_ep: u8,
    iface: u8,
    tn: u8,
}

impl FtdiRdmPort {
    fn open(serial: &str) -> Result<Self, String> {
        let devices = rusb::devices().map_err(|e| format!("rusb::devices: {e}"))?;
        for dev in devices.iter() {
            let Ok(desc) = dev.device_descriptor() else {
                continue;
            };
            if desc.vendor_id() != FTDI_VID {
                continue;
            }
            let Ok(handle) = dev.open() else { continue };
            let dev_serial = handle
                .read_serial_number_string_ascii(&desc)
                .unwrap_or_default();
            if dev_serial != serial {
                continue;
            }
            let _ = handle.set_auto_detach_kernel_driver(true);
            let iface: u8 = 0;
            if handle.kernel_driver_active(iface).unwrap_or(false) {
                handle
                    .detach_kernel_driver(iface)
                    .map_err(|e| format!("detach_kernel_driver: {e}"))?;
            }
            handle
                .claim_interface(iface)
                .map_err(|e| format!("claim_interface: {e}"))?;
            let out_ep = find_out_endpoint(&dev, iface).unwrap_or(0x02);
            // Bulk IN is the OUT address with the direction bit set on
            // every FT232-family chip (0x81 for 0x02).
            let in_ep = 0x81;

            ftdi_control(&handle, FTDI_REQ_RESET, FTDI_RESET_SIO, 0)?;
            ftdi_control(&handle, FTDI_REQ_SET_LATENCY, 1, 0)?;
            ftdi_control(&handle, FTDI_REQ_SET_FLOW_CTRL, 0, 0)?;
            ftdi_control(&handle, FTDI_REQ_SET_BAUD_RATE, BAUD_DIVISOR_250K, 0)?;
            ftdi_control(&handle, FTDI_REQ_SET_LINE_PROP, LINE_8N2, 0)?;
            ftdi_control(
                &handle,
                FTDI_REQ_SET_MODEM_CTRL,
                modem_value(false, false),
                0,
            )?;
            ftdi_control(&handle, FTDI_REQ_RESET, FTDI_RESET_PURGE_RX, 0)?;
            ftdi_control(&handle, FTDI_REQ_RESET, FTDI_RESET_PURGE_TX, 0)?;
            return Ok(Self {
                handle,
                out_ep,
                in_ep,
                iface,
                tn: 0,
            });
        }
        Err(format!("FTDI serial {serial} not found"))
    }

    fn next_tn(&mut self) -> u8 {
        self.tn = self.tn.wrapping_add(1);
        self.tn
    }

    /// BREAK + MAB + frame, then wait `response_window` and collect
    /// whatever arrives on the line.
    fn transact(&mut self, frame: &[u8], response_window: Duration) -> Result<Vec<u8>, String> {
        ftdi_control(&self.handle, FTDI_REQ_RESET, FTDI_RESET_PURGE_RX, 0)?;
        ftdi_control(&self.handle, FTDI_REQ_SET_LINE_PROP, LINE_8N2_BREAK, 0)?;
        spin_for(BREAK_DURATION);
        ftdi_control(&self.handle, FTDI_REQ_SET_LINE_PROP, LINE_8N2, 0)?;
        spin_for(MAB_DURATION);
        self.handle
            .write_bulk(self.out_ep, frame, USB_TIMEOUT)
            .map_err(|e| format!("write_bulk: {e}"))?;
        // Wait for the line turnaround + response. The FT232 loops our
        // own transmit back on half-duplex wiring, so the reader below
        // may see the request too — parse_response / decode handle that
        // by scanning (our request bytes fail their checksums-at-offset
        // or get skipped by the signature scan).
        let deadline = Instant::now() + response_window;
        let mut collected = Vec::new();
        let mut chunk = [0u8; 64];
        while Instant::now() < deadline {
            match self.handle.read_bulk(self.in_ep, &mut chunk, USB_TIMEOUT) {
                // FT232 prefixes every packet with 2 modem-status bytes.
                Ok(n) if n > 2 => collected.extend_from_slice(&chunk[2..n]),
                Ok(_) => {}
                Err(rusb::Error::Timeout) => {}
                Err(e) => return Err(format!("read_bulk: {e}")),
            }
        }
        Ok(collected)
    }

    /// Send a request and parse a normal RDM response addressed to us.
    fn request(
        &mut self,
        dest: Uid,
        cc: u8,
        pid: u16,
        pd: &[u8],
        window: Duration,
    ) -> Result<Option<RdmResponse>, String> {
        let tn = self.next_tn();
        let frame = build_request(dest, CONTROLLER_UID, tn, 1, cc, pid, pd);
        let raw = self.transact(&frame, window)?;
        // Skip our own loopback: scan for a response whose CC is a
        // *_RESPONSE class and whose PID matches.
        let mut cursor = &raw[..];
        while let Some(resp) = parse_response(cursor) {
            if resp.cc == cc | 0x01 && resp.pid == pid {
                return Ok(Some(resp));
            }
            // Advance past the first CC signature we matched on.
            let pos = cursor
                .windows(2)
                .position(|w| w == [0xCC, 0x01])
                .unwrap_or(0);
            if pos + 2 >= cursor.len() {
                break;
            }
            cursor = &cursor[pos + 2..];
        }
        Ok(None)
    }
}

impl Drop for FtdiRdmPort {
    fn drop(&mut self) {
        let _ = ftdi_control(&self.handle, FTDI_REQ_SET_LINE_PROP, LINE_8N2, 0);
        let _ = self.handle.release_interface(self.iface);
    }
}

// ---- Discovery --------------------------------------------------------------

/// Full E1.20 discovery on the given FTDI serial. The caller is
/// responsible for pausing the DMX output thread first — two owners of
/// one FTDI handle cannot coexist.
pub fn discover(serial: &str) -> Result<Vec<RdmDeviceInfo>, String> {
    let mut port = FtdiRdmPort::open(serial)?;
    let dub_window = Duration::from_millis(25);
    let resp_window = Duration::from_millis(40);

    // Un-mute everything so a previous (aborted) discovery doesn't hide
    // devices. Broadcast — no response expected.
    let tn = port.next_tn();
    let unmute = build_request(
        BROADCAST_ALL,
        CONTROLLER_UID,
        tn,
        1,
        CC_DISCOVERY,
        PID_DISC_UN_MUTE,
        &[],
    );
    let _ = port.transact(&unmute, Duration::from_millis(5));

    let mut found: Vec<Uid> = Vec::new();
    let mut stack: Vec<(u64, u64)> = vec![(0, BROADCAST_ALL.as_u64() - 1)];
    // Bounded: each iteration either finds a device, kills a branch, or
    // splits one. 512 handles hundreds of fixtures with headroom.
    let mut budget = 512usize;
    while let Some((lo, hi)) = stack.pop() {
        if budget == 0 {
            tracing::warn!("rdm discovery budget exhausted — returning partial results");
            break;
        }
        budget -= 1;
        let tn = port.next_tn();
        let dub =
            build_disc_unique_branch(Uid::from_u64(lo), Uid::from_u64(hi), CONTROLLER_UID, tn);
        let raw = port.transact(&dub, dub_window)?;
        match decode_disc_response(&raw) {
            DiscOutcome::Silence => {}
            DiscOutcome::Single(uid) => {
                // Verify + silence it, then re-scan the same branch for
                // anyone it was masking.
                let ok = port
                    .request(uid, CC_DISCOVERY, PID_DISC_MUTE, &[], resp_window)?
                    .map(|r| r.response_type == RESPONSE_TYPE_ACK)
                    .unwrap_or(false);
                if ok && !found.contains(&uid) {
                    tracing::info!(uid = %uid, "rdm: responder found");
                    found.push(uid);
                    stack.push((lo, hi));
                }
                // A DUB hit that won't ACK a mute is likely noise —
                // drop the branch rather than loop on it.
            }
            DiscOutcome::Collision => {
                if lo < hi {
                    let mid = lo + (hi - lo) / 2;
                    stack.push((lo, mid));
                    stack.push((mid + 1, hi));
                }
            }
        }
    }

    // Decorate every responder with the descriptive PIDs, best-effort.
    let mut out = Vec::with_capacity(found.len());
    for uid in found {
        let mut info = RdmDeviceInfo {
            uid: uid.to_string(),
            ..Default::default()
        };
        if let Ok(Some(r)) = port.request(uid, CC_GET, PID_DEVICE_INFO, &[], resp_window) {
            if r.response_type == RESPONSE_TYPE_ACK && r.pd.len() >= 19 {
                info.footprint = u16::from_be_bytes([r.pd[8], r.pd[9]]);
                info.personality = format!("{}/{}", r.pd[10], r.pd[11]);
                info.dmx_start_address = u16::from_be_bytes([r.pd[12], r.pd[13]]);
            }
        }
        for (pid, slot) in [
            (PID_DEVICE_MODEL_DESCRIPTION, 0usize),
            (PID_MANUFACTURER_LABEL, 1),
            (PID_DEVICE_LABEL, 2),
            (PID_SOFTWARE_VERSION_LABEL, 3),
        ] {
            if let Ok(Some(r)) = port.request(uid, CC_GET, pid, &[], resp_window) {
                if r.response_type == RESPONSE_TYPE_ACK {
                    let text = ascii_field(&r.pd);
                    match slot {
                        0 => info.model = text,
                        1 => info.manufacturer = text,
                        2 => info.label = text,
                        _ => info.software_version = text,
                    }
                }
            }
        }
        out.push(info);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_frame_layout_and_checksum() {
        let f = build_request(
            BROADCAST_ALL,
            CONTROLLER_UID,
            7,
            1,
            CC_GET,
            PID_DEVICE_INFO,
            &[],
        );
        assert_eq!(f.len(), 26); // 24 header/pd + 2 checksum
        assert_eq!(f[0], 0xCC);
        assert_eq!(f[1], 0x01);
        assert_eq!(f[2], 24); // message length without checksum
        assert_eq!(&f[3..9], &[0xFF; 6]); // broadcast dest
        assert_eq!(f[15], 7); // tn
        assert_eq!(f[20], CC_GET);
        assert_eq!(u16::from_be_bytes([f[21], f[22]]), PID_DEVICE_INFO);
        assert_eq!(f[23], 0); // pdl
        let sum: u16 = f[..24].iter().map(|&b| b as u16).fold(0, u16::wrapping_add);
        assert_eq!(u16::from_be_bytes([f[24], f[25]]), sum);
    }

    #[test]
    fn dub_request_carries_bounds() {
        let lo = Uid::from_u64(0);
        let hi = Uid::from_u64(0x0000_7FFF_FFFF_FFFF & 0xFFFF_FFFF_FFFF);
        let f = build_disc_unique_branch(lo, hi, CONTROLLER_UID, 1);
        assert_eq!(f[23], 12); // pdl = two uids
        assert_eq!(&f[24..30], &lo.to_bytes());
        assert_eq!(&f[30..36], &hi.to_bytes());
    }

    /// Encode a DUB response the way a responder would (E1.20 §7.5).
    fn encode_dub_response(uid: Uid, preamble: usize) -> Vec<u8> {
        let mut out = vec![0xFE; preamble];
        out.push(0xAA);
        let b = uid.to_bytes();
        let mut euid = Vec::with_capacity(12);
        for byte in b {
            euid.push(byte | 0xAA);
            euid.push(byte | 0x55);
        }
        let cs: u16 = euid.iter().map(|&x| x as u16).fold(0, u16::wrapping_add);
        let cb = cs.to_be_bytes();
        out.extend_from_slice(&euid);
        out.push(cb[0] | 0xAA);
        out.push(cb[0] | 0x55);
        out.push(cb[1] | 0xAA);
        out.push(cb[1] | 0x55);
        out
    }

    #[test]
    fn dub_response_roundtrip() {
        let uid = Uid {
            manufacturer: 0x02CA,
            device: 0x1234_5678,
        };
        for preamble in [0usize, 3, 7] {
            let raw = encode_dub_response(uid, preamble);
            assert_eq!(decode_disc_response(&raw), DiscOutcome::Single(uid));
        }
    }

    #[test]
    fn dub_collision_and_silence() {
        assert_eq!(decode_disc_response(&[]), DiscOutcome::Silence);
        // Two overlapping responses XOR into bit-soup → collision.
        let a = encode_dub_response(
            Uid {
                manufacturer: 0x02CA,
                device: 1,
            },
            2,
        );
        let b = encode_dub_response(
            Uid {
                manufacturer: 0x4A4C,
                device: 99,
            },
            2,
        );
        let mixed: Vec<u8> = a.iter().zip(b.iter()).map(|(x, y)| x & y).collect();
        assert_eq!(decode_disc_response(&mixed), DiscOutcome::Collision);
        // Checksum corruption → collision.
        let mut bad = encode_dub_response(
            Uid {
                manufacturer: 0x02CA,
                device: 1,
            },
            2,
        );
        let last = bad.len() - 1;
        bad[last] ^= 0x10;
        assert_eq!(decode_disc_response(&bad), DiscOutcome::Collision);
    }

    #[test]
    fn parse_response_roundtrip_with_leading_noise() {
        // A GET_RESPONSE for DEVICE_LABEL carrying "Par LED".
        let mut resp = build_request(
            CONTROLLER_UID,
            Uid {
                manufacturer: 0x02CA,
                device: 42,
            },
            9,
            RESPONSE_TYPE_ACK, // port-id slot doubles as response type
            CC_GET_RESPONSE,
            PID_DEVICE_LABEL,
            b"Par LED",
        );
        let mut buf = vec![0x00, 0xFF, 0x13]; // line noise before the frame
        buf.append(&mut resp);
        let parsed = parse_response(&buf).expect("parse");
        assert_eq!(parsed.cc, CC_GET_RESPONSE);
        assert_eq!(parsed.pid, PID_DEVICE_LABEL);
        assert_eq!(parsed.response_type, RESPONSE_TYPE_ACK);
        assert_eq!(ascii_field(&parsed.pd), "Par LED");
    }

    #[test]
    fn parse_response_rejects_bad_checksum() {
        let mut resp = build_request(
            CONTROLLER_UID,
            Uid {
                manufacturer: 1,
                device: 2,
            },
            1,
            0,
            CC_GET_RESPONSE,
            PID_DEVICE_INFO,
            &[0u8; 19],
        );
        let n = resp.len();
        resp[n - 1] ^= 0xFF;
        assert!(parse_response(&resp).is_none());
    }

    #[test]
    fn uid_u64_roundtrip_and_display() {
        let uid = Uid {
            manufacturer: 0x7FF0,
            device: 0xDEAD_BEEF,
        };
        assert_eq!(Uid::from_u64(uid.as_u64()), uid);
        assert_eq!(uid.to_string(), "7FF0:DEADBEEF");
    }
}
