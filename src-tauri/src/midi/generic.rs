//! Generic "MIDI learn" layer: arbitrary note/CC controls from any
//! controller connected through the hub, mapped to actions or to the
//! grand master. Lives alongside the Launchpad surface router — the
//! hub calls this dispatcher for every incoming message, always.
//!
//! Learn flow: the UI arms capture (`MidiLearnState.armed`), the next
//! note-on / CC seen on the wire is stored (and NOT dispatched), the
//! UI polls it out and creates a `GenericMidiBinding`. Dispatch after
//! that is plain lookup by (kind, channel, data1).
//!
//! The action match below deliberately mirrors the Launchpad
//! controller's dispatch — same `*_impl` free functions, same
//! semantics (toggles flip, scenes release on re-press, blind is
//! momentary).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use ts_rs::TS;

use crate::engine::loop_playback::SharedLoopPlayback;
use crate::engine::output_thread::{SharedChasers, SharedGlobals, SharedMovement};
use crate::engine::scene_playback::SharedScenePlayback;
use crate::engine::EngineState;
use crate::midi::hub::SharedMidi;
use crate::midi::MidiMessage;
use crate::show::button_bindings::{ButtonAction, GenericMidiBinding, MidiControlTarget};
use crate::show::ShowState;
use crate::snapshot::SharedSnapshotRuntime;

/// A control captured by learn mode, ready for the UI to bind.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct LearnedControl {
    pub is_cc: bool,
    pub channel: u8,
    pub data1: u8,
    /// Human-readable summary ("CC 21 · ch 1") for the UI chip.
    pub description: String,
}

#[derive(Default)]
pub struct MidiLearnState {
    armed: bool,
    captured: Option<LearnedControl>,
}

pub type SharedMidiLearn = Arc<Mutex<MidiLearnState>>;

pub fn shared_midi_learn() -> SharedMidiLearn {
    Arc::new(Mutex::new(MidiLearnState::default()))
}

pub fn learn_arm(state: &SharedMidiLearn) {
    let mut s = state.lock();
    s.armed = true;
    s.captured = None;
}

pub fn learn_cancel(state: &SharedMidiLearn) {
    let mut s = state.lock();
    s.armed = false;
    s.captured = None;
}

/// Returns the captured control once, then clears it.
pub fn learn_poll(state: &SharedMidiLearn) -> Option<LearnedControl> {
    state.lock().captured.take()
}

#[derive(Clone)]
pub struct GenericHandles {
    pub app: AppHandle,
    pub chasers: SharedChasers,
    pub movement: SharedMovement,
    pub globals: SharedGlobals,
    pub scenes: SharedScenePlayback,
    pub loops: SharedLoopPlayback,
    pub engine: EngineState,
    pub show: ShowState,
    pub snapshots: SharedSnapshotRuntime,
    /// Momentary-blind latch, mirroring the surface controllers.
    pub blind_held: Arc<AtomicBool>,
    /// Last seen value per CC control, for press-edge detection on
    /// button-style CCs (fire on crossing ≥64, not on every repeat).
    cc_last: Arc<Mutex<HashMap<(u8, u8), u8>>>,
}

impl GenericHandles {
    /// Standalone handle set for non-MIDI dispatchers (OS2L buttons,
    /// future remote surfaces) that want the exact same action
    /// semantics as the MIDI-learn layer. Fresh latches — the blind
    /// momentary state is tracked per surface.
    #[allow(clippy::too_many_arguments)]
    pub fn standalone(
        app: AppHandle,
        chasers: SharedChasers,
        movement: SharedMovement,
        globals: SharedGlobals,
        scenes: SharedScenePlayback,
        loops: SharedLoopPlayback,
        engine: EngineState,
        show: ShowState,
        snapshots: SharedSnapshotRuntime,
    ) -> Self {
        Self {
            app,
            chasers,
            movement,
            globals,
            scenes,
            loops,
            engine,
            show,
            snapshots,
            blind_held: Arc::new(AtomicBool::new(false)),
            cc_last: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn install(
    midi: &SharedMidi,
    learn: SharedMidiLearn,
    app: AppHandle,
    chasers: SharedChasers,
    movement: SharedMovement,
    globals: SharedGlobals,
    scenes: SharedScenePlayback,
    loops: SharedLoopPlayback,
    engine: EngineState,
    show: ShowState,
    snapshots: SharedSnapshotRuntime,
) {
    let handles = GenericHandles {
        app,
        chasers,
        movement,
        globals,
        scenes,
        loops,
        engine,
        show,
        snapshots,
        blind_held: Arc::new(AtomicBool::new(false)),
        cc_last: Arc::new(Mutex::new(HashMap::new())),
    };
    let router: crate::midi::hub::InputRouter = Arc::new(move |msg: &MidiMessage| {
        handle_message(msg, &learn, &handles);
    });
    midi.lock().set_generic_router(Some(router));
}

fn handle_message(msg: &MidiMessage, learn: &SharedMidiLearn, handles: &GenericHandles) {
    let high = msg.status & 0xF0;
    let channel = msg.status & 0x0F;
    let Some(data1) = msg.data1 else { return };
    let data2 = msg.data2.unwrap_or(0);
    let (is_cc, pressed, value) = match high {
        0x90 => (false, data2 > 0, data2),
        0x80 => (false, false, 0),
        0xB0 => (true, data2 >= 64, data2),
        _ => return,
    };

    // Learn capture swallows the event: while armed, nothing dispatches.
    {
        let mut l = learn.lock();
        if l.armed {
            // Only capture "activations" — a fader mid-sweep or a pad
            // press both qualify; note-off releases don't.
            if pressed || (is_cc && value > 0) {
                let kind = if is_cc { "CC" } else { "NOTE" };
                l.captured = Some(LearnedControl {
                    is_cc,
                    channel,
                    data1,
                    description: format!("{kind} {data1} · ch {}", channel + 1),
                });
                l.armed = false;
            }
            return;
        }
    }

    let bindings: Vec<GenericMidiBinding> = {
        let s = handles.show.read();
        s.show
            .button_bindings
            .generic
            .iter()
            .filter(|b| b.is_cc == is_cc && b.channel == channel && b.data1 == data1)
            .cloned()
            .collect()
    };
    if bindings.is_empty() {
        return;
    }

    // Press-edge detection for button-style CC targets: fire once when
    // the value crosses the threshold, re-arm when it drops below.
    let cc_edge_pressed = if is_cc {
        let mut last = handles.cc_last.lock();
        let prev = last.insert((channel, data1), value).unwrap_or(0);
        value >= 64 && prev < 64
    } else {
        pressed
    };

    for b in bindings {
        match &b.target {
            MidiControlTarget::Master => {
                let m = ((value as u16 * 255) / 127).min(255) as u8;
                handles.engine.write().master = m;
                let _ = tauri::Emitter::emit(
                    &handles.app,
                    crate::commands::MASTER_EVENT,
                    crate::commands::MasterChange { master: m },
                );
            }
            MidiControlTarget::Action { action } => {
                dispatch_action(action.clone(), cc_edge_pressed, pressed, handles);
            }
        }
    }
}

/// Mirrors `midi::launchpad::dispatch_action` — same impl helpers, same
/// semantics — but driven by learned bindings instead of pad notes.
/// `edge_pressed` is the debounced "fire now" edge; `raw_pressed` is
/// the level (used by momentary Blind, which needs the release too).
pub(crate) fn dispatch_action(
    action: ButtonAction,
    edge_pressed: bool,
    raw_pressed: bool,
    handles: &GenericHandles,
) {
    // Blind is the one momentary action: both press and release fire.
    if let ButtonAction::Blind = action {
        let was = handles.blind_held.swap(raw_pressed, Ordering::Relaxed);
        if was != raw_pressed {
            handles.globals.lock().set_blind(raw_pressed);
            let _ = tauri::Emitter::emit(
                &handles.app,
                crate::commands::BLIND_EVENT,
                crate::commands::BlindChange {
                    pressed: raw_pressed,
                },
            );
        }
        return;
    }
    if !edge_pressed {
        return;
    }
    // Modo Mágico steps aside when a human fires a look by hand.
    crate::autopilot::note_manual_action(&action);
    let resolved = {
        let s = handles.show.read();
        match &action {
            ButtonAction::ToggleChaserByIndex { index } => s
                .show
                .chasers
                .get(*index as usize)
                .map(|c| ButtonAction::ToggleChaser { id: c.id.clone() }),
            ButtonAction::ToggleMovementByIndex { index } => s
                .show
                .movements
                .get(*index as usize)
                .map(|m| ButtonAction::ToggleMovement { id: m.id.clone() }),
            ButtonAction::RecallSceneByIndex { index } => s
                .show
                .scenes
                .get(*index as usize)
                .map(|sc| ButtonAction::RecallScene { id: sc.id.clone() }),
            ButtonAction::StartLoopGroupByIndex { index } => s
                .show
                .scene_loop_groups
                .get(*index as usize)
                .map(|g| ButtonAction::StartLoopGroup { id: g.id.clone() }),
            ButtonAction::ToggleSnapshotByIndex { index } => s
                .show
                .snapshots
                .get(*index as usize)
                .map(|sn| ButtonAction::ToggleSnapshot { id: sn.id.clone() }),
            other => Some(other.clone()),
        }
    };
    let Some(action) = resolved else { return };
    match action {
        ButtonAction::None | ButtonAction::Blind => (),
        ButtonAction::ToggleAutopilot => crate::autopilot::toggle_from_action(&handles.app),
        ButtonAction::AutopilotEnergy { level } => {
            crate::autopilot::set_energy_from_action(&handles.app, level)
        }
        ButtonAction::AutopilotNudge { kind } => crate::autopilot::nudge(match kind {
            crate::show::button_bindings::AutopilotNudgeKind::Next => crate::autopilot::Nudge::Next,
            crate::show::button_bindings::AutopilotNudgeKind::Back => crate::autopilot::Nudge::Back,
        }),
        ButtonAction::AutopilotHold => crate::autopilot::toggle_hold(),
        ButtonAction::AutopilotBurst => crate::autopilot::burst_bar(),
        ButtonAction::AutopilotDrop => crate::autopilot::drop_next(),
        ButtonAction::AutopilotPalette { style } => {
            crate::autopilot::set_palette_from_action(&handles.app, style)
        }
        ButtonAction::AutopilotMovement => {
            crate::autopilot::toggle_movement_from_action(&handles.app)
        }
        ButtonAction::ToggleChaser { id } => {
            let desired = !handles
                .show
                .read()
                .show
                .chasers
                .iter()
                .any(|c| c.id == id && c.enabled);
            if let Err(err) = crate::commands::toggle_chaser_impl(
                &handles.app,
                &handles.show,
                &handles.chasers,
                &id,
                desired,
            ) {
                tracing::warn!(?err, "midi-learn toggle_chaser failed");
            }
        }
        ButtonAction::ToggleMovement { id } => {
            let desired = !handles
                .show
                .read()
                .show
                .movements
                .iter()
                .any(|m| m.id == id && m.enabled);
            if let Err(err) = crate::commands::toggle_movement_impl(
                &handles.app,
                &handles.show,
                &handles.movement,
                &id,
                desired,
            ) {
                tracing::warn!(?err, "midi-learn toggle_movement failed");
            }
        }
        ButtonAction::RecallScene { id } => {
            let active = handles
                .scenes
                .lock()
                .active_scene_id()
                .map(|s| s.to_string());
            if active.as_deref() == Some(id.as_str()) {
                handles.scenes.lock().release(std::time::Instant::now());
                let _ = tauri::Emitter::emit(
                    &handles.app,
                    crate::commands::SCENE_ACTIVE_EVENT,
                    crate::commands::SceneActiveChange {
                        active_scene_id: None,
                        step_index: None,
                    },
                );
                return;
            }
            if let Err(err) = crate::commands::recall_scene_impl(
                &handles.app,
                &handles.engine,
                &handles.show,
                &handles.chasers,
                &handles.movement,
                &handles.scenes,
                &id,
            ) {
                tracing::warn!(?err, "midi-learn recall_scene failed");
            }
        }
        ButtonAction::Blackout => {
            let on = !handles.show.read().show.globals.blackout.active;
            if let Err(err) = crate::commands::set_blackout_impl(
                &handles.app,
                &handles.show,
                &handles.globals,
                on,
            ) {
                tracing::warn!(?err, "midi-learn blackout failed");
            }
        }
        ButtonAction::Tap => {
            if let Err(err) =
                crate::commands::tap_overall_bpm_impl(&handles.app, &handles.show, &handles.globals)
            {
                tracing::warn!(?err, "midi-learn tap failed");
            }
        }
        ButtonAction::ToggleOverallBpm => {
            let next = !handles.show.read().show.globals.overall_bpm_enabled;
            if let Err(err) = crate::commands::set_overall_bpm_enabled_impl(
                &handles.app,
                &handles.show,
                &handles.globals,
                next,
            ) {
                tracing::warn!(?err, "midi-learn toggle overall bpm failed");
            }
        }
        ButtonAction::BumpActiveChaserBpm { delta } => {
            let active = {
                let s = handles.show.read();
                s.show.chasers.iter().find(|c| c.enabled).cloned()
            };
            if let Some(mut chaser) = active {
                let crate::chaser::TempoSource::Fixed { bpm } = chaser.tempo;
                let new_bpm = (bpm + delta).clamp(20.0, 300.0);
                if (new_bpm - bpm).abs() >= f32::EPSILON {
                    chaser.tempo = crate::chaser::TempoSource::Fixed { bpm: new_bpm };
                    if let Err(err) = crate::commands::update_chaser_impl(
                        &handles.app,
                        &handles.show,
                        &handles.chasers,
                        chaser,
                    ) {
                        tracing::warn!(?err, "midi-learn bpm nudge failed");
                    }
                }
            }
        }
        ButtonAction::StartLoopGroup { id } => {
            let active = handles
                .loops
                .lock()
                .active_group_id()
                .map(|s| s.to_string());
            if active.as_deref() == Some(id.as_str()) {
                crate::commands::stop_loop_group_impl(
                    &handles.app,
                    &handles.scenes,
                    &handles.loops,
                );
                return;
            }
            if let Err(err) = crate::commands::start_loop_group_impl(
                &handles.app,
                &handles.engine,
                &handles.show,
                &handles.chasers,
                &handles.movement,
                &handles.globals,
                &handles.scenes,
                &handles.loops,
                &id,
            ) {
                tracing::warn!(?err, "midi-learn start loop group failed");
            }
        }
        ButtonAction::StopLoopGroup => {
            crate::commands::stop_loop_group_impl(&handles.app, &handles.scenes, &handles.loops);
        }
        ButtonAction::ToggleSnapshot { id } => {
            let active = handles.snapshots.lock().active_id().map(|s| s.to_string());
            let result = if active.as_deref() == Some(id.as_str()) {
                crate::commands::deactivate_snapshot_impl(
                    &handles.app,
                    &handles.engine,
                    &handles.show,
                    &handles.chasers,
                    &handles.movement,
                    &handles.globals,
                    &handles.scenes,
                    &handles.loops,
                    &handles.snapshots,
                )
            } else {
                crate::commands::activate_snapshot_impl(
                    &handles.app,
                    &handles.engine,
                    &handles.show,
                    &handles.chasers,
                    &handles.movement,
                    &handles.globals,
                    &handles.scenes,
                    &handles.loops,
                    &handles.snapshots,
                    &id,
                )
            };
            if let Err(err) = result {
                tracing::warn!(?err, "midi-learn toggle snapshot failed");
            }
        }
        // *ByIndex resolved above.
        _ => (),
    }
}

// ---- Blind as a one-beat hit ------------------------------------------------

/// One beat at the current tempo (the globals' overall BPM), else 500 ms.
pub fn beat_len(handles: &GenericHandles) -> std::time::Duration {
    let bpm = handles
        .globals
        .lock()
        .current_overall_bpm()
        .unwrap_or(120.0);
    std::time::Duration::from_secs_f32(60.0 / bpm.max(20.0))
}

/// Blind on now, off after `hold`. Cues and POIs can't say "release", so a
/// blind fired from them is a blinder bump, not a latch that outlives the
/// track. A real release arriving earlier still wins (both just set off).
pub fn blind_pulse(handles: &GenericHandles, hold: std::time::Duration) {
    dispatch_action(ButtonAction::Blind, true, true, handles);
    // The release rides its own thread, not the async runtime: a busy
    // runtime must never leave a blinder stuck on.
    let h = handles.clone();
    let spawned = std::thread::Builder::new()
        .name("dmx-blind-release".into())
        .spawn(move || {
            std::thread::sleep(hold);
            dispatch_action(ButtonAction::Blind, false, false, &h);
        });
    if spawned.is_err() {
        dispatch_action(ButtonAction::Blind, false, false, handles);
    }
}

// ---- Button modes: Toggle / Flash / Swop ------------------------------------
//
// grandMA executor styles for every surface (Launchpad, Stream Deck, OS2L
// pads): Toggle fires on press, Flash holds the action while the button is
// down and puts things back on release, Swop does the same and darkens
// every fixture the action does not drive meanwhile.

use crate::show::button_bindings::ButtonMode;

/// What a release has to undo.
enum Restore {
    ChaserOff(String),
    MovementOff(String),
    Snapshot(Option<String>),
    Scene(Option<String>),
    Loop(Option<String>),
    Blackout(bool),
    Autopilot(bool),
    Burst,
    Hold(bool),
    Nothing,
}

struct Held {
    restore: Restore,
    swop: bool,
}

fn held() -> &'static parking_lot::Mutex<HashMap<String, Held>> {
    static H: std::sync::OnceLock<parking_lot::Mutex<HashMap<String, Held>>> =
        std::sync::OnceLock::new();
    H.get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
}

/// Resolve `*ByIndex` variants against the current show.
pub(crate) fn resolve_indexed(
    handles: &GenericHandles,
    action: &ButtonAction,
) -> Option<ButtonAction> {
    let s = handles.show.read();
    Some(match action {
        ButtonAction::ToggleChaserByIndex { index } => ButtonAction::ToggleChaser {
            id: s.show.chasers.get(*index as usize)?.id.clone(),
        },
        ButtonAction::ToggleMovementByIndex { index } => ButtonAction::ToggleMovement {
            id: s.show.movements.get(*index as usize)?.id.clone(),
        },
        ButtonAction::RecallSceneByIndex { index } => ButtonAction::RecallScene {
            id: s.show.scenes.get(*index as usize)?.id.clone(),
        },
        ButtonAction::StartLoopGroupByIndex { index } => ButtonAction::StartLoopGroup {
            id: s.show.scene_loop_groups.get(*index as usize)?.id.clone(),
        },
        ButtonAction::ToggleSnapshotByIndex { index } => ButtonAction::ToggleSnapshot {
            id: s.show.snapshots.get(*index as usize)?.id.clone(),
        },
        other => other.clone(),
    })
}

/// Fixtures an action drives — what a Swop leaves lit. `None` = the whole
/// rig (snapshots, loop groups, globals).
fn owned_fixtures(
    handles: &GenericHandles,
    action: &ButtonAction,
) -> Option<std::collections::HashSet<String>> {
    let s = handles.show.read();
    match action {
        ButtonAction::ToggleChaser { id } => s
            .show
            .chasers
            .iter()
            .find(|c| &c.id == id)
            .map(|c| c.slots.iter().map(|sl| sl.fixture_id.clone()).collect()),
        ButtonAction::ToggleMovement { id } => s
            .show
            .movements
            .iter()
            .find(|m| &m.id == id)
            .map(|m| m.fixtures.iter().map(|sl| sl.fixture_id.clone()).collect()),
        ButtonAction::RecallScene { id } => {
            s.show.scenes.iter().find(|sc| &sc.id == id).map(|sc| {
                sc.steps
                    .iter()
                    .flat_map(|st| st.fixtures.iter().map(|f| f.fixture_id.clone()))
                    .collect()
            })
        }
        _ => None,
    }
}

/// A surface button went down (`pressed`) or up. `key` identifies the
/// physical button so the release finds what the press did.
pub fn button_event(
    handles: &GenericHandles,
    key: &str,
    action: &ButtonAction,
    mode: ButtonMode,
    pressed: bool,
) {
    match mode {
        ButtonMode::Toggle => {
            // Both edges go through: `dispatch_action` fires latching
            // actions on the press edge only, but Blind is momentary and
            // needs the release (VirtualDJ sends on/off for every button).
            dispatch_action(action.clone(), pressed, pressed, handles);
        }
        ButtonMode::Flash | ButtonMode::Swop => {
            if pressed {
                flash_down(handles, key, action, mode == ButtonMode::Swop);
            } else {
                flash_up(handles, key);
            }
        }
    }
}

fn flash_down(handles: &GenericHandles, key: &str, action: &ButtonAction, swop: bool) {
    let Some(action) = resolve_indexed(handles, action) else {
        return;
    };
    crate::autopilot::note_manual_action(&action);
    let restore = match &action {
        ButtonAction::ToggleChaser { id } => {
            let on = handles
                .show
                .read()
                .show
                .chasers
                .iter()
                .any(|c| &c.id == id && c.enabled);
            if on {
                Restore::Nothing
            } else {
                if let Err(err) = crate::commands::toggle_chaser_impl(
                    &handles.app,
                    &handles.show,
                    &handles.chasers,
                    id,
                    true,
                ) {
                    tracing::warn!(?err, "flash chaser on failed");
                }
                Restore::ChaserOff(id.clone())
            }
        }
        ButtonAction::ToggleMovement { id } => {
            let on = handles
                .show
                .read()
                .show
                .movements
                .iter()
                .any(|m| &m.id == id && m.enabled);
            if on {
                Restore::Nothing
            } else {
                if let Err(err) = crate::commands::toggle_movement_impl(
                    &handles.app,
                    &handles.show,
                    &handles.movement,
                    id,
                    true,
                ) {
                    tracing::warn!(?err, "flash movement on failed");
                }
                Restore::MovementOff(id.clone())
            }
        }
        ButtonAction::ToggleSnapshot { id } => {
            let prev = handles.snapshots.lock().active_id().map(|s| s.to_string());
            if prev.as_deref() == Some(id.as_str()) {
                Restore::Nothing
            } else {
                if let Err(err) = crate::commands::activate_snapshot_impl(
                    &handles.app,
                    &handles.engine,
                    &handles.show,
                    &handles.chasers,
                    &handles.movement,
                    &handles.globals,
                    &handles.scenes,
                    &handles.loops,
                    &handles.snapshots,
                    id,
                ) {
                    tracing::warn!(?err, "flash snapshot failed");
                }
                Restore::Snapshot(prev)
            }
        }
        ButtonAction::RecallScene { id } => {
            let prev = handles
                .scenes
                .lock()
                .active_scene_id()
                .map(|s| s.to_string());
            if prev.as_deref() == Some(id.as_str()) {
                Restore::Nothing
            } else {
                if let Err(err) = crate::commands::recall_scene_impl(
                    &handles.app,
                    &handles.engine,
                    &handles.show,
                    &handles.chasers,
                    &handles.movement,
                    &handles.scenes,
                    id,
                ) {
                    tracing::warn!(?err, "flash scene failed");
                }
                Restore::Scene(prev)
            }
        }
        ButtonAction::StartLoopGroup { id } => {
            let prev = handles
                .loops
                .lock()
                .active_group_id()
                .map(|s| s.to_string());
            if prev.as_deref() == Some(id.as_str()) {
                Restore::Nothing
            } else {
                if let Err(err) = crate::commands::start_loop_group_impl(
                    &handles.app,
                    &handles.engine,
                    &handles.show,
                    &handles.chasers,
                    &handles.movement,
                    &handles.globals,
                    &handles.scenes,
                    &handles.loops,
                    id,
                ) {
                    tracing::warn!(?err, "flash loop failed");
                }
                Restore::Loop(prev)
            }
        }
        ButtonAction::Blackout => {
            let prev = handles.show.read().show.globals.blackout.active;
            if !prev {
                if let Err(err) = crate::commands::set_blackout_impl(
                    &handles.app,
                    &handles.show,
                    &handles.globals,
                    true,
                ) {
                    tracing::warn!(?err, "flash blackout failed");
                }
            }
            Restore::Blackout(prev)
        }
        ButtonAction::ToggleAutopilot => {
            let prev = crate::autopilot::is_enabled();
            crate::autopilot::set_enabled_from_action(&handles.app, true);
            Restore::Autopilot(prev)
        }
        ButtonAction::AutopilotBurst => {
            crate::autopilot::burst_set(true);
            Restore::Burst
        }
        ButtonAction::AutopilotHold => {
            let prev = crate::autopilot::is_hold();
            crate::autopilot::set_hold(true);
            Restore::Hold(prev)
        }
        other => {
            // Tap, energy pads, BPM nudges…: a press is the whole gesture.
            dispatch_action(other.clone(), true, true, handles);
            Restore::Nothing
        }
    };
    if swop {
        crate::fixture_test::set_exclusive(owned_fixtures(handles, &action));
        crate::autopilot::set_muted(true);
    }
    held()
        .lock()
        .insert(key.to_string(), Held { restore, swop });
}

fn flash_up(handles: &GenericHandles, key: &str) {
    let Some(h) = held().lock().remove(key) else {
        return;
    };
    if h.swop {
        crate::fixture_test::set_exclusive(None);
        crate::autopilot::set_muted(false);
    }
    match h.restore {
        Restore::ChaserOff(id) => {
            let _ = crate::commands::toggle_chaser_impl(
                &handles.app,
                &handles.show,
                &handles.chasers,
                &id,
                false,
            );
        }
        Restore::MovementOff(id) => {
            let _ = crate::commands::toggle_movement_impl(
                &handles.app,
                &handles.show,
                &handles.movement,
                &id,
                false,
            );
        }
        Restore::Snapshot(prev) => {
            let _ = match prev {
                Some(p) => crate::commands::activate_snapshot_impl(
                    &handles.app,
                    &handles.engine,
                    &handles.show,
                    &handles.chasers,
                    &handles.movement,
                    &handles.globals,
                    &handles.scenes,
                    &handles.loops,
                    &handles.snapshots,
                    &p,
                ),
                None => crate::commands::deactivate_snapshot_impl(
                    &handles.app,
                    &handles.engine,
                    &handles.show,
                    &handles.chasers,
                    &handles.movement,
                    &handles.globals,
                    &handles.scenes,
                    &handles.loops,
                    &handles.snapshots,
                ),
            };
        }
        Restore::Scene(prev) => match prev {
            Some(p) => {
                let _ = crate::commands::recall_scene_impl(
                    &handles.app,
                    &handles.engine,
                    &handles.show,
                    &handles.chasers,
                    &handles.movement,
                    &handles.scenes,
                    &p,
                );
            }
            None => {
                handles.scenes.lock().release(std::time::Instant::now());
                let _ = tauri::Emitter::emit(
                    &handles.app,
                    crate::commands::SCENE_ACTIVE_EVENT,
                    crate::commands::SceneActiveChange {
                        active_scene_id: None,
                        step_index: None,
                    },
                );
            }
        },
        Restore::Loop(prev) => match prev {
            Some(p) => {
                let _ = crate::commands::start_loop_group_impl(
                    &handles.app,
                    &handles.engine,
                    &handles.show,
                    &handles.chasers,
                    &handles.movement,
                    &handles.globals,
                    &handles.scenes,
                    &handles.loops,
                    &p,
                );
            }
            None => {
                crate::commands::stop_loop_group_impl(&handles.app, &handles.scenes, &handles.loops)
            }
        },
        Restore::Blackout(prev) => {
            let _ = crate::commands::set_blackout_impl(
                &handles.app,
                &handles.show,
                &handles.globals,
                prev,
            );
        }
        Restore::Autopilot(prev) => crate::autopilot::set_enabled_from_action(&handles.app, prev),
        Restore::Burst => crate::autopilot::burst_set(false),
        Restore::Hold(prev) => crate::autopilot::set_hold(prev),
        Restore::Nothing => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learn_captures_once_and_disarms() {
        let learn = shared_midi_learn();
        learn_arm(&learn);
        assert!(learn.lock().armed);
        // Simulate the capture branch directly (no full handles needed).
        {
            let mut l = learn.lock();
            l.captured = Some(LearnedControl {
                is_cc: true,
                channel: 0,
                data1: 21,
                description: "CC 21 · ch 1".into(),
            });
            l.armed = false;
        }
        let got = learn_poll(&learn).expect("captured");
        assert_eq!(got.data1, 21);
        assert!(learn_poll(&learn).is_none(), "poll clears the capture");
    }
}
