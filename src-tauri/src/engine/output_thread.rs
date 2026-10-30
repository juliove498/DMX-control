use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::chaser::engine::ChaserEngine;
use crate::engine::scene_playback::SharedScenePlayback;
use crate::engine::{merge_overlays, EngineState, EngineStats, DMX_CHANNELS};
use crate::globals::runtime::GlobalsRuntime;
use crate::movement::engine::MovementEngine;
use crate::output::OutputDriver;

/// 30 fps, matching what Freestyler / QLC+ / hardware consoles put on the
/// wire. It used to be 15, and that was implicated in real hardware
/// misbehaviour: cheap fixtures run a "DMX signal present" watchdog with a
/// window not much above 66 ms, so a 15 fps line sits right at the edge and
/// flaps between signal/no-signal — harmless mid-show (they hold values)
/// but catastrophic while a moving head runs its power-on homing sweep.
/// Don't raise past ~40: a full 513-slot frame takes 22.8 ms on the wire
/// at 250 kbaud, and the FTDI drain guard (see `d2xx.rs`) needs headroom.
pub const TARGET_FPS: u32 = 30;
/// Frame budget warning threshold. Sized at ~80% of the period (33.3 ms at
/// 30 fps) so we only warn when a frame is genuinely at risk of overshooting.
pub const WATCHDOG_THRESHOLD: Duration = Duration::from_millis(26);
const STATS_PERIOD: Duration = Duration::from_secs(1);

pub struct OutputBinding {
    /// Config id, so send results can be reported per output (header chips).
    pub id: String,
    pub driver: Box<dyn OutputDriver>,
    pub universes: Vec<u16>,
}

/// Live-mutable list of bindings, swappable from the main thread without
/// stopping the output loop. Each iteration acquires a brief lock.
pub type SharedBindings = Arc<Mutex<Vec<OutputBinding>>>;

/// Live-mutable chaser engine, ticked once per output frame. Sharing the
/// `ChaserEngine` between UI (config edits) and output thread (tick) keeps
/// the chaser's clock continuous across re-bindings.
pub type SharedChasers = Arc<Mutex<ChaserEngine>>;
/// Same idea for the Movement Generator. One instance per show.
pub type SharedMovement = Arc<Mutex<MovementEngine>>;
/// Globals runtime (Blackout + Blind). One instance per show.
pub type SharedGlobals = Arc<Mutex<GlobalsRuntime>>;

pub fn shared_bindings(initial: Vec<OutputBinding>) -> SharedBindings {
    Arc::new(Mutex::new(initial))
}

pub fn shared_chasers() -> SharedChasers {
    Arc::new(Mutex::new(ChaserEngine::new()))
}

pub fn shared_movement() -> SharedMovement {
    Arc::new(Mutex::new(MovementEngine::new()))
}

pub fn shared_globals() -> SharedGlobals {
    Arc::new(Mutex::new(GlobalsRuntime::default()))
}

pub struct OutputThreadHandle {
    stop: Arc<AtomicBool>,
    bindings: SharedBindings,
    chasers: SharedChasers,
    movement: SharedMovement,
    globals: SharedGlobals,
    join: Option<JoinHandle<()>>,
}

impl OutputThreadHandle {
    /// Signal the thread to stop, send a final blackout frame, and join it.
    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }

    /// Replace the live bindings without stopping the output thread.
    /// The next loop iteration picks up the new set; old drivers are dropped here.
    pub fn replace_bindings(&self, new_bindings: Vec<OutputBinding>) {
        let mut guard = self.bindings.lock();
        *guard = new_bindings;
    }

    pub fn bindings_handle(&self) -> SharedBindings {
        self.bindings.clone()
    }

    pub fn chasers_handle(&self) -> SharedChasers {
        self.chasers.clone()
    }

    pub fn movement_handle(&self) -> SharedMovement {
        self.movement.clone()
    }

    pub fn globals_handle(&self) -> SharedGlobals {
        self.globals.clone()
    }
}

impl Drop for OutputThreadHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn<F>(
    engine: EngineState,
    bindings: SharedBindings,
    chasers: SharedChasers,
    movement: SharedMovement,
    globals: SharedGlobals,
    scenes: SharedScenePlayback,
    on_stats: F,
) -> OutputThreadHandle
where
    F: Fn(EngineStats) + Send + 'static,
{
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let bindings_thread = bindings.clone();
    let chasers_thread = chasers.clone();
    let movement_thread = movement.clone();
    let globals_thread = globals.clone();
    let scenes_thread = scenes.clone();

    let join = thread::Builder::new()
        .name("dmx-output".into())
        .spawn(move || {
            // Hoist this thread above the UI / Tauri runtime / system noise.
            // DMX is real-time: a 50 ms scheduler gap shows up as a fixture
            // flicker. macOS maps `Max` priority to the QOS_CLASS_USER_INTERACTIVE
            // band, the same one Core Audio uses for live playback.
            if let Err(e) =
                thread_priority::set_current_thread_priority(thread_priority::ThreadPriority::Max)
            {
                tracing::warn!(error = ?e, "could not raise output thread priority");
            } else {
                tracing::info!(target: "dmx::output", "output thread elevated to real-time priority");
            }

            let period = Duration::from_nanos(1_000_000_000 / TARGET_FPS as u64);
            let mut next_tick = Instant::now();
            let mut frames_total: u32 = 0;
            let mut late_frames: u32 = 0;
            let mut max_frame = Duration::ZERO;
            let mut last_stats_at = Instant::now();
            let mut frames_at_last_stats: u32 = 0;

            tracing::info!(target: "dmx::output", target_fps = TARGET_FPS, "output thread started");

            // Per-universe snapshot of the previous frame so we can log only the
            // channels that actually changed and confirm UI fader moves are
            // making it all the way to the wire.
            let mut previous: HashMap<u16, [u8; DMX_CHANNELS]> = HashMap::new();
            let mut send_logs: Vec<SendLog> = Vec::new();

            loop {
                if stop_thread.load(Ordering::Relaxed) {
                    tracing::info!(target: "dmx::output", "stop requested; sending blackout");
                    let mut bs = bindings_thread.lock();
                    send_blackout(&mut bs);
                    break;
                }

                let frame_start = Instant::now();
                // Scene playback runs first because it writes directly
                // into Universe.data (the base layer). Effects + blind
                // + master + blackout will compose on top of that.
                //
                // Read the Overall BPM override once for this frame so
                // every clock-driven module (scene playback, chaser,
                // movement) shares one consistent view of the tempo and
                // doesn't drift across the same frame.
                // Snapshot the global tempo state under one lock so the
                // chaser engine sees a consistent (bpm, pattern, anchor)
                // tuple even if the operator commits a new pattern mid-
                // frame.
                let (overall_bpm, pattern_snapshot) = {
                    let g = globals_thread.lock();
                    let bpm = g.current_overall_bpm();
                    let pat = g
                        .current_tempo_pattern()
                        .map(|(p, a)| (p.clone(), a));
                    (bpm, pat)
                };
                {
                    let updates = scenes_thread.lock().tick(frame_start, overall_bpm);
                    if !updates.is_empty() {
                        let mut e = engine.write();
                        for ((u, ch), v) in updates {
                            let _ = e.set_channel(u, ch, v);
                        }
                    }
                }
                // A snapshot crossfade writes the base layer the same way.
                if engine.read().base_fade_active() {
                    engine.write().tick_base_fade(frame_start);
                }
                // Tick the effect modules + the globals runtime, then push
                // their outputs into the engine before we snapshot. Each
                // tick is a few arithmetic ops per slot so locks are
                // released quickly.
                {
                    let pattern_ref = pattern_snapshot.as_ref().map(|(p, a)| (p, *a));
                    let chaser_ov = chasers_thread
                        .lock()
                        .tick(frame_start, overall_bpm, pattern_ref);
                    let movement_ov = movement_thread.lock().tick(frame_start, overall_bpm);
                    let merged = merge_overlays(chaser_ov, movement_ov);
                    // Modo Mágico rides on top: it owns its participants outright.
                    let autopilot_ov = crate::autopilot::tick(frame_start, overall_bpm);
                    let merged = merge_overlays(merged, autopilot_ov);
                    // Pre-show fixture check: a highlight on top of everything,
                    // still under blind / blackout / master.
                    let test_ov = crate::fixture_test::tick(frame_start);
                    let merged = merge_overlays(merged, test_ov);
                    let (blind_overlay, blackout_overlay, master_mask, blackout_factor, blind_factor) = {
                        let mut g = globals_thread.lock();
                        let out = g.tick(frame_start);
                        (
                            out.blind,
                            out.blackout,
                            out.master,
                            g.blackout_factor,
                            g.blind_factor,
                        )
                    };
                    let mut e = engine.write();
                    e.replace_effects_overlay(merged);
                    e.replace_blind_overlay(blind_overlay);
                    e.replace_blackout_overlay(blackout_overlay);
                    e.replace_master_mask(master_mask);
                    e.blackout_factor = blackout_factor;
                    e.blind_factor = blind_factor;
                }
                {
                    let mut bs = bindings_thread.lock();
                    let snapshots = collect_snapshots(&engine, &bs);
                    // Frame-diff logging is opt-in via RUST_LOG=trace — too
                    // noisy for the default INFO level once shows have any
                    // movement on them.
                    if tracing::enabled!(target: "dmx::output", tracing::Level::TRACE) {
                        log_snapshot_changes(&snapshots, &mut previous);
                    }
                    for binding in bs.iter_mut() {
                        for &u in &binding.universes {
                            if let Some(data) =
                                snapshots.iter().find(|(id, _)| *id == u).map(|(_, d)| d)
                            {
                                match binding.driver.send(u, data) {
                                    Ok(()) => {
                                        crate::output::health::note_send_ok(&binding.id);
                                        if let Some(l) = send_logs.iter_mut().find(|l| {
                                            l.failing && l.binding_id == binding.id && l.universe == u
                                        }) {
                                            l.failing = false;
                                            tracing::info!(
                                                target: "dmx::output",
                                                driver = binding.driver.name(),
                                                universe = u,
                                                "send recovered"
                                            );
                                        }
                                    }
                                    Err(err) => {
                                        let text = err.to_string();
                                        let l = send_log_entry(&mut send_logs, &binding.id, u);
                                        let changed = !l.failing || l.error != text;
                                        if changed || l.since_warn.elapsed() >= SEND_WARN_EVERY {
                                            tracing::warn!(
                                                target: "dmx::output",
                                                driver = binding.driver.name(),
                                                universe = u,
                                                error = %text,
                                                suppressed = l.suppressed,
                                                "send failed"
                                            );
                                            l.since_warn = Instant::now();
                                            l.suppressed = 0;
                                            l.error = text.clone();
                                        } else {
                                            l.suppressed += 1;
                                        }
                                        l.failing = true;
                                        crate::output::health::note_send_error(&binding.id, &text);
                                    }
                                }
                            }
                        }
                    }
                }

                crate::output::health::tick(frame_start);

                let elapsed = frame_start.elapsed();
                if elapsed > WATCHDOG_THRESHOLD {
                    tracing::warn!(
                        target: "dmx::output",
                        elapsed_us = elapsed.as_micros() as u64,
                        "frame exceeded watchdog threshold"
                    );
                    late_frames = late_frames.saturating_add(1);
                }
                if elapsed > max_frame {
                    max_frame = elapsed;
                }
                frames_total = frames_total.saturating_add(1);

                let stats_elapsed = last_stats_at.elapsed();
                if stats_elapsed >= STATS_PERIOD {
                    let secs = stats_elapsed.as_secs_f32();
                    let frames_in_window = frames_total.wrapping_sub(frames_at_last_stats);
                    let fps = if secs > 0.0 {
                        frames_in_window as f32 / secs
                    } else {
                        0.0
                    };
                    on_stats(EngineStats {
                        fps,
                        frames_total,
                        late_frames,
                        max_frame_micros: max_frame.as_micros().min(u32::MAX as u128) as u32,
                    });
                    last_stats_at = Instant::now();
                    frames_at_last_stats = frames_total;
                    max_frame = Duration::ZERO;
                }

                next_tick += period;
                let now = Instant::now();
                if next_tick <= now {
                    // Late frame. Do NOT fire a catch-up frame immediately:
                    // two DMX frames back-to-back overlap the FTDI's TX
                    // drain (write_bulk returns while ~256 bytes are still
                    // shifting out), and the second frame then goes on the
                    // wire with residual bytes prepended — the receiver
                    // reads those as start code + first channels and every
                    // real channel lands shifted. Cheap movers read that
                    // garbage as pan/tilt commands. Skip the missed ticks
                    // and re-align to the grid instead.
                    while next_tick <= now {
                        next_tick += period;
                    }
                }
                thread::sleep(next_tick - now);
            }

            tracing::info!(target: "dmx::output", frames_total, late_frames, "output thread stopped");
        })
        .expect("spawn dmx-output thread");

    OutputThreadHandle {
        stop,
        bindings,
        chasers,
        movement,
        globals,
        join: Some(join),
    }
}

/// Log per-frame channel diffs at TRACE level. Only channels whose value
/// changed since the previous frame are reported, capped at the first 8 to
/// keep the log readable. Off by default; enable with
/// `RUST_LOG=dmx::output=trace` when you need to confirm a UI change is
/// reaching the wire.
fn log_snapshot_changes(
    snapshots: &[(u16, [u8; DMX_CHANNELS])],
    previous: &mut HashMap<u16, [u8; DMX_CHANNELS]>,
) {
    const MAX_LOGGED: usize = 8;
    for (universe, snap) in snapshots {
        let entry = previous.entry(*universe).or_insert([0u8; DMX_CHANNELS]);
        if entry == snap {
            continue;
        }
        let mut changes: Vec<(u16, u8, u8)> = Vec::new();
        let mut total = 0usize;
        for i in 0..DMX_CHANNELS {
            if entry[i] != snap[i] {
                total += 1;
                if changes.len() < MAX_LOGGED {
                    changes.push((i as u16 + 1, entry[i], snap[i]));
                }
            }
        }
        tracing::trace!(
            target: "dmx::output",
            universe = *universe,
            changed = total,
            sample = ?changes,
            "frame diff"
        );
        *entry = *snap;
    }
}

fn collect_snapshots(
    engine: &EngineState,
    bindings: &[OutputBinding],
) -> Vec<(u16, [u8; DMX_CHANNELS])> {
    let mut wanted: Vec<u16> = bindings
        .iter()
        .flat_map(|b| b.universes.iter().copied())
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    let g = engine.read();
    wanted
        .into_iter()
        .filter_map(|u| g.snapshot_universe(u).map(|d| (u, d)))
        .collect()
}

/// Per-universe send-failure log throttle: the first failure (and any
/// change of error text) is a WARN, repeats are summarised every
/// `SEND_WARN_EVERY`, recovery is an INFO. An unplugged node used to
/// log ~90 lines per second from the real-time thread.
struct SendLog {
    binding_id: String,
    universe: u16,
    failing: bool,
    error: String,
    since_warn: Instant,
    suppressed: u32,
}

const SEND_WARN_EVERY: Duration = Duration::from_secs(10);

fn send_log_entry<'a>(logs: &'a mut Vec<SendLog>, id: &str, u: u16) -> &'a mut SendLog {
    if let Some(i) = logs
        .iter()
        .position(|l| l.binding_id == id && l.universe == u)
    {
        &mut logs[i]
    } else {
        logs.push(SendLog {
            binding_id: id.to_string(),
            universe: u,
            failing: false,
            error: String::new(),
            since_warn: Instant::now(),
            suppressed: 0,
        });
        logs.last_mut().expect("just pushed")
    }
}

fn send_blackout(bindings: &mut [OutputBinding]) {
    let zero = [0u8; DMX_CHANNELS];
    for binding in bindings.iter_mut() {
        for &u in &binding.universes {
            if let Err(err) = binding.driver.send(u, &zero) {
                tracing::warn!(
                    target: "dmx::output",
                    driver = binding.driver.name(),
                    universe = u,
                    error = %err,
                    "blackout send failed"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{OutputDriver, OutputError};
    use std::sync::atomic::AtomicU32;

    struct CountingDriver {
        frames: Arc<AtomicU32>,
        last: Arc<std::sync::Mutex<[u8; DMX_CHANNELS]>>,
    }

    impl OutputDriver for CountingDriver {
        fn name(&self) -> &'static str {
            "counting"
        }
        fn send(&mut self, _u: u16, data: &[u8; DMX_CHANNELS]) -> Result<(), OutputError> {
            self.frames.fetch_add(1, Ordering::Relaxed);
            *self.last.lock().unwrap() = *data;
            Ok(())
        }
    }

    #[test]
    fn thread_runs_and_blackouts_on_shutdown() {
        let engine = EngineState::new(1);
        engine.write().set_channel(0, 0, 200).unwrap();

        let frames = Arc::new(AtomicU32::new(0));
        let last = Arc::new(std::sync::Mutex::new([0u8; DMX_CHANNELS]));
        let driver = CountingDriver {
            frames: frames.clone(),
            last: last.clone(),
        };
        let bindings = shared_bindings(vec![OutputBinding {
            id: "test".into(),
            driver: Box::new(driver),
            universes: vec![0],
        }]);

        let handle = spawn(
            engine.clone(),
            bindings,
            shared_chasers(),
            shared_movement(),
            shared_globals(),
            crate::engine::scene_playback::shared_scene_playback(),
            |_| {},
        );
        thread::sleep(Duration::from_millis(400));
        handle.shutdown();

        // ~12 frames in 400ms at 30Hz; allow generous slack for slow CI.
        assert!(frames.load(Ordering::Relaxed) >= 4);
        assert_eq!(last.lock().unwrap()[0], 0);
    }

    #[test]
    fn replace_bindings_swaps_drivers_at_runtime() {
        let engine = EngineState::new(2);
        engine.write().set_channel(1, 0, 99).unwrap();

        let frames_a = Arc::new(AtomicU32::new(0));
        let frames_b = Arc::new(AtomicU32::new(0));
        let last_b = Arc::new(std::sync::Mutex::new([0u8; DMX_CHANNELS]));

        let initial = vec![OutputBinding {
            id: "test".into(),
            driver: Box::new(CountingDriver {
                frames: frames_a.clone(),
                last: Arc::new(std::sync::Mutex::new([0u8; DMX_CHANNELS])),
            }),
            universes: vec![0],
        }];
        let bindings = shared_bindings(initial);
        let handle = spawn(
            engine.clone(),
            bindings,
            shared_chasers(),
            shared_movement(),
            shared_globals(),
            crate::engine::scene_playback::shared_scene_playback(),
            |_| {},
        );

        thread::sleep(Duration::from_millis(220));
        let frames_a_at_swap = frames_a.load(Ordering::Relaxed);

        handle.replace_bindings(vec![OutputBinding {
            id: "test".into(),
            driver: Box::new(CountingDriver {
                frames: frames_b.clone(),
                last: last_b.clone(),
            }),
            universes: vec![1],
        }]);

        thread::sleep(Duration::from_millis(330));
        handle.shutdown();

        // Driver A should have stopped receiving frames after the swap.
        let frames_a_final = frames_a.load(Ordering::Relaxed);
        assert!(
            frames_a_final - frames_a_at_swap <= 2,
            "driver A still got frames after swap"
        );
        // Driver B should have received frames including the universe 1 data.
        assert!(frames_b.load(Ordering::Relaxed) >= 2);
    }
}
