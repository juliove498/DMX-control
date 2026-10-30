//! LAN presence beacon + collision detector + peer discovery for sync.
//!
//! Every running instance advertises itself over mDNS (`_dmxctl._tcp`) and
//! browses for other instances at the same time. The TXT record carries a
//! per-launch id (so we can skip our own reflection) and the instance's
//! current **role** (live/mirror). That gives us three things at once:
//!
//! 1. The collision alarm: two consoles both claiming "live" on one network
//!    means two engines could be driving the same Art-Net nodes — the
//!    frontend turns that into a red banner.
//! 2. Discovery for replication: mirrors find the live console's address and
//!    sync port here (the beacon advertises `sync::SYNC_PORT`).
//! 3. A visible identity per machine ("DMX Control (Juliere-M1)").
//!
//! Same-machine double launches are already prevented by the single-instance
//! plugin; this module covers the *cross-machine* cases. Best-effort by
//! design: if mDNS is unavailable the app works normally, just without the
//! radar (and mirrors can't auto-find the live console).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

pub const PEERS_EVENT: &str = "instances:peers";

const SERVICE_TYPE: &str = "_dmxctl._tcp.local.";

/// What the frontend sees about one peer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct PeerInfo {
    pub name: String,
    /// "live" | "mirror" (older builds without a role TXT count as "live":
    /// they always drive outputs).
    pub role: String,
}

#[derive(Debug, Clone)]
struct Peer {
    name: String,
    role: String,
    addr: Option<SocketAddr>,
}

struct Identity {
    iid: String,
    instance: String,
    fullname: String,
    host_label: String,
    role: String,
}

#[derive(Default)]
pub struct PresenceInner {
    peers: Mutex<HashMap<String, Peer>>,
    daemon: Mutex<Option<mdns_sd::ServiceDaemon>>,
    identity: Mutex<Option<Identity>>,
}

#[derive(Default, Clone)]
pub struct PresenceState(Arc<PresenceInner>);

impl PresenceState {
    pub fn peers_info(&self) -> Vec<PeerInfo> {
        let mut infos: Vec<PeerInfo> = self
            .0
            .peers
            .lock()
            .unwrap()
            .values()
            .map(|p| PeerInfo {
                name: p.name.clone(),
                role: p.role.clone(),
            })
            .collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos.dedup();
        infos
    }
}

/// Current peer list for the frontend's initial render; later changes arrive
/// via `instances:peers` events.
#[tauri::command]
pub fn instance_peers(state: tauri::State<'_, PresenceState>) -> Vec<PeerInfo> {
    state.peers_info()
}

/// First visible live console (name + sync address), for the mirror's
/// replication client.
pub fn live_peer(app: &AppHandle) -> Option<(String, SocketAddr)> {
    let state = app.try_state::<PresenceState>()?;
    let peers = state.0.peers.lock().unwrap();
    peers.values().find_map(|p| {
        if p.role == "live" {
            p.addr.map(|a| (p.name.clone(), a))
        } else {
            None
        }
    })
}

/// Re-advertise this instance with a new role after a promotion/demotion so
/// every other console (and its operator) sees the flip within seconds.
pub fn advertise_role(app: &AppHandle, role: &str) {
    let Some(state) = app.try_state::<PresenceState>() else {
        return;
    };
    let daemon_guard = state.0.daemon.lock().unwrap();
    let Some(daemon) = daemon_guard.as_ref() else {
        return;
    };
    let mut id_guard = state.0.identity.lock().unwrap();
    let Some(identity) = id_guard.as_mut() else {
        return;
    };
    let _ = daemon.unregister(&identity.fullname);
    identity.role = role.to_string();
    if let Err(e) = register_identity(daemon, identity) {
        tracing::warn!(target: "dmx::presence", error = %e, "could not re-advertise role");
    } else {
        tracing::info!(target: "dmx::presence", role, "re-advertised with new role");
    }
}

/// Short machine hostname ("Juliere-M1", no ".local" suffix) used to build a
/// per-machine mDNS identity. Falls back to "mac" when the hostname can't be
/// read — a duplicate identity across machines is degraded, not fatal.
pub(crate) fn machine_host_label() -> String {
    let raw = std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let name = raw.trim().trim_end_matches('.');
    let name = name.strip_suffix(".local").unwrap_or(name);
    if name.is_empty() {
        "mac".to_string()
    } else {
        name.to_string()
    }
}

fn register_identity(
    daemon: &mdns_sd::ServiceDaemon,
    identity: &Identity,
) -> Result<(), mdns_sd::Error> {
    let mut props = HashMap::new();
    props.insert("iid".to_string(), identity.iid.clone());
    props.insert("role".to_string(), identity.role.clone());
    let info = mdns_sd::ServiceInfo::new(
        SERVICE_TYPE,
        &identity.instance,
        &format!("{}.local.", identity.host_label),
        "0.0.0.0",
        crate::mirror::MIRROR_PORT,
        props,
    )?
    .enable_addr_auto();
    daemon.register(info)?;
    Ok(())
}

pub fn start(app: AppHandle, state: PresenceState, initial_role: &str) {
    let initial_role = initial_role.to_string();
    let _ = std::thread::Builder::new()
        .name("dmx-presence".into())
        .spawn(move || {
            if let Err(e) = run(&app, &state, &initial_role) {
                tracing::warn!(target: "dmx::presence", error = %e, "presence beacon stopped");
            }
        });
}

fn run(app: &AppHandle, state: &PresenceState, initial_role: &str) -> Result<(), mdns_sd::Error> {
    let daemon = mdns_sd::ServiceDaemon::new()?;

    let iid = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let host_label = machine_host_label();
    let instance = format!("DMX Control ({host_label})");
    let identity = Identity {
        iid: iid.clone(),
        fullname: format!("{instance}.{SERVICE_TYPE}"),
        instance,
        host_label,
        role: initial_role.to_string(),
    };
    register_identity(&daemon, &identity)?;
    tracing::info!(
        target: "dmx::presence",
        instance = %identity.instance,
        role = %identity.role,
        "presence beacon up"
    );

    let events = daemon.browse(SERVICE_TYPE)?;
    *state.0.identity.lock().unwrap() = Some(identity);
    *state.0.daemon.lock().unwrap() = Some(daemon);

    while let Ok(event) = events.recv() {
        match event {
            mdns_sd::ServiceEvent::ServiceResolved(peer) => {
                if peer.get_property_val_str("iid") == Some(iid.as_str()) {
                    continue; // that's us
                }
                let fullname = peer.get_fullname().to_string();
                let display_name = fullname
                    .strip_suffix(&format!(".{SERVICE_TYPE}"))
                    .unwrap_or(&fullname)
                    .to_string();
                let role = peer
                    .get_property_val_str("role")
                    .unwrap_or("live")
                    .to_string();
                let addr = peer
                    .get_addresses_v4()
                    .into_iter()
                    .next()
                    .map(|ip| SocketAddr::from((*ip, peer.get_port())));
                let entry = Peer {
                    name: display_name.clone(),
                    role: role.clone(),
                    addr,
                };
                let changed = {
                    let mut peers = state.0.peers.lock().unwrap();
                    let prev = peers.insert(fullname, entry);
                    !matches!(prev, Some(p) if p.name == display_name && p.role == role)
                };
                if changed {
                    tracing::warn!(target: "dmx::presence", peer = %display_name, %role, "instance on the network");
                    let _ = app.emit(PEERS_EVENT, state.peers_info());
                }
            }
            mdns_sd::ServiceEvent::ServiceRemoved(_ty, fullname) => {
                let removed = state.0.peers.lock().unwrap().remove(&fullname).is_some();
                if removed {
                    tracing::info!(target: "dmx::presence", peer = %fullname, "instance left the network");
                    let _ = app.emit(PEERS_EVENT, state.peers_info());
                }
            }
            _ => {}
        }
    }
    Ok(())
}
