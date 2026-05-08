pub mod ai;
pub mod bridge;
pub mod chaser;
pub mod commands;
pub mod engine;
pub mod globals;
pub mod midi;
pub mod movement;
pub mod output;
pub mod programmer;
pub mod show;
pub mod streamdeck;
pub mod sync;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::Manager;

use crate::commands::{apply_outputs, OutputThreadState};
use crate::engine::output_thread::{shared_chasers, shared_globals, shared_movement};
use crate::engine::scene_playback::{shared_scene_playback, SharedScenePlayback};
use crate::engine::EngineState;
use crate::midi::hub::{shared_midi, SharedMidi};
use crate::midi::launchpad::{shared_launchpad, SharedLaunchpad};
use crate::streamdeck::controller::{shared_streamdeck, SharedStreamDeck};
use crate::programmer::shared_programmer;
use crate::show::library::{ensure_seeded, library_dir, load_all};
use crate::show::session::{
    read_autosave, read_engine_autosave, write_engine_autosave, EngineAutosave, UniverseAutosave,
};
use crate::show::{ShowFileV1, ShowState, ShowStateInner};

/// Catch any panic before `panic = "abort"` calls `abort()`, dump the
/// payload + backtrace to a file we can read after the fact, and also
/// echo to stderr for terminal runs. Without this, release builds launched
/// from Finder lose the panic message entirely (stderr goes nowhere and
/// the non-blocking tracing writer can't flush before the process dies).
fn install_panic_logger() {
    use std::backtrace::Backtrace;
    let log_dir = log_directory();
    let _ = std::fs::create_dir_all(&log_dir);
    let panic_path = log_dir.join("panic.log");
    std::panic::set_hook(Box::new(move |info| {
        let mut msg = String::new();
        msg.push_str("=== PANIC ===\n");
        if let Some(loc) = info.location() {
            msg.push_str(&format!("location: {}:{}\n", loc.file(), loc.line()));
        }
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "<non-string panic payload>".to_string()
        };
        msg.push_str(&format!("payload: {}\n", payload));
        let bt = Backtrace::force_capture();
        msg.push_str(&format!("backtrace:\n{}\n", bt));
        let _ = std::fs::write(&panic_path, &msg);
        eprintln!("{}", msg);
    }));
}

fn init_tracing() {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};
    use tracing_subscriber::fmt;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    let log_dir = log_directory();
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        eprintln!("could not create log dir {:?}: {}", log_dir, e);
    }

    // Probe writability before handing the dir to RollingFileAppender — if
    // a previous run left files owned by root (sudo accident), the appender
    // would panic in its constructor and there's no way to catch that under
    // `panic = "abort"`. Falling back to stderr-only is much safer than
    // refusing to boot.
    let probe = log_dir.join(".dmx-write-probe");
    let can_write_log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&probe)
        .is_ok();
    let _ = std::fs::remove_file(&probe);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,dmx=debug"));
    let stdout_layer = fmt::layer().with_target(false);
    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer);

    if can_write_log {
        let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_dir, "dmx-control.log");
        let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
        Box::leak(Box::new(guard));
        let file_layer = fmt::layer()
            .with_writer(file_writer)
            .with_ansi(false)
            .with_target(true);
        registry.with(file_layer).init();
        tracing::info!(log_dir = ?log_dir, "tracing initialized (file + stderr)");
    } else {
        registry.init();
        eprintln!(
            "[dmx-control] log dir {:?} not writable (check ownership); falling back to stderr-only",
            log_dir
        );
        tracing::warn!(
            log_dir = ?log_dir,
            "log dir not writable; tracing to stderr only — check ownership/permissions of the directory"
        );
    }
}

/// Resolve the log directory in a way that respects each platform's
/// conventions:
/// - macOS: `~/Library/Logs/dmx-control` (Apple's recommended path —
///   what `Console.app` indexes by default).
/// - Windows: `%LOCALAPPDATA%\dmx-control\logs` via `dirs::data_local_dir`.
/// - Linux / other: `~/.local/share/dmx-control/logs` via the same.
///
/// The macOS branch is preserved literally so existing installs keep
/// finding their logs in the same place after this refactor — we tested
/// the heck out of that path and don't want to migrate it.
fn log_directory() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            return home.join("Library/Logs/dmx-control");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(base) = dirs::data_local_dir() {
            return base.join("dmx-control").join("logs");
        }
    }
    std::env::temp_dir().join("dmx-control-logs")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_panic_logger();
    init_tracing();

    // Library: seed if missing, load fixtures.
    let library = match library_dir() {
        Some(dir) => match ensure_seeded(&dir) {
            Ok(_) => match load_all(&dir) {
                Ok(lib) => {
                    tracing::info!(count = lib.len(), dir = %dir.display(), "fixture library loaded");
                    lib
                }
                Err(err) => {
                    tracing::warn!(?err, "failed to load fixture library; starting empty");
                    Default::default()
                }
            },
            Err(err) => {
                tracing::warn!(?err, "failed to seed fixture library; starting empty");
                Default::default()
            }
        },
        None => {
            tracing::warn!("no config dir available; fixture library will be empty");
            Default::default()
        }
    };

    let (initial_show, initial_path) = match read_autosave() {
        Some((show, path)) => {
            tracing::info!(
                fixtures = show.fixtures.len(),
                outputs = show.outputs.bindings.len(),
                path = ?path,
                "restored autosave"
            );
            (show, path)
        }
        None => (ShowFileV1::default(), None),
    };

    let show_inner = ShowStateInner {
        show: initial_show,
        library,
        path: initial_path,
        dirty: false,
    };

    let engine = EngineState::with_universes(&show_inner.show.outputs.universes());
    // Resilience: restore the previous engine snapshot (channel values +
    // master) so a crash/relaunch comes back to the same look the operator
    // had on stage. Show-level config (chasers, movement, etc.) is restored
    // separately via the show file autosave above.
    if let Some(snap) = read_engine_autosave() {
        let mut e = engine.write();
        e.master = snap.master;
        for u_snap in snap.universes {
            if let Some(u) = e.universes.iter_mut().find(|u| u.id == u_snap.id) {
                if u_snap.data.len() == crate::engine::DMX_CHANNELS {
                    let bytes: [u8; crate::engine::DMX_CHANNELS] = u_snap
                        .data
                        .try_into()
                        .unwrap_or([0u8; crate::engine::DMX_CHANNELS]);
                    u.data = bytes;
                }
            }
        }
        tracing::info!("restored engine autosave");
    }
    let show_state = ShowState::new(show_inner);
    let chasers_handle = shared_chasers();
    let movement_handle = shared_movement();
    let globals_handle = shared_globals();
    let midi_handle = shared_midi();
    let launchpad_handle = shared_launchpad();
    let streamdeck_handle = shared_streamdeck();
    let scene_playback_handle = shared_scene_playback();
    let programmer_handle = shared_programmer();
    let bridge_state = crate::bridge::BridgeState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(engine.clone())
        .manage(show_state.clone())
        .manage(OutputThreadState(Mutex::new(None)))
        .manage(chasers_handle.clone())
        .manage(movement_handle.clone())
        .manage(globals_handle.clone())
        .manage(midi_handle.clone())
        .manage(launchpad_handle.clone())
        .manage(streamdeck_handle.clone())
        .manage(scene_playback_handle.clone())
        .manage(programmer_handle.clone())
        .manage(bridge_state.clone())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let engine_state: tauri::State<'_, EngineState> = app.state();
            let show_state_st: tauri::State<'_, ShowState> = app.state();
            let output_thread: tauri::State<'_, OutputThreadState> = app.state();
            let chasers_st: tauri::State<'_, crate::engine::output_thread::SharedChasers> =
                app.state();
            let movement_st: tauri::State<'_, crate::engine::output_thread::SharedMovement> =
                app.state();
            let globals_st: tauri::State<'_, crate::engine::output_thread::SharedGlobals> =
                app.state();
            let scenes_st: tauri::State<'_, SharedScenePlayback> = app.state();

            let outputs = show_state_st.read().show.outputs.clone();
            if let Err(err) = apply_outputs(
                &app_handle,
                &engine_state,
                &output_thread,
                &chasers_st,
                &movement_st,
                &globals_st,
                &scenes_st,
                &outputs,
            ) {
                tracing::warn!(?err, "failed to apply initial outputs");
            }
            // Push the loaded show into the runtime engines so anything
            // persisted in the file resumes on launch.
            let (fixtures, library, chasers_list, movement_list, globals_cfg) = {
                let s = show_state_st.read();
                (
                    s.show.fixtures.clone(),
                    s.library.clone(),
                    s.show.chasers.clone(),
                    s.show.movements.clone(),
                    s.show.globals.clone(),
                )
            };
            {
                let mut e = chasers_st.lock();
                e.update_show_context(fixtures.clone(), library.clone());
                e.replace_chasers(chasers_list);
            }
            {
                let mut m = movement_st.lock();
                m.update_show_context(fixtures.clone(), library.clone());
                m.replace_generators(movement_list);
            }
            {
                let mut g = globals_st.lock();
                g.update_show_context(fixtures, library);
                g.replace_config(globals_cfg);
            }

            // Auto-connect: if a Launchpad is already plugged in at
            // launch, fire it up without waiting for the user to open
            // the MIDI tab and click Connect. We pick the first device
            // whose name looks Launchpad-y; multi-LP rigs are rare
            // enough that "first match wins" is fine for now.
            let midi_st: tauri::State<'_, SharedMidi> = app.state();
            let lp_st: tauri::State<'_, SharedLaunchpad> = app.state();
            if let Some(name) = midi::hub::list_devices()
                .into_iter()
                .find(|d| midi::launchpad::is_launchpad(&d.name))
                .map(|d| d.name)
            {
                let shared_midi_inner = midi_st.inner().clone();
                let connect_result =
                    midi_st
                        .lock()
                        .connect(&name, app_handle.clone(), shared_midi_inner.clone());
                match connect_result {
                    Ok(()) => {
                        let controller = midi::launchpad::start(
                            app_handle.clone(),
                            shared_midi_inner,
                            chasers_st.inner().clone(),
                            movement_st.inner().clone(),
                            globals_st.inner().clone(),
                            scenes_st.inner().clone(),
                            engine_state.inner().clone(),
                            show_state_st.inner().clone(),
                        );
                        *lp_st.lock() = Some(controller);
                        tracing::info!(%name, "auto-connected MIDI surface at launch");
                    }
                    Err(err) => tracing::warn!(%name, %err, "auto-connect failed"),
                }
            }

            // Same auto-connect logic for the Stream Deck. Independent of
            // MIDI: an operator may have just an SD, or both surfaces
            // running side by side.
            let sd_st: tauri::State<'_, SharedStreamDeck> = app.state();
            let sd_devices = streamdeck::controller::list_streamdeck_devices();
            if let Some(first) = sd_devices.into_iter().next() {
                match streamdeck::controller::start(
                    app_handle.clone(),
                    Some(&first.serial),
                    chasers_st.inner().clone(),
                    movement_st.inner().clone(),
                    globals_st.inner().clone(),
                    scenes_st.inner().clone(),
                    engine_state.inner().clone(),
                    show_state_st.inner().clone(),
                ) {
                    Ok(controller) => {
                        *sd_st.lock() = Some(controller);
                        tracing::info!(serial = %first.serial, kind = %first.kind, "auto-connected streamdeck surface at launch");
                    }
                    Err(err) => tracing::warn!(serial = %first.serial, %err, "streamdeck auto-connect failed"),
                }
            } else {
                tracing::debug!("no streamdeck devices found at launch");
            }

            // Background snapshot thread: every ~1.5 s we dump the live
            // engine state (universes + master) to disk. The output thread
            // can't do this itself without risking I/O latency in the DMX
            // loop, so we run a separate sleeper. Cheap (a few KB) and the
            // worst-case data loss on a crash is whatever happened in the
            // last 1.5 s of slider movement.
            let engine_for_snapshot = engine_state.inner().clone();
            std::thread::Builder::new()
                .name("dmx-engine-autosave".into())
                .spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    let snap = {
                        let g = engine_for_snapshot.read();
                        EngineAutosave {
                            master: g.master,
                            universes: g
                                .universes
                                .iter()
                                .map(|u| UniverseAutosave {
                                    id: u.id,
                                    data: u.data.to_vec(),
                                })
                                .collect(),
                        }
                    };
                    if let Err(e) = write_engine_autosave(&snap) {
                        tracing::warn!(error = %e, "engine autosave write failed");
                    }
                })
                .expect("spawn engine autosave thread");

            // Scene FX consumer: drains the channel that ScenePlayback
            // pushes to whenever it crosses a step boundary or fires a
            // release-restore. Living off the output thread keeps the
            // chaser/movement persist + emit machinery from blocking
            // DMX frames. Idle until a recall happens; cheap.
            let (fx_tx, fx_rx) =
                crossbeam_channel::unbounded::<crate::engine::scene_playback::SceneFxApply>();
            scene_playback_handle.lock().set_fx_sender(fx_tx);
            let app_for_fx = app_handle.clone();
            let show_for_fx = show_state_st.inner().clone();
            let chasers_for_fx = chasers_st.inner().clone();
            let movement_for_fx = movement_st.inner().clone();
            std::thread::Builder::new()
                .name("dmx-scene-fx".into())
                .spawn(move || {
                    for req in fx_rx.iter() {
                        commands::apply_scene_fx_request(
                            &app_for_fx,
                            &show_for_fx,
                            &chasers_for_fx,
                            &movement_for_fx,
                            &req,
                        );
                    }
                })
                .expect("spawn scene FX consumer thread");

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                // Order matters here:
                //   1. Stop the Launchpad feedback thread first so it
                //      blanks every pad and clears the input router
                //      cleanly. Doing this BEFORE dropping the MIDI
                //      hub means the final SysEx (turn-everything-off)
                //      actually reaches the device.
                //   2. Disconnect the MIDI hub — drops `midir`'s input
                //      callback and the output port. After this the
                //      controller is no longer talking to the OS.
                //   3. Stop the DMX output thread last. Killing it
                //      first would leave its `OutputDriver` open while
                //      the LP teardown is still trying to send SysEx,
                //      which is fine but makes the logs noisy.
                if let Some(sd) = window.state::<SharedStreamDeck>().lock().take() {
                    tracing::info!("window close: shutting down streamdeck controller");
                    sd.shutdown();
                }
                if let Some(lp) = window.state::<SharedLaunchpad>().lock().take() {
                    tracing::info!("window close: shutting down launchpad controller");
                    lp.shutdown();
                }
                window.state::<SharedMidi>().lock().disconnect();
                let handle_opt = window.state::<OutputThreadState>().0.lock().unwrap().take();
                if let Some(handle) = handle_opt {
                    tracing::info!("window close: shutting down output thread");
                    handle.shutdown();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Direct DMX
            commands::set_channel,
            commands::set_master,
            commands::blackout,
            commands::clear_universe,
            commands::get_universe,
            commands::get_universe_output,
            commands::list_universes,
            commands::dmx_channels,
            // Outputs
            commands::list_serial_ports_cmd,
            commands::list_ftdi_devices,
            commands::get_outputs,
            commands::set_outputs,
            // Library + Show
            commands::list_fixture_definitions,
            commands::reload_library,
            commands::get_library_dir,
            commands::set_fixture_image,
            commands::set_channel_range_image,
            commands::get_show,
            commands::get_show_path,
            commands::new_show,
            commands::open_show,
            commands::save_show,
            commands::rename_show,
            // Patch
            commands::add_fixture,
            commands::add_fixtures,
            commands::remove_fixture,
            commands::update_fixture,
            commands::move_fixture,
            commands::validate_patch_cmd,
            commands::set_fixture_channel,
            commands::get_fixture_values,
            // Ambient Chaser
            commands::list_chasers,
            commands::create_chaser,
            commands::update_chaser,
            commands::delete_chaser,
            commands::toggle_chaser,
            commands::add_example_chasers,
            // Movement Generator
            commands::list_movements,
            commands::create_movement,
            commands::update_movement,
            commands::delete_movement,
            commands::toggle_movement,
            // Scenes (Phase 4)
            commands::list_scenes,
            commands::create_scene_from_state,
            commands::add_scene_step,
            commands::remove_scene_step,
            commands::update_scene_step_from_state,
            commands::update_scene,
            commands::delete_scene,
            commands::recall_scene,
            commands::release_scene,
            commands::active_scene_id,
            commands::active_scene_step,
            commands::programmer_status,
            commands::programmer_clear,
            commands::programmer_untouch,
            // AI scene generation (POC)
            commands::get_ai_config,
            commands::set_ai_config,
            commands::ai_list_models,
            commands::ai_test_connection,
            commands::ai_generate_scene_draft,
            commands::ai_apply_draft_scene,
            commands::ai_replace_scene,
            // Globals (Blackout + Blind)
            commands::get_globals,
            commands::update_globals,
            commands::set_blackout,
            commands::set_blind,
            commands::set_overall_bpm,
            commands::set_overall_bpm_enabled,
            commands::tap_overall_bpm,
            // MIDI
            commands::list_midi_devices,
            commands::connect_midi_device,
            commands::disconnect_midi,
            commands::get_midi_status,
            commands::send_midi_raw,
            // Stream Deck
            commands::list_streamdeck_devices,
            commands::connect_streamdeck_device,
            commands::disconnect_streamdeck,
            commands::get_streamdeck_status,
            // Mobile remote bridge
            bridge::bridge_start,
            bridge::bridge_stop,
            bridge::bridge_status,
            bridge::bridge_begin_pairing,
            bridge::bridge_cancel_pairing,
            bridge::bridge_list_devices,
            bridge::bridge_revoke_device,
            // Cross-machine config sync (private GitHub Gist)
            sync::sync_status,
            sync::sync_save_settings,
            sync::sync_set_token,
            sync::sync_clear_token,
            sync::sync_whoami,
            sync::sync_probe,
            sync::sync_push,
            sync::sync_pull,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
