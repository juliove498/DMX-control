//! Mobile remote bridge: tauri commands + lifecycle.
//!
//! Responsible for:
//! - starting/stopping the axum HTTP+WS server
//! - publishing the service over mDNS
//! - generating pairing PINs + QR codes
//! - persisting paired devices in `devices.json`
//!
//! The bridge does NOT reimplement engine logic — every RPC method
//! the phone can call delegates to the same Rust functions the desktop
//! `#[tauri::command]` set already exercises.

pub mod auth;
pub mod discovery;
pub mod events;
pub mod rpc;
pub mod server;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::engine::output_thread::{SharedChasers, SharedGlobals, SharedMovement};
use crate::engine::scene_playback::SharedScenePlayback;
use crate::engine::EngineState;
use crate::programmer::SharedProgrammer;
use crate::show::ShowState;

use auth::{
    generate_pin, load_devices, save_devices, PairedDevice, PendingPairing,
};

const DEFAULT_PORT: u16 = 7878;

/// Build a minimal SVG out of a `QrCode`. qrcodegen ships the encoder
/// but no renderer — the official crate's demo file has the same
/// snippet; we inline a slightly tighter version here so we don't
/// have to ship a separate SVG library just for one tag.
fn qr_to_svg(qr: &qrcodegen::QrCode, border: i32) -> String {
    let size = qr.size() + border * 2;
    let mut path = String::new();
    for y in 0..qr.size() {
        for x in 0..qr.size() {
            if qr.get_module(x, y) {
                if !path.is_empty() {
                    path.push(' ');
                }
                path.push_str(&format!("M{},{}h1v1h-1z", x + border, y + border));
            }
        }
    }
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {size} {size}" stroke="none"><rect width="100%" height="100%" fill="#fff"/><path d="{path}" fill="#000"/></svg>"##,
        size = size,
        path = path,
    )
}

/// Public-facing snapshot of the bridge's runtime state. Drives the
/// "Remote" tab in the desktop UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct BridgeStatus {
    pub running: bool,
    pub host_ip: Option<String>,
    pub port: Option<u16>,
    pub clients_connected: usize,
    pub paired_devices: usize,
    pub pairing: Option<PairingState>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct PairingState {
    pub pin: String,
    /// SVG markup for the QR. The frontend can drop this straight
    /// into a <div dangerouslySetInnerHTML> — saves shipping a JS
    /// QR library and keeps the source of truth in Rust.
    pub qr_svg: String,
    pub remaining_secs: u64,
    /// Pre-built `dmxctrl://pair?...` URL used as the QR payload —
    /// also handy for "share with iMessage" debugging.
    pub pair_url: String,
}

pub struct BridgeStateInner {
    pub server: Option<server::ServerHandle>,
    pub bus: Arc<events::EventBus>,
    pub devices: Arc<Mutex<Vec<PairedDevice>>>,
    pub pending: Arc<Mutex<Option<PendingPairing>>>,
    pub discovery: discovery::SharedDiscovery,
    pub host_ip: Option<String>,
    pub last_error: Option<String>,
}

impl Default for BridgeStateInner {
    fn default() -> Self {
        Self {
            server: None,
            bus: Arc::new(events::EventBus::new()),
            devices: Arc::new(Mutex::new(load_devices())),
            pending: Arc::new(Mutex::new(None)),
            discovery: discovery::shared_discovery(),
            host_ip: None,
            last_error: None,
        }
    }
}

#[derive(Clone, Default)]
pub struct BridgeState(pub Arc<Mutex<BridgeStateInner>>);

impl BridgeState {
    pub fn new() -> Self {
        Self::default()
    }
}

fn snapshot_status(inner: &BridgeStateInner) -> BridgeStatus {
    let port = inner.server.as_ref().map(|s| s.port);
    let pairing = inner.pending.lock().as_ref().and_then(|p| {
        if p.is_expired() {
            None
        } else {
            let pair_url = format!(
                "dmxctrl://pair?host={}&port={}&pin={}",
                inner.host_ip.clone().unwrap_or_else(|| "0.0.0.0".into()),
                port.unwrap_or(DEFAULT_PORT),
                p.pin,
            );
            let qr = qrcodegen::QrCode::encode_text(
                &pair_url,
                qrcodegen::QrCodeEcc::Medium,
            )
            .map(|c| qr_to_svg(&c, 2))
            .unwrap_or_default();
            Some(PairingState {
                pin: p.pin.clone(),
                qr_svg: qr,
                remaining_secs: p.remaining_secs(),
                pair_url,
            })
        }
    });
    BridgeStatus {
        running: inner.server.is_some(),
        host_ip: inner.host_ip.clone(),
        port,
        clients_connected: inner.bus.client_count(),
        paired_devices: inner.devices.lock().len(),
        pairing,
        last_error: inner.last_error.clone(),
    }
}

// ---- Tauri commands ------------------------------------------------------

#[tauri::command]
pub async fn bridge_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, BridgeState>,
    engine: tauri::State<'_, EngineState>,
    show: tauri::State<'_, ShowState>,
    chasers: tauri::State<'_, SharedChasers>,
    movement: tauri::State<'_, SharedMovement>,
    globals: tauri::State<'_, SharedGlobals>,
    scenes_pb: tauri::State<'_, SharedScenePlayback>,
    programmer: tauri::State<'_, SharedProgrammer>,
) -> Result<BridgeStatus, String> {
    // Snapshot the bits we need from the BridgeState lock and drop it
    // *before* awaiting the server start — `parking_lot::Mutex` isn't
    // Send across .await points (its guard isn't either), and holding
    // it would also let the UI deadlock if it polled status while
    // bind() was blocking on a bad interface.
    let (already_running, bus, devices, pending, discovery_handle) = {
        let inner = state.0.lock();
        (
            inner.server.is_some(),
            inner.bus.clone(),
            inner.devices.clone(),
            inner.pending.clone(),
            inner.discovery.clone(),
        )
    };
    if already_running {
        return Ok(snapshot_status(&state.0.lock()));
    }

    let ctx = Arc::new(rpc::RpcContext {
        app: app.clone(),
        engine: engine.inner().clone(),
        show: show.inner().clone(),
        chasers: chasers.inner().clone(),
        movement: movement.inner().clone(),
        globals: globals.inner().clone(),
        scenes_pb: scenes_pb.inner().clone(),
        programmer: programmer.inner().clone(),
        bus: bus.clone(),
    });

    let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_PORT);
    let started = server::start(bind, ctx, bus, devices, pending).await;
    match started {
        Ok((local, handle)) => {
            let host_ip = discovery::pick_advertise_ip();
            // mDNS instance name has to be unique on the LAN — host
            // name + a short suffix is enough; if two desktops on the
            // same WiFi run the bridge, the second one's record gets
            // a numeric tail anyway.
            // Instance name shows up on the phone's "available
            // bridges" list; "DMX Control" is the discriminator most
            // users will recognise. If two desktops share the LAN
            // mdns-sd auto-suffixes the second one with " (2)", which
            // is fine for now — multi-desktop is well outside the MVP.
            let instance = "DMX Control".to_string();
            if let Err(e) = discovery_handle.start(local.port(), &host_ip, &instance) {
                tracing::warn!(error = %e, "mDNS advertise failed; manual IP entry still works");
            }
            let mut inner = state.0.lock();
            inner.server = Some(handle);
            inner.host_ip = Some(host_ip);
            inner.last_error = None;
            tracing::info!(
                port = local.port(),
                host_ip = inner.host_ip.as_deref(),
                "bridge started"
            );
            Ok(snapshot_status(&inner))
        }
        Err(e) => {
            let mut inner = state.0.lock();
            inner.last_error = Some(e.clone());
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn bridge_stop(
    app: tauri::AppHandle,
    state: tauri::State<'_, BridgeState>,
) -> Result<BridgeStatus, String> {
    let (server_opt, discovery_handle) = {
        let mut inner = state.0.lock();
        (inner.server.take(), inner.discovery.clone())
    };
    if let Some(handle) = server_opt {
        handle.shutdown(&app).await;
    }
    discovery_handle.stop();
    let mut inner = state.0.lock();
    inner.host_ip = None;
    Ok(snapshot_status(&inner))
}

#[tauri::command]
pub fn bridge_status(state: tauri::State<'_, BridgeState>) -> BridgeStatus {
    snapshot_status(&state.0.lock())
}

#[tauri::command]
pub fn bridge_begin_pairing(state: tauri::State<'_, BridgeState>) -> BridgeStatus {
    let pin = generate_pin();
    {
        let inner = state.0.lock();
        *inner.pending.lock() = Some(PendingPairing::new(pin.clone()));
    }
    tracing::info!(%pin, "bridge pairing window opened");
    snapshot_status(&state.0.lock())
}

#[tauri::command]
pub fn bridge_cancel_pairing(state: tauri::State<'_, BridgeState>) -> BridgeStatus {
    {
        let inner = state.0.lock();
        *inner.pending.lock() = None;
    }
    snapshot_status(&state.0.lock())
}

#[tauri::command]
pub fn bridge_list_devices(state: tauri::State<'_, BridgeState>) -> Vec<PairedDevice> {
    state.0.lock().devices.lock().clone()
}

#[tauri::command]
pub fn bridge_revoke_device(
    state: tauri::State<'_, BridgeState>,
    device_id: String,
) -> Result<BridgeStatus, String> {
    let inner = state.0.lock();
    let mut devs = inner.devices.lock();
    let before = devs.len();
    devs.retain(|d| d.id != device_id);
    if devs.len() == before {
        return Err(format!("device {device_id} not found"));
    }
    save_devices(&devs).map_err(|e| e.to_string())?;
    drop(devs);
    Ok(snapshot_status(&inner))
}
