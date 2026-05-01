use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::artnet::{ArtNetDriver, ARTNET_PORT};
use super::d2xx::D2xxOpenDmxDriver;
use super::enttec::EnttecDriver;
use super::mock::MockDriver;
use super::open_dmx::OpenDmxDriver;
use super::sacn::SacnDriver;
use super::OutputDriver;
use crate::engine::output_thread::OutputBinding;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutputBindingConfig {
    Mock {
        id: String,
        universes: Vec<u16>,
    },
    ArtNet {
        id: String,
        target: String,
        universes: Vec<u16>,
    },
    Sacn {
        id: String,
        source_name: String,
        priority: u8,
        universes: Vec<u16>,
    },
    EnttecUsb {
        id: String,
        port: String,
        universes: Vec<u16>,
    },
    /// Raw FTDI dongles via the OS serial driver. Works on Linux/Windows; on
    /// macOS the BREAK timing is unreliable (Apple's FTDI VCP doesn't expose
    /// fast TIOCSBRK ack) — prefer `OpenDmxFtdi` there.
    OpenDmx {
        id: String,
        port: String,
        universes: Vec<u16>,
    },
    /// Raw FTDI dongles via FTDI's D2XX library (vendored statically).
    /// Bypasses the OS serial driver and exposes proper FT_SetBreakOn/Off.
    /// This is the recommended path for ElectroTAS TZ-MINI on macOS.
    OpenDmxFtdi {
        id: String,
        /// FTDI device serial number (stable across reboots).
        serial: String,
        universes: Vec<u16>,
        /// DTR pin state. Wiring varies between Open DMX clones; some need
        /// HIGH, some LOW. Default LOW (matches QLC+/libftdi).
        #[serde(default)]
        dtr_high: bool,
        /// RTS pin state. See `dtr_high`.
        #[serde(default)]
        rts_high: bool,
    },
}

impl OutputBindingConfig {
    pub fn id(&self) -> &str {
        match self {
            Self::Mock { id, .. }
            | Self::ArtNet { id, .. }
            | Self::Sacn { id, .. }
            | Self::EnttecUsb { id, .. }
            | Self::OpenDmx { id, .. }
            | Self::OpenDmxFtdi { id, .. } => id,
        }
    }

    pub fn universes(&self) -> &[u16] {
        match self {
            Self::Mock { universes, .. }
            | Self::ArtNet { universes, .. }
            | Self::Sacn { universes, .. }
            | Self::EnttecUsb { universes, .. }
            | Self::OpenDmx { universes, .. }
            | Self::OpenDmxFtdi { universes, .. } => universes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../bindings/")]
pub struct OutputsConfig {
    pub bindings: Vec<OutputBindingConfig>,
    /// Stable component identifier (CID) for sACN, persisted across runs.
    /// Generated on first save if missing.
    pub sacn_cid: Option<[u8; 16]>,
}

impl OutputsConfig {
    /// All universes mentioned by any binding, deduplicated and sorted.
    pub fn universes(&self) -> Vec<u16> {
        let mut us: Vec<u16> = self
            .bindings
            .iter()
            .flat_map(|b| b.universes().to_vec())
            .collect();
        us.sort_unstable();
        us.dedup();
        us
    }

    /// A reasonable default for a fresh install: a mock binding for universe 0
    /// so the engine has somewhere to send frames.
    pub fn default_starter() -> Self {
        Self {
            bindings: vec![OutputBindingConfig::Mock {
                id: "mock-0".to_string(),
                universes: vec![0],
            }],
            sacn_cid: None,
        }
    }
}

/// Build the runtime drivers from a config snapshot. Drivers that fail to
/// initialise (e.g. Art-Net socket bind failure) are skipped and logged so
/// the rest of the config still applies — never panic on a config error.
pub fn instantiate(config: &OutputsConfig) -> Vec<OutputBinding> {
    let cid = config.sacn_cid.unwrap_or_else(default_cid);
    let mut out = Vec::with_capacity(config.bindings.len());
    for cfg in &config.bindings {
        match cfg {
            OutputBindingConfig::Mock { universes, .. } => {
                out.push(OutputBinding {
                    driver: Box::new(MockDriver),
                    universes: universes.clone(),
                });
            }
            OutputBindingConfig::ArtNet {
                target, universes, ..
            } => {
                let addr = if target.contains(':') {
                    target.clone()
                } else {
                    format!("{target}:{ARTNET_PORT}")
                };
                match addr.parse() {
                    Ok(parsed) => match ArtNetDriver::new(parsed) {
                        Ok(d) => out.push(OutputBinding {
                            driver: Box::new(d) as Box<dyn OutputDriver>,
                            universes: universes.clone(),
                        }),
                        Err(e) => {
                            tracing::warn!(target = %target, error = %e, "art-net bind failed");
                        }
                    },
                    Err(e) => {
                        tracing::warn!(target = %target, error = %e, "art-net target parse failed");
                    }
                }
            }
            OutputBindingConfig::Sacn {
                source_name,
                priority,
                universes,
                ..
            } => match SacnDriver::new(cid, source_name, *priority) {
                Ok(d) => out.push(OutputBinding {
                    driver: Box::new(d) as Box<dyn OutputDriver>,
                    universes: universes.clone(),
                }),
                Err(e) => tracing::warn!(error = %e, "sacn driver init failed"),
            },
            OutputBindingConfig::EnttecUsb {
                port, universes, ..
            } => {
                let d = EnttecDriver::new(port.clone());
                out.push(OutputBinding {
                    driver: Box::new(d) as Box<dyn OutputDriver>,
                    universes: universes.clone(),
                });
            }
            OutputBindingConfig::OpenDmx {
                port, universes, ..
            } => {
                let d = OpenDmxDriver::new(port.clone());
                out.push(OutputBinding {
                    driver: Box::new(d) as Box<dyn OutputDriver>,
                    universes: universes.clone(),
                });
            }
            OutputBindingConfig::OpenDmxFtdi {
                serial,
                universes,
                dtr_high,
                rts_high,
                ..
            } => {
                let d = D2xxOpenDmxDriver::new(serial.clone(), *dtr_high, *rts_high);
                out.push(OutputBinding {
                    driver: Box::new(d) as Box<dyn OutputDriver>,
                    universes: universes.clone(),
                });
            }
        }
    }
    out
}

fn default_cid() -> [u8; 16] {
    *uuid::Uuid::new_v4().as_bytes()
}
