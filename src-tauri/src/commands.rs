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
use crate::engine::{EngineError, EngineState, EngineStats, DMX_CHANNELS};
use crate::globals::GlobalsConfig;
use crate::midi::hub::SharedMidi;
use crate::midi::launchpad::{self, SharedLaunchpad};
use crate::midi::{MidiDeviceInfo, MidiStatus};
use crate::movement::MovementGenerator;
use crate::output::config::{instantiate, OutputBindingConfig, OutputsConfig};
use crate::output::d2xx::{list_devices as list_d2xx_devices, D2xxDeviceInfo};
use crate::output::discovery::{list_serial_ports, SerialPortInfo};
use crate::show::file::{load as load_show_file, save as save_show_file, ShowError, ShowFileV1};
use crate::show::fixture::{validate_patch, FixtureDefinition, FixtureInstance, PatchReport};
use crate::show::library::{ensure_seeded, library_dir, load_all};
use crate::show::ShowState;

pub const STATS_EVENT: &str = "engine:stats";
pub const SHOW_EVENT: &str = "show:updated";

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
pub fn set_master(engine: State<'_, EngineState>, value: u8) {
    tracing::trace!(target: "dmx::input", value, "master → engine");
    engine.write().master = value;
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
pub fn set_blind(globals: State<'_, SharedGlobals>, pressed: bool) {
    tracing::trace!(target: "dmx::input", pressed, "blind → globals");
    globals.lock().set_blind(pressed);
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
pub fn new_show(
    app: AppHandle,
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
    output_thread: State<'_, OutputThreadState>,
    chasers: State<'_, SharedChasers>,
    movement: State<'_, SharedMovement>,
    globals: State<'_, SharedGlobals>,
) -> Result<(), CommandError> {
    let outputs = {
        let mut s = show.write();
        s.show = ShowFileV1::default();
        s.path = None;
        s.dirty = false;
        s.show.outputs.clone()
    };
    apply_outputs(
        &app,
        &engine,
        &output_thread,
        &chasers,
        &movement,
        &globals,
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
        // resolve. We copy in (overwriting same-id entries from disk),
        // but we DON'T write them back to the on-disk library — that
        // would leak show-specific fixture defs into every other show.
        for def in bundled_defs {
            s.library.insert(def.id.clone(), def);
        }
    }
    apply_outputs(
        &app,
        &engine,
        &output_thread,
        &chasers,
        &movement,
        &globals,
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
pub fn remove_fixture(
    app: AppHandle,
    show: State<'_, ShowState>,
    chasers: State<'_, SharedChasers>,
    movement: State<'_, SharedMovement>,
    globals: State<'_, SharedGlobals>,
    id: String,
) -> Result<(), CommandError> {
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
    engine: State<'_, EngineState>,
    show: State<'_, ShowState>,
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

// ---- helpers + error type ------------------------------------------------

pub fn apply_outputs(
    app: &AppHandle,
    engine: &EngineState,
    output_thread: &OutputThreadState,
    chasers: &SharedChasers,
    movement: &SharedMovement,
    globals: &SharedGlobals,
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

/// Persist on every mutation. Always writes the autosave so an Untitled show
/// survives a restart; additionally writes the user-named file (with rotating
/// backups) when one is set. Errors writing the user path are propagated;
/// autosave failures are logged so a missing config dir doesn't block edits.
fn persist_show(show: &ShowState, _app: &AppHandle) -> Result<(), CommandError> {
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
