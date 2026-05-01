//! Driver for "Open DMX USB"-style interfaces (ElectroTAS, Enttec Open DMX USB,
//! generic FTDI dongles wired as a DMX cable).
//!
//! These dongles have no on-board MCU. The FTDI chip is just a UART; the host
//! is responsible for generating every part of the DMX-512 frame, including
//! the BREAK and Mark-After-Break (MAB) that precede every packet.
//!
//! Per ANSI E1.11 / DMX-512:
//! - Line idles high.
//! - BREAK: line held low ≥ 88 µs.
//! - MAB: line back high ≥ 8 µs.
//! - Slots: 8N2 framed bytes at 250 000 baud, start code first.
//!
//! The naive approach — `set_break()` + sleep + `clear_break()` — is unreliable
//! on macOS because each ioctl crosses USB to the FTDI chip with several
//! milliseconds of jitter, so the resulting BREAK width is wildly wrong.
//!
//! We use the well-known **baud-rate trick** instead: drop the port to a much
//! slower baud rate, send a single zero byte (which holds the line low for the
//! start bit + 8 data bits = a real BREAK), then switch back to 250 kbaud and
//! transmit the slot data. The natural stop bit at the slow baud (and the time
//! the kernel spends switching baud rates) doubles as the MAB.

use std::io::Write;
use std::time::{Duration, Instant};

use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};

use super::{OutputDriver, OutputError};
use crate::engine::DMX_CHANNELS;

pub const OPEN_DMX_BAUD: u32 = 250_000;
/// Slow baud used to synthesise the BREAK. At 50 000 baud one frame bit lasts
/// 20 µs, so a 0x00 byte (start + 8 zero bits = 9 low bits) holds the line low
/// for ≈180 µs — well above the 88 µs DMX minimum and below the 1 s maximum.
const BREAK_BAUD: u32 = 50_000;
const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);
const FRAME_LEN: usize = DMX_CHANNELS + 1; // start code + 512 channels

pub struct OpenDmxDriver {
    port_path: String,
    port: Option<Box<dyn SerialPort>>,
    last_attempt: Option<Instant>,
    warned_disconnected: bool,
}

impl OpenDmxDriver {
    pub fn new(port_path: impl Into<String>) -> Self {
        let mut d = Self {
            port_path: port_path.into(),
            port: None,
            last_attempt: None,
            warned_disconnected: false,
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
        let opened = serialport::new(&self.port_path, OPEN_DMX_BAUD)
            .data_bits(DataBits::Eight)
            .parity(Parity::None)
            // DMX-512 wire format requires 2 stop bits.
            .stop_bits(StopBits::Two)
            .flow_control(FlowControl::None)
            .timeout(Duration::from_millis(100))
            .open();
        match opened {
            Ok(p) => {
                tracing::info!(
                    target: "dmx::open_dmx",
                    port = %self.port_path,
                    "open dmx port opened (250k 8N2)"
                );
                self.port = Some(p);
                self.warned_disconnected = false;
            }
            Err(e) => {
                if !self.warned_disconnected {
                    tracing::warn!(
                        target: "dmx::open_dmx",
                        port = %self.port_path,
                        error = %e,
                        "open failed; will retry"
                    );
                    self.warned_disconnected = true;
                }
                self.port = None;
            }
        }
    }
}

impl OutputDriver for OpenDmxDriver {
    fn name(&self) -> &'static str {
        "open-dmx"
    }

    fn send(&mut self, _universe: u16, data: &[u8; DMX_CHANNELS]) -> Result<(), OutputError> {
        if self.port.is_none() {
            self.try_open();
        }
        let Some(port) = self.port.as_mut() else {
            return Ok(());
        };

        let mut frame = [0u8; FRAME_LEN];
        // frame[0] is the DMX start code (0x00 = standard 8-bit dimmer data).
        frame[1..].copy_from_slice(data);

        let result = (|| -> std::io::Result<()> {
            // Drain anything left over from the previous frame so the baud
            // switch below doesn't get applied to half-sent bytes.
            port.flush()?;
            // BREAK: drop baud, transmit one zero byte (≈180 µs of line-low),
            // then drain so the byte actually leaves before we switch back.
            port.set_baud_rate(BREAK_BAUD)?;
            port.write_all(&[0u8])?;
            port.flush()?;
            // MAB happens implicitly: the stop bits at low baud + the time the
            // kernel spends switching baud rate keep the line idle-high before
            // the start bit of the first slot byte.
            port.set_baud_rate(OPEN_DMX_BAUD)?;
            port.write_all(&frame)?;
            port.flush()?;
            Ok(())
        })();

        if let Err(err) = result {
            tracing::warn!(
                target: "dmx::open_dmx",
                port = %self.port_path,
                error = %err,
                "write failed; disconnecting"
            );
            self.port = None;
            self.last_attempt = Some(Instant::now());
            self.warned_disconnected = false;
            return Err(OutputError::Io(err));
        }
        Ok(())
    }
}
