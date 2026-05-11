//! JSON-RPC 2.0 dispatcher used by the mobile remote.
//!
//! Hard rule: the dispatcher is a literal `match` against a whitelist
//! of method names. The `#[tauri::command]` set in [crate::commands]
//! intentionally exposes write-everything (delete fixtures, edit
//! scenes, run AI, blow away the universe) — that's fine for the
//! desktop UI but is a footgun the moment a phone in the green room
//! can hit it. So we lift only the GO/RELEASE/master/blackout/blind/
//! snapshot subset by name. Anything not on the list returns a
//! "method not found" error without ever touching engine state.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::commands::{BlindChange, MasterChange, BLIND_EVENT, MASTER_EVENT};
use crate::engine::output_thread::{SharedChasers, SharedGlobals, SharedMovement};
use crate::engine::scene_playback::SharedScenePlayback;
use crate::engine::EngineState;
use crate::globals::GlobalsConfig;
use crate::programmer::SharedProgrammer;
use crate::show::ShowState;

/// Methods the mobile client is allowed to invoke. The desktop UI
/// keeps using the existing `#[tauri::command]` set; this is the
/// subset we deem safe over the LAN.
pub const ALLOWED_METHODS: &[&str] = &[
    "list_scenes",
    "recall_scene",
    "release_scene",
    "active_scene_id",
    "active_scene_step",
    "get_globals",
    "set_master",
    "set_blackout",
    "set_blind",
    "set_chaser_enabled",
    "set_chaser_master",
    "get_show",
    "programmer_status",
    "get_engine_stats",
];

#[derive(Debug, Clone, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl RpcResponse {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }
    pub fn err(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

/// Bundle of references the dispatcher needs. We grab strong handles
/// once at server start and reuse them per request — `tauri::State`
/// can't cross the axum boundary cleanly because state is keyed by
/// type at the AppHandle level and the WS handler doesn't have one.
#[derive(Clone)]
pub struct RpcContext {
    pub app: tauri::AppHandle,
    pub engine: EngineState,
    pub show: ShowState,
    pub chasers: SharedChasers,
    pub movement: SharedMovement,
    pub globals: SharedGlobals,
    pub scenes_pb: SharedScenePlayback,
    pub programmer: SharedProgrammer,
    pub bus: Arc<super::events::EventBus>,
}

pub fn dispatch(ctx: &Arc<RpcContext>, req: RpcRequest) -> RpcResponse {
    if req.jsonrpc != "2.0" {
        return RpcResponse::err(req.id, -32600, "invalid jsonrpc version");
    }
    if !ALLOWED_METHODS.contains(&req.method.as_str()) {
        return RpcResponse::err(
            req.id,
            -32601,
            format!("method not allowed: {}", req.method),
        );
    }
    match req.method.as_str() {
        "list_scenes" => {
            let scenes = ctx.show.read().show.scenes.clone();
            RpcResponse::ok(req.id, serde_json::to_value(scenes).unwrap_or(Value::Null))
        }
        "active_scene_id" => {
            let id = ctx
                .scenes_pb
                .lock()
                .active_scene_id()
                .map(|s| s.to_string());
            RpcResponse::ok(req.id, json!(id))
        }
        "active_scene_step" => {
            let step = ctx.scenes_pb.lock().current_step_index().map(|i| i as u32);
            RpcResponse::ok(req.id, json!(step))
        }
        "recall_scene" => {
            let id = match req.params.get("id").and_then(Value::as_str) {
                Some(s) => s.to_string(),
                None => {
                    return RpcResponse::err(req.id, -32602, "missing param 'id'");
                }
            };
            match crate::commands::recall_scene_impl(
                &ctx.app,
                &ctx.engine,
                &ctx.show,
                &ctx.chasers,
                &ctx.movement,
                &ctx.scenes_pb,
                &id,
            ) {
                Ok(()) => RpcResponse::ok(req.id, json!(true)),
                Err(e) => RpcResponse::err(req.id, -32000, e.to_string()),
            }
        }
        "release_scene" => {
            ctx.scenes_pb.lock().release(std::time::Instant::now());
            // Mirror what the command does so subscribers see the
            // change without having to poll.
            let _ = tauri::Emitter::emit(
                &ctx.app,
                crate::commands::SCENE_ACTIVE_EVENT,
                crate::commands::SceneActiveChange {
                    active_scene_id: None,
                    step_index: None,
                },
            );
            RpcResponse::ok(req.id, json!(true))
        }
        "get_globals" => {
            let g = ctx.show.read().show.globals.clone();
            RpcResponse::ok(req.id, serde_json::to_value(g).unwrap_or(Value::Null))
        }
        "set_master" => {
            let value = match req.params.get("value").and_then(Value::as_u64) {
                Some(v) if v <= 255 => v as u8,
                _ => return RpcResponse::err(req.id, -32602, "param 'value' must be 0..=255"),
            };
            ctx.engine.write().master = value;
            let _ = tauri::Emitter::emit(&ctx.app, MASTER_EVENT, MasterChange { master: value });
            RpcResponse::ok(req.id, json!(true))
        }
        "set_blackout" => {
            let on = match req.params.get("on").and_then(Value::as_bool) {
                Some(b) => b,
                None => return RpcResponse::err(req.id, -32602, "param 'on' must be bool"),
            };
            match crate::commands::set_blackout_impl(&ctx.app, &ctx.show, &ctx.globals, on) {
                Ok(()) => RpcResponse::ok(req.id, json!(true)),
                Err(e) => RpcResponse::err(req.id, -32000, e.to_string()),
            }
        }
        "set_blind" => {
            let pressed = match req.params.get("pressed").and_then(Value::as_bool) {
                Some(b) => b,
                None => return RpcResponse::err(req.id, -32602, "param 'pressed' must be bool"),
            };
            ctx.globals.lock().set_blind(pressed);
            let _ = tauri::Emitter::emit(&ctx.app, BLIND_EVENT, BlindChange { pressed });
            RpcResponse::ok(req.id, json!(true))
        }
        "set_chaser_enabled" => {
            let id = match req.params.get("id").and_then(Value::as_str) {
                Some(s) => s.to_string(),
                None => return RpcResponse::err(req.id, -32602, "missing param 'id'"),
            };
            let enabled = match req.params.get("enabled").and_then(Value::as_bool) {
                Some(b) => b,
                None => return RpcResponse::err(req.id, -32602, "param 'enabled' must be bool"),
            };
            match crate::commands::toggle_chaser_impl(
                &ctx.app,
                &ctx.show,
                &ctx.chasers,
                &id,
                enabled,
            ) {
                Ok(()) => RpcResponse::ok(req.id, json!(true)),
                Err(e) => RpcResponse::err(req.id, -32000, e.to_string()),
            }
        }
        "set_chaser_master" => {
            let id = match req.params.get("id").and_then(Value::as_str) {
                Some(s) => s.to_string(),
                None => return RpcResponse::err(req.id, -32602, "missing param 'id'"),
            };
            let value = match req.params.get("value").and_then(Value::as_f64) {
                Some(v) => v as f32,
                None => return RpcResponse::err(req.id, -32602, "param 'value' must be a number"),
            };
            match crate::commands::set_chaser_master_impl(
                &ctx.app,
                &ctx.show,
                &ctx.chasers,
                &id,
                value,
            ) {
                Ok(()) => RpcResponse::ok(req.id, json!(true)),
                Err(e) => RpcResponse::err(req.id, -32000, e.to_string()),
            }
        }
        "get_show" => {
            let show = ctx.show.read().show.clone();
            RpcResponse::ok(req.id, serde_json::to_value(show).unwrap_or(Value::Null))
        }
        "programmer_status" => {
            let status = ctx.programmer.lock().snapshot();
            RpcResponse::ok(req.id, serde_json::to_value(status).unwrap_or(Value::Null))
        }
        "get_engine_stats" => {
            // The engine emits stats from its output thread; we cache
            // the most recent payload in the event bus so RPC callers
            // can read it without a private accessor on EngineInner.
            // Returns null if no stats event has fired yet (engine
            // booting or paused).
            let stats = ctx.bus.last_stats();
            RpcResponse::ok(req.id, serde_json::to_value(stats).unwrap_or(Value::Null))
        }
        // unreachable: we already checked ALLOWED_METHODS above.
        _ => RpcResponse::err(req.id, -32601, "method not allowed"),
    }
}

/// Snapshot pushed to the client right after auth. Bundles every piece
/// of state the mobile screens need to render their first frame
/// without making N separate RPC calls. After this, only deltas via
/// `BridgeEvent`.
#[derive(Debug, Clone, Serialize)]
pub struct InitialState {
    pub show: crate::show::ShowFileV1,
    pub master: u8,
    pub globals: GlobalsConfig,
    pub active_scene_id: Option<String>,
    pub active_scene_step: Option<u32>,
    pub programmer: crate::programmer::ProgrammerStatus,
    pub engine_stats: Option<crate::engine::EngineStats>,
}

pub fn build_initial_state(ctx: &RpcContext) -> InitialState {
    let show = ctx.show.read().show.clone();
    let master = ctx.engine.read().master;
    let active_scene_id = ctx
        .scenes_pb
        .lock()
        .active_scene_id()
        .map(|s| s.to_string());
    let active_scene_step = ctx.scenes_pb.lock().current_step_index().map(|i| i as u32);
    InitialState {
        show: show.clone(),
        master,
        globals: show.globals,
        active_scene_id,
        active_scene_step,
        programmer: ctx.programmer.lock().snapshot(),
        engine_stats: ctx.bus.last_stats(),
    }
}
