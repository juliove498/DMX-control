//! Tee Tauri events to every connected WebSocket client. The bridge
//! listens to the same emit calls the desktop frontend already
//! consumes (`show:updated`, `engine:stats`, the new `scene:active_
//! changed` / `engine:master_changed` / `programmer:changed`) and
//! republishes them as a tagged-union JSON message over WS.
//!
//! `engine:stats` is throttled — it fires at the engine's frame rate
//! (~30 Hz) and we don't want to burn radio on a phone for it. 5 Hz is
//! enough to keep the FPS readout responsive without saturating low-
//! quality WiFi.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Listener};
use tokio::sync::broadcast;
use ts_rs::TS;

use crate::commands::{
    BlindChange, MasterChange, SceneActiveChange, BLIND_EVENT, MASTER_EVENT, PROGRAMMER_EVENT,
    SCENE_ACTIVE_EVENT, SHOW_EVENT, STATS_EVENT,
};
use crate::engine::EngineStats;
use crate::programmer::ProgrammerStatus;

const STATS_THROTTLE: Duration = Duration::from_millis(200);
const CHANNEL_CAPACITY: usize = 64;

/// Tagged union of every push event the bridge sends to mobile clients.
/// Matches `BridgeEvent` on the JS side. Adding a variant here means
/// clients can ignore it (forward-compat) — never *remove* a variant
/// without bumping a protocol version.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeEvent {
    ShowUpdated,
    SceneActiveChanged(SceneActiveChange),
    MasterChanged(MasterChange),
    BlindChanged(BlindChange),
    ProgrammerChanged(ProgrammerStatus),
    EngineStats(EngineStats),
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<BridgeEvent>,
    last_stats_at: Arc<Mutex<Instant>>,
    /// Last EngineStats payload we forwarded — used as the seed value
    /// in `InitialState` so a fresh client doesn't need to wait up to
    /// 200 ms for the first stats event to render an FPS readout.
    cached_stats: Arc<Mutex<Option<EngineStats>>>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            tx,
            // Set to one throttle window in the past so the very first
            // stats event always gets through.
            last_stats_at: Arc::new(Mutex::new(Instant::now() - STATS_THROTTLE)),
            cached_stats: Arc::new(Mutex::new(None)),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BridgeEvent> {
        self.tx.subscribe()
    }

    pub fn client_count(&self) -> usize {
        self.tx.receiver_count()
    }

    pub fn last_stats(&self) -> Option<EngineStats> {
        *self.cached_stats.lock()
    }

    fn send(&self, ev: BridgeEvent) {
        // `send` only errors when there are zero receivers. That's
        // expected when no client is connected; nothing to log.
        let _ = self.tx.send(ev);
    }
}

/// Wire the Tauri event bus into our broadcast channel. Returns the
/// listener handles so the caller can keep them alive (dropping them
/// would unsubscribe).
pub struct EventBridge {
    pub handles: Vec<tauri::EventId>,
}

pub fn install_listeners(app: &AppHandle, bus: Arc<EventBus>) -> EventBridge {
    let mut handles = Vec::new();

    // show:updated → opaque ping. The mobile client refetches the
    // snapshot via RPC. Cheap and reuses code paths the desktop
    // already uses on every show mutation.
    {
        let bus = bus.clone();
        let id = app.listen(SHOW_EVENT, move |_| bus.send(BridgeEvent::ShowUpdated));
        handles.push(id);
    }

    {
        let bus = bus.clone();
        let id = app.listen(SCENE_ACTIVE_EVENT, move |evt| {
            if let Ok(payload) = serde_json::from_str::<SceneActiveChange>(evt.payload()) {
                bus.send(BridgeEvent::SceneActiveChanged(payload));
            }
        });
        handles.push(id);
    }

    {
        let bus = bus.clone();
        let id = app.listen(MASTER_EVENT, move |evt| {
            if let Ok(payload) = serde_json::from_str::<MasterChange>(evt.payload()) {
                bus.send(BridgeEvent::MasterChanged(payload));
            }
        });
        handles.push(id);
    }

    {
        let bus = bus.clone();
        let id = app.listen(BLIND_EVENT, move |evt| {
            if let Ok(payload) = serde_json::from_str::<BlindChange>(evt.payload()) {
                bus.send(BridgeEvent::BlindChanged(payload));
            }
        });
        handles.push(id);
    }

    {
        let bus = bus.clone();
        let id = app.listen(PROGRAMMER_EVENT, move |evt| {
            if let Ok(payload) = serde_json::from_str::<ProgrammerStatus>(evt.payload()) {
                bus.send(BridgeEvent::ProgrammerChanged(payload));
            }
        });
        handles.push(id);
    }

    {
        let bus = bus.clone();
        let id = app.listen(STATS_EVENT, move |evt| {
            // Throttle: stats fire ~30 Hz; mobile cares about FPS as
            // a "is this thing alive" indicator, not a graph. We
            // *do* cache the latest payload outside the throttle so
            // `InitialState` can seed clients with current values.
            if let Ok(payload) = serde_json::from_str::<EngineStats>(evt.payload()) {
                *bus.cached_stats.lock() = Some(payload);
                let mut last = bus.last_stats_at.lock();
                if last.elapsed() < STATS_THROTTLE {
                    return;
                }
                *last = Instant::now();
                drop(last);
                bus.send(BridgeEvent::EngineStats(payload));
            }
        });
        handles.push(id);
    }

    EventBridge { handles }
}

pub fn uninstall_listeners(app: &AppHandle, bridge: EventBridge) {
    for id in bridge.handles {
        app.unlisten(id);
    }
}
