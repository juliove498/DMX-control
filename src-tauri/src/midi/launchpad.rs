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
use crate::engine::output_thread::{SharedChasers, SharedGlobals, SharedMovement};
use crate::engine::scene_playback::SharedScenePlayback;
use crate::engine::EngineState;
use crate::midi::hub::SharedMidi;
use crate::midi::MidiMessage;
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
pub const BLACKOUT_NOTE: u8 = 19;
pub const BLIND_NOTE: u8 = 29;

/// (dim, bright) palette pairs for chaser pads. Off pads sit at `dim`,
/// active pads flash between `dim` and `bright`.
const CHASER_PAD_PALETTE: [(u8, u8); 8] = [
    (7, 5),   // red
    (11, 9),  // orange
    (15, 13), // yellow
    (19, 17), // green
    (39, 37), // cyan
    (43, 41), // light blue
    (47, 45), // blue
    (55, 53), // magenta
];

/// Distinct palette for movement pads — picked so a glance at the LP
/// instantly tells chasers (row 1) and movements (row 2) apart even
/// when both rows are partly lit.
const MOVEMENT_PAD_PALETTE: [(u8, u8); 8] = [
    (96, 95),  // pink
    (82, 81),  // purple
    (85, 84),  // amber
    (74, 73),  // lime
    (34, 33),  // teal
    (50, 49),  // fuchsia
    (57, 56),  // pale pink
    (100, 99), // periwinkle
];

/// Yet another distinct palette for scene pads — using the cool side of
/// the wheel (blues, violets, teals, whites) so chasers/movements/
/// scenes are all visually separable.
const SCENE_PAD_PALETTE: [(u8, u8); 8] = [
    (1, 3),     // white
    (43, 41),   // light blue
    (47, 45),   // blue
    (78, 79),   // sea-green
    (44, 117),  // pale teal
    (115, 116), // pastel rose
    (113, 114), // pastel violet
    (60, 61),   // mint
];

const BLACKOUT_PALETTE: (u8, u8) = (7, 5); // dim red / bright red
const BLIND_PALETTE: (u8, u8) = (1, 3); // dim white / bright white

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
#[derive(Debug, Clone, PartialEq, Eq)]
struct LedTargets {
    chasers: [PadState; 8],
    movements: [PadState; 8],
    scenes: [PadState; 8],
    /// Top row of round buttons. Each entry is the live RGB the matching
    /// chaser slot is being driven to right now (after intensity scaling).
    /// All zeros means dark — either no chaser is enabled or that slot is
    /// in its "off" frame within the pattern.
    top_row: [TopRowRgb; 8],
    blackout: PadState,
    blind: PadState,
}

impl LedTargets {
    fn empty() -> Self {
        Self {
            chasers: [PadState::Empty; 8],
            movements: [PadState::Empty; 8],
            scenes: [PadState::Empty; 8],
            top_row: [TopRowRgb::default(); 8],
            blackout: PadState::Empty,
            blind: PadState::Empty,
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

fn handle_note(msg: &MidiMessage, handles: &LpHandles) {
    let Some(note) = msg.data1 else { return };
    let vel = msg.data2.unwrap_or(0);

    if note == BLIND_NOTE {
        let pressed = vel > 0;
        handles.blind_held.store(pressed, Ordering::Relaxed);
        handles.globals.lock().set_blind(pressed);
        let _ = tauri::Emitter::emit(
            &handles.app,
            crate::commands::BLIND_EVENT,
            crate::commands::BlindChange { pressed },
        );
        return;
    }

    // Everything else latches on press only.
    if vel == 0 {
        return;
    }

    if note == BLACKOUT_NOTE {
        let on = !handles.show.read().show.globals.blackout.active;
        if let Err(err) =
            crate::commands::set_blackout_impl(&handles.app, &handles.show, &handles.globals, on)
        {
            tracing::warn!(?err, "launchpad blackout dispatch failed");
        }
        return;
    }

    if let Some(pad_idx) = CHASER_PAD_NOTES.iter().position(|&n| n == note) {
        handle_chaser_press(pad_idx, handles);
        return;
    }

    if let Some(pad_idx) = MOVEMENT_PAD_NOTES.iter().position(|&n| n == note) {
        handle_movement_press(pad_idx, handles);
        return;
    }

    if let Some(pad_idx) = SCENE_PAD_NOTES.iter().position(|&n| n == note) {
        handle_scene_press(pad_idx, handles);
    }
}

fn handle_cc(msg: &MidiMessage, handles: &LpHandles) {
    let Some(cc) = msg.data1 else { return };
    let val = msg.data2.unwrap_or(0);
    // CCs send press as val=127 and release as val=0 — only act on press
    // so a single tap nudges BPM by exactly one step.
    if val == 0 {
        return;
    }
    match cc {
        BPM_UP_CC => bump_active_chaser_bpm(handles, BPM_STEP),
        BPM_DOWN_CC => bump_active_chaser_bpm(handles, -BPM_STEP),
        _ => (),
    }
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

    let (chasers_snapshot, movements_snapshot, scenes_snapshot, blackout_active) = {
        let s = handles.show.read();
        (
            s.show.chasers.clone(),
            s.show.movements.clone(),
            s.show.scenes.clone(),
            s.show.globals.blackout.active,
        )
    };
    let active_scene_id = handles
        .scenes
        .lock()
        .active_scene_id()
        .map(|s| s.to_string());

    // Chasers --------------------------------------------------------------
    for (i, ch) in chasers_snapshot.iter().take(8).enumerate() {
        let (dim, bright) = CHASER_PAD_PALETTE[i];
        out.chasers[i] = if ch.enabled {
            PadState::OnFlash { dim, bright }
        } else {
            PadState::OffDim(dim)
        };
    }

    // Top row --------------------------------------------------------------
    // Live mirror of the active chaser's per-slot output so the round
    // buttons emulate the actual fixtures. We pull the latest slot
    // outputs straight from the engine — that's what the DMX universe
    // received on the previous frame, scaled per-slot for intensity.
    if let Some(slots) = handles.chasers.lock().active_slot_outputs() {
        for (i, slot) in slots.iter().take(8).enumerate() {
            out.top_row[i] = slot_output_to_rgb(slot);
        }
    }

    // Movements ------------------------------------------------------------
    for (i, m) in movements_snapshot.iter().take(8).enumerate() {
        let (dim, bright) = MOVEMENT_PAD_PALETTE[i];
        out.movements[i] = if m.enabled {
            PadState::OnFlash { dim, bright }
        } else {
            PadState::OffDim(dim)
        };
    }

    // Scenes (row 3) -------------------------------------------------------
    for (i, scene) in scenes_snapshot.iter().take(8).enumerate() {
        let (dim, bright) = SCENE_PAD_PALETTE[i];
        out.scenes[i] = if active_scene_id.as_deref() == Some(scene.id.as_str()) {
            PadState::OnFlash { dim, bright }
        } else {
            PadState::OffDim(dim)
        };
    }

    // Blackout (toggle) ---------------------------------------------------
    let (bo_dim, bo_bright) = BLACKOUT_PALETTE;
    out.blackout = if blackout_active {
        PadState::OnFlash {
            dim: bo_dim,
            bright: bo_bright,
        }
    } else {
        PadState::OffDim(bo_dim)
    };

    // Blind (momentary) ---------------------------------------------------
    let (bl_dim, bl_bright) = BLIND_PALETTE;
    out.blind = if handles.blind_held.load(Ordering::Relaxed) {
        PadState::OnFlash {
            dim: bl_dim,
            bright: bl_bright,
        }
    } else {
        PadState::OffDim(bl_dim)
    };

    out
}

fn diff_and_push(midi: &SharedMidi, last: &LedTargets, target: &LedTargets) {
    for i in 0..8 {
        if target.chasers[i] != last.chasers[i] {
            push_pad(midi, CHASER_PAD_NOTES[i], target.chasers[i]);
        }
        if target.movements[i] != last.movements[i] {
            push_pad(midi, MOVEMENT_PAD_NOTES[i], target.movements[i]);
        }
        if target.scenes[i] != last.scenes[i] {
            push_pad(midi, SCENE_PAD_NOTES[i], target.scenes[i]);
        }
        if target.top_row[i] != last.top_row[i] {
            push_top_rgb(midi, TOP_ROW_CCS[i], target.top_row[i]);
        }
    }
    if target.blackout != last.blackout {
        push_pad(midi, BLACKOUT_NOTE, target.blackout);
    }
    if target.blind != last.blind {
        push_pad(midi, BLIND_NOTE, target.blind);
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
    for i in 0..8 {
        push_pad(midi, CHASER_PAD_NOTES[i], target.chasers[i]);
        push_pad(midi, MOVEMENT_PAD_NOTES[i], target.movements[i]);
        push_pad(midi, SCENE_PAD_NOTES[i], target.scenes[i]);
        push_top_rgb(midi, TOP_ROW_CCS[i], target.top_row[i]);
    }
    push_pad(midi, BLACKOUT_NOTE, target.blackout);
    push_pad(midi, BLIND_NOTE, target.blind);
}

fn clear_all_pads(midi: &SharedMidi) {
    let mut hub = midi.lock();
    for &n in &CHASER_PAD_NOTES {
        let _ = hub.send_raw(&[0x90, n, 0]);
    }
    for &n in &MOVEMENT_PAD_NOTES {
        let _ = hub.send_raw(&[0x90, n, 0]);
    }
    for &n in &SCENE_PAD_NOTES {
        let _ = hub.send_raw(&[0x90, n, 0]);
    }
    for &cc in &TOP_ROW_CCS {
        let _ = hub.send_raw(&[0xB0, cc, 0]);
    }
    let _ = hub.send_raw(&[0x90, BLACKOUT_NOTE, 0]);
    let _ = hub.send_raw(&[0x90, BLIND_NOTE, 0]);
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

fn handle_chaser_press(pad_idx: usize, handles: &LpHandles) {
    let target = {
        let s = handles.show.read();
        s.show
            .chasers
            .get(pad_idx)
            .map(|c| (c.id.clone(), !c.enabled))
    };
    let Some((id, desired)) = target else {
        return;
    };
    if let Err(err) = crate::commands::toggle_chaser_impl(
        &handles.app,
        &handles.show,
        &handles.chasers,
        &id,
        desired,
    ) {
        tracing::warn!(?err, "launchpad pad toggle_chaser failed");
    }
}

/// Toggle the n-th scene from the show: pressing the pad of the
/// currently-active scene **releases** it (restoring the pre-recall FX
/// context); pressing any other scene's pad recalls it. Routes through
/// `recall_scene_impl` so the launchpad respects the same multi-step
/// playback + FX capture activation as the UI's GO button.
fn handle_scene_press(pad_idx: usize, handles: &LpHandles) {
    let scene_id = {
        let s = handles.show.read();
        s.show.scenes.get(pad_idx).map(|c| c.id.clone())
    };
    let Some(scene_id) = scene_id else {
        return;
    };
    // Snapshot the currently-active scene id and drop the lock before
    // calling release/recall — both internally take the same lock.
    let active_id = handles
        .scenes
        .lock()
        .active_scene_id()
        .map(|s| s.to_string());
    if active_id.as_deref() == Some(scene_id.as_str()) {
        // Same scene that's already playing → toggle off. release()
        // also fires the pre-recall FX restoration request through the
        // playback's channel, so chasers/movements come back to the
        // pre-recall state.
        handles.scenes.lock().release(std::time::Instant::now());
        let _ = tauri::Emitter::emit(
            &handles.app,
            crate::commands::SCENE_ACTIVE_EVENT,
            crate::commands::SceneActiveChange {
                active_scene_id: None,
                step_index: None,
            },
        );
        tracing::info!(scene = %scene_id, "launchpad released active scene");
        return;
    }
    if let Err(err) = crate::commands::recall_scene_impl(
        &handles.app,
        &handles.engine,
        &handles.show,
        &handles.chasers,
        &handles.movement,
        &handles.scenes,
        &scene_id,
    ) {
        tracing::warn!(?err, "launchpad scene recall failed");
    }
}

fn handle_movement_press(pad_idx: usize, handles: &LpHandles) {
    let target = {
        let s = handles.show.read();
        s.show
            .movements
            .get(pad_idx)
            .map(|m| (m.id.clone(), !m.enabled))
    };
    let Some((id, desired)) = target else {
        return;
    };
    if let Err(err) = crate::commands::toggle_movement_impl(
        &handles.app,
        &handles.show,
        &handles.movement,
        &id,
        desired,
    ) {
        tracing::warn!(?err, "launchpad pad toggle_movement failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_palette_covers_all_eight_pads() {
        // Off-by-one in the palette would silently dim or mis-colour a
        // pad — guard the table size matches the note row.
        assert_eq!(CHASER_PAD_NOTES.len(), 8);
        assert_eq!(CHASER_PAD_PALETTE.len(), 8);
        assert_eq!(MOVEMENT_PAD_NOTES.len(), 8);
        assert_eq!(MOVEMENT_PAD_PALETTE.len(), 8);
        assert_eq!(SCENE_PAD_NOTES.len(), 8);
        assert_eq!(SCENE_PAD_PALETTE.len(), 8);
    }

    #[test]
    fn pad_layout_is_disjoint() {
        // Catch a layout drift before two different roles fight for
        // the same note. Scene buttons + chaser/movement pads + the
        // two scene-column notes should all be distinct.
        let mut all = Vec::new();
        all.extend_from_slice(&CHASER_PAD_NOTES);
        all.extend_from_slice(&MOVEMENT_PAD_NOTES);
        all.extend_from_slice(&SCENE_PAD_NOTES);
        all.push(BLACKOUT_NOTE);
        all.push(BLIND_NOTE);
        let unique: std::collections::HashSet<_> = all.iter().copied().collect();
        assert_eq!(all.len(), unique.len(), "duplicate pad note in layout");
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
