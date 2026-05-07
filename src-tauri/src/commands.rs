use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use ts_rs::TS;

use crate::chaser::AmbientChaser;
use crate::engine::output_thread::{
    shared_bindings, OutputBinding, OutputThreadHandle, SharedChasers, SharedGlobals,
    SharedMovement,
};
use crate::engine::scene_playback::SharedScenePlayback;
use crate::engine::{EngineError, EngineState, EngineStats, DMX_CHANNELS};
use crate::globals::GlobalsConfig;
use crate::midi::hub::SharedMidi;
use crate::midi::launchpad::{self, SharedLaunchpad};
use crate::midi::{MidiDeviceInfo, MidiStatus};
use crate::movement::MovementGenerator;
use crate::output::config::{instantiate, OutputBindingConfig, OutputsConfig};
use crate::output::d2xx::{list_devices as list_d2xx_devices, D2xxDeviceInfo};
use crate::output::discovery::{list_serial_ports, SerialPortInfo};
use crate::programmer::{ProgrammerStatus, SharedProgrammer};
use crate::show::file::{load as load_show_file, save as save_show_file, ShowError, ShowFileV1};
use crate::show::fixture::{validate_patch, FixtureDefinition, FixtureInstance, PatchReport};
use crate::show::library::{ensure_seeded, library_dir, load_all, save_def};
use crate::show::scene::{Scene, SceneChannel, SceneFixture, SceneFxState, SceneStep};
use crate::show::ShowState;

pub const STATS_EVENT: &str = "engine:stats";
pub const SHOW_EVENT: &str = "show:updated";
// Push events for the mobile remote bridge — also consumable by the
// desktop frontend so the 200 ms polling loop in ScenesView can be
// retired without changing the rest of the contract.
pub const PROGRAMMER_EVENT: &str = "programmer:changed";
pub const SCENE_ACTIVE_EVENT: &str = "scene:active_changed";
pub const MASTER_EVENT: &str = "engine:master_changed";
pub const BLIND_EVENT: &str = "engine:blind_changed";

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct SceneActiveChange {
    pub active_scene_id: Option<String>,
    pub step_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct MasterChange {
    pub master: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct BlindChange {
    pub pressed: bool,
}

pub struct OutputThreadState(pub Mutex<Option<OutputThreadHandle>>);

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct UniverseSnapshot {
    pub id: u16,
    pub data: Vec<u8>,
}

// ---- Direct DMX commands (Phase 1) ---------------------------------------

#[tauri::command]
pub fn set_channel(
    engine: State<'_, EngineState>,
    universe: u16,
    channel: u16,
    value: u8,
) -> Result<(), EngineError> {
    let result = engine.write().set_channel(universe, channel, value);
    match &result {
        Ok(_) => tracing::trace!(
            target: "dmx::input",
            universe,
            channel_1based = channel + 1,
            value,
            "fader → engine"
        ),
        Err(e) => tracing::warn!(
            target: "dmx::input",
            universe,
            channel_1based = channel + 1,
            value,
            error = %e,
            "set_channel failed"
        ),
    }
    result
}

#[tauri::command]
pub fn set_master(app: AppHandle, engine: State<'_, EngineState>, value: u8) {
    tracing::trace!(target: "dmx::input", value, "master → engine");
    engine.write().master = value;
    let _ = app.emit(MASTER_EVENT, MasterChange { master: value });
}

/// Legacy: kept temporarily so any code path still calling this command
/// doesn't compile-error. Routes through the globals runtime so the user's
/// configured fade times still apply.
#[tauri::command]
pub fn blackout(
    app: AppHandle,
    show: State<'_, ShowState>,
    globals: State<'_, SharedGlobals>,
    on: bool,
) -> Result<(), CommandError> {
    set_blackout(app, show, globals, on)
}

#[tauri::command]
pub fn set_blackout(
    app: AppHandle,
    show: State<'_, ShowState>,
    globals: State<'_, SharedGlobals>,
    on: bool,
) -> Result<(), CommandError> {
    set_blackout_impl(&app, &show, &globals, on)
}

/// Persist blackout state to the show file and apply it to the runtime.
/// Free function so surfaces (Launchpad, future scripting) can drive
/// blackout without going through Tauri's IPC.
pub fn set_blackout_impl(
    app: &AppHandle,
    show: &ShowState,
    globals: &SharedGlobals,
    on: bool,
) -> Result<(), CommandError> {
    tracing::debug!(target: "dmx::input", on, "blackout → globals");
    {
        let mut s = show.write();
        s.show.globals.blackout.active = on;
        s.dirty = true;
    }
    globals.lock().set_blackout(on);
    persist_show(show, app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

/// Momentary halogen-blinder press. Not persisted: only the runtime knows
/// the user is holding the button right now.
#[tauri::command]
pub fn set_blind(app: AppHandle, globals: State<'_, SharedGlobals>, pressed: bool) {
    tracing::trace!(target: "dmx::input", pressed, "blind → globals");
    globals.lock().set_blind(pressed);
    let _ = app.emit(BLIND_EVENT, BlindChange { pressed });
}

#[tauri::command]
pub fn get_globals(show: State<'_, ShowState>) -> GlobalsConfig {
    show.read().show.globals.clone()
}

/// Replace the persisted globals config (fade times, blind fixture list,
/// blackout target). The runtime is updated in place — fades to the new
/// targets begin on the next frame.
#[tauri::command]
pub fn update_globals(
    app: AppHandle,
    show: State<'_, ShowState>,
    globals: State<'_, SharedGlobals>,
    config: GlobalsConfig,
) -> Result<(), CommandError> {
    {
        let mut s = show.write();
        s.show.globals = config.clone();
        s.dirty = true;
    }
    globals.lock().replace_config(config);
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

#[tauri::command]
pub fn clear_universe(engine: State<'_, EngineState>, universe: u16) -> Result<(), EngineError> {
    let mut g = engine.write();
    let u = g
        .universes
        .iter_mut()
        .find(|u| u.id == universe)
        .ok_or(EngineError::UniverseNotFound(universe))?;
    u.blackout();
    Ok(())
}

#[tauri::command]
pub fn get_universe(
    engine: State<'_, EngineState>,
    universe: u16,
) -> Result<UniverseSnapshot, EngineError> {
    let g = engine.read();
    let u = g
        .universes
        .iter()
        .find(|u| u.id == universe)
        .ok_or(EngineError::UniverseNotFound(universe))?;
    Ok(UniverseSnapshot {
        id: u.id,
        data: u.data.to_vec(),
    })
}

/// The same shape as `get_universe` but returns the *post-merge* snapshot
/// — base + effects (chaser, movement) + blind, scaled by master and
/// blackout. Stage uses this so the per-fixture colour bar reflects what's
/// actually on the wire, including running chasers and the halogen blind.
/// Direct Output keeps using the raw `get_universe` so its sliders stay
/// pinned to what the user wrote, not what the engine is animating.
#[tauri::command]
pub fn get_universe_output(
    engine: State<'_, EngineState>,
    universe: u16,
) -> Result<UniverseSnapshot, EngineError> {
    let snap = engine
        .read()
        .snapshot_universe(universe)
        .ok_or(EngineError::UniverseNotFound(universe))?;
    Ok(UniverseSnapshot {
        id: universe,
        data: snap.to_vec(),
    })
}

#[tauri::command]
pub fn list_universes(engine: State<'_, EngineState>) -> Vec<u16> {
    engine.read().universes.iter().map(|u| u.id).collect()
}

#[tauri::command]
pub fn dmx_channels() -> usize {
    DMX_CHANNELS
}

// ---- Outputs (Phase 2) ---------------------------------------------------

#[tauri::command]
pub fn list_serial_ports_cmd() -> Vec<SerialPortInfo> {
    list_serial_ports()
}

#[tauri::command]
pub fn list_ftdi_devices() -> Vec<D2xxDeviceInfo> {
    list_d2xx_devices()
}

#[tauri::command]
pub fn get_outputs(show: State<'_, ShowState>) -> OutputsConfig {
    show.read().show.outputs.clone()
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn set_outputs(
    app: AppHandle,
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
    output_thread: State<'_, OutputThreadState>,
    chasers: State<'_, SharedChasers>,
    movement: State<'_, SharedMovement>,
    globals: State<'_, SharedGlobals>,
    scenes: State<'_, SharedScenePlayback>,
    outputs: OutputsConfig,
) -> Result<(), CommandError> {
    {
        let mut s = show.write();
        s.show.outputs = outputs.clone();
        s.dirty = true;
    }
    apply_outputs(
        &app,
        &engine,
        &output_thread,
        &chasers,
        &movement,
        &globals,
        &scenes,
        &outputs,
    )?;
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

// ---- Show / Fixtures (Phase 3) -------------------------------------------

#[tauri::command]
pub fn list_fixture_definitions(show: State<'_, ShowState>) -> Vec<FixtureDefinition> {
    show.read().library.values().cloned().collect()
}

#[tauri::command]
pub fn reload_library(show: State<'_, ShowState>) -> Result<usize, CommandError> {
    let dir = library_dir().ok_or_else(|| CommandError::Other("no config dir".into()))?;
    ensure_seeded(&dir).map_err(|e| CommandError::Io(e.to_string()))?;
    let lib = load_all(&dir).map_err(|e| CommandError::Show(e.to_string()))?;
    let n = lib.len();
    show.write().library = lib;
    Ok(n)
}

#[tauri::command]
pub fn get_show(show: State<'_, ShowState>) -> ShowFileV1 {
    show.read().show.clone()
}

/// Inline rename of the current show. Doesn't touch the file path —
/// that only changes via Save As. Persisted through the normal autosave
/// path so the new name survives a restart even if the user never hits
/// Save explicitly.
#[tauri::command]
pub fn rename_show(
    app: AppHandle,
    show: State<'_, ShowState>,
    name: String,
) -> Result<(), CommandError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(CommandError::Other("show name cannot be empty".into()));
    }
    {
        let mut s = show.write();
        s.show.name = trimmed.to_string();
        s.dirty = true;
    }
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

/// Pick the fixture definitions actually referenced by the show's
/// patched fixtures, preserving each one only once. Iterates fixtures
/// in order so a stable bundle order means stable diffs across saves.
fn bundle_used_definitions(show: &ShowState) -> Vec<FixtureDefinition> {
    let s = show.read();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut out: Vec<FixtureDefinition> = Vec::new();
    for f in &s.show.fixtures {
        if !seen.insert(f.definition_id.as_str()) {
            continue;
        }
        if let Some(def) = s.library.get(&f.definition_id) {
            out.push(def.clone());
        }
    }
    out
}

#[tauri::command]
pub fn get_show_path(show: State<'_, ShowState>) -> Option<String> {
    show.read()
        .path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn new_show(
    app: AppHandle,
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
    output_thread: State<'_, OutputThreadState>,
    chasers: State<'_, SharedChasers>,
    movement: State<'_, SharedMovement>,
    globals: State<'_, SharedGlobals>,
    scenes: State<'_, SharedScenePlayback>,
) -> Result<(), CommandError> {
    let outputs = {
        let mut s = show.write();
        s.show = ShowFileV1::default();
        s.path = None;
        s.dirty = false;
        s.show.outputs.clone()
    };
    scenes.lock().release(std::time::Instant::now());
    apply_outputs(
        &app,
        &engine,
        &output_thread,
        &chasers,
        &movement,
        &globals,
        &scenes,
        &outputs,
    )?;
    sync_chasers(&show, &chasers);
    sync_movements(&show, &movement);
    sync_globals(&show, &globals);
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn open_show(
    app: AppHandle,
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
    output_thread: State<'_, OutputThreadState>,
    chasers: State<'_, SharedChasers>,
    movement: State<'_, SharedMovement>,
    globals: State<'_, SharedGlobals>,
    scenes: State<'_, SharedScenePlayback>,
    path: String,
) -> Result<(), CommandError> {
    let p = PathBuf::from(&path);
    let loaded = load_show_file(&p).map_err(CommandError::from)?;
    let outputs = loaded.outputs.clone();
    let bundled_defs = loaded.library.clone();
    {
        let mut s = show.write();
        s.show = loaded;
        s.path = Some(p);
        s.dirty = false;
        // Merge the bundled defs into the runtime library so fixtures
        // referencing definitions the local install doesn't have still
        // resolve. Same-id entries from disk get overwritten — the show
        // file is treated as authoritative for any fixture it bundles.
        for def in &bundled_defs {
            s.library.insert(def.id.clone(), def.clone());
        }
    }
    // Persist the bundled defs to the on-disk library so they survive a
    // restart. Done outside the lock to keep disk I/O off the hot path.
    // Best-effort: if a write fails, the runtime still has the def for
    // this session and we just log a warning.
    if !bundled_defs.is_empty() {
        if let Some(lib_dir) = library_dir() {
            for def in &bundled_defs {
                if let Err(e) = save_def(&lib_dir, def) {
                    tracing::warn!(
                        target: "dmx::library",
                        id = %def.id,
                        error = %e,
                        "could not persist bundled definition; runtime still has it"
                    );
                } else {
                    tracing::info!(
                        target: "dmx::library",
                        id = %def.id,
                        "bundled definition persisted to library"
                    );
                }
            }
        }
    }
    scenes.lock().release(std::time::Instant::now());
    apply_outputs(
        &app,
        &engine,
        &output_thread,
        &chasers,
        &movement,
        &globals,
        &scenes,
        &outputs,
    )?;
    sync_chasers(&show, &chasers);
    sync_movements(&show, &movement);
    sync_globals(&show, &globals);
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

#[tauri::command]
pub fn save_show(
    app: AppHandle,
    show: State<'_, ShowState>,
    path: Option<String>,
) -> Result<String, CommandError> {
    let target_path: PathBuf = match path {
        Some(p) => PathBuf::from(p),
        None => {
            let s = show.read();
            s.path
                .clone()
                .ok_or_else(|| CommandError::Other("no path; provide one".into()))?
        }
    };
    // "Save As" with a fresh path → take the filename stem as the new
    // show name, but only if the user hasn't already given the show a
    // name they care about. "Untitled" is the default; if it's still
    // sitting on that, the filename is a better label.
    let was_unnamed = {
        let s = show.read();
        s.show.name.is_empty() || s.show.name == "Untitled"
    };
    if was_unnamed {
        if let Some(stem) = target_path.file_stem().and_then(|s| s.to_str()) {
            show.write().show.name = stem.to_string();
        }
    }

    // Snapshot the show, then bundle into it just the fixture defs that
    // its `fixtures` reference. Saving the whole installed library would
    // bloat the file with dozens of unrelated definitions; saving only
    // the in-use ones keeps the file portable + small. The on-disk
    // library JSONs are NOT modified here — bundling is purely a copy
    // into the show file.
    let mut snapshot = show.read().show.clone();
    snapshot.library = bundle_used_definitions(&show);

    save_show_file(&target_path, &snapshot).map_err(CommandError::from)?;
    {
        let mut s = show.write();
        s.path = Some(target_path.clone());
        s.dirty = false;
    }
    if let Err(e) = crate::show::session::write_autosave(&snapshot, Some(&target_path)) {
        tracing::warn!(error = %e, "autosave failed");
    }
    let _ = app.emit(SHOW_EVENT, ());
    Ok(target_path.to_string_lossy().to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn add_fixture(
    app: AppHandle,
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
    chasers: State<'_, SharedChasers>,
    movement: State<'_, SharedMovement>,
    globals: State<'_, SharedGlobals>,
    fixture: FixtureInstance,
) -> Result<(), CommandError> {
    let new_ids;
    {
        let mut s = show.write();
        if s.show.fixtures.iter().any(|f| f.id == fixture.id) {
            return Err(CommandError::Other(format!(
                "fixture id {} already exists",
                fixture.id
            )));
        }
        new_ids = vec![fixture.id.clone()];
        s.show.fixtures.push(fixture);
        s.dirty = true;
    }
    apply_channel_defaults(&engine, &show, &new_ids);
    sync_chasers(&show, &chasers);
    sync_movements(&show, &movement);
    sync_globals(&show, &globals);
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

/// Atomic bulk add. Validates that none of the new ids collide with existing
/// fixtures or with each other before pushing any, so a malformed batch leaves
/// the patch untouched.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn add_fixtures(
    app: AppHandle,
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
    chasers: State<'_, SharedChasers>,
    movement: State<'_, SharedMovement>,
    globals: State<'_, SharedGlobals>,
    fixtures: Vec<FixtureInstance>,
) -> Result<(), CommandError> {
    if fixtures.is_empty() {
        return Ok(());
    }
    let new_ids: Vec<String>;
    {
        let mut s = show.write();
        let mut seen: std::collections::HashSet<&str> =
            s.show.fixtures.iter().map(|f| f.id.as_str()).collect();
        for f in &fixtures {
            if !seen.insert(f.id.as_str()) {
                return Err(CommandError::Other(format!(
                    "fixture id {} already exists or is duplicated in batch",
                    f.id
                )));
            }
        }
        new_ids = fixtures.iter().map(|f| f.id.clone()).collect();
        s.show.fixtures.extend(fixtures);
        s.dirty = true;
    }
    apply_channel_defaults(&engine, &show, &new_ids);
    sync_chasers(&show, &chasers);
    sync_movements(&show, &movement);
    sync_globals(&show, &globals);
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

/// Park the listed fixtures at their per-channel `default` values. Used
/// when patching new fixtures so a moving head lands at home (typically
/// pan/tilt 127 = centre) instead of at DMX 0 — saves the operator from
/// having to "unpark" every new fixture by hand. Silently skips fixtures
/// that don't resolve (unknown definition / mode / out-of-range address).
fn apply_channel_defaults(engine: &EngineState, show: &ShowState, ids: &[String]) {
    let s = show.read();
    let mut g = engine.write();
    for id in ids {
        let Some(inst) = s.show.fixtures.iter().find(|f| &f.id == id) else {
            continue;
        };
        let Some(def) = s.library.get(&inst.definition_id) else {
            continue;
        };
        let Some(mode) = def.mode(inst.mode_index as usize) else {
            continue;
        };
        for (offset, ch) in mode.channels.iter().enumerate() {
            // `offset` is 0-indexed; address is 1-indexed in DMX-land,
            // but `set_channel` itself takes 0-indexed too — see
            // `engine::EngineInner::set_channel`.
            let channel = (inst.address as usize + offset).saturating_sub(1) as u16;
            if let Err(err) = g.set_channel(inst.universe, channel, ch.default) {
                tracing::warn!(
                    %id, channel, error = %err,
                    "couldn't park fixture channel at default"
                );
            }
        }
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn remove_fixture(
    app: AppHandle,
    show: State<'_, ShowState>,
    chasers: State<'_, SharedChasers>,
    movement: State<'_, SharedMovement>,
    globals: State<'_, SharedGlobals>,
    programmer: State<'_, SharedProgrammer>,
    id: String,
) -> Result<(), CommandError> {
    let prog_snap = {
        let mut p = programmer.lock();
        p.untouch(&id);
        p.snapshot()
    };
    let _ = app.emit(PROGRAMMER_EVENT, prog_snap);
    {
        let mut s = show.write();
        let before = s.show.fixtures.len();
        s.show.fixtures.retain(|f| f.id != id);
        if s.show.fixtures.len() == before {
            return Err(CommandError::Other(format!("fixture {id} not found")));
        }
        // A removed fixture would leave its slots dangling — drop them so
        // chasers / movement / blind stop trying to write to a no-longer-
        // patched address.
        for c in &mut s.show.chasers {
            c.slots.retain(|slot| slot.fixture_id != id);
        }
        if let Some(m) = s.show.movement.as_mut() {
            m.fixtures.retain(|slot| slot.fixture_id != id);
        }
        for m in &mut s.show.movements {
            m.fixtures.retain(|slot| slot.fixture_id != id);
        }
        s.show.globals.blind.fixtures.retain(|f| f.fixture_id != id);
        s.show
            .globals
            .blackout
            .fixtures
            .retain(|f| f.fixture_id != id);
        s.dirty = true;
    }
    sync_chasers(&show, &chasers);
    sync_movements(&show, &movement);
    sync_globals(&show, &globals);
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

#[tauri::command]
pub fn update_fixture(
    app: AppHandle,
    show: State<'_, ShowState>,
    chasers: State<'_, SharedChasers>,
    movement: State<'_, SharedMovement>,
    globals: State<'_, SharedGlobals>,
    fixture: FixtureInstance,
) -> Result<(), CommandError> {
    {
        let mut s = show.write();
        let f = s
            .show
            .fixtures
            .iter_mut()
            .find(|f| f.id == fixture.id)
            .ok_or_else(|| CommandError::Other(format!("fixture {} not found", fixture.id)))?;
        *f = fixture;
        s.dirty = true;
    }
    sync_chasers(&show, &chasers);
    sync_movements(&show, &movement);
    sync_globals(&show, &globals);
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

#[tauri::command]
pub fn move_fixture(
    app: AppHandle,
    show: State<'_, ShowState>,
    id: String,
    x: f32,
    y: f32,
) -> Result<(), CommandError> {
    {
        let mut s = show.write();
        let f = s
            .show
            .fixtures
            .iter_mut()
            .find(|f| f.id == id)
            .ok_or_else(|| CommandError::Other(format!("fixture {id} not found")))?;
        f.position = [x, y];
        s.dirty = true;
    }
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

#[tauri::command]
pub fn validate_patch_cmd(show: State<'_, ShowState>) -> PatchReport {
    let s = show.read();
    validate_patch(&s.show.fixtures, &s.library)
}

/// Read the current u8 values for every channel of a fixture, in mode-channel
/// order. Used by the editor on the frontend so that switching away from a
/// fixture and back re-hydrates the sliders/picker with what the engine
/// actually has on the wire (rather than the mode defaults).
#[tauri::command]
pub fn get_fixture_values(
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
    fixture_id: String,
) -> Result<Vec<u8>, CommandError> {
    let s = show.read();
    let inst = s
        .show
        .fixtures
        .iter()
        .find(|f| f.id == fixture_id)
        .ok_or_else(|| CommandError::Other(format!("fixture {fixture_id} not found")))?;
    let def = s
        .library
        .get(&inst.definition_id)
        .ok_or_else(|| CommandError::Other(format!("unknown definition {}", inst.definition_id)))?;
    let mode = def
        .mode(inst.mode_index as usize)
        .ok_or_else(|| CommandError::Other(format!("unknown mode index {}", inst.mode_index)))?;
    let len = mode.channels.len();
    let snap = engine
        .read()
        .snapshot_universe(inst.universe)
        .ok_or_else(|| CommandError::Other(format!("universe {} not found", inst.universe)))?;
    let start = (inst.address as usize).saturating_sub(1);
    if start + len > snap.len() {
        return Err(CommandError::Other(format!(
            "fixture {fixture_id} channels overflow universe"
        )));
    }
    Ok(snap[start..start + len].to_vec())
}

#[tauri::command]
pub fn set_fixture_channel(
    app: AppHandle,
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
    programmer: State<'_, SharedProgrammer>,
    fixture_id: String,
    channel_offset: u16,
    value: u8,
) -> Result<(), CommandError> {
    tracing::trace!(
        target: "dmx::input",
        fixture = %fixture_id,
        channel_offset,
        value,
        "fixture → engine"
    );
    let (universe, channel) = {
        let s = show.read();
        let inst = s
            .show
            .fixtures
            .iter()
            .find(|f| f.id == fixture_id)
            .ok_or_else(|| CommandError::Other(format!("fixture {fixture_id} not found")))?;
        let def = s.library.get(&inst.definition_id).ok_or_else(|| {
            CommandError::Other(format!("unknown definition {}", inst.definition_id))
        })?;
        let mode = def.mode(inst.mode_index as usize).ok_or_else(|| {
            CommandError::Other(format!("unknown mode index {}", inst.mode_index))
        })?;
        if (channel_offset as usize) >= mode.channels.len() {
            return Err(CommandError::Other(format!(
                "channel offset {channel_offset} out of bounds for mode with {} channels",
                mode.channels.len()
            )));
        }
        let ch = inst
            .address
            .saturating_add(channel_offset)
            .saturating_sub(1);
        (inst.universe, ch)
    };
    engine
        .write()
        .set_channel(universe, channel, value)
        .map_err(|e| CommandError::Other(e.to_string()))?;
    // Mark this fixture *and the specific channel* as touched. The
    // recording flows still capture at fixture granularity (Update / Add
    // step / Solo touched), but the per-channel detail powers the
    // "what did I touch on this fixture" UI on the stage canvas.
    let snap = {
        let mut p = programmer.lock();
        p.touch(fixture_id, channel_offset);
        p.snapshot()
    };
    let _ = app.emit(PROGRAMMER_EVENT, snap);
    Ok(())
}

/// Returns the absolute path of the fixture library directory so the frontend
/// can build `asset://` URLs for fixture images without an extra IPC per render.
#[tauri::command]
pub fn get_library_dir() -> Option<String> {
    library_dir().map(|p| p.to_string_lossy().to_string())
}

/// Maximum source image size we'll accept. Big enough to fit a high-res
/// product photo without us caring, small enough that the resulting
/// definition JSON loads fast and stays portable. base64 inflates by
/// ~33% so the JSON ends up under ~3 MB even at the cap.
const MAX_FIXTURE_IMAGE_BYTES: usize = 2 * 1024 * 1024;

/// Encode a user-picked image as a `data:image/...;base64,...` URL and
/// inline it into the matching fixture-definition JSON. Storing the
/// bytes inside the same file the user edits / copies makes the
/// definition fully portable: send one .json across machines and the
/// picture goes with it, no `images/` sidecar to remember.
///
/// Returns the data URL itself so the UI can drop it straight into an
/// `<img src>` without round-tripping through `convertFileSrc`.
#[tauri::command]
pub fn set_fixture_image(
    show: State<'_, ShowState>,
    definition_id: String,
    source_path: String,
) -> Result<String, CommandError> {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;

    tracing::info!(target: "dmx::library", %definition_id, source = %source_path, "set_fixture_image start");
    let lib_dir = library_dir().ok_or_else(|| CommandError::Other("no config dir".into()))?;

    {
        let s = show.read();
        if !s.library.contains_key(&definition_id) {
            return Err(CommandError::Other(format!(
                "unknown definition {definition_id}"
            )));
        }
    }

    let src = PathBuf::from(&source_path);
    if !src.exists() {
        return Err(CommandError::Io(format!(
            "source image not found: {source_path}"
        )));
    }
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "png".to_string());
    let mime = mime_for_extension(&ext);

    let bytes = std::fs::read(&src).map_err(|e| {
        tracing::warn!(target: "dmx::library", error = %e, source = %source_path, "image read failed");
        CommandError::Io(format!("read {source_path}: {e}"))
    })?;
    if bytes.len() > MAX_FIXTURE_IMAGE_BYTES {
        return Err(CommandError::Other(format!(
            "image is {} KB; please use one under {} KB so the fixture JSON stays portable",
            bytes.len() / 1024,
            MAX_FIXTURE_IMAGE_BYTES / 1024
        )));
    }

    let data_url = format!("data:{};base64,{}", mime, B64.encode(&bytes));

    // Patch the matching JSON definition file. Scan because the filename on
    // disk is not necessarily `<id>.json`.
    let mut patched_path: Option<PathBuf> = None;
    for entry in std::fs::read_dir(&lib_dir).map_err(|e| CommandError::Io(e.to_string()))? {
        let entry = entry.map_err(|e| CommandError::Io(e.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let raw = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let mut def: FixtureDefinition = match serde_json::from_slice(&raw) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(target: "dmx::library", path = %path.display(), error = %e, "skipping invalid definition file");
                continue;
            }
        };
        if def.id != definition_id {
            continue;
        }
        def.image = Some(data_url.clone());
        let body =
            serde_json::to_vec_pretty(&def).map_err(|e| CommandError::Show(e.to_string()))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &body).map_err(|e| CommandError::Io(e.to_string()))?;
        std::fs::rename(&tmp, &path).map_err(|e| CommandError::Io(e.to_string()))?;
        tracing::info!(
            target: "dmx::library",
            path = %path.display(),
            bytes = bytes.len(),
            "definition patched with inline image"
        );
        patched_path = Some(path);
        break;
    }
    if patched_path.is_none() {
        return Err(CommandError::Other(format!(
            "no library file matched definition {definition_id}"
        )));
    }

    // Reload the in-memory library so subsequent `list_fixture_definitions`
    // calls return the updated `image` field.
    let lib = load_all(&lib_dir).map_err(|e| CommandError::Show(e.to_string()))?;
    show.write().library = lib;
    tracing::info!(target: "dmx::library", "library reloaded after image change");

    Ok(data_url)
}

fn mime_for_extension(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        // Fall back to PNG — browsers usually sniff the actual format
        // anyway, and this keeps the URL syntactically valid.
        _ => "image/png",
    }
}

/// Cap on per-range gobo bitmaps. Tighter than the fixture-thumbnail
/// budget because a single fixture can carry 16+ ranges and we don't
/// want a single rig to make `fixtures/*.json` files megabyte-sized
/// and slow to parse on every open.
const MAX_RANGE_IMAGE_BYTES: usize = 512 * 1024;

/// Inline a bitmap into a single `ChannelRange.image` of a fixture
/// definition (the per-position gobo / colour-wheel thumbnail).
/// Mirrors `set_fixture_image` byte-for-byte except the patch site is
/// `def.modes[mode_index].channels[channel_index].ranges[range_index]
/// .image` instead of `def.image`. Returns the data URL so the UI can
/// drop it into an `<img src>` immediately without re-fetching the
/// library.
///
/// The patched JSON file is rewritten atomically (tmp + rename) and
/// the in-memory library is reloaded so the next render sees the new
/// image. The 3D preview's gobo path keys off the *active* range's
/// image at runtime, so the new bitmap shows up the next time the
/// fixture's gobo channel sits in this range.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn set_channel_range_image(
    show: State<'_, ShowState>,
    definition_id: String,
    mode_index: u32,
    channel_index: u32,
    range_index: u32,
    source_path: String,
) -> Result<String, CommandError> {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;

    let lib_dir = library_dir().ok_or_else(|| CommandError::Other("no config dir".into()))?;

    {
        let s = show.read();
        if !s.library.contains_key(&definition_id) {
            return Err(CommandError::Other(format!(
                "unknown definition {definition_id}"
            )));
        }
    }

    let src = PathBuf::from(&source_path);
    if !src.exists() {
        return Err(CommandError::Io(format!(
            "source image not found: {source_path}"
        )));
    }
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "png".to_string());
    let mime = mime_for_extension(&ext);

    let bytes = std::fs::read(&src).map_err(|e| CommandError::Io(format!("read {source_path}: {e}")))?;
    if bytes.len() > MAX_RANGE_IMAGE_BYTES {
        return Err(CommandError::Other(format!(
            "image is {} KB; per-range thumbnails are capped at {} KB so the fixture JSON stays portable across many ranges",
            bytes.len() / 1024,
            MAX_RANGE_IMAGE_BYTES / 1024
        )));
    }

    let data_url = format!("data:{};base64,{}", mime, B64.encode(&bytes));

    let mut patched = false;
    for entry in std::fs::read_dir(&lib_dir).map_err(|e| CommandError::Io(e.to_string()))? {
        let entry = entry.map_err(|e| CommandError::Io(e.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let raw = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let mut def: FixtureDefinition = match serde_json::from_slice(&raw) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if def.id != definition_id {
            continue;
        }
        // Bounds-check the requested mode/channel/range path before
        // writing — a stale UI sending an out-of-bounds index should
        // get a clear error instead of silently corrupting the file.
        let mode = def.modes.get_mut(mode_index as usize).ok_or_else(|| {
            CommandError::Other(format!("mode index {mode_index} out of bounds"))
        })?;
        let channel = mode
            .channels
            .get_mut(channel_index as usize)
            .ok_or_else(|| {
                CommandError::Other(format!("channel index {channel_index} out of bounds"))
            })?;
        let range = channel
            .ranges
            .get_mut(range_index as usize)
            .ok_or_else(|| {
                CommandError::Other(format!("range index {range_index} out of bounds"))
            })?;
        range.image = Some(data_url.clone());
        // The legacy on-disk `image_path` is informational from the
        // import flow; clear it so future readers don't think the
        // bitmap lives at some external path.
        range.image_path = None;
        let body = serde_json::to_vec_pretty(&def).map_err(|e| CommandError::Show(e.to_string()))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &body).map_err(|e| CommandError::Io(e.to_string()))?;
        std::fs::rename(&tmp, &path).map_err(|e| CommandError::Io(e.to_string()))?;
        tracing::info!(
            target: "dmx::library",
            path = %path.display(),
            mode_index, channel_index, range_index,
            bytes = bytes.len(),
            "definition range patched with inline image"
        );
        patched = true;
        break;
    }
    if !patched {
        return Err(CommandError::Other(format!(
            "no library file matched definition {definition_id}"
        )));
    }

    let lib = load_all(&lib_dir).map_err(|e| CommandError::Show(e.to_string()))?;
    show.write().library = lib;
    Ok(data_url)
}

// ---- Ambient Chaser ------------------------------------------------------

/// Push the show's chasers and current fixture/library context into the
/// shared chaser engine. Runtime state for each chaser id is preserved
/// inside the engine, so calling this after a fixture move or unrelated
/// chaser edit doesn't reset timing.
fn sync_chasers(show: &ShowState, chasers: &SharedChasers) {
    let snapshot = {
        let s = show.read();
        (
            s.show.fixtures.clone(),
            s.library.clone(),
            s.show.chasers.clone(),
        )
    };
    let mut engine = chasers.lock();
    engine.update_show_context(snapshot.0, snapshot.1);
    engine.replace_chasers(snapshot.2);
}

#[tauri::command]
pub fn list_chasers(show: State<'_, ShowState>) -> Vec<AmbientChaser> {
    show.read().show.chasers.clone()
}

#[tauri::command]
pub fn create_chaser(
    app: AppHandle,
    show: State<'_, ShowState>,
    chasers: State<'_, SharedChasers>,
    name: Option<String>,
) -> Result<AmbientChaser, CommandError> {
    let id = uuid::Uuid::new_v4().to_string();
    let mut chaser = AmbientChaser::default_with_id(id);
    if let Some(n) = name {
        if !n.trim().is_empty() {
            chaser.name = n;
        }
    }
    {
        let mut s = show.write();
        s.show.chasers.push(chaser.clone());
        s.dirty = true;
    }
    sync_chasers(&show, &chasers);
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(chaser)
}

#[tauri::command]
pub fn update_chaser(
    app: AppHandle,
    show: State<'_, ShowState>,
    chasers: State<'_, SharedChasers>,
    chaser: AmbientChaser,
) -> Result<(), CommandError> {
    update_chaser_impl(&app, &show, &chasers, chaser)
}

/// Replace one chaser config (matched by id). Free-function entry point
/// so surfaces (Launchpad BPM nudge, future scripting) can mutate a
/// chaser without going through IPC.
pub fn update_chaser_impl(
    app: &AppHandle,
    show: &ShowState,
    chasers: &SharedChasers,
    chaser: AmbientChaser,
) -> Result<(), CommandError> {
    {
        let mut s = show.write();
        let entry = s
            .show
            .chasers
            .iter_mut()
            .find(|c| c.id == chaser.id)
            .ok_or_else(|| CommandError::Other(format!("chaser {} not found", chaser.id)))?;
        *entry = chaser;
        s.dirty = true;
    }
    sync_chasers(show, chasers);
    persist_show(show, app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

#[tauri::command]
pub fn delete_chaser(
    app: AppHandle,
    show: State<'_, ShowState>,
    chasers: State<'_, SharedChasers>,
    id: String,
) -> Result<(), CommandError> {
    {
        let mut s = show.write();
        let before = s.show.chasers.len();
        s.show.chasers.retain(|c| c.id != id);
        if s.show.chasers.len() == before {
            return Err(CommandError::Other(format!("chaser {id} not found")));
        }
        s.dirty = true;
    }
    sync_chasers(&show, &chasers);
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

/// Append a curated set of pre-configured "spectacular" chasers to the
/// show, filling each one's slots with whatever fixtures are patched right
/// now. Returns how many chasers were added so the UI can show a quick
/// confirmation.
#[tauri::command]
pub fn add_example_chasers(
    app: AppHandle,
    show: State<'_, ShowState>,
    chasers: State<'_, SharedChasers>,
) -> Result<usize, CommandError> {
    let presets = crate::chaser::presets::example_chasers(&show.read().show.fixtures);
    let count = presets.len();
    {
        let mut s = show.write();
        s.show.chasers.extend(presets);
        s.dirty = true;
    }
    sync_chasers(&show, &chasers);
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(count)
}

/// Toggle a chaser. Exclusive: enabling one chaser disables every other
/// chaser. Disabling a chaser leaves the rest untouched. This matches the
/// "scenes are mutually exclusive" mental model the operator uses on a
/// Launchpad — you press a pad to switch scenes, you don't blend two
/// generative chases on top of each other.
#[tauri::command]
pub fn toggle_chaser(
    app: AppHandle,
    show: State<'_, ShowState>,
    chasers: State<'_, SharedChasers>,
    id: String,
    enabled: bool,
) -> Result<(), CommandError> {
    toggle_chaser_impl(&app, &show, &chasers, &id, enabled)
}

/// Free-function entry point used by the Tauri command and by surfaces
/// (e.g. the Launchpad controller) that need to toggle a chaser without
/// going through the IPC layer. Implements the exclusivity rule.
pub fn toggle_chaser_impl(
    app: &AppHandle,
    show: &ShowState,
    chasers: &SharedChasers,
    id: &str,
    enabled: bool,
) -> Result<(), CommandError> {
    {
        let mut s = show.write();
        if !s.show.chasers.iter().any(|c| c.id == id) {
            return Err(CommandError::Other(format!("chaser {id} not found")));
        }
        for c in s.show.chasers.iter_mut() {
            if c.id == id {
                c.enabled = enabled;
            } else if enabled {
                c.enabled = false;
            }
        }
        s.dirty = true;
    }
    sync_chasers(show, chasers);
    persist_show(show, app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

/// Set just the `master` multiplier on a chaser without touching the rest
/// of its config. Surfaces that only need a "level" knob (the mobile
/// remote, the Launchpad's CC pots) call this instead of `update_chaser`
/// so they can't accidentally clobber the slot list / pattern / tempo.
/// Clamped to 0.0..=1.0 to keep the engine's `intensity * master`
/// multiplication well-defined.
pub fn set_chaser_master_impl(
    app: &AppHandle,
    show: &ShowState,
    chasers: &SharedChasers,
    id: &str,
    value: f32,
) -> Result<(), CommandError> {
    let value = value.clamp(0.0, 1.0);
    {
        let mut s = show.write();
        let entry = s
            .show
            .chasers
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| CommandError::Other(format!("chaser {id} not found")))?;
        entry.master = value;
        s.dirty = true;
    }
    sync_chasers(show, chasers);
    persist_show(show, app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

// ---- Globals (Blackout + Blind) -----------------------------------------

/// Push the show's fixture/library context plus the persisted globals
/// config into the live runtime. The runtime's interpolating factors are
/// preserved, so a slot list change doesn't snap blackout/blind state.
fn sync_globals(show: &ShowState, globals: &SharedGlobals) {
    let snapshot = {
        let s = show.read();
        (
            s.show.fixtures.clone(),
            s.library.clone(),
            s.show.globals.clone(),
        )
    };
    let mut g = globals.lock();
    g.update_show_context(snapshot.0, snapshot.1);
    g.replace_config(snapshot.2);
}

// ---- Movement Generators ------------------------------------------------

/// Push the show's current fixture/library context plus the movement
/// list into the live engine. Runtime phase is preserved per-id by the
/// engine itself, so editing one generator (or adding/removing others)
/// doesn't snap surviving generators back to phase 0.
fn sync_movements(show: &ShowState, movement: &SharedMovement) {
    let snapshot = {
        let s = show.read();
        (
            s.show.fixtures.clone(),
            s.library.clone(),
            s.show.movements.clone(),
        )
    };
    let mut engine = movement.lock();
    engine.update_show_context(snapshot.0, snapshot.1);
    engine.replace_generators(snapshot.2);
}

#[tauri::command]
pub fn list_movements(show: State<'_, ShowState>) -> Vec<MovementGenerator> {
    show.read().show.movements.clone()
}

#[tauri::command]
pub fn create_movement(
    app: AppHandle,
    show: State<'_, ShowState>,
    movement: State<'_, SharedMovement>,
    name: Option<String>,
) -> Result<MovementGenerator, CommandError> {
    let mut gen = MovementGenerator::default_disabled();
    if let Some(n) = name {
        if !n.trim().is_empty() {
            gen.name = n;
        }
    }
    {
        let mut s = show.write();
        s.show.movements.push(gen.clone());
        s.dirty = true;
    }
    sync_movements(&show, &movement);
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(gen)
}

/// Replace one Movement Generator wholesale (matched by id). The slot
/// list is run through `apply_spread` here so a spread-mode change or a
/// fixture add/remove instantly produces the right canon offsets without
/// the frontend having to mirror the math.
#[tauri::command]
pub fn update_movement(
    app: AppHandle,
    show: State<'_, ShowState>,
    movement: State<'_, SharedMovement>,
    generator: MovementGenerator,
) -> Result<(), CommandError> {
    let mut normalised = generator;
    normalised.apply_spread();
    {
        let mut s = show.write();
        let entry = s
            .show
            .movements
            .iter_mut()
            .find(|m| m.id == normalised.id)
            .ok_or_else(|| CommandError::Other(format!("movement {} not found", normalised.id)))?;
        *entry = normalised;
        s.dirty = true;
    }
    sync_movements(&show, &movement);
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

#[tauri::command]
pub fn delete_movement(
    app: AppHandle,
    show: State<'_, ShowState>,
    movement: State<'_, SharedMovement>,
    id: String,
) -> Result<(), CommandError> {
    {
        let mut s = show.write();
        let before = s.show.movements.len();
        s.show.movements.retain(|m| m.id != id);
        if s.show.movements.len() == before {
            return Err(CommandError::Other(format!("movement {id} not found")));
        }
        s.dirty = true;
    }
    sync_movements(&show, &movement);
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

/// Toggle a Movement Generator. Exclusive: enabling one disables every
/// other movement (mirrors `toggle_chaser`). Disabling a movement leaves
/// the rest untouched.
#[tauri::command]
pub fn toggle_movement(
    app: AppHandle,
    show: State<'_, ShowState>,
    movement: State<'_, SharedMovement>,
    id: String,
    enabled: bool,
) -> Result<(), CommandError> {
    toggle_movement_impl(&app, &show, &movement, &id, enabled)
}

pub fn toggle_movement_impl(
    app: &AppHandle,
    show: &ShowState,
    movement: &SharedMovement,
    id: &str,
    enabled: bool,
) -> Result<(), CommandError> {
    {
        let mut s = show.write();
        if !s.show.movements.iter().any(|m| m.id == id) {
            return Err(CommandError::Other(format!("movement {id} not found")));
        }
        for m in s.show.movements.iter_mut() {
            if m.id == id {
                m.enabled = enabled;
            } else if enabled {
                m.enabled = false;
            }
        }
        s.dirty = true;
    }
    sync_movements(show, movement);
    persist_show(show, app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

// ---- Programmer (Phase 4 it. 2) -----------------------------------------

#[tauri::command]
pub fn programmer_status(programmer: State<'_, SharedProgrammer>) -> ProgrammerStatus {
    programmer.lock().snapshot()
}

#[tauri::command]
pub fn programmer_clear(app: AppHandle, programmer: State<'_, SharedProgrammer>) {
    let snap = {
        let mut p = programmer.lock();
        p.clear();
        p.snapshot()
    };
    let _ = app.emit(PROGRAMMER_EVENT, snap);
}

/// Drop a single fixture from the touched set without affecting any of
/// the others. Used by the canvas badge / context-menu "Untouch" action
/// when the operator wants to selectively exclude a fixture from the
/// next Record/Update without losing the whole programmer state.
/// Idempotent: untouching a fixture that wasn't touched is a no-op.
#[tauri::command]
pub fn programmer_untouch(
    app: AppHandle,
    programmer: State<'_, SharedProgrammer>,
    fixture_id: String,
) {
    let snap = {
        let mut p = programmer.lock();
        p.untouch(&fixture_id);
        p.snapshot()
    };
    let _ = app.emit(PROGRAMMER_EVENT, snap);
}

// ---- Scenes (Phase 4 it. 3: multi-step + FX capture) ---------------------

#[tauri::command]
pub fn list_scenes(show: State<'_, ShowState>) -> Vec<Scene> {
    show.read().show.scenes.clone()
}

/// Build a `SceneFixture` list from the current engine state for the
/// listed fixture ids. Used by both create_scene_from_state and
/// add_scene_step so they share a single capture path.
fn capture_fixtures(
    engine: &EngineState,
    show: &ShowState,
    fixture_ids: &[String],
) -> Vec<SceneFixture> {
    let s = show.read();
    let target_ids: std::collections::HashSet<&str> = if fixture_ids.is_empty() {
        s.show.fixtures.iter().map(|f| f.id.as_str()).collect()
    } else {
        fixture_ids.iter().map(String::as_str).collect()
    };
    let g = engine.read();
    let mut out: Vec<SceneFixture> = Vec::new();
    for f in &s.show.fixtures {
        if !target_ids.contains(f.id.as_str()) {
            continue;
        }
        let Some(def) = s.library.get(&f.definition_id) else {
            continue;
        };
        let Some(mode) = def.mode(f.mode_index as usize) else {
            continue;
        };
        let Some(snap) = g.snapshot_universe(f.universe) else {
            continue;
        };
        let base = (f.address as usize).saturating_sub(1);
        let mut values: Vec<SceneChannel> = Vec::with_capacity(mode.channels.len());
        for offset in 0..mode.channels.len() {
            let idx = base + offset;
            if idx < snap.len() {
                values.push(SceneChannel {
                    channel_offset: offset as u16,
                    value: snap[idx],
                });
            }
        }
        out.push(SceneFixture {
            fixture_id: f.id.clone(),
            values,
        });
    }
    out
}

/// Resolve the requested fixture set: explicit list, or "all touched"
/// when `restrict_to_touched`. Empty result means "use everything in
/// the scene's existing fixture list" (the caller decides).
fn effective_fixture_ids(
    programmer: &SharedProgrammer,
    fixture_ids: Vec<String>,
    restrict_to_touched: bool,
) -> Vec<String> {
    if restrict_to_touched {
        programmer.lock().touched_ids()
    } else {
        fixture_ids
    }
}

/// Inspect the live show state and produce the FX state to embed in a
/// new scene. Default-on capture: if the user hasn't told us otherwise,
/// the scene records "the chaser/movement that was running at the
/// moment of Record" so recall reproduces the full look.
fn capture_fx_state(
    show: &ShowState,
    capture_chaser: bool,
    capture_movement: bool,
) -> (SceneFxState, SceneFxState) {
    let s = show.read();
    let chaser = if capture_chaser {
        match s.show.chasers.iter().find(|c| c.enabled) {
            Some(c) => SceneFxState::Enabled { id: c.id.clone() },
            None => SceneFxState::Disabled,
        }
    } else {
        SceneFxState::Inherit
    };
    let movement = if capture_movement {
        match s.show.movements.iter().find(|m| m.enabled) {
            Some(m) => SceneFxState::Enabled { id: m.id.clone() },
            None => SceneFxState::Disabled,
        }
    } else {
        SceneFxState::Inherit
    };
    (chaser, movement)
}

/// Capture the current rig state as a brand-new scene with one step.
/// The newly-created scene contains exactly one `SceneStep` — the
/// initial frame. To turn it into a looping animation, add more steps
/// via `add_scene_step`. Empty `fixture_ids` records every patched
/// fixture; `restrict_to_touched` overrides that and uses the
/// programmer's touched set instead.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn create_scene_from_state(
    app: AppHandle,
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
    programmer: State<'_, SharedProgrammer>,
    name: String,
    fixture_ids: Vec<String>,
    fade_in_ms: u32,
    restrict_to_touched: bool,
    capture_chaser: bool,
    capture_movement: bool,
) -> Result<Scene, CommandError> {
    let effective_ids = effective_fixture_ids(&programmer, fixture_ids, restrict_to_touched);
    let id = uuid::Uuid::new_v4().to_string();
    let scene_name = if name.trim().is_empty() {
        format!("Scene {}", show.read().show.scenes.len() + 1)
    } else {
        name.trim().to_string()
    };
    let mut scene = Scene::default_with_id(id, scene_name);
    // FX context is captured per-step now: each step remembers what was
    // running when grabbed, and the playback re-applies on the
    // transition into that step. Scene-level fields stay Inherit.
    let (chaser_state, movement_state) = capture_fx_state(&show, capture_chaser, capture_movement);
    let fixtures = capture_fixtures(&engine, &show, &effective_ids);
    scene.steps.push(SceneStep {
        id: uuid::Uuid::new_v4().to_string(),
        name: None,
        fade_in_ms,
        hold_ms: 0,
        fixtures,
        chaser_state,
        movement_state,
    });

    {
        let mut s = show.write();
        s.show.scenes.push(scene.clone());
        s.dirty = true;
    }
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(scene)
}

/// Append a new step to an existing scene, captured from the current
/// engine state. The new step inherits the previous step's fixture set
/// when `fixture_ids` is empty AND `restrict_to_touched` is false —
/// that matches the operator's mental model of "I'm extending this
/// scene, capture the same fixtures with their new values".
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn add_scene_step(
    app: AppHandle,
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
    programmer: State<'_, SharedProgrammer>,
    scene_id: String,
    fixture_ids: Vec<String>,
    fade_in_ms: u32,
    hold_ms: u32,
    restrict_to_touched: bool,
) -> Result<Scene, CommandError> {
    // Default fallback: reuse whatever fixtures the previous step
    // already had. Operators rarely change the fixture set across the
    // steps of one scene.
    let mut effective_ids = effective_fixture_ids(&programmer, fixture_ids, restrict_to_touched);
    if effective_ids.is_empty() && !restrict_to_touched {
        let s = show.read();
        if let Some(scene) = s.show.scenes.iter().find(|c| c.id == scene_id) {
            if let Some(last) = scene.steps.last() {
                effective_ids = last.fixtures.iter().map(|f| f.fixture_id.clone()).collect();
            }
        }
    }

    let fixtures = capture_fixtures(&engine, &show, &effective_ids);
    // Default capture-on-grab for FX state, mirroring the create_scene
    // flow. The operator can override per-step by editing the step's
    // FX state from the UI; here we just snapshot whatever's running
    // right now.
    let (chaser_state, movement_state) = capture_fx_state(&show, true, true);
    let new_step = SceneStep {
        id: uuid::Uuid::new_v4().to_string(),
        name: None,
        fade_in_ms,
        hold_ms,
        fixtures,
        chaser_state,
        movement_state,
    };
    let updated = {
        let mut s = show.write();
        let entry = s
            .show
            .scenes
            .iter_mut()
            .find(|c| c.id == scene_id)
            .ok_or_else(|| CommandError::Other(format!("scene {scene_id} not found")))?;
        entry.steps.push(new_step);
        let cloned = entry.clone();
        s.dirty = true;
        cloned
    };
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(updated)
}

/// Remove one step from a scene by id. The remaining steps shift up so
/// playback's `current_step` index in a live recall stays meaningful.
/// Refuses to remove the last step (use `delete_scene` to drop the
/// whole scene instead).
#[tauri::command]
pub fn remove_scene_step(
    app: AppHandle,
    show: State<'_, ShowState>,
    scene_id: String,
    step_id: String,
) -> Result<Scene, CommandError> {
    let updated = {
        let mut s = show.write();
        let entry = s
            .show
            .scenes
            .iter_mut()
            .find(|c| c.id == scene_id)
            .ok_or_else(|| CommandError::Other(format!("scene {scene_id} not found")))?;
        if entry.steps.len() <= 1 {
            return Err(CommandError::Other(
                "cannot remove the last step; delete the scene instead".into(),
            ));
        }
        let before = entry.steps.len();
        entry.steps.retain(|s| s.id != step_id);
        if entry.steps.len() == before {
            return Err(CommandError::Other(format!("step {step_id} not found")));
        }
        s.dirty = true;
        s.show
            .scenes
            .iter()
            .find(|c| c.id == scene_id)
            .cloned()
            .unwrap()
    };
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(updated)
}

/// Re-record one step of an existing scene from the engine's current
/// state. Updates only the fixtures already in that step; if
/// `restrict_to_touched` is true, only the touched ones get refreshed.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn update_scene_step_from_state(
    app: AppHandle,
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
    programmer: State<'_, SharedProgrammer>,
    scene_id: String,
    step_id: String,
    restrict_to_touched: bool,
) -> Result<Scene, CommandError> {
    let touched: std::collections::HashSet<String> = if restrict_to_touched {
        programmer.lock().touched_ids().into_iter().collect()
    } else {
        std::collections::HashSet::new()
    };

    // Build the new fixture list outside the show lock so the inner
    // borrow chain stays simple.
    let new_fixtures: Vec<SceneFixture> = {
        let scene = {
            let s = show.read();
            s.show
                .scenes
                .iter()
                .find(|c| c.id == scene_id)
                .cloned()
                .ok_or_else(|| CommandError::Other(format!("scene {scene_id} not found")))?
        };
        let step = scene
            .steps
            .iter()
            .find(|st| st.id == step_id)
            .ok_or_else(|| CommandError::Other(format!("step {step_id} not found")))?;
        let s = show.read();
        let g = engine.read();
        let mut out = step.fixtures.clone();
        for sf in out.iter_mut() {
            if restrict_to_touched && !touched.contains(&sf.fixture_id) {
                continue;
            }
            let Some(inst) = s.show.fixtures.iter().find(|f| f.id == sf.fixture_id) else {
                continue;
            };
            let Some(def) = s.library.get(&inst.definition_id) else {
                continue;
            };
            let Some(mode) = def.mode(inst.mode_index as usize) else {
                continue;
            };
            let Some(snap) = g.snapshot_universe(inst.universe) else {
                continue;
            };
            let base = (inst.address as usize).saturating_sub(1);
            let mut values = Vec::with_capacity(mode.channels.len());
            for offset in 0..mode.channels.len() {
                let idx = base + offset;
                if idx < snap.len() {
                    values.push(SceneChannel {
                        channel_offset: offset as u16,
                        value: snap[idx],
                    });
                }
            }
            sf.values = values;
        }
        out
    };

    let updated = {
        let mut s = show.write();
        let scene = s
            .show
            .scenes
            .iter_mut()
            .find(|c| c.id == scene_id)
            .ok_or_else(|| CommandError::Other(format!("scene {scene_id} not found")))?;
        let step = scene
            .steps
            .iter_mut()
            .find(|st| st.id == step_id)
            .ok_or_else(|| CommandError::Other(format!("step {step_id} not found")))?;
        step.fixtures = new_fixtures;
        s.dirty = true;
        s.show
            .scenes
            .iter()
            .find(|c| c.id == scene_id)
            .cloned()
            .unwrap()
    };
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(updated)
}

/// Wholesale replacement of a scene (name, FX state, step shells +
/// timings). The frontend uses this for inline edits like renaming a
/// step or tweaking its fade/hold without a full re-record.
#[tauri::command]
pub fn update_scene(
    app: AppHandle,
    show: State<'_, ShowState>,
    scene: Scene,
) -> Result<(), CommandError> {
    {
        let mut s = show.write();
        let entry = s
            .show
            .scenes
            .iter_mut()
            .find(|c| c.id == scene.id)
            .ok_or_else(|| CommandError::Other(format!("scene {} not found", scene.id)))?;
        *entry = scene;
        s.dirty = true;
    }
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

#[tauri::command]
pub fn delete_scene(
    app: AppHandle,
    show: State<'_, ShowState>,
    scenes_pb: State<'_, SharedScenePlayback>,
    id: String,
) -> Result<(), CommandError> {
    {
        let mut s = show.write();
        let before = s.show.scenes.len();
        s.show.scenes.retain(|c| c.id != id);
        if s.show.scenes.len() == before {
            return Err(CommandError::Other(format!("scene {id} not found")));
        }
        s.dirty = true;
    }
    {
        let mut pb = scenes_pb.lock();
        if pb.active_scene_id() == Some(id.as_str()) {
            pb.release(std::time::Instant::now());
        }
    }
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(())
}

/// Recall a scene: kicks off the multi-step playback (looping if more
/// than one step) and applies the captured chaser/movement state by
/// dispatching the same toggle helpers the UI uses.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn recall_scene(
    app: AppHandle,
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
    chasers: State<'_, SharedChasers>,
    movement: State<'_, SharedMovement>,
    scenes_pb: State<'_, SharedScenePlayback>,
    id: String,
) -> Result<(), CommandError> {
    recall_scene_impl(
        &app,
        engine.inner(),
        &show,
        chasers.inner(),
        movement.inner(),
        scenes_pb.inner(),
        &id,
    )
}

/// Free-function recall used by the Tauri command and by the Launchpad
/// input router (which doesn't have access to `State<>`). Snapshots the
/// rig's *current* FX context as the pre-recall restore point, then
/// hands the per-step targets + per-step FX state to ScenePlayback. The
/// playback fires FX-apply requests through its channel as steps cross;
/// a separate consumer thread (spawned in `lib.rs`) actually toggles
/// chasers/movements so disk persistence stays off the DMX hot path.
pub fn recall_scene_impl(
    app: &AppHandle,
    engine: &EngineState,
    show: &ShowState,
    _chasers: &SharedChasers,
    _movement: &SharedMovement,
    scenes_pb: &SharedScenePlayback,
    id: &str,
) -> Result<(), CommandError> {
    let scene = {
        let s = show.read();
        s.show
            .scenes
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| CommandError::Other(format!("scene {id} not found")))?
    };

    // Snapshot the live FX context as the "pre-recall" anchor. Release
    // restores this. If the rig had chaser X running before this
    // recall, releasing brings X back even if the scene's last step
    // had set a different chaser.
    let pre_recall_fx = current_fx_state(show);

    // Resolve every step into (universe, channel) → value maps and
    // build a single pre-recall snapshot covering every channel any
    // step will touch. The playback uses this map for *both* roles:
    //   - lerp source for step 0's fade-in (so the rig glides from
    //     wherever it is right now into the scene), and
    //   - target for the release fade-out (so "Liberar" returns the
    //     rig to whatever was on the wire just before recall, even
    //     channels only touched by later steps).
    //
    // We capture the BASE layer only (no effects/blind/master/blackout
    // merge). The overlays apply independently each frame, and on
    // release the FX state is also restored to its pre-recall value
    // — so capturing the merged output here would double-count the
    // chaser/movement contribution and leave channels too bright
    // after release.
    let mut pre_recall_values: std::collections::HashMap<(u16, u16), u8> =
        std::collections::HashMap::new();
    let mut step_inputs: Vec<crate::engine::scene_playback::ResolvedStepInput> = Vec::new();
    {
        let s = show.read();
        let g = engine.read();
        // Snapshot per-universe once so we don't read it on every
        // channel: a 64-fixture scene over 2 universes only takes 2
        // snapshot calls.
        let mut universe_snaps: std::collections::HashMap<u16, [u8; crate::engine::DMX_CHANNELS]> =
            std::collections::HashMap::new();
        for step in &scene.steps {
            let mut targets: std::collections::HashMap<(u16, u16), u8> =
                std::collections::HashMap::new();
            for sf in &step.fixtures {
                let Some(inst) = s.show.fixtures.iter().find(|f| f.id == sf.fixture_id) else {
                    continue;
                };
                let snap = match universe_snaps.get(&inst.universe) {
                    Some(s) => Some(s),
                    None => match g.snapshot_base(inst.universe) {
                        Some(s) => {
                            universe_snaps.insert(inst.universe, s);
                            universe_snaps.get(&inst.universe)
                        }
                        None => None,
                    },
                };
                let base = (inst.address as usize).saturating_sub(1);
                for ch in &sf.values {
                    let idx = base + ch.channel_offset as usize;
                    if idx >= crate::engine::DMX_CHANNELS {
                        continue;
                    }
                    let key = (inst.universe, idx as u16);
                    targets.insert(key, ch.value);
                    // Capture the *current* rig value once per key.
                    // `or_insert` keeps the first observation (i.e.
                    // pre-recall), so later steps revisiting the same
                    // channel don't overwrite the snapshot.
                    if let Some(s) = snap {
                        pre_recall_values.entry(key).or_insert(s[idx]);
                    }
                }
            }
            step_inputs.push(crate::engine::scene_playback::ResolvedStepInput {
                fade_in_ms: step.fade_in_ms,
                hold_ms: step.hold_ms,
                targets,
                chaser_state: step.chaser_state.clone(),
                movement_state: step.movement_state.clone(),
            });
        }
    }

    scenes_pb.lock().recall(
        id.to_string(),
        step_inputs,
        pre_recall_values,
        pre_recall_fx,
        std::time::Instant::now(),
    );
    let step_index = scenes_pb.lock().current_step_index().map(|i| i as u32);
    let _ = app.emit(
        SCENE_ACTIVE_EVENT,
        SceneActiveChange {
            active_scene_id: Some(id.to_string()),
            step_index,
        },
    );
    Ok(())
}

/// Read the rig's currently-running chaser + movement and package them
/// into FX states. Used to seed pre-recall snapshots on every recall.
fn current_fx_state(show: &ShowState) -> (SceneFxState, SceneFxState) {
    let s = show.read();
    let chaser = match s.show.chasers.iter().find(|c| c.enabled) {
        Some(c) => SceneFxState::Enabled { id: c.id.clone() },
        None => SceneFxState::Disabled,
    };
    let movement = match s.show.movements.iter().find(|m| m.enabled) {
        Some(m) => SceneFxState::Enabled { id: m.id.clone() },
        None => SceneFxState::Disabled,
    };
    (chaser, movement)
}

/// Apply one FX-apply request emitted by ScenePlayback. Called from the
/// consumer thread spawned in `lib.rs` — the playback can fire these
/// from the DMX hot path safely because actual chaser/movement toggles
/// (with their disk persistence) happen here, off-thread.
pub fn apply_scene_fx_request(
    app: &AppHandle,
    show: &ShowState,
    chasers: &SharedChasers,
    movement: &SharedMovement,
    req: &crate::engine::scene_playback::SceneFxApply,
) {
    if let Err(err) = apply_fx_state_chaser(app, show, chasers, &req.chaser) {
        tracing::warn!(?err, "scene FX consumer: chaser apply failed");
    }
    if let Err(err) = apply_fx_state_movement(app, show, movement, &req.movement) {
        tracing::warn!(?err, "scene FX consumer: movement apply failed");
    }
}

fn apply_fx_state_chaser(
    app: &AppHandle,
    show: &ShowState,
    chasers: &SharedChasers,
    state: &SceneFxState,
) -> Result<(), CommandError> {
    match state {
        SceneFxState::Inherit => Ok(()),
        SceneFxState::Disabled => {
            // Disable whichever chaser is currently on. Exclusivity
            // means there's at most one.
            let active_id = {
                let s = show.read();
                s.show
                    .chasers
                    .iter()
                    .find(|c| c.enabled)
                    .map(|c| c.id.clone())
            };
            if let Some(id) = active_id {
                toggle_chaser_impl(app, show, chasers, &id, false)?;
            }
            Ok(())
        }
        SceneFxState::Enabled { id } => {
            // Toggling enabled = true on a chaser already does the
            // exclusivity dance internally (disables every other
            // chaser). Silently no-op if the id no longer exists, so
            // recalling an old scene whose chaser was deleted doesn't
            // explode the recall path.
            let exists = show.read().show.chasers.iter().any(|c| &c.id == id);
            if !exists {
                tracing::warn!(%id, "scene references a chaser that no longer exists; skipping");
                return Ok(());
            }
            toggle_chaser_impl(app, show, chasers, id, true)
        }
    }
}

fn apply_fx_state_movement(
    app: &AppHandle,
    show: &ShowState,
    movement: &SharedMovement,
    state: &SceneFxState,
) -> Result<(), CommandError> {
    match state {
        SceneFxState::Inherit => Ok(()),
        SceneFxState::Disabled => {
            let active_id = {
                let s = show.read();
                s.show
                    .movements
                    .iter()
                    .find(|m| m.enabled)
                    .map(|m| m.id.clone())
            };
            if let Some(id) = active_id {
                toggle_movement_impl(app, show, movement, &id, false)?;
            }
            Ok(())
        }
        SceneFxState::Enabled { id } => {
            let exists = show.read().show.movements.iter().any(|m| &m.id == id);
            if !exists {
                tracing::warn!(%id, "scene references a movement that no longer exists; skipping");
                return Ok(());
            }
            toggle_movement_impl(app, show, movement, id, true)
        }
    }
}

#[tauri::command]
pub fn release_scene(app: AppHandle, scenes_pb: State<'_, SharedScenePlayback>) {
    scenes_pb.lock().release(std::time::Instant::now());
    let _ = app.emit(
        SCENE_ACTIVE_EVENT,
        SceneActiveChange {
            active_scene_id: None,
            step_index: None,
        },
    );
}

#[tauri::command]
pub fn active_scene_id(scenes_pb: State<'_, SharedScenePlayback>) -> Option<String> {
    scenes_pb.lock().active_scene_id().map(|s| s.to_string())
}

/// Index of the step currently driving playback within the active
/// scene, if any. Used by the UI to highlight the live step in the
/// step list. `None` means there's no active scene OR the playback is
/// in an idle phase between recall and first frame.
#[tauri::command]
pub fn active_scene_step(scenes_pb: State<'_, SharedScenePlayback>) -> Option<u32> {
    scenes_pb.lock().current_step_index().map(|i| i as u32)
}

// ---- MIDI ----------------------------------------------------------------

#[tauri::command]
pub fn list_midi_devices() -> Vec<MidiDeviceInfo> {
    crate::midi::hub::list_devices()
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn connect_midi_device(
    app: AppHandle,
    midi: State<'_, SharedMidi>,
    launchpad_state: State<'_, SharedLaunchpad>,
    chasers: State<'_, SharedChasers>,
    movement: State<'_, SharedMovement>,
    globals: State<'_, SharedGlobals>,
    scenes: State<'_, SharedScenePlayback>,
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
    name: String,
) -> Result<(), CommandError> {
    // Tear down any previous Launchpad controller before opening a new
    // hub connection — re-using a stale router across re-connects would
    // route pad presses to the wrong show state if the user swapped
    // controllers.
    if let Some(prev) = launchpad_state.lock().take() {
        prev.shutdown();
    }
    let shared = midi.inner().clone();
    midi.lock()
        .connect(&name, app.clone(), shared.clone())
        .map_err(CommandError::Other)?;
    if launchpad::is_launchpad(&name) {
        let controller = launchpad::start(
            app,
            shared,
            chasers.inner().clone(),
            movement.inner().clone(),
            globals.inner().clone(),
            scenes.inner().clone(),
            engine.inner().clone(),
            show.inner().clone(),
        );
        *launchpad_state.lock() = Some(controller);
    }
    Ok(())
}

#[tauri::command]
pub fn disconnect_midi(midi: State<'_, SharedMidi>, launchpad_state: State<'_, SharedLaunchpad>) {
    // Stop the surface controller first so the feedback thread doesn't
    // try to push to a port that's about to be dropped.
    if let Some(prev) = launchpad_state.lock().take() {
        prev.shutdown();
    }
    midi.lock().disconnect();
}

#[tauri::command]
pub fn get_midi_status(midi: State<'_, SharedMidi>) -> MidiStatus {
    midi.lock().status()
}

#[tauri::command]
pub fn send_midi_raw(midi: State<'_, SharedMidi>, bytes: Vec<u8>) -> Result<(), CommandError> {
    midi.lock().send_raw(&bytes).map_err(CommandError::Other)
}

// ---- Stream Deck ---------------------------------------------------------

#[tauri::command]
pub fn list_streamdeck_devices() -> Vec<crate::streamdeck::StreamDeckDeviceInfo> {
    tracing::debug!("cmd: list_streamdeck_devices");
    crate::streamdeck::controller::list_streamdeck_devices()
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn connect_streamdeck_device(
    app: AppHandle,
    streamdeck_state: State<'_, crate::streamdeck::controller::SharedStreamDeck>,
    chasers: State<'_, SharedChasers>,
    movement: State<'_, SharedMovement>,
    globals: State<'_, SharedGlobals>,
    scenes: State<'_, SharedScenePlayback>,
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
    serial: Option<String>,
) -> Result<(), CommandError> {
    // Tear down any previous controller — same reasoning as the MIDI
    // path: never reuse a stale worker thread across reconnects.
    if let Some(prev) = streamdeck_state.lock().take() {
        prev.shutdown();
    }
    let controller = crate::streamdeck::controller::start(
        app,
        serial.as_deref(),
        chasers.inner().clone(),
        movement.inner().clone(),
        globals.inner().clone(),
        scenes.inner().clone(),
        engine.inner().clone(),
        show.inner().clone(),
    )
    .map_err(CommandError::Other)?;
    *streamdeck_state.lock() = Some(controller);
    Ok(())
}

#[tauri::command]
pub fn disconnect_streamdeck(
    streamdeck_state: State<'_, crate::streamdeck::controller::SharedStreamDeck>,
) {
    if let Some(prev) = streamdeck_state.lock().take() {
        prev.shutdown();
    }
}

#[tauri::command]
pub fn get_streamdeck_status(
    streamdeck_state: State<'_, crate::streamdeck::controller::SharedStreamDeck>,
) -> crate::streamdeck::StreamDeckStatus {
    tracing::debug!("cmd: get_streamdeck_status");
    let guard = streamdeck_state.lock();
    match guard.as_ref() {
        Some(ctrl) => {
            let info = ctrl.info();
            crate::streamdeck::StreamDeckStatus {
                connected: Some(info.serial),
                kind: Some(info.kind),
                key_count: Some(info.key_count),
            }
        }
        None => crate::streamdeck::StreamDeckStatus {
            connected: None,
            kind: None,
            key_count: None,
        },
    }
}

// ---- helpers + error type ------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn apply_outputs(
    app: &AppHandle,
    engine: &EngineState,
    output_thread: &OutputThreadState,
    chasers: &SharedChasers,
    movement: &SharedMovement,
    globals: &SharedGlobals,
    scenes: &SharedScenePlayback,
    outputs: &OutputsConfig,
) -> Result<(), CommandError> {
    let universes = outputs.universes();
    let universes = if universes.is_empty() {
        vec![0]
    } else {
        universes
    };
    engine.write().reconcile_universes(&universes);

    let new_bindings = instantiate(outputs);
    let mut guard = output_thread.0.lock().unwrap();
    if let Some(handle) = guard.as_ref() {
        handle.replace_bindings(new_bindings);
    } else {
        let app_for_stats = app.clone();
        let h = crate::engine::output_thread::spawn(
            engine.clone(),
            shared_bindings(new_bindings),
            chasers.clone(),
            movement.clone(),
            globals.clone(),
            scenes.clone(),
            move |stats: EngineStats| {
                if let Err(err) = app_for_stats.emit(STATS_EVENT, stats) {
                    tracing::warn!(?err, "failed to emit engine stats");
                }
            },
        );
        *guard = Some(h);
    }
    Ok(())
}

// ---- AI scene generation (POC) ------------------------------------------

#[derive(Debug, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../bindings/")]
pub struct AiAvailableModels {
    pub anthropic: Vec<crate::ai::config::AiModelOption>,
    pub openai: Vec<crate::ai::config::AiModelOption>,
}

#[tauri::command]
pub fn get_ai_config(app: AppHandle) -> crate::ai::config::AiConfig {
    crate::ai::config::load(&app)
}

#[tauri::command]
pub fn set_ai_config(
    app: AppHandle,
    config: crate::ai::config::AiConfig,
) -> Result<(), CommandError> {
    crate::ai::config::save(&app, &config).map_err(CommandError::Other)
}

#[tauri::command]
pub fn ai_list_models() -> AiAvailableModels {
    AiAvailableModels {
        anthropic: crate::ai::config::anthropic_models(),
        openai: crate::ai::config::openai_models(),
    }
}

/// Validate the active provider's API key by making a tiny request.
/// Returns a human-readable success message or an error string with
/// the API's response body.
#[tauri::command]
pub async fn ai_test_connection(app: AppHandle) -> Result<String, CommandError> {
    use crate::ai::config::AiProvider;
    let cfg = crate::ai::config::load(&app);
    let Some((provider, api_key, model)) = cfg.active() else {
        return Err(CommandError::Other(
            "Configurá un provider y una API key primero".into(),
        ));
    };
    // The closures need owned strings to satisfy 'static for spawn.
    let api_key = api_key.to_string();
    let model = model.to_string();
    match provider {
        AiProvider::Anthropic => crate::ai::anthropic::test_connection(&api_key, &model)
            .await
            .map_err(CommandError::Other),
        AiProvider::Openai => crate::ai::openai::test_connection(&api_key, &model)
            .await
            .map_err(CommandError::Other),
        AiProvider::None => unreachable!("active() filtered None"),
    }
}

// `seed`, when provided, makes the LLM treat the existing scene as
// the starting point and the prompt as a tweak instruction. Fixture
// context is still sent so the model can dip into available channels.
#[tauri::command]
pub async fn ai_generate_scene_draft(
    app: AppHandle,
    show: State<'_, ShowState>,
    prompt: String,
    step_count: u32,
    fixture_ids: Option<Vec<String>>,
    seed: Option<crate::ai::scene_gen::DraftScene>,
) -> Result<crate::ai::scene_gen::DraftScene, CommandError> {
    let cfg = crate::ai::config::load(&app);
    let Some((provider, api_key, model)) = cfg.active() else {
        return Err(CommandError::Other(
            "Configurá un provider y una API key primero".into(),
        ));
    };
    let api_key = api_key.to_string();
    let model = model.to_string();

    // Snapshot the show + library while we hold the read lock; the HTTP
    // call below is async and we don't want to hold a parking_lot guard
    // across await points. Cloning a few hundred KB here is fine for
    // the cost; the alternative is restructuring the show state.
    let (show_snapshot, library_snapshot) = {
        let s = show.read();
        (s.show.clone(), s.library.clone())
    };

    let context = crate::ai::scene_gen::build_context(
        &show_snapshot,
        &library_snapshot,
        fixture_ids.as_deref(),
    );

    crate::ai::scene_gen::generate(
        provider,
        &api_key,
        &model,
        &prompt,
        step_count,
        &context,
        seed.as_ref(),
    )
    .await
    .map_err(CommandError::Other)
}

/// Replace an existing scene's name + steps with the contents of a
/// validated draft. Keeps the scene's id (so MIDI bindings, UI
/// selection, etc. stay anchored to the same scene) and preserves
/// the chaser/movement layer state — only the steps and name change.
#[tauri::command]
pub fn ai_replace_scene(
    app: AppHandle,
    show: State<'_, ShowState>,
    scene_id: String,
    draft: crate::ai::scene_gen::DraftScene,
) -> Result<crate::show::scene::Scene, CommandError> {
    use crate::show::scene::{SceneChannel, SceneFixture, SceneFxState, SceneStep};
    if draft.steps.is_empty() {
        return Err(CommandError::Other(
            "El draft no tiene ningún step válido luego de la validación".into(),
        ));
    }
    let new_steps: Vec<SceneStep> = draft
        .steps
        .into_iter()
        .map(|step| SceneStep {
            id: uuid::Uuid::new_v4().to_string(),
            name: step.name,
            fade_in_ms: step.fade_in_ms,
            hold_ms: step.hold_ms,
            fixtures: step
                .fixtures
                .into_iter()
                .map(|fx| SceneFixture {
                    fixture_id: fx.fixture_id,
                    values: fx
                        .values
                        .into_iter()
                        .map(|v| SceneChannel {
                            channel_offset: v.channel_offset,
                            value: v.value,
                        })
                        .collect(),
                })
                .collect(),
            chaser_state: SceneFxState::Inherit,
            movement_state: SceneFxState::Inherit,
        })
        .collect();
    let new_name = if draft.name.trim().is_empty() {
        None
    } else {
        Some(draft.name.trim().to_string())
    };

    let updated = {
        let mut s = show.write();
        let updated = {
            let Some(scene) = s.show.scenes.iter_mut().find(|sc| sc.id == scene_id) else {
                return Err(CommandError::Other(format!(
                    "Escena {scene_id} no encontrada"
                )));
            };
            if let Some(name) = new_name {
                scene.name = name;
            }
            scene.steps = new_steps;
            scene.clone()
        };
        s.dirty = true;
        updated
    };
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(updated)
}

/// Materialise a validated draft into the show as a real scene. Runs
/// after the operator hits "Apply" in the preview UI; nothing in the
/// generation path mutates the show on its own.
#[tauri::command]
pub fn ai_apply_draft_scene(
    app: AppHandle,
    show: State<'_, ShowState>,
    draft: crate::ai::scene_gen::DraftScene,
) -> Result<crate::show::scene::Scene, CommandError> {
    use crate::show::scene::{Scene, SceneChannel, SceneFixture, SceneFxState, SceneStep};
    if draft.steps.is_empty() {
        return Err(CommandError::Other(
            "El draft no tiene ningún step válido luego de la validación".into(),
        ));
    }

    let scene_id = uuid::Uuid::new_v4().to_string();
    let scene_name = if draft.name.trim().is_empty() {
        format!("AI Scene {}", show.read().show.scenes.len() + 1)
    } else {
        draft.name.trim().to_string()
    };
    let mut scene = Scene::default_with_id(scene_id, scene_name);

    for step in draft.steps {
        let fixtures: Vec<SceneFixture> = step
            .fixtures
            .into_iter()
            .map(|fx| SceneFixture {
                fixture_id: fx.fixture_id,
                values: fx
                    .values
                    .into_iter()
                    .map(|v| SceneChannel {
                        channel_offset: v.channel_offset,
                        value: v.value,
                    })
                    .collect(),
            })
            .collect();
        scene.steps.push(SceneStep {
            id: uuid::Uuid::new_v4().to_string(),
            name: step.name,
            fade_in_ms: step.fade_in_ms,
            hold_ms: step.hold_ms,
            fixtures,
            // AI-generated steps don't manage chaser/movement layers —
            // operators add that in post if they want it. Inherit
            // means "leave whatever's running alone" on recall.
            chaser_state: SceneFxState::Inherit,
            movement_state: SceneFxState::Inherit,
        });
    }

    {
        let mut s = show.write();
        s.show.scenes.push(scene.clone());
        s.dirty = true;
    }
    persist_show(&show, &app)?;
    let _ = app.emit(SHOW_EVENT, ());
    Ok(scene)
}

/// Persist on every mutation. Always writes the autosave so an Untitled show
/// survives a restart; additionally writes the user-named file (with rotating
/// backups) when one is set. Errors writing the user path are propagated;
/// autosave failures are logged so a missing config dir doesn't block edits.
pub(crate) fn persist_show(show: &ShowState, _app: &AppHandle) -> Result<(), CommandError> {
    let (snapshot, target_path) = {
        let s = show.read();
        (s.show.clone(), s.path.clone())
    };
    if let Err(e) = crate::show::session::write_autosave(&snapshot, target_path.as_deref()) {
        tracing::warn!(error = %e, "autosave failed");
    }
    if let Some(p) = target_path {
        save_show_file(&p, &snapshot).map_err(CommandError::from)?;
    }
    show.write().dirty = false;
    Ok(())
}

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum CommandError {
    #[error("io: {0}")]
    Io(String),
    #[error("show: {0}")]
    Show(String),
    #[error("{0}")]
    Other(String),
}

impl From<ShowError> for CommandError {
    fn from(e: ShowError) -> Self {
        CommandError::Show(e.to_string())
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        CommandError::Io(e.to_string())
    }
}

#[allow(dead_code)]
fn _ensure_used(_: &OutputBindingConfig, _: &[OutputBinding]) {}
