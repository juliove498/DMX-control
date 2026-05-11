//! Launchpad MK2 surface integration.
//!
//! Layout:
//! - Row 1 of the grid (notes 11–18): first 8 chasers. One pad per
//!   chaser, hard-coded palette colour. Off → solid dim. On → flash
//!   between dim and bright (channel 2 NoteOn). Press = toggle.
//! - Row 2 of the grid (notes 21–28): first 8 movement generators, same
//!   semantics as chasers but with their own palette so a glance at the
//!   board distinguishes layers.
//! - Right-side scene buttons:
//!     - Note 19 (bottom-right scene): Blackout. Press = toggle. Off → dim
//!       red. On → flashing bright red.
//!     - Note 29 (one above): Blind. Press = blind on, release = blind
//!       off (momentary). Held → flashing bright white.
//!
//! Architecture:
//! - An input router installed on the [`MidiHub`](super::hub::MidiHub)
//!   intercepts NoteOn ch=1 on managed pads and dispatches to the same
//!   `*_impl` helpers used by Tauri commands. No Tauri-event indirection
//!   for the controller's own actions — keeps latency tight.
//! - A background thread polls show + globals state ~7 Hz and pushes any
//!   pad colour changes back over MIDI. Polling (instead of event hooks)
//!   keeps the LED layer simple and decoupled from every mutation site.
//!
//! Hardware reference: *Launchpad MK2 Programmer's Reference Manual*,
//! Novation, 2015 — colour palette indices are taken from §11.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use parking_lot::Mutex;
use tauri::AppHandle;

use crate::chaser::runtime::SlotOutput;
use crate::chaser::Rgb;
use crate::engine::loop_playback::SharedLoopPlayback;
use crate::engine::output_thread::{SharedChasers, SharedGlobals, SharedMovement};
use crate::engine::scene_playback::SharedScenePlayback;
use crate::engine::EngineState;
use crate::midi::hub::SharedMidi;
use crate::midi::MidiMessage;
use crate::show::button_bindings::{ButtonAction, ButtonActiveMode, LaunchpadBinding};
use crate::show::ShowState;

/// MK2 row-1 pad notes, left-to-right. Each maps to a chaser slot.
pub const CHASER_PAD_NOTES: [u8; 8] = [11, 12, 13, 14, 15, 16, 17, 18];

/// MK2 row-2 pad notes. Each maps to a movement generator slot.
pub const MOVEMENT_PAD_NOTES: [u8; 8] = [21, 22, 23, 24, 25, 26, 27, 28];

/// MK2 row-3 pad notes. Each maps to one of the first 8 scenes; press
/// triggers a recall with the scene's recorded fade time.
pub const SCENE_PAD_NOTES: [u8; 8] = [31, 32, 33, 34, 35, 36, 37, 38];

/// Top row of round buttons (CC numbers, not notes). The MK2 lays them
/// out as Up, Down, Left, Right, Session, User1, User2, Mixer. We treat
/// the entire row as a live mirror of the active chaser's per-slot output
/// — each round button shows the colour and brightness that the
/// corresponding fixture is currently being driven to, so the operator
/// gets a "miniature stage" up top without taking eyes off the LP.
///
/// CC 104 (Up arrow) and CC 105 (Down arrow) keep their input role:
/// pressing them nudges the active chaser's BPM by ±1. The visual mirror
/// runs on top regardless — input and output are decoupled.
pub const TOP_ROW_CCS: [u8; 8] = [104, 105, 106, 107, 108, 109, 110, 111];
pub const BPM_UP_CC: u8 = 104;
pub const BPM_DOWN_CC: u8 = 105;
pub const BPM_STEP: f32 = 1.0;
pub const BPM_MIN: f32 = 20.0;
pub const BPM_MAX: f32 = 300.0;

/// Scene button column note numbers we use:
///   - `BLACKOUT_NOTE` (19): bottom-right scene → toggles blackout.
///   - `BLIND_NOTE` (29): scene above → momentary blind.
///   - `TAP_NOTE` (39): one above blind → register a TAP press to
///     compute the global Overall BPM.
///   - `BPM_TOGGLE_NOTE` (49): one above TAP → toggle the Overall BPM
///     override on/off.
pub const BLACKOUT_NOTE: u8 = 19;
pub const BLIND_NOTE: u8 = 29;
pub const TAP_NOTE: u8 = 39;
pub const BPM_TOGGLE_NOTE: u8 = 49;

// Legacy hardcoded palettes used to live here; they now live in
// `show::button_bindings::default_launchpad_bindings()` so the factory
// layout is the source of truth for both runtime and the UI's
// "Load defaults" button.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PadState {
    /// No assignment for this pad — keep the LED dark.
    Empty,
    /// Solid dim colour (e.g. chaser exists but is off).
    OffDim(u8),
    /// Flash mode (e.g. chaser is running, blackout is engaged) —
    /// hardware blinks between the two values at ~1 Hz.
    OnFlash { dim: u8, bright: u8 },
}

/// Live RGB target for a top-row round button. Sent via SysEx so the
/// colour matches the actual fixture output rather than snapping to the
/// nearest palette index. `None` means "keep the LED dark".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct TopRowRgb {
    r: u8,
    g: u8,
    b: u8,
}

/// Snapshot of every managed pad's desired LED state. Diffed against the
/// previous tick so we only emit MIDI when something actually changed.
///
/// `pads` and `ccs` are keyed by raw note/CC number so the same struct
/// covers both the legacy hardcoded layout and arbitrary user-customised
/// layouts. The `top_row` array stays separate because those CCs
/// double as the live RGB mirror — a passive visual that doesn't fit
/// the binding model.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LedTargets {
    pads: std::collections::HashMap<u8, PadState>,
    ccs: std::collections::HashMap<u8, PadState>,
    top_row: [TopRowRgb; 8],
}

impl LedTargets {
    fn empty() -> Self {
        Self {
            pads: std::collections::HashMap::new(),
            ccs: std::collections::HashMap::new(),
            top_row: [TopRowRgb::default(); 8],
        }
    }
}

pub struct LaunchpadController {
    shutdown: Arc<AtomicBool>,
    feedback_thread: Option<JoinHandle<()>>,
    midi: SharedMidi,
    /// Held by the input router and feedback thread. Press of `BLIND_NOTE`
    /// flips it true; release flips it false. The feedback thread reads
    /// it to drive the LED so the pad stays lit while held.
    blind_held: Arc<AtomicBool>,
    globals: SharedGlobals,
}

#[derive(Clone)]
struct LpHandles {
    app: AppHandle,
    midi: SharedMidi,
    chasers: SharedChasers,
    movement: SharedMovement,
    globals: SharedGlobals,
    scenes: SharedScenePlayback,
    loops: SharedLoopPlayback,
    engine: EngineState,
    show: ShowState,
    blind_held: Arc<AtomicBool>,
}

impl LaunchpadController {
    /// Stop the feedback thread, blank out every managed pad, release any
    /// held blind state, and clear the input router on the hub.
    pub fn shutdown(mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.feedback_thread.take() {
            let _ = handle.join();
        }
        // Make sure we don't leave the user in blind if they unplugged
        // mid-press. Cheap and safe — set_blind to false is a no-op when
        // it's already false.
        if self.blind_held.swap(false, Ordering::Relaxed) {
            self.globals.lock().set_blind(false);
        }
        clear_all_pads(&self.midi);
        self.midi.lock().set_input_router(None);
    }
}

pub type SharedLaunchpad = Arc<Mutex<Option<LaunchpadController>>>;

pub fn shared_launchpad() -> SharedLaunchpad {
    Arc::new(Mutex::new(None))
}

/// Heuristic: only spin up the controller for devices whose name looks
/// like a Launchpad. Other MK2/X variants share the bottom-row layout
/// closely enough that this should still be useful — if not, we'll add
/// a stricter match later.
pub fn is_launchpad(name: &str) -> bool {
    name.to_lowercase().contains("launchpad")
}

/// Install the input router and start the LED feedback thread. Returns
/// the controller; store it so [`LaunchpadController::shutdown`] can be
/// called on disconnect.
#[allow(clippy::too_many_arguments)]
pub fn start(
    app: AppHandle,
    midi: SharedMidi,
    chasers: SharedChasers,
    movement: SharedMovement,
    globals: SharedGlobals,
    scenes: SharedScenePlayback,
    loops: SharedLoopPlayback,
    engine: EngineState,
    show: ShowState,
) -> LaunchpadController {
    let blind_held = Arc::new(AtomicBool::new(false));
    let handles = LpHandles {
        app,
        midi: midi.clone(),
        chasers,
        movement,
        globals: globals.clone(),
        scenes,
        loops,
        engine,
        show,
        blind_held: blind_held.clone(),
    };

    install_input_router(&midi, handles.clone());

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_for_thread = shutdown.clone();
    let handles_for_thread = handles.clone();
    let handle = thread::Builder::new()
        .name("dmx-launchpad-feedback".into())
        .spawn(move || {
            // Force a fresh push on first tick — the previous run might
            // have left pads in an arbitrary state.
            let initial = compute_targets(&handles_for_thread);
            push_all(&handles_for_thread.midi, &initial);
            let mut last = initial;
            while !shutdown_for_thread.load(Ordering::Relaxed) {
                let target = compute_targets(&handles_for_thread);
                diff_and_push(&handles_for_thread.midi, &last, &target);
                last = target;
                // 50 ms ≈ 20 fps. Fast enough to follow a sixteenth-note
                // chase at 240 BPM (≈ 60 ms per step) without visible
                // tearing, and still well under the LP's USB MIDI
                // throughput ceiling (8 SysEx × 20 Hz × 12 bytes ≈ 2 KB/s).
                thread::sleep(Duration::from_millis(50));
            }
            clear_all_pads(&handles_for_thread.midi);
        })
        .expect("spawn launchpad feedback thread");

    LaunchpadController {
        shutdown,
        feedback_thread: Some(handle),
        midi,
        blind_held,
        globals,
    }
}

fn install_input_router(midi_for_install: &SharedMidi, handles: LpHandles) {
    midi_for_install
        .lock()
        .set_input_router(Some(Arc::new(move |msg: &MidiMessage| {
            // The MK2 sends grid + scene presses as NoteOn ch=1 (status
            // 0x90) and the round-button row as CC ch=1 (status 0xB0).
            // Branch on the high nibble; data1 carries the address.
            match msg.status {
                0x90 => handle_note(msg, &handles),
                0xB0 => handle_cc(msg, &handles),
                _ => (),
            }
        })));
}

/// Resolve the effective bindings list — user-customised or the
/// hardcoded factory layout — for this tick.
fn resolve_bindings(handles: &LpHandles) -> Vec<LaunchpadBinding> {
    let bindings = handles.show.read().show.button_bindings.clone();
    if bindings.custom_enabled {
        bindings.launchpad
    } else {
        crate::show::button_bindings::default_launchpad_bindings()
    }
}

/// Resolve a `*ByIndex` action against the live show. Returns a
/// concrete-id variant the dispatch and LED helpers can act on, or
/// `None` if the index points past the end of the show's list.
fn resolve_indexed_action(handles: &LpHandles, action: &ButtonAction) -> Option<ButtonAction> {
    let s = handles.show.read();
    Some(match action {
        ButtonAction::ToggleChaserByIndex { index } => {
            let id = s.show.chasers.get(*index as usize)?.id.clone();
            ButtonAction::ToggleChaser { id }
        }
        ButtonAction::ToggleMovementByIndex { index } => {
            let id = s.show.movements.get(*index as usize)?.id.clone();
            ButtonAction::ToggleMovement { id }
        }
        ButtonAction::RecallSceneByIndex { index } => {
            let id = s.show.scenes.get(*index as usize)?.id.clone();
            ButtonAction::RecallScene { id }
        }
        ButtonAction::StartLoopGroupByIndex { index } => {
            let id = s.show.scene_loop_groups.get(*index as usize)?.id.clone();
            ButtonAction::StartLoopGroup { id }
        }
        // Already concrete: clone through.
        other => other.clone(),
    })
}

/// Decide whether a button should appear "active" (flash) on the LP.
/// Mirrors the on-screen highlight rules: chaser toggle pads flash
/// while the chaser is enabled, scene pads flash while the scene is
/// the active playback, etc.
fn is_action_active(action: &ButtonAction, handles: &LpHandles) -> bool {
    match action {
        ButtonAction::None => false,
        ButtonAction::ToggleChaser { id } => handles
            .show
            .read()
            .show
            .chasers
            .iter()
            .any(|c| &c.id == id && c.enabled),
        ButtonAction::ToggleMovement { id } => handles
            .show
            .read()
            .show
            .movements
            .iter()
            .any(|m| &m.id == id && m.enabled),
        ButtonAction::RecallScene { id } => handles
            .scenes
            .lock()
            .active_scene_id()
            .map(|a| a == id)
            .unwrap_or(false),
        ButtonAction::Blackout => handles.show.read().show.globals.blackout.active,
        ButtonAction::Blind => handles.blind_held.load(Ordering::Relaxed),
        ButtonAction::Tap => false,
        ButtonAction::ToggleOverallBpm => handles.show.read().show.globals.overall_bpm_enabled,
        ButtonAction::BumpActiveChaserBpm { .. } => false,
        ButtonAction::StartLoopGroup { id } => handles
            .loops
            .lock()
            .active_group_id()
            .map(|a| a == id)
            .unwrap_or(false),
        ButtonAction::StopLoopGroup => false,
        // *ByIndex never gets here — resolved upstream.
        _ => false,
    }
}

fn pad_state_for_binding(b: &LaunchpadBinding, handles: &LpHandles) -> PadState {
    if let ButtonAction::None = b.action {
        return PadState::Empty;
    }
    let active = match b.active_mode {
        ButtonActiveMode::AlwaysIdle => false,
        ButtonActiveMode::AlwaysActive => true,
        ButtonActiveMode::Auto => {
            let resolved = resolve_indexed_action(handles, &b.action);
            resolved
                .as_ref()
                .map(|a| is_action_active(a, handles))
                .unwrap_or(false)
        }
    };
    if active {
        PadState::OnFlash {
            dim: b.color_dim,
            bright: b.color_bright,
        }
    } else if b.color_dim == 0 {
        // An explicit "dark when idle" — treat as Empty so the LED is
        // turned off (vs. solid black-channel-0 which is the same on
        // hardware but cleaner intent).
        PadState::Empty
    } else {
        PadState::OffDim(b.color_dim)
    }
}

fn dispatch_action(action: ButtonAction, vel: u8, handles: &LpHandles) {
    let pressed = vel > 0;
    // Blind is the one momentary action: both press and release fire.
    // Everything else latches on press (vel > 0).
    if let ButtonAction::Blind = action {
        handles.blind_held.store(pressed, Ordering::Relaxed);
        handles.globals.lock().set_blind(pressed);
        let _ = tauri::Emitter::emit(
            &handles.app,
            crate::commands::BLIND_EVENT,
            crate::commands::BlindChange { pressed },
        );
        return;
    }
    if !pressed {
        return;
    }
    // Resolve *ByIndex variants once so the dispatch logic only deals
    // with concrete id'd actions.
    let Some(action) = resolve_indexed_action(handles, &action) else {
        return;
    };
    match action {
        ButtonAction::None => (),
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
                tracing::warn!(?err, "launchpad action toggle_chaser failed");
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
                tracing::warn!(?err, "launchpad action toggle_movement failed");
            }
        }
        ButtonAction::RecallScene { id } => {
            let active_id = handles
                .scenes
                .lock()
                .active_scene_id()
                .map(|s| s.to_string());
            if active_id.as_deref() == Some(id.as_str()) {
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
                tracing::warn!(?err, "launchpad action recall_scene failed");
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
                tracing::warn!(?err, "launchpad action blackout failed");
            }
        }
        ButtonAction::Blind => {
            // Handled above — kept for completeness.
        }
        ButtonAction::Tap => {
            match crate::commands::tap_overall_bpm_impl(
                &handles.app,
                &handles.show,
                &handles.globals,
            ) {
                Ok(Some(bpm)) => tracing::info!(bpm, "launchpad action tap → bpm"),
                Ok(None) => tracing::info!("launchpad action tap → first of window"),
                Err(err) => tracing::warn!(?err, "launchpad action tap failed"),
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
                tracing::warn!(?err, "launchpad action toggle overall bpm failed");
            }
        }
        ButtonAction::BumpActiveChaserBpm { delta } => {
            bump_active_chaser_bpm(handles, delta);
        }
        ButtonAction::StartLoopGroup { id } => {
            let active = handles
                .loops
                .lock()
                .active_group_id()
                .map(|s| s.to_string());
            if active.as_deref() == Some(id.as_str()) {
                // Same group is playing — pressing again stops it.
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
                &handles.scenes,
                &handles.loops,
                &id,
            ) {
                tracing::warn!(?err, "launchpad action start loop group failed");
            }
        }
        ButtonAction::StopLoopGroup => {
            crate::commands::stop_loop_group_impl(&handles.app, &handles.scenes, &handles.loops);
        }
        // *ByIndex were resolved upstream.
        _ => (),
    }
}

fn handle_note(msg: &MidiMessage, handles: &LpHandles) {
    let Some(note) = msg.data1 else { return };
    let vel = msg.data2.unwrap_or(0);
    let bindings = resolve_bindings(handles);
    let Some(binding) = bindings.iter().find(|b| !b.is_cc && b.note == note) else {
        return;
    };
    dispatch_action(binding.action.clone(), vel, handles);
}

fn handle_cc(msg: &MidiMessage, handles: &LpHandles) {
    let Some(cc) = msg.data1 else { return };
    let val = msg.data2.unwrap_or(0);
    let bindings = resolve_bindings(handles);
    let Some(binding) = bindings.iter().find(|b| b.is_cc && b.note == cc) else {
        return;
    };
    dispatch_action(binding.action.clone(), val, handles);
}

/// Find the currently-enabled chaser (exclusive activation guarantees at
/// most one) and shift its tempo. No-op if no chaser is on — the operator
/// then has to enable one before the arrows do anything, which matches
/// the "tweak the active scene" mental model.
fn bump_active_chaser_bpm(handles: &LpHandles, delta_bpm: f32) {
    let active = {
        let s = handles.show.read();
        s.show.chasers.iter().find(|c| c.enabled).cloned()
    };
    let Some(mut chaser) = active else {
        return;
    };
    let crate::chaser::TempoSource::Fixed { bpm } = chaser.tempo;
    let new_bpm = (bpm + delta_bpm).clamp(BPM_MIN, BPM_MAX);
    if (new_bpm - bpm).abs() < f32::EPSILON {
        return;
    }
    chaser.tempo = crate::chaser::TempoSource::Fixed { bpm: new_bpm };
    if let Err(err) =
        crate::commands::update_chaser_impl(&handles.app, &handles.show, &handles.chasers, chaser)
    {
        tracing::warn!(?err, "launchpad BPM nudge failed");
    }
}

fn compute_targets(handles: &LpHandles) -> LedTargets {
    let mut out = LedTargets::empty();
    let bindings = resolve_bindings(handles);
    for b in &bindings {
        let state = pad_state_for_binding(b, handles);
        if b.is_cc {
            out.ccs.insert(b.note, state);
        } else {
            out.pads.insert(b.note, state);
        }
    }
    // Top row RGB mirror: only paint if the user has NOT explicitly
    // bound those CCs themselves. The factory layout uses CC 104/105
    // for BPM bump only — slots 106..=111 stay free and we use them
    // to mirror the chaser's slot RGB. When the user binds those CCs
    // for something else, their colour wins.
    if let Some(slots) = handles.chasers.lock().active_slot_outputs() {
        for (i, slot) in slots.iter().take(8).enumerate() {
            let cc = TOP_ROW_CCS[i];
            // Skip the slot if the user has bound that CC to an
            // action — their LED state wins.
            if bindings.iter().any(|b| b.is_cc && b.note == cc) {
                continue;
            }
            out.top_row[i] = slot_output_to_rgb(slot);
        }
    }
    out
}

fn diff_and_push(midi: &SharedMidi, last: &LedTargets, target: &LedTargets) {
    // Union the key sets so a binding that was just removed gets blanked.
    let mut pad_keys: std::collections::HashSet<u8> = std::collections::HashSet::new();
    pad_keys.extend(last.pads.keys().copied());
    pad_keys.extend(target.pads.keys().copied());
    for note in pad_keys {
        let prev = last.pads.get(&note).copied().unwrap_or(PadState::Empty);
        let next = target.pads.get(&note).copied().unwrap_or(PadState::Empty);
        if prev != next {
            push_pad(midi, note, next);
        }
    }
    let mut cc_keys: std::collections::HashSet<u8> = std::collections::HashSet::new();
    cc_keys.extend(last.ccs.keys().copied());
    cc_keys.extend(target.ccs.keys().copied());
    for cc in cc_keys {
        let prev = last.ccs.get(&cc).copied().unwrap_or(PadState::Empty);
        let next = target.ccs.get(&cc).copied().unwrap_or(PadState::Empty);
        if prev != next {
            // CCs use NoteOn-on-CC palette wire format same as pads on
            // the MK2 grid because the SysEx LED protocol addresses both
            // by data1. push_pad sends 0x90 which the firmware also
            // accepts for CC LEDs — but the canonical way is 0xB0. Stay
            // consistent with the historical code: only the bottom-row
            // pads use 0x90; the top-row CCs are RGB-only via SysEx.
            // So for "palette colour on a top-row CC button", emit the
            // closest dim white SysEx and call it a day.
            push_cc_palette(midi, cc, next);
        }
    }
    for (i, &cc) in TOP_ROW_CCS.iter().enumerate() {
        if target.top_row[i] != last.top_row[i] {
            push_top_rgb(midi, cc, target.top_row[i]);
        }
    }
}

fn push_pad(midi: &SharedMidi, note: u8, state: PadState) {
    let mut hub = midi.lock();
    match state {
        PadState::Empty => {
            let _ = hub.send_raw(&[0x90, note, 0]);
        }
        PadState::OffDim(dim) => {
            // Solid dim colour. Sending channel 1 also cancels any prior
            // flash setting on the MK2 for this LED.
            let _ = hub.send_raw(&[0x90, note, dim]);
        }
        PadState::OnFlash { dim, bright } => {
            // MK2 flash: ch1 NoteOn sets the "background" colour, ch2
            // NoteOn sets the alternating "flash" colour. Hardware blinks
            // between them at ~1 Hz.
            let _ = hub.send_raw(&[0x90, note, dim]);
            let _ = hub.send_raw(&[0x91, note, bright]);
        }
    }
}

/// Send a true-RGB colour to one of the LP's LEDs via SysEx. Works for
/// the round top-row buttons (CC 104–111) the same as for the grid,
/// because the SysEx LED-lighting message addresses every LED by its
/// note/CC byte. RGB inputs are 0–255 (DMX range); the MK2 takes 0–63
/// so we shift right by 2.
fn push_top_rgb(midi: &SharedMidi, cc: u8, rgb: TopRowRgb) {
    let mut hub = midi.lock();
    let _ = hub.send_raw(&[
        0xF0,
        0x00,
        0x20,
        0x29,
        0x02,
        0x18,
        0x0B,
        cc,
        rgb.r >> 2,
        rgb.g >> 2,
        rgb.b >> 2,
        0xF7,
    ]);
}

fn push_all(midi: &SharedMidi, target: &LedTargets) {
    for (&note, &state) in &target.pads {
        push_pad(midi, note, state);
    }
    for (&cc, &state) in &target.ccs {
        push_cc_palette(midi, cc, state);
    }
    for (i, &cc) in TOP_ROW_CCS.iter().enumerate() {
        push_top_rgb(midi, cc, target.top_row[i]);
    }
}

/// Send a palette-colour update for a top-row CC button. The MK2
/// accepts NoteOn on CC numbers too, but for clarity we emit the
/// RGB SysEx with a quantised palette — same hardware effect at a
/// slight wire-cost.
fn push_cc_palette(midi: &SharedMidi, cc: u8, state: PadState) {
    let (r, g, b) = match state {
        PadState::Empty => (0, 0, 0),
        PadState::OffDim(_) => (32, 32, 32),
        PadState::OnFlash { .. } => (255, 255, 255),
    };
    push_top_rgb(midi, cc, TopRowRgb { r, g, b });
}

fn clear_all_pads(midi: &SharedMidi) {
    let mut hub = midi.lock();
    // Brute-force: blank every grid pad note and every top-row CC.
    // Bounds chosen to cover the whole MK2 surface.
    for n in 11..=88u8 {
        let _ = hub.send_raw(&[0x90, n, 0]);
    }
    for cc in 104..=111u8 {
        let _ = hub.send_raw(&[0xB0, cc, 0]);
    }
}

/// Translate one chaser slot's output (intensity + RGB) into the actual
/// colour the matching fixture is being driven to right now. This mirrors
/// what the engine writes to DMX:
/// - Both colour and intensity present: scale RGB by intensity/255.
/// - RGB only (no intensity channel): use the colour at full.
/// - Intensity only (white-ish fixture): emit white scaled by intensity.
/// - Neither: dark.
fn slot_output_to_rgb(slot: &SlotOutput) -> TopRowRgb {
    let intensity = slot.intensity;
    let rgb = slot.rgb;
    match (rgb, intensity) {
        (Some(Rgb { r, g, b }), Some(i)) => {
            let scale = i as u16;
            TopRowRgb {
                r: ((r as u16 * scale) / 255) as u8,
                g: ((g as u16 * scale) / 255) as u8,
                b: ((b as u16 * scale) / 255) as u8,
            }
        }
        (Some(Rgb { r, g, b }), None) => TopRowRgb { r, g, b },
        (None, Some(i)) => TopRowRgb { r: i, g: i, b: i },
        (None, None) => TopRowRgb::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_is_disjoint() {
        // Catch a layout drift before two different roles fight for
        // the same note. Every default binding (notes + CCs) should
        // be unique within its address space.
        let defaults = crate::show::button_bindings::default_launchpad_bindings();
        let mut seen: std::collections::HashSet<(u8, bool)> = std::collections::HashSet::new();
        for b in &defaults {
            assert!(
                seen.insert((b.note, b.is_cc)),
                "duplicate binding at note {} cc={}",
                b.note,
                b.is_cc
            );
        }
    }

    #[test]
    fn slot_output_scales_rgb_by_intensity() {
        // RGB (255, 0, 0) at 50% intensity → (~127, 0, 0).
        let rgb = slot_output_to_rgb(&SlotOutput {
            intensity: Some(127),
            rgb: Some(Rgb { r: 255, g: 0, b: 0 }),
        });
        assert!(rgb.r >= 126 && rgb.r <= 128, "got r={}", rgb.r);
        assert_eq!(rgb.g, 0);
        assert_eq!(rgb.b, 0);
    }

    #[test]
    fn slot_output_uses_rgb_at_full_when_no_intensity() {
        let rgb = slot_output_to_rgb(&SlotOutput {
            intensity: None,
            rgb: Some(Rgb {
                r: 100,
                g: 200,
                b: 50,
            }),
        });
        assert_eq!((rgb.r, rgb.g, rgb.b), (100, 200, 50));
    }

    #[test]
    fn slot_output_emits_white_when_only_intensity() {
        // Halogen-style fixture: no RGB roles, just an intensity dimmer —
        // mirror as white scaled by the dimmer value.
        let rgb = slot_output_to_rgb(&SlotOutput {
            intensity: Some(180),
            rgb: None,
        });
        assert_eq!((rgb.r, rgb.g, rgb.b), (180, 180, 180));
    }

    #[test]
    fn slot_output_dark_when_neither_set() {
        let rgb = slot_output_to_rgb(&SlotOutput {
            intensity: None,
            rgb: None,
        });
        assert_eq!((rgb.r, rgb.g, rgb.b), (0, 0, 0));
    }

    #[test]
    fn is_launchpad_matches_common_names() {
        assert!(is_launchpad("Launchpad MK2"));
        assert!(is_launchpad("Launchpad Mini MK3"));
        assert!(is_launchpad("Launchpad X"));
        assert!(is_launchpad("MIDIIN2 (Launchpad Pro)"));
        assert!(!is_launchpad("FCB1010"));
        assert!(!is_launchpad("APC mini"));
    }
}
