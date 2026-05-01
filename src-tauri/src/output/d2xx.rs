//! Open DMX driver that talks to the FTDI chip directly via libusb (rusb).
//!
//! Why not the OS serial port? On macOS, Apple's `AppleUSBFTDI` kext claims
//! every FTDI device at enumeration time. Through the BSD `tty` layer
//! (`/dev/cu.usbserial-…`) we cannot generate the precise BREAK + MAB timing
//! that DMX-512 requires.
//!
//! Why not FTDI's own D2XX library? Because on macOS D2XX cannot evict
//! `AppleUSBFTDI` from the device — D2XX is itself a userspace libusb client
//! and it does not call `detach_kernel_driver`. Writes "succeed" but never
//! reach the wire because the kernel driver still owns the bulk endpoints.
//!
//! What works: open the FTDI directly with libusb, call
//! `libusb_detach_kernel_driver` (which on macOS uses IOKit to release the
//! kext from this specific device), claim the interface, then send the FTDI
//! vendor control requests for baud rate / line settings / break, and the
//! DMX bytes via the bulk OUT endpoint.
//!
//! Reference: AN_232B-04, AN_232R-01 (FTDI), and the libftdi source.

use std::time::{Duration, Instant};

use rusb::{Direction, GlobalContext, UsbContext};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{OutputDriver, OutputError};
use crate::engine::DMX_CHANNELS;

const FTDI_VID: u16 = 0x0403;

const REQ_TYPE_VENDOR_OUT: u8 = 0x40;
const FTDI_REQ_RESET: u8 = 0x00;
const FTDI_REQ_SET_MODEM_CTRL: u8 = 0x01;
const FTDI_REQ_SET_FLOW_CTRL: u8 = 0x02;
const FTDI_REQ_SET_BAUD_RATE: u8 = 0x03;
const FTDI_REQ_SET_LINE_PROP: u8 = 0x04;
const FTDI_REQ_SET_LATENCY: u8 = 0x09;

/// Modem-control wValue encoding:
///   bit 0 = DTR state (0 = low, 1 = high)
///   bit 1 = RTS state
///   bit 8 = enable DTR write (always set)
///   bit 9 = enable RTS write (always set)
///
/// Different Open DMX clones wire DTR/RTS differently to the RS-485
/// transceiver enable pins, and the right combination is hardware-specific.
/// We expose these as runtime-selectable values so the user can sweep them
/// without recompiling.
fn modem_value(dtr_high: bool, rts_high: bool) -> u16 {
    let mut v: u16 = 0x0300; // both writes enabled
    if dtr_high {
        v |= 0x0001;
    }
    if rts_high {
        v |= 0x0002;
    }
    v
}

const FTDI_RESET_SIO: u16 = 0;
const FTDI_RESET_PURGE_RX: u16 = 1;
const FTDI_RESET_PURGE_TX: u16 = 2;

/// Line property word for `SIO_SET_DATA_REQUEST`. Encoding (per libftdi /
/// FTDI's own protocol — note the bit ordering trips people up):
///   bits 0-7   word length (8)
///   bits 8-10  parity      (0 = none, 1 = odd, 2 = even, 3 = mark, 4 = space)
///   bits 11-13 stop bits   (0 = 1 stop, 1 = 1.5, 2 = 2 stop)
///   bit  14    break       (0 = off, 1 = on)
///
/// 8N2 = word_len 8 + parity 0 + stop_bits 2 + break 0
///     = 0x08 | (0 << 8) | (2 << 11) | (0 << 14)
///     = 0x1008
const LINE_8N2: u16 = 0x1008;
const LINE_8N2_BREAK: u16 = LINE_8N2 | 0x4000;

/// Baud rate divisor for 250 000 baud assuming the standard 3 MHz UART clock
/// (FT232R / FT232BL / FT245). 3_000_000 / 250_000 = 12, no fractional part.
const BAUD_DIVISOR_250K: u16 = 12;

const USB_TIMEOUT: Duration = Duration::from_millis(100);
const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);
const BREAK_DURATION: Duration = Duration::from_micros(120);
const MAB_DURATION: Duration = Duration::from_micros(12);
const FRAME_LEN: usize = DMX_CHANNELS + 1;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct D2xxDeviceInfo {
    pub serial_number: String,
    pub description: String,
    pub vendor_id: u16,
    pub product_id: u16,
    /// True if some other process / kernel driver currently has it claimed.
    pub port_open: bool,
}

pub fn list_devices() -> Vec<D2xxDeviceInfo> {
    let mut out = Vec::new();
    let Ok(devices) = rusb::devices() else {
        return out;
    };
    for dev in devices.iter() {
        let Ok(desc) = dev.device_descriptor() else {
            continue;
        };
        if desc.vendor_id() != FTDI_VID {
            continue;
        }
        let mut info = D2xxDeviceInfo {
            serial_number: String::new(),
            description: String::new(),
            vendor_id: desc.vendor_id(),
            product_id: desc.product_id(),
            port_open: false,
        };
        match dev.open() {
            Ok(handle) => {
                info.serial_number = handle
                    .read_serial_number_string_ascii(&desc)
                    .unwrap_or_default();
                info.description = handle.read_product_string_ascii(&desc).unwrap_or_default();
                // Don't try to claim — that would bump the kext. Just probe.
                info.port_open = handle.kernel_driver_active(0).unwrap_or(false);
            }
            Err(rusb::Error::Access) => {
                info.port_open = true;
            }
            Err(_) => {}
        }
        out.push(info);
    }
    out
}

pub struct D2xxOpenDmxDriver {
    serial: String,
    dtr_high: bool,
    rts_high: bool,
    handle: Option<rusb::DeviceHandle<GlobalContext>>,
    out_endpoint: u8,
    interface: u8,
    last_attempt: Option<Instant>,
    warned_disconnected: bool,
    frames_sent: u32,
    last_heartbeat: Instant,
}

impl D2xxOpenDmxDriver {
    pub fn new(serial: impl Into<String>, dtr_high: bool, rts_high: bool) -> Self {
        let mut d = Self {
            serial: serial.into(),
            dtr_high,
            rts_high,
            handle: None,
            out_endpoint: 0x02, // FT232R/FT245 default; overridden on open()
            interface: 0,
            last_attempt: None,
            warned_disconnected: false,
            frames_sent: 0,
            last_heartbeat: Instant::now(),
        };
        d.try_open();
        d
    }

    fn try_open(&mut self) {
        let now = Instant::now();
        if let Some(t) = self.last_attempt {
            if now.duration_since(t) < RECONNECT_INTERVAL {
                return;
            }
        }
        self.last_attempt = Some(now);

        match self.open_inner() {
            Ok((handle, out_ep, iface)) => {
                tracing::info!(
                    target: "dmx::ftdi",
                    serial = %self.serial,
                    out_ep = format!("0x{:02x}", out_ep),
                    iface,
                    "FTDI claimed via libusb (kext detached)"
                );
                self.handle = Some(handle);
                self.out_endpoint = out_ep;
                self.interface = iface;
                self.warned_disconnected = false;
            }
            Err(e) => {
                if !self.warned_disconnected {
                    tracing::warn!(
                        target: "dmx::ftdi",
                        serial = %self.serial,
                        error = %e,
                        "FTDI claim failed; will retry"
                    );
                    self.warned_disconnected = true;
                }
                self.handle = None;
            }
        }
    }

    fn open_inner(&self) -> Result<(rusb::DeviceHandle<GlobalContext>, u8, u8), String> {
        let devices = rusb::devices().map_err(|e| format!("rusb::devices: {e}"))?;
        let mut last_err = String::from("no matching FTDI device");
        for dev in devices.iter() {
            let desc = match dev.device_descriptor() {
                Ok(d) => d,
                Err(_) => continue,
            };
            if desc.vendor_id() != FTDI_VID {
                continue;
            }
            let handle = match dev.open() {
                Ok(h) => h,
                Err(e) => {
                    last_err = format!("open: {e}");
                    continue;
                }
            };
            let dev_serial = handle
                .read_serial_number_string_ascii(&desc)
                .unwrap_or_default();
            if dev_serial != self.serial {
                continue;
            }

            // 1) Detach Apple's FTDI kext from this specific device (the whole
            //    point of going via libusb instead of the BSD tty layer).
            // Kernel-driver detachment only matters on macOS/Linux; on
            // Windows libusb expects WinUSB to already own the device
            // (via Zadig) and `set_auto_detach_kernel_driver` returns
            // NotSupported. The unwraps below absorb that.
            let _ = handle.set_auto_detach_kernel_driver(true);
            let iface: u8 = 0;
            if handle.kernel_driver_active(iface).unwrap_or(false) {
                if let Err(e) = handle.detach_kernel_driver(iface) {
                    let detail = ftdi_access_hint(&e);
                    return Err(if detail.is_empty() {
                        format!("detach_kernel_driver: {e}")
                    } else {
                        format!("detach_kernel_driver: {e} — {detail}")
                    });
                }
            }

            // 2) Claim the interface.
            handle
                .claim_interface(iface)
                .map_err(|e| format!("claim_interface: {e}"))?;

            // 3) Find the bulk OUT endpoint (it's 0x02 on most FT232 chips but
            //    we read the descriptor to be safe).
            let out_ep = find_out_endpoint(&dev, iface).unwrap_or(0x02);

            // 4) Configure the chip for DMX-512.
            ftdi_control(&handle, FTDI_REQ_RESET, FTDI_RESET_SIO, 0)?;
            ftdi_control(&handle, FTDI_REQ_SET_LATENCY, 1, 0)?;
            ftdi_control(&handle, FTDI_REQ_SET_FLOW_CTRL, 0, 0)?;
            ftdi_control(&handle, FTDI_REQ_SET_BAUD_RATE, BAUD_DIVISOR_250K, 0)?;
            ftdi_control(&handle, FTDI_REQ_SET_LINE_PROP, LINE_8N2, 0)?;
            ftdi_control(
                &handle,
                FTDI_REQ_SET_MODEM_CTRL,
                modem_value(self.dtr_high, self.rts_high),
                0,
            )?;
            ftdi_control(&handle, FTDI_REQ_RESET, FTDI_RESET_PURGE_RX, 0)?;
            ftdi_control(&handle, FTDI_REQ_RESET, FTDI_RESET_PURGE_TX, 0)?;

            return Ok((handle, out_ep, iface));
        }
        Err(format!(
            "FTDI serial {} not found ({})",
            self.serial, last_err
        ))
    }
}

impl OutputDriver for D2xxOpenDmxDriver {
    fn name(&self) -> &'static str {
        "ftdi-libusb"
    }

    fn send(&mut self, _universe: u16, data: &[u8; DMX_CHANNELS]) -> Result<(), OutputError> {
        if self.handle.is_none() {
            self.try_open();
        }
        let Some(handle) = self.handle.as_ref() else {
            return Ok(());
        };

        let mut frame = [0u8; FRAME_LEN];
        // frame[0] = DMX start code (0x00). frame[1..] = the 512 channels.
        frame[1..].copy_from_slice(data);

        let result = (|| -> Result<usize, String> {
            // BREAK on
            ftdi_control(handle, FTDI_REQ_SET_LINE_PROP, LINE_8N2_BREAK, 0)?;
            spin_for(BREAK_DURATION);
            // BREAK off — back to mark/idle high; doubles as start of MAB.
            ftdi_control(handle, FTDI_REQ_SET_LINE_PROP, LINE_8N2, 0)?;
            spin_for(MAB_DURATION);
            // 513-byte frame via bulk OUT.
            let bytes = handle
                .write_bulk(self.out_endpoint, &frame, USB_TIMEOUT)
                .map_err(|e| format!("write_bulk: {e}"))?;
            Ok(bytes)
        })();

        let bytes = match result {
            Ok(n) => n,
            Err(err) => {
                tracing::warn!(
                    target: "dmx::ftdi",
                    serial = %self.serial,
                    error = %err,
                    "send failed; releasing and reconnecting"
                );
                self.handle = None;
                self.last_attempt = Some(Instant::now());
                self.warned_disconnected = false;
                return Err(OutputError::Config(format!("ftdi error: {err}")));
            }
        };
        if bytes != FRAME_LEN {
            tracing::warn!(
                target: "dmx::ftdi",
                serial = %self.serial,
                wrote = bytes,
                expected = FRAME_LEN,
                "short bulk write — frame may be truncated on the wire"
            );
        }
        self.frames_sent = self.frames_sent.saturating_add(1);
        if self.last_heartbeat.elapsed() >= Duration::from_secs(1) {
            // Heartbeat at TRACE — useful when debugging USB throughput,
            // but otherwise just clutters the console during a show.
            // Enable with `RUST_LOG=dmx::ftdi=trace`.
            tracing::trace!(
                target: "dmx::ftdi",
                serial = %self.serial,
                frames = self.frames_sent,
                bytes_per_frame = bytes,
                "ftdi heartbeat"
            );
            self.frames_sent = 0;
            self.last_heartbeat = Instant::now();
        }
        Ok(())
    }
}

impl Drop for D2xxOpenDmxDriver {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            // Best-effort: clear the break line and release the interface so
            // the next start can claim cleanly.
            let _ = ftdi_control(&handle, FTDI_REQ_SET_LINE_PROP, LINE_8N2, 0);
            let _ = handle.release_interface(self.interface);
        }
    }
}

fn ftdi_control(
    handle: &rusb::DeviceHandle<GlobalContext>,
    request: u8,
    value: u16,
    index: u16,
) -> Result<(), String> {
    handle
        .write_control(REQ_TYPE_VENDOR_OUT, request, value, index, &[], USB_TIMEOUT)
        .map(drop)
        .map_err(|e| format!("ftdi req=0x{request:02x} value=0x{value:04x}: {e}"))
}

fn find_out_endpoint<C: UsbContext>(dev: &rusb::Device<C>, iface_num: u8) -> Option<u8> {
    let cfg = dev.active_config_descriptor().ok()?;
    for iface in cfg.interfaces() {
        if iface.number() != iface_num {
            continue;
        }
        for desc in iface.descriptors() {
            for ep in desc.endpoint_descriptors() {
                if ep.direction() == Direction::Out
                    && ep.transfer_type() == rusb::TransferType::Bulk
                {
                    return Some(ep.address());
                }
            }
        }
    }
    None
}

/// Tailored hint for the most common "I can't claim this FTDI device"
/// error on each OS. Returns an empty string when the error isn't an
/// access error so the caller falls back to the raw libusb message.
fn ftdi_access_hint(e: &rusb::Error) -> &'static str {
    if !matches!(e, rusb::Error::Access) {
        return "";
    }
    #[cfg(target_os = "macos")]
    {
        "AppleUSBFTDI is holding the device and macOS won't let a \
         non-root process release it. Run with `sudo`, install \
         FTDI's VCP driver (https://ftdichip.com/drivers/vcp-drivers/), \
         or `sudo kextunload -bundle com.apple.driver.AppleUSBFTDI`"
    }
    #[cfg(target_os = "windows")]
    {
        "Windows expects the FTDI interface to be bound to WinUSB before \
         libusb can talk to it. Run Zadig (https://zadig.akeo.ie/) once \
         and select the WinUSB driver for this device. Note that doing so \
         removes the COM port — pick either D2XX or Serial output, not \
         both. If you'd rather keep the COM port, switch this binding to \
         the Serial output type."
    }
    #[cfg(target_os = "linux")]
    {
        "udev hasn't granted access to this FTDI device. Add the standard \
         FTDI udev rule (plugdev group) or run with sudo."
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        ""
    }
}

/// Busy-wait for the given duration. macOS `thread::sleep` is too coarse
/// (~1 ms granularity) for our 12 µs MAB target; CPU spin is correct here.
fn spin_for(d: Duration) {
    let until = Instant::now() + d;
    while Instant::now() < until {
        std::hint::spin_loop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression guard for the FTDI line-property wValue layout. The bug
    /// that ate hours of debugging on the ElectroTAS TZ-MINI was that we had
    /// stop bits and parity in swapped positions, sending 8E1 instead of 8N2.
    /// Half the slots came out with parity-bit garbage and the fixture
    /// rejected them as framing errors, even though the BREAK looked fine.
    ///
    /// Layout (per libftdi / FTDI spec):
    ///   bits 0-7   word length
    ///   bits 8-10  parity (0 = none)
    ///   bits 11-13 stop bits (2 = 2 stop)
    ///   bit  14    break (1 = on)
    #[test]
    fn line_8n2_encoding_is_correct() {
        // word_len = 8 (0x08), parity = 0 (omitted), stop_bits = 2 (<<11), break = 0
        assert_eq!(LINE_8N2, 0x08 | (2 << 11));
        assert_eq!(LINE_8N2, 0x1008);

        // 8N2 with break flag set.
        assert_eq!(LINE_8N2_BREAK, LINE_8N2 | (1 << 14));
        assert_eq!(LINE_8N2_BREAK, 0x5008);

        // Word length must be in the low 8 bits.
        assert_eq!(LINE_8N2 & 0x00FF, 8);
        // Parity must be NONE.
        assert_eq!((LINE_8N2 >> 8) & 0b111, 0);
        // Stop bits must be 2.
        assert_eq!((LINE_8N2 >> 11) & 0b111, 2);
        // Break must be off in the non-break value.
        assert_eq!((LINE_8N2 >> 14) & 1, 0);
        // ...and on in the break value.
        assert_eq!((LINE_8N2_BREAK >> 14) & 1, 1);
    }

    #[test]
    fn baud_divisor_for_250k() {
        // 3 MHz UART clock / 250 kbaud = 12, no fractional component.
        assert_eq!(BAUD_DIVISOR_250K, 12);
        assert_eq!(3_000_000 / BAUD_DIVISOR_250K as u32, 250_000);
    }

    #[test]
    fn modem_value_combinations() {
        // Both LOW (QLC+/libftdi default) — the magic combo for ElectroTAS.
        assert_eq!(modem_value(false, false), 0x0300);
        // DTR HIGH only.
        assert_eq!(modem_value(true, false), 0x0301);
        // RTS HIGH only.
        assert_eq!(modem_value(false, true), 0x0302);
        // Both HIGH.
        assert_eq!(modem_value(true, true), 0x0303);
    }
}
