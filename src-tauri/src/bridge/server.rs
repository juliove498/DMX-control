//! axum + tokio-tungstenite server for the mobile remote bridge.
//!
//! Two REST endpoints (`/health`, `/pair`) and one WebSocket
//! (`/ws`). The WS multiplexes JSON-RPC requests + initial-state
//! snapshot + tagged-union events. Bound to `0.0.0.0` so the LAN
//! reaches us, but the connection guard rejects peers whose IP isn't
//! in an RFC1918 / link-local range — the operator gets a yes/no
//! choice, not "your rig is on the open internet" by accident.
//!
//! Cross-platform notes:
//! - macOS: works as-is, mDNSResponder publishes our service.
//! - Windows: the first launch typically pops a Firewall prompt for
//!   incoming connections on the chosen port. The user has to accept
//!   "Private networks" at minimum. We don't try to suppress that
//!   prompt — it's the right ask.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;

use super::auth::{
    constant_time_eq, generate_token, hash_token, new_paired_device, now_secs, save_devices,
    PairedDevice, PendingPairing,
};
use super::events::{install_listeners, uninstall_listeners, BridgeEvent, EventBridge, EventBus};
use super::rpc::{build_initial_state, dispatch, RpcContext, RpcRequest, RpcResponse};

/// Server handle the bridge module hands back to the Tauri command
/// layer. `shutdown_tx` is a oneshot channel; sending on it tears down
/// the axum task. We can't just `task.abort()` because that wouldn't
/// give axum a chance to drop its socket cleanly — and a dirty socket
/// on Windows can leave the port in TIME_WAIT for 30 s, which makes
/// rapid Stop/Start cycles fail.
pub struct ServerHandle {
    pub port: u16,
    pub shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    pub task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub event_bridge: Mutex<Option<EventBridge>>,
}

impl ServerHandle {
    pub async fn shutdown(&self, app: &tauri::AppHandle) {
        if let Some(tx) = self.shutdown_tx.lock().take() {
            let _ = tx.send(());
        }
        let task = self.task.lock().take();
        if let Some(task) = task {
            // Brief grace period; if the task wedges we abort.
            let _ = tokio::time::timeout(std::time::Duration::from_millis(500), task).await;
        }
        if let Some(bridge) = self.event_bridge.lock().take() {
            uninstall_listeners(app, bridge);
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub ctx: Arc<RpcContext>,
    pub bus: Arc<EventBus>,
    pub pending: Arc<Mutex<Option<PendingPairing>>>,
    pub devices: Arc<Mutex<Vec<PairedDevice>>>,
}

pub async fn start(
    bind_addr: SocketAddr,
    ctx: Arc<RpcContext>,
    bus: Arc<EventBus>,
    devices: Arc<Mutex<Vec<PairedDevice>>>,
    pending: Arc<Mutex<Option<PendingPairing>>>,
) -> Result<(SocketAddr, ServerHandle), String> {
    let state = AppState {
        ctx: ctx.clone(),
        bus: bus.clone(),
        pending,
        devices,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/pair", post(pair))
        .route("/ws", get(ws_upgrade))
        .with_state(state)
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(|e| format!("bind {bind_addr} failed: {e}"))?;
    let local = listener
        .local_addr()
        .map_err(|e| format!("local_addr failed: {e}"))?;

    let event_bridge = install_listeners(&ctx.app, bus.clone());

    let (tx, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let server = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            let _ = rx.await;
        });
        if let Err(e) = server.await {
            tracing::warn!(error = %e, "bridge axum server exited with error");
        }
    });

    let handle = ServerHandle {
        port: local.port(),
        shutdown_tx: Mutex::new(Some(tx)),
        task: Mutex::new(Some(task)),
        event_bridge: Mutex::new(Some(event_bridge)),
    };
    Ok((local, handle))
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    version: &'static str,
}

async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok",
        service: "dmx-control-bridge",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[derive(Deserialize)]
struct PairRequest {
    pin: String,
    #[serde(default)]
    device_name: Option<String>,
}

#[derive(Serialize)]
struct PairResponse {
    token: String,
    device_id: String,
    device_name: String,
}

async fn pair(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<PairRequest>,
) -> Result<Json<PairResponse>, (StatusCode, String)> {
    if !is_lan_peer(peer.ip()) {
        tracing::warn!(%peer, "pair rejected: peer is not on a private network");
        return Err((
            StatusCode::FORBIDDEN,
            "peer must be on private network".into(),
        ));
    }
    let pending = {
        let mut g = state.pending.lock();
        match g.as_ref() {
            Some(p) if !p.is_expired() => p.clone(),
            _ => {
                *g = None;
                return Err((StatusCode::GONE, "no active pairing window".into()));
            }
        }
    };
    if !constant_time_eq(pending.pin.as_bytes(), body.pin.as_bytes()) {
        return Err((StatusCode::UNAUTHORIZED, "invalid pin".into()));
    }
    let token = generate_token();
    let token_hash = hash_token(&token);
    let name = body
        .device_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "iPhone".to_string());
    let device = new_paired_device(name, token_hash);
    {
        let mut devs = state.devices.lock();
        devs.push(device.clone());
        if let Err(e) = save_devices(&devs) {
            tracing::warn!(error = %e, "failed to persist devices.json");
        }
    }
    // One-shot: a successful pair consumes the PIN window.
    *state.pending.lock() = None;
    Ok(Json(PairResponse {
        token,
        device_id: device.id,
        device_name: device.name,
    }))
}

#[derive(Deserialize)]
struct WsQuery {
    /// Optional fallback for clients that can't set headers (e.g. very
    /// old WS libraries). Most clients should send `Authorization:
    /// Bearer <token>` instead — but Expo's WebSocket polyfill is one
    /// of the ones that can't, so we accept either.
    #[serde(default)]
    token: Option<String>,
}

async fn ws_upgrade(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Query(q): Query<WsQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if !is_lan_peer(peer.ip()) {
        tracing::warn!(%peer, "ws rejected: peer is not on a private network");
        return (StatusCode::FORBIDDEN, "peer must be on private network").into_response();
    }
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer ").map(str::to_string))
        .or(q.token);
    let Some(token) = token else {
        return (StatusCode::UNAUTHORIZED, "missing token").into_response();
    };
    let hash = hash_token(&token);
    let matched_device = {
        let mut devs = state.devices.lock();
        let now = now_secs();
        let pos = devs
            .iter()
            .position(|d| constant_time_eq(d.token_hash.as_bytes(), hash.as_bytes()));
        if let Some(idx) = pos {
            devs[idx].last_seen = Some(now);
            let snap = devs[idx].clone();
            // Best-effort persist; we don't block auth on disk I/O.
            let copy = devs.clone();
            tokio::task::spawn_blocking(move || {
                if let Err(e) = save_devices(&copy) {
                    tracing::warn!(error = %e, "failed to persist last_seen");
                }
            });
            Some(snap)
        } else {
            None
        }
    };
    let Some(device) = matched_device else {
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    };

    tracing::info!(%peer, device = %device.name, "bridge ws authorized");
    ws.on_upgrade(move |socket| handle_socket(socket, state, device))
}

async fn handle_socket(socket: WebSocket, state: AppState, device: PairedDevice) {
    let (mut sender, mut receiver) = socket.split();
    use futures_util::{SinkExt, StreamExt};

    // Push initial state immediately. The client renders this
    // synchronously on connect; otherwise it would have to call N
    // RPCs and stitch them together with no obvious "we're ready"
    // moment.
    let initial = build_initial_state(&state.ctx);
    let initial_msg = serde_json::json!({
        "type": "initial_state",
        "data": initial,
    });
    if let Ok(text) = serde_json::to_string(&initial_msg) {
        if sender.send(Message::Text(text)).await.is_err() {
            return;
        }
    }

    let mut bus_rx = state.bus.subscribe();
    let ctx = state.ctx.clone();

    // Two-pump loop: forward broadcast events to the socket, and
    // parse incoming RPC frames from the socket. `tokio::select!`
    // wakes whichever side fires first; whichever returns None / Err
    // terminates the connection.
    loop {
        tokio::select! {
            ev = bus_rx.recv() => {
                match ev {
                    Ok(ev) => {
                        if let Ok(text) = encode_event(&ev) {
                            if sender.send(Message::Text(text)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(device = %device.name, lagged = n, "bridge ws lagged; client may miss events");
                    }
                    Err(_) => break,
                }
            }
            msg = receiver.next() => {
                let Some(Ok(msg)) = msg else { break; };
                match msg {
                    Message::Text(text) => {
                        let resp = match serde_json::from_str::<RpcRequest>(&text) {
                            Ok(req) => dispatch(&ctx, req),
                            Err(e) => RpcResponse::err(None, -32700, format!("parse error: {e}")),
                        };
                        if let Ok(text) = serde_json::to_string(&resp) {
                            if sender.send(Message::Text(text)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Message::Ping(p) => {
                        let _ = sender.send(Message::Pong(p)).await;
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }
    tracing::info!(device = %device.name, "bridge ws closed");
}

fn encode_event(ev: &BridgeEvent) -> Result<String, serde_json::Error> {
    let envelope = serde_json::json!({
        "type": "event",
        "data": ev,
    });
    serde_json::to_string(&envelope)
}

/// Reject anything that isn't on the user's LAN. Phase 1 has no TLS
/// and no rate limiting, so accepting a public-IP peer would be
/// "open phone control of the rig to anyone who can reach the
/// machine". Private + link-local + loopback only.
pub fn is_lan_peer(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_private_v4(v4) || v4.is_loopback() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || matches!(
                    v6.octets()[0],
                    // fc00::/7 unique local + fe80::/10 link-local prefix.
                    0xfc | 0xfd
                )
                || (v6.octets()[0] == 0xfe && (v6.octets()[1] & 0xc0) == 0x80)
        }
    }
}

fn is_private_v4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    // RFC1918: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16.
    octets[0] == 10
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lan_peer_filter() {
        assert!(is_lan_peer("10.0.0.1".parse().unwrap()));
        assert!(is_lan_peer("192.168.1.42".parse().unwrap()));
        assert!(is_lan_peer("172.20.5.1".parse().unwrap()));
        assert!(is_lan_peer("127.0.0.1".parse().unwrap()));
        assert!(is_lan_peer("169.254.10.10".parse().unwrap()));
        assert!(!is_lan_peer("8.8.8.8".parse().unwrap()));
        assert!(!is_lan_peer("172.32.0.1".parse().unwrap()));
    }
}
