//! Stream Deck device controller — single worker thread that owns the
//! device, polls input, and pushes LCD updates.
//!
//! Mirrors the shape of [`crate::midi::launchpad::LaunchpadController`]
//! but consolidates input + feedback into one thread (the
//! `elgato-streamdeck` API gives us `read_input(timeout)` so we can
//! interleave reads and writes naturally without the Mutex juggling
//! the MIDI hub needs).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use elgato_streamdeck::{list_devices, new_hidapi, StreamDeck, StreamDeckInput};
use hidapi::HidApi;
use image::DynamicImage;
use parking_lot::Mutex;
use tauri::AppHandle;

use crate::chaser::runtime::SlotOutput;
use crate::chaser::Rgb;
use crate::engine::loop_playback::SharedLoopPlayback;
use crate::engine::output_thread::{SharedChasers, SharedGlobals, SharedMovement};
use crate::engine::scene_playback::SharedScenePlayback;
use crate::engine::EngineState;
use crate::show::button_bindings::{ButtonAction, ButtonActiveMode, ButtonIcon, StreamDeckBinding};
use crate::show::ShowState;
use crate::streamdeck::render::{KeyVisual, RenderCache, TileKind};
use crate::streamdeck::StreamDeckDeviceInfo;

/// Brightness on a fresh connection. The MK.2 is bright enough at full
/// to wash out a stage in low light; 80 % is comfortable.
const BRIGHTNESS_PERCENT: u8 = 80;

/// Process-wide HidApi handle. The hidapi C library tracks global state
/// (a singleton IOHIDManager on macOS, a global device cache on Linux)
/// that gets torn down when the last `HidApi` value drops. Open
/// `HidDevice`s outlive that teardown — so dropping a transient
/// `HidApi` while our worker thread is still using its `StreamDeck`
/// (which owns a `HidDevice`) leaves the surviving device in undefined
/// territory. The next `new_hidapi()` then either deadlocks under the
/// re-init lock or returns "already initialised". We saw that hang the
/// app the moment the user opened the Stream Deck config tab, because
/// `list_streamdeck_devices` was creating a fresh `HidApi` while the
/// auto-connected worker was actively reading from the device.
///
/// Keeping a single `HidApi` alive in a `OnceLock` for the lifetime of
/// the process side-steps the entire problem: hid_init runs once,
/// hid_exit never runs, and every operation borrows the same instance
/// behind a `Mutex` (the C library serialises most operations
/// internally anyway).
static HIDAPI: OnceLock<Mutex<HidApi>> = OnceLock::new();

fn shared_hidapi() -> Result<&'static Mutex<HidApi>, String> {
    if let Some(api) = HIDAPI.get() {
        return Ok(api);
    }
    let api = new_hidapi().map_err(|e| format!("hidapi init failed: {e}"))?;
    // `set` only succeeds the first time. If we lost the race against a
    // concurrent caller, fall through and return the value the winner
    // installed.
    let _ = HIDAPI.set(Mutex::new(api));
    Ok(HIDAPI.get().expect("HIDAPI just set"))
}

/// Worker tick period. Drives input polling. The animation phase
/// advances on a separate, slower clock (`ANIMATION_FRAME_MS`) so the
/// input loop stays responsive (50 ms = 20 Hz polling) while the LCD
/// re-render cost stays bounded (10 fps animation).
const TICK_MS: u64 = 50;

/// Animation tick. Each `phase` increment is one frame of pulse /
/// strobe / orbit. 100 ms = 10 fps gives a smooth-enough sine pulse and
/// caps USB bandwidth at ~10 image uploads/sec/active-key.
const ANIMATION_FRAME_MS: u128 = 100;

pub struct StreamDeckController {
    shutdown: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    blind_held: Arc<AtomicBool>,
    globals: SharedGlobals,
    /// Snapshot of the device info at connect time. The worker thread
    /// owns the actual `StreamDeck` value; this struct only carries
    /// metadata for the status query.
    info: StreamDeckDeviceInfo,
}

#[derive(Clone)]
struct SdHandles {
    app: AppHandle,
    chasers: SharedChasers,
    movement: SharedMovement,
    globals: SharedGlobals,
    scenes: SharedScenePlayback,
    loops: SharedLoopPlayback,
    engine: EngineState,
    show: ShowState,
    blind_held: Arc<AtomicBool>,
}

impl StreamDeckController {
    /// Stop the worker and blank every key. Safe to call from any
    /// thread; the worker observes `shutdown` between ticks (so worst
    /// case is one tick of latency).
    pub fn shutdown(mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
        // Mirror the LP shutdown: never leave the show in blind if the
        // user yanked the cable while holding the key.
        if self.blind_held.swap(false, Ordering::Relaxed) {
            self.globals.lock().set_blind(false);
        }
    }

    pub fn info(&self) -> StreamDeckDeviceInfo {
        self.info.clone()
    }
}

pub type SharedStreamDeck = Arc<Mutex<Option<StreamDeckController>>>;

pub fn shared_streamdeck() -> SharedStreamDeck {
    Arc::new(Mutex::new(None))
}

/// Enumerate every connected Stream Deck. Borrows the shared HidApi
/// (see `HIDAPI` doc). The lock is held only for the brief
/// `list_devices` enumeration — `refresh_devices()` is intentionally
/// skipped here to avoid a multi-second IOKit scan on macOS while the
/// worker thread is mid-write to an open device. The user's "Refresh"
/// button hits this path and gets a cached view; for fresh enumeration
/// they can disconnect-then-reconnect, which always re-scans.
pub fn list_streamdeck_devices() -> Vec<StreamDeckDeviceInfo> {
    tracing::info!("list_streamdeck_devices: entering");
    let Ok(api_mutex) = shared_hidapi() else {
        tracing::warn!("list_streamdeck_devices: shared_hidapi failed");
        return Vec::new();
    };
    tracing::info!("list_streamdeck_devices: acquiring hidapi lock");
    let api = api_mutex.lock();
    tracing::info!("list_streamdeck_devices: lock acquired, enumerating");
    let result: Vec<StreamDeckDeviceInfo> = list_devices(&api)
        .into_iter()
        .map(|(kind, serial)| StreamDeckDeviceInfo {
            serial,
            kind: format!("{kind:?}"),
            key_count: kind.key_count(),
        })
        .collect();
    tracing::info!(count = result.len(), "list_streamdeck_devices: returning");
    result
}

/// Spawn the worker for a specific device. Picks the device whose serial
/// matches `serial`, or falls back to the first device if `serial` is
/// `None` (used by the auto-connect-at-launch path).
#[allow(clippy::too_many_arguments)]
pub fn start(
    app: AppHandle,
    serial: Option<&str>,
    chasers: SharedChasers,
    movement: SharedMovement,
    globals: SharedGlobals,
    scenes: SharedScenePlayback,
    loops: SharedLoopPlayback,
    engine: EngineState,
    show: ShowState,
) -> Result<StreamDeckController, String> {
    tracing::info!(?serial, "streamdeck::start: entering");
    let api_mutex = shared_hidapi()?;
    tracing::info!("streamdeck::start: acquiring hidapi lock for connect");
    let mut api = api_mutex.lock();
    // refresh_devices is necessary HERE (we want a fresh scan before
    // connecting) but NOT in list_streamdeck_devices (called frequently
    // from the UI). On macOS this scan can take ~hundreds of ms.
    let _ = api.refresh_devices();
    let devices = list_devices(&api);
    tracing::info!(count = devices.len(), "streamdeck::start: enumeration done");
    if devices.is_empty() {
        return Err("no streamdeck devices found".into());
    }
    let (kind, found_serial) = match serial {
        Some(s) => devices
            .into_iter()
            .find(|(_, ser)| ser == s)
            .ok_or_else(|| format!("streamdeck with serial {s} not found"))?,
        None => devices.into_iter().next().expect("non-empty checked above"),
    };
    tracing::info!(?kind, %found_serial, "streamdeck::start: connecting to device");
    let device = StreamDeck::connect(&api, kind, &found_serial)
        .map_err(|e| format!("streamdeck connect failed: {e}"))?;
    tracing::info!("streamdeck::start: device connected, releasing hidapi lock");
    // Drop the global HidApi lock now that the device handle is open —
    // subsequent reads from the worker thread don't need it, and other
    // Tauri commands (list_streamdeck_devices) need the lock available.
    drop(api);

    // Reset clears whatever previous client left on screen and brings
    // the panel to a known state.
    let _ = device.reset();
    let _ = device.set_brightness(BRIGHTNESS_PERCENT);

    let info = StreamDeckDeviceInfo {
        serial: found_serial.clone(),
        kind: format!("{kind:?}"),
        key_count: kind.key_count(),
    };

    let blind_held = Arc::new(AtomicBool::new(false));
    let handles = SdHandles {
        app,
        chasers,
        movement,
        globals: globals.clone(),
        scenes,
        loops,
        engine,
        show,
        blind_held: blind_held.clone(),
    };

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_for_thread = shutdown.clone();
    let key_count = kind.key_count() as usize;

    let worker = thread::Builder::new()
        .name("dmx-streamdeck-worker".into())
        .spawn(move || {
            run_worker(device, key_count, handles, shutdown_for_thread);
        })
        .expect("spawn streamdeck worker");

    Ok(StreamDeckController {
        shutdown,
        worker: Some(worker),
        blind_held,
        globals,
        info,
    })
}

fn run_worker(device: StreamDeck, key_count: usize, handles: SdHandles, shutdown: Arc<AtomicBool>) {
    let mut cache = RenderCache::default();
    let mut last_button_state = vec![false; key_count];

    // Animation phase: increments once every `ANIMATION_FRAME_MS`.
    // Independent of the input poll cadence so we can read input at
    // 20 Hz (low latency) while only re-pushing pulsing keys at 10 Hz
    // (cheap enough on USB even when several keys are active).
    let mut animation_phase: u32 = 0;
    let mut last_animation_tick = Instant::now();

    // Force an initial push: leftover state from a previous session
    // could otherwise persist on the LCDs until the first natural diff.
    tracing::info!(key_count, "streamdeck worker: pushing initial frame");
    let initial = compute_targets(&handles, key_count, animation_phase);
    push_all(&device, &mut cache, &initial);
    if let Err(e) = device.flush() {
        tracing::warn!(?e, "streamdeck worker: flush after initial push failed");
    }
    tracing::info!("streamdeck worker: initial frame pushed, entering loop");
    let mut last_targets = initial;

    while !shutdown.load(Ordering::Relaxed) {
        // 1. Poll input. Times out at TICK_MS so the loop progresses
        //    even when no buttons are touched.
        match device.read_input(Some(Duration::from_millis(TICK_MS))) {
            Ok(StreamDeckInput::ButtonStateChange(state)) => {
                process_button_changes(&state, &mut last_button_state, &handles);
            }
            Ok(_) => {
                // Encoder / touchscreen events come from MK.2-Plus and
                // similar — silently ignore on the MK.2 fixed layout.
            }
            Err(e) => {
                // Common failure mode: the user unplugged the device.
                // Log and exit so the controller can be replaced with a
                // fresh one on reconnect rather than spinning on errors.
                tracing::warn!(?e, "streamdeck read_input error — exiting worker");
                break;
            }
        }

        // 2. Advance the animation phase if enough wall-clock time has
        //    passed. We compare elapsed millis instead of "every other
        //    tick" because read_input's timeout is best-effort — a busy
        //    device or partial event can return early.
        let now = Instant::now();
        if now.duration_since(last_animation_tick).as_millis() >= ANIMATION_FRAME_MS {
            animation_phase = animation_phase.wrapping_add(1);
            last_animation_tick = now;
        }

        // 3. Compute desired LCD state and diff against last frame.
        //    Idle tiles emerge identical across ticks (their `phase`
        //    is forced to 0) — the diff catches that and skips the
        //    USB write. Active tiles carry the live `phase`, so they
        //    differ each animation tick and re-push.
        let target = compute_targets(&handles, key_count, animation_phase);
        diff_and_push(&device, &mut cache, &last_targets, &target);
        last_targets = target;
    }

    // Best-effort cleanup. If the device is unplugged these will fail
    // silently — that's fine.
    let _ = device.clear_all_button_images();
    let _ = device.reset();
}

fn process_button_changes(state: &[bool], last_state: &mut [bool], handles: &SdHandles) {
    // The crate gives us the full state vector each event — we have to
    // diff to detect transitions. Press = false→true, release = true→false.
    let len = state.len().min(last_state.len());
    for i in 0..len {
        if state[i] == last_state[i] {
            continue;
        }
        let pressed = state[i];
        last_state[i] = pressed;
        handle_button_transition(i as u8, pressed, handles);
    }
}

fn resolve_bindings(handles: &SdHandles) -> Vec<StreamDeckBinding> {
    let bindings = handles.show.read().show.button_bindings.clone();
    if bindings.custom_enabled {
        bindings.streamdeck
    } else {
        crate::show::button_bindings::default_streamdeck_bindings()
    }
}

/// Resolve `*ByIndex` against the show's lists. Mirror of the launchpad
/// helper — kept local so the two surfaces stay independent.
fn resolve_indexed_action(handles: &SdHandles, action: &ButtonAction) -> Option<ButtonAction> {
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
        other => other.clone(),
    })
}

fn is_action_active(action: &ButtonAction, handles: &SdHandles) -> bool {
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
        _ => false,
    }
}

fn handle_button_transition(key: u8, pressed: bool, handles: &SdHandles) {
    let bindings = resolve_bindings(handles);
    let Some(binding) = bindings.iter().find(|b| b.key == key) else {
        return;
    };
    let action = binding.action.clone();
    // Blind is momentary — both press and release dispatch.
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
                tracing::warn!(?err, "streamdeck action toggle_chaser failed");
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
                tracing::warn!(?err, "streamdeck action toggle_movement failed");
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
                tracing::warn!(?err, "streamdeck action recall_scene failed");
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
                tracing::warn!(?err, "streamdeck action blackout failed");
            }
        }
        ButtonAction::Blind => (),
        ButtonAction::Tap => {
            match crate::commands::tap_overall_bpm_impl(
                &handles.app,
                &handles.show,
                &handles.globals,
            ) {
                Ok(Some(bpm)) => tracing::info!(bpm, "streamdeck action tap → bpm"),
                Ok(None) => tracing::info!("streamdeck action tap → first of window"),
                Err(err) => tracing::warn!(?err, "streamdeck action tap failed"),
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
                tracing::warn!(?err, "streamdeck action toggle overall bpm failed");
            }
        }
        ButtonAction::BumpActiveChaserBpm { delta } => {
            let active = {
                let s = handles.show.read();
                s.show.chasers.iter().find(|c| c.enabled).cloned()
            };
            let Some(mut chaser) = active else { return };
            let crate::chaser::TempoSource::Fixed { bpm } = chaser.tempo;
            let new_bpm = (bpm + delta).clamp(20.0, 300.0);
            if (new_bpm - bpm).abs() < f32::EPSILON {
                return;
            }
            chaser.tempo = crate::chaser::TempoSource::Fixed { bpm: new_bpm };
            if let Err(err) = crate::commands::update_chaser_impl(
                &handles.app,
                &handles.show,
                &handles.chasers,
                chaser,
            ) {
                tracing::warn!(?err, "streamdeck BPM nudge failed");
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
                &handles.scenes,
                &handles.loops,
                &id,
            ) {
                tracing::warn!(?err, "streamdeck action start loop group failed");
            }
        }
        ButtonAction::StopLoopGroup => {
            crate::commands::stop_loop_group_impl(&handles.app, &handles.scenes, &handles.loops);
        }
        _ => (),
    }
}

fn compute_targets(handles: &SdHandles, key_count: usize, animation_phase: u32) -> Vec<KeyVisual> {
    let mut out = vec![KeyVisual::Empty; key_count];
    let bindings = resolve_bindings(handles);
    let active_slots = handles.chasers.lock().active_slot_outputs();
    let current_bpm = handles.show.read().show.globals.overall_bpm;

    for b in &bindings {
        if (b.key as usize) >= key_count {
            continue;
        }
        if let ButtonAction::None = b.action {
            out[b.key as usize] = KeyVisual::Empty;
            continue;
        }
        let resolved = resolve_indexed_action(handles, &b.action);
        let active = match b.active_mode {
            ButtonActiveMode::AlwaysIdle => false,
            ButtonActiveMode::AlwaysActive => true,
            ButtonActiveMode::Auto => resolved
                .as_ref()
                .map(|a| is_action_active(a, handles))
                .unwrap_or(false),
        };
        let phase = if active { animation_phase } else { 0 };
        // Live RGB slot mirror: only for chaser bindings whose action
        // resolves to a chaser that's actually enabled. Mirror of the
        // Launchpad top-row CC behaviour, but inlined into the tile.
        let slots = if active {
            match &resolved {
                Some(ButtonAction::ToggleChaser { .. }) => {
                    Some(slots_to_rgb(active_slots.as_deref()))
                }
                _ => None,
            }
        } else {
            None
        };
        // Substitute a runtime label when the binding's label is the
        // empty default and the action has a natural number to show
        // (current BPM for the toggle).
        let mut label = b.label.clone();
        if label.is_empty() {
            if let ButtonAction::ToggleOverallBpm = b.action {
                label = format!("{}", current_bpm.round() as u32);
            }
        }
        let kind = tile_kind_for(b.icon, &b.action);
        out[b.key as usize] = KeyVisual::Tile {
            kind,
            label,
            palette: (b.color_off, b.color_on),
            active,
            phase,
            slots,
        };
    }
    out
}

/// Pick a TileKind from the binding's icon and (as a fallback) its
/// action. The renderer's TileKind enum still drives the glyph; this
/// helper bridges from the user-facing ButtonIcon enum to it. When
/// the icon is None, infer one from the action so older shows (or
/// minimal bindings) still get a recognisable tile.
fn tile_kind_for(icon: ButtonIcon, action: &ButtonAction) -> TileKind {
    match icon {
        ButtonIcon::Play => TileKind::Chaser,
        ButtonIcon::Stage => TileKind::Scene,
        ButtonIcon::Orbit => TileKind::Movement,
        ButtonIcon::Bolt => TileKind::Blackout,
        ButtonIcon::Eye => TileKind::Blind,
        ButtonIcon::Tap => TileKind::Tap,
        ButtonIcon::Metronome => TileKind::BpmToggle,
        ButtonIcon::Loop => TileKind::LoopGroup,
        ButtonIcon::None => match action {
            ButtonAction::Blackout => TileKind::Blackout,
            ButtonAction::Blind => TileKind::Blind,
            ButtonAction::Tap => TileKind::Tap,
            ButtonAction::ToggleOverallBpm => TileKind::BpmToggle,
            ButtonAction::ToggleMovement { .. } | ButtonAction::ToggleMovementByIndex { .. } => {
                TileKind::Movement
            }
            ButtonAction::RecallScene { .. } | ButtonAction::RecallSceneByIndex { .. } => {
                TileKind::Scene
            }
            ButtonAction::StartLoopGroup { .. }
            | ButtonAction::StartLoopGroupByIndex { .. }
            | ButtonAction::StopLoopGroup => TileKind::LoopGroup,
            _ => TileKind::Chaser,
        },
    }
}

fn diff_and_push(
    device: &StreamDeck,
    cache: &mut RenderCache,
    last: &[KeyVisual],
    target: &[KeyVisual],
) {
    let mut pushed = 0u8;
    let mut errored = 0u8;
    for (key, visual) in target.iter().enumerate() {
        // Idle tiles serialise identically across ticks (their `phase`
        // is forced to 0 by `compute_targets`) so `prev == visual`
        // skips them. Active tiles carry the live phase, so they
        // differ each animation tick and re-push automatically. Live-
        // chaser tiles also include changing slot RGB in the visual,
        // which the Eq impl picks up.
        let needs_push = last.get(key).map(|prev| prev != visual).unwrap_or(true);
        if !needs_push {
            continue;
        }
        // Detect and log idle ↔ active transitions explicitly. Helps
        // distinguish "the show state changed and the worker noticed"
        // from "the worker is just animating". Only fires on real
        // toggles — not on every animation tick of an already-active
        // tile.
        if let (
            Some(KeyVisual::Tile {
                active: prev_active,
                ..
            }),
            KeyVisual::Tile { active, kind, .. },
        ) = (last.get(key), visual)
        {
            if prev_active != active {
                tracing::info!(
                    key,
                    ?kind,
                    from_active = prev_active,
                    to_active = active,
                    "streamdeck tile transition"
                );
            }
        }
        let img = cache.render(visual);
        // Cheap rolling fingerprint of the image bytes — catches the
        // "every push has identical bytes" failure mode where the
        // worker thinks it's pushing different frames but the renderer
        // is actually producing the same content. Sample 5 pixels
        // spread across the image so a constant background change is
        // visible without paying for a full sha256.
        let fingerprint = if let DynamicImage::ImageRgb8(buf) = &img {
            let s = KEY_IMAGE_SIZE_FOR_FINGERPRINT;
            let mut acc: u32 = 0;
            for (px, py) in [
                (s / 4, s / 4),
                (s / 2, s / 2),
                (3 * s / 4, 3 * s / 4),
                (s / 2, s / 4),
                (s / 4, 3 * s / 4),
            ] {
                let p = buf.get_pixel(px, py).0;
                acc = acc.wrapping_mul(131).wrapping_add(p[0] as u32);
                acc = acc.wrapping_mul(131).wrapping_add(p[1] as u32);
                acc = acc.wrapping_mul(131).wrapping_add(p[2] as u32);
            }
            acc
        } else {
            0
        };
        match device.set_button_image(key as u8, img) {
            Ok(_) => {
                pushed += 1;
                tracing::debug!(
                    key,
                    fingerprint = format!("{:08x}", fingerprint),
                    "pushed key"
                );
            }
            Err(e) => {
                errored += 1;
                tracing::warn!(key, ?e, "streamdeck set_button_image failed");
            }
        }
    }
    // Flush after the batch. Some Stream Deck firmwares (notably the
    // OriginalV2) buffer image-set packets and only commit them on the
    // next read or on an explicit flush — without this the device can
    // happily ack each set_button_image but display only the first
    // one. Cheap call (a couple of HID writes), worth the safety.
    if pushed > 0 {
        if let Err(e) = device.flush() {
            tracing::warn!(?e, "streamdeck flush failed");
        }
    }
    if pushed > 0 || errored > 0 {
        tracing::debug!(pushed, errored, "streamdeck diff_and_push tick");
    }
}

/// Local copy of the key image side — `KEY_IMAGE_SIZE` lives in
/// `render` and is `pub`, but importing the type-state of the renderer
/// here just for a fingerprint sample would be backwards. Re-declare
/// at controller scope to keep the tracing helper self-contained.
const KEY_IMAGE_SIZE_FOR_FINGERPRINT: u32 = 72;

fn push_all(device: &StreamDeck, cache: &mut RenderCache, target: &[KeyVisual]) {
    let mut pushed = 0u8;
    let mut errored = 0u8;
    for (key, visual) in target.iter().enumerate() {
        let img = cache.render(visual);
        match device.set_button_image(key as u8, img) {
            Ok(_) => pushed += 1,
            Err(e) => {
                errored += 1;
                tracing::warn!(key, ?e, "streamdeck push_all set_button_image failed");
            }
        }
    }
    tracing::info!(pushed, errored, "streamdeck push_all complete");
}

/// Slot outputs → simple RGB tuples for the live strip. Mirrors
/// `slot_output_to_rgb` in [`crate::midi::launchpad`] except we always
/// produce 8 entries (the Launchpad has 8 round buttons; we render 8
/// stripes inside the chaser key for visual parity).
fn slots_to_rgb(slots: Option<&[SlotOutput]>) -> [(u8, u8, u8); 8] {
    let mut out = [(0u8, 0u8, 0u8); 8];
    let Some(slots) = slots else {
        return out;
    };
    for (i, slot) in slots.iter().take(8).enumerate() {
        out[i] = match (slot.rgb, slot.intensity) {
            (Some(Rgb { r, g, b }), Some(intensity)) => {
                let scale = intensity as u16;
                (
                    ((r as u16 * scale) / 255) as u8,
                    ((g as u16 * scale) / 255) as u8,
                    ((b as u16 * scale) / 255) as u8,
                )
            }
            (Some(Rgb { r, g, b }), None) => (r, g, b),
            (None, Some(i)) => (i, i, i),
            (None, None) => (0, 0, 0),
        };
    }
    out
}
