//! Per-output health for the header chips: is each configured output
//! opening, sending frames, and — for Art-Net — is the node actually there?
//!
//! The output thread reports every send result; a probe thread polls the
//! Art-Net targets; the table is pushed to the UI as `output:health`.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

use super::config::{OutputBindingConfig, OutputsConfig};

pub const HEALTH_EVENT: &str = "output:health";

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct OutputHealth {
    pub id: String,
    /// "art_net" | "sacn" | "enttec_usb" | "open_dmx" | "open_dmx_ftdi" | "mock"
    pub kind: String,
    /// Short human label: "Art-Net 192.168.1.50", "FTDI A50285BI", "Enttec tty.usbserial-1".
    pub label: String,
    pub universes: Vec<u16>,
    /// "ok" (frames going out) | "error" (open/send failing) | "idle" (nothing sent yet)
    pub state: String,
    pub last_error: Option<String>,
    /// Art-Net: the node answered a poll in the last few seconds. USB: the
    /// port is open. `None` = the protocol can't tell (sACN multicast, mock).
    pub linked: Option<bool>,
}

struct Entry {
    health: OutputHealth,
    /// Art-Net target IP (without port) for the link probe.
    artnet_ip: Option<String>,
}

struct Inner {
    entries: Vec<Entry>,
    app: Option<AppHandle>,
    last_emit: Option<Instant>,
    last_snapshot: Vec<OutputHealth>,
}

fn state() -> &'static Mutex<Inner> {
    static S: OnceLock<Mutex<Inner>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(Inner {
            entries: Vec::new(),
            app: None,
            last_emit: None,
            last_snapshot: Vec::new(),
        })
    })
}

/// Give the module an app handle to emit through and start the Art-Net probe.
pub fn install(app: AppHandle) {
    state().lock().app = Some(app);
    let _ = std::thread::Builder::new()
        .name("artnet-link-probe".into())
        .spawn(probe_loop);
}

fn short_port(p: &str) -> String {
    p.rsplit('/').next().unwrap_or(p).to_string()
}

/// Rebuild the table from the configured bindings.
pub fn configure(cfg: &OutputsConfig) {
    let mut entries = Vec::new();
    for b in &cfg.bindings {
        let (id, kind, label, universes, artnet_ip, linked) = match b {
            OutputBindingConfig::Mock { id, universes, .. } => (
                id.clone(),
                "mock",
                "Mock".to_string(),
                universes.clone(),
                None,
                None,
            ),
            OutputBindingConfig::ArtNet {
                id,
                target,
                universes,
                ..
            } => {
                let ip = target.split(':').next().unwrap_or(target).to_string();
                (
                    id.clone(),
                    "art_net",
                    format!("Art-Net {ip}"),
                    universes.clone(),
                    Some(ip),
                    Some(false),
                )
            }
            OutputBindingConfig::Sacn { id, universes, .. } => (
                id.clone(),
                "sacn",
                "sACN".to_string(),
                universes.clone(),
                None,
                None,
            ),
            OutputBindingConfig::EnttecUsb {
                id,
                port,
                universes,
                ..
            } => (
                id.clone(),
                "enttec_usb",
                format!("Enttec {}", short_port(port)),
                universes.clone(),
                None,
                Some(false),
            ),
            OutputBindingConfig::OpenDmx {
                id,
                port,
                universes,
                ..
            } => (
                id.clone(),
                "open_dmx",
                format!("Open DMX {}", short_port(port)),
                universes.clone(),
                None,
                Some(false),
            ),
            OutputBindingConfig::OpenDmxFtdi {
                id,
                serial,
                universes,
                ..
            } => (
                id.clone(),
                "open_dmx_ftdi",
                format!("FTDI {serial}"),
                universes.clone(),
                None,
                Some(false),
            ),
        };
        entries.push(Entry {
            health: OutputHealth {
                id,
                kind: kind.into(),
                label,
                universes,
                state: "idle".into(),
                last_error: None,
                linked,
            },
            artnet_ip,
        });
    }
    let mut s = state().lock();
    s.entries = entries;
    s.last_emit = None; // push the fresh table on the next tick
    s.last_snapshot.clear();
}

fn with_entry(id: &str, f: impl FnOnce(&mut Entry)) {
    let mut s = state().lock();
    if let Some(e) = s.entries.iter_mut().find(|e| e.health.id == id) {
        f(e);
    }
}

/// The driver could not even be created (bad target, socket error…).
pub fn note_open_failure(id: &str, err: &str) {
    with_entry(id, |e| {
        e.health.state = "error".into();
        e.health.last_error = Some(err.to_string());
        if e.health.linked.is_some() {
            e.health.linked = Some(false);
        }
    });
}

pub fn note_send_ok(id: &str) {
    with_entry(id, |e| {
        if e.health.state != "ok" {
            e.health.state = "ok".into();
            e.health.last_error = None;
        }
        // A successful write on a serial port means it is open.
        if e.health.kind != "art_net" && e.health.linked.is_some() {
            e.health.linked = Some(true);
        }
    });
}

pub fn note_send_error(id: &str, err: &str) {
    with_entry(id, |e| {
        e.health.state = "error".into();
        e.health.last_error = Some(err.to_string());
        if e.health.kind != "art_net" && e.health.linked.is_some() {
            e.health.linked = Some(false);
        }
    });
}

/// Called once per output frame: pushes the table when something changed
/// (at most every 250 ms) and as a heartbeat every 2 s.
pub fn tick(now: Instant) {
    let mut s = state().lock();
    let Some(app) = s.app.clone() else { return };
    let snapshot: Vec<OutputHealth> = s.entries.iter().map(|e| e.health.clone()).collect();
    let changed = snapshot != s.last_snapshot;
    let min_gap = if changed {
        Duration::from_millis(250)
    } else {
        Duration::from_secs(2)
    };
    let due = s
        .last_emit
        .map(|t| now.duration_since(t) >= min_gap)
        .unwrap_or(true);
    if !due {
        return;
    }
    s.last_emit = Some(now);
    s.last_snapshot = snapshot.clone();
    drop(s);
    let _ = app.emit(HEALTH_EVENT, snapshot);
}

pub fn snapshot() -> Vec<OutputHealth> {
    state()
        .lock()
        .entries
        .iter()
        .map(|e| e.health.clone())
        .collect()
}

/// Art-Net nodes never acknowledge ArtDmx, so we poll them: every few
/// seconds one broadcast ArtPoll, and each configured target that answers
/// counts as linked.
fn probe_loop() {
    loop {
        std::thread::sleep(Duration::from_secs(4));
        let targets: Vec<String> = state()
            .lock()
            .entries
            .iter()
            .filter_map(|e| e.artnet_ip.clone())
            .collect();
        if targets.is_empty() {
            continue;
        }
        let Ok(found) = super::artnet::scan(900) else {
            continue;
        };
        let mut s = state().lock();
        for e in s.entries.iter_mut() {
            if let Some(ip) = &e.artnet_ip {
                e.health.linked = Some(found.iter().any(|n| &n.ip == ip));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_follows_config_and_send_results() {
        let cfg = OutputsConfig {
            bindings: vec![
                OutputBindingConfig::ArtNet {
                    id: "an".into(),
                    target: "192.168.1.50".into(),
                    universes: vec![0],
                },
                OutputBindingConfig::OpenDmxFtdi {
                    id: "ftdi".into(),
                    serial: "A50285BI".into(),
                    universes: vec![1],
                    dtr_high: false,
                    rts_high: false,
                },
            ],
            sacn_cid: None,
        };
        configure(&cfg);
        let snap = snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].label, "Art-Net 192.168.1.50");
        assert_eq!(snap[0].state, "idle");
        assert_eq!(snap[1].label, "FTDI A50285BI");
        note_send_ok("ftdi");
        note_send_error("an", "boom");
        let snap = snapshot();
        assert_eq!(snap[0].state, "error");
        assert_eq!(snap[0].last_error.as_deref(), Some("boom"));
        assert_eq!(snap[1].state, "ok");
        assert_eq!(
            snap[1].linked,
            Some(true),
            "a serial write proves the port is open"
        );
        note_send_ok("an");
        assert_eq!(snapshot()[0].state, "ok");
        assert_eq!(
            snapshot()[0].linked,
            Some(false),
            "art-net link waits for the probe"
        );
        note_open_failure("ftdi", "no device");
        assert_eq!(snapshot()[1].linked, Some(false));
    }
}
