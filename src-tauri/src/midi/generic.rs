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
fn dispatch_action(
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
