use std::io::Write;
use std::time::{Duration, Instant};

use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};

use super::{OutputDriver, OutputError};
use crate::engine::DMX_CHANNELS;

const DELIMITER_START: u8 = 0x7E;
const DELIMITER_END: u8 = 0xE7;
const LABEL_SEND_DMX: u8 = 0x06;
const LABEL_SET_PARAMS: u8 = 0x04;

/// Per Enttec Pro firmware notes the USB virtual COM rate is irrelevant for
/// the on-wire DMX (the FTDI bridge clocks DMX at 250kbaud internally), but
/// 250000 is what every reference implementation (OLA, QLC+) uses.
pub const ENTTEC_BAUD: u32 = 250_000;
const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);
const PACKET_HEADER_LEN: usize = 4;
const PACKET_DATA_LEN: usize = DMX_CHANNELS + 1; // start code + 512 channels
pub const PACKET_LEN: usize = PACKET_HEADER_LEN + PACKET_DATA_LEN + 1; // + END
const INIT_MESSAGE_LEN: usize = 10;

pub struct EnttecDriver {
    port_path: String,
    port: Option<Box<dyn SerialPort>>,
    last_attempt: Option<Instant>,
    warned_disconnected: bool,
}

impl EnttecDriver {
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
        let opened = serialport::new(&self.port_path, ENTTEC_BAUD)
            .data_bits(DataBits::Eight)
            .parity(Parity::None)
            .stop_bits(StopBits::One)
            .flow_control(FlowControl::None)
            .timeout(Duration::from_millis(100))
            .open();
        match opened {
            Ok(mut p) => {
                tracing::info!(target: "dmx::enttec", port = %self.port_path, "port opened");
                if let Err(e) = init_widget(p.as_mut()) {
                    tracing::warn!(
                        target: "dmx::enttec",
                        port = %self.port_path,
                        error = %e,
                        "init message failed; widget may not output DMX"
                    );
                    self.port = None;
                    return;
                }
                self.port = Some(p);
                self.warned_disconnected = false;
                tracing::info!(target: "dmx::enttec", port = %self.port_path, "init complete");
            }
            Err(e) => {
                if !self.warned_disconnected {
                    tracing::warn!(
                        target: "dmx::enttec",
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

impl OutputDriver for EnttecDriver {
    fn name(&self) -> &'static str {
        "enttec-pro"
    }

    fn send(&mut self, _universe: u16, data: &[u8; DMX_CHANNELS]) -> Result<(), OutputError> {
        if self.port.is_none() {
            self.try_open();
        }
        let Some(port) = self.port.as_mut() else {
            return Ok(());
        };

        let mut buf = [0u8; PACKET_LEN];
        write_packet(&mut buf, data);

        let result = port.write_all(&buf).and_then(|_| port.flush());
        if let Err(err) = result {
            tracing::warn!(
                target: "dmx::enttec",
                port = %self.port_path,
                error = %err,
                "write failed; disconnecting"
            );
            self.port = None;
            // Reset attempt timer so reconnect tries again after RECONNECT_INTERVAL,
            // not on the very next frame tick.
            self.last_attempt = Some(Instant::now());
            self.warned_disconnected = false;
            return Err(OutputError::Io(err));
        }
        Ok(())
    }
}

/// Build the Enttec USB Pro `Send DMX Packet (label 0x06)` frame.
///
/// ```text
/// 0x7E              start delimiter
/// 0x06              label
/// data_len LSB
/// data_len MSB
/// 0x00              DMX start code
/// data...           512 DMX channels
/// 0xE7              end delimiter
/// ```
pub fn write_packet(buf: &mut [u8; PACKET_LEN], data: &[u8; DMX_CHANNELS]) {
    let data_len = PACKET_DATA_LEN as u16;
    buf[0] = DELIMITER_START;
    buf[1] = LABEL_SEND_DMX;
    buf[2] = (data_len & 0xFF) as u8;
    buf[3] = ((data_len >> 8) & 0xFF) as u8;
    buf[4] = 0; // DMX start code
    buf[5..5 + DMX_CHANNELS].copy_from_slice(data);
    buf[PACKET_LEN - 1] = DELIMITER_END;
}

/// Build the `Set Widget Parameters Request (label 0x04)` packet.
///
/// Without this some widget firmwares stay in receive-only mode after USB
/// enumeration and never start clocking out DMX even after the host sends a
/// Send DMX packet. Sending sane defaults (88us break, 8us MAB, 40 fps cap)
/// puts the widget into transmit mode.
pub fn write_init_message(buf: &mut [u8; INIT_MESSAGE_LEN]) {
    buf[0] = DELIMITER_START;
    buf[1] = LABEL_SET_PARAMS;
    buf[2] = 0x05; // data_len LSB
    buf[3] = 0x00; // data_len MSB
    buf[4] = 0x00; // user_size LSB
    buf[5] = 0x00; // user_size MSB
    buf[6] = 0x09; // break time (9 → 96us)
    buf[7] = 0x01; // MAB time (1 → 10.7us)
    buf[8] = 40; // refresh rate (40 packets/sec, the DMX max)
    buf[9] = DELIMITER_END;
}

fn init_widget(port: &mut dyn SerialPort) -> std::io::Result<()> {
    let mut buf = [0u8; INIT_MESSAGE_LEN];
    write_init_message(&mut buf);
    port.write_all(&buf)?;
    port.flush()?;
    // Send an immediate blackout frame so the widget has a packet to repeat
    // and starts driving the line right away (rather than waiting for the
    // next engine tick).
    let mut frame = [0u8; PACKET_LEN];
    write_packet(&mut frame, &[0u8; DMX_CHANNELS]);
    port.write_all(&frame)?;
    port.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_total_length_is_518() {
        assert_eq!(PACKET_LEN, 518);
    }

    #[test]
    fn header_and_footer() {
        let mut buf = [0u8; PACKET_LEN];
        write_packet(&mut buf, &[0u8; DMX_CHANNELS]);
        assert_eq!(buf[0], 0x7E);
        assert_eq!(buf[1], 0x06);
        assert_eq!(buf[2], 0x01); // 513 LSB
        assert_eq!(buf[3], 0x02); // 513 MSB
        assert_eq!(buf[4], 0x00); // start code
        assert_eq!(buf[PACKET_LEN - 1], 0xE7);
    }

    #[test]
    fn dmx_data_copied_after_start_code() {
        let mut buf = [0u8; PACKET_LEN];
        let mut data = [0u8; DMX_CHANNELS];
        data[0] = 1;
        data[1] = 128;
        data[511] = 255;
        write_packet(&mut buf, &data);
        assert_eq!(buf[5], 1);
        assert_eq!(buf[6], 128);
        assert_eq!(buf[5 + 511], 255);
    }

    #[test]
    fn init_message_matches_qlc_reference() {
        let mut buf = [0u8; INIT_MESSAGE_LEN];
        write_init_message(&mut buf);
        // Bytes match QLC+ EnttecDMXUSBPro::initializeWidget reference.
        assert_eq!(
            &buf[..],
            &[0x7E, 0x04, 0x05, 0x00, 0x00, 0x00, 0x09, 0x01, 0x28, 0xE7]
        );
    }
}
