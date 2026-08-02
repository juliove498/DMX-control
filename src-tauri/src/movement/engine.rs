//! Live movement engine. Owns a list of `MovementGenerator` configs, each
//! with its own runtime state. Ticked once per output-thread frame; only
//! generators with `enabled = true` contribute to the overlay. The
//! exclusivity rule ("at most one enabled at a time") is enforced at the
//! command layer (`toggle_movement_impl`), so the engine can stay simple
//! and just iterate. Produces an overlay map keyed by universe, the same
//! shape the Ambient Chaser uses, so the engine's `replace_effects_overlay`
//! can take both modules' outputs.

use std::collections::HashMap;
use std::time::Instant;

use crate::engine::beatgrid::BeatAnchor;
use crate::engine::{empty_overlay, ChannelOverlay, DMX_CHANNELS};
use crate::show::fixture::{ChannelRole, FixtureDefinition, FixtureInstance, PanTiltRange};

use super::pipeline::{evaluate_slot, map_to_dmx};
use super::{Direction, MovementGenerator, MovementSlot, Subdivision, TempoSource};

/// Pre-resolved per-slot routing: which DMX channels in which universe
/// receive pan-coarse, pan-fine, tilt-coarse and tilt-fine, plus the
/// physical ranges to use during the final mapping.
#[derive(Debug, Clone)]
struct ResolvedSlot {
    /// The user's `MovementSlot` cloned through so `evaluate_slot` has the
    /// invert flags and phase offset without an extra lookup.
    slot: MovementSlot,
    universe: u16,
    pan_coarse: Option<usize>,
    pan_fine: Option<usize>,
    tilt_coarse: Option<usize>,
    tilt_fine: Option<usize>,
    pan_range: PanTiltRange,
    tilt_range: PanTiltRange,
}

#[derive(Debug)]
pub struct MovementRuntime {
    /// `f64` to keep precision over multi-hour sessions. Convert to `f32`
    /// only at the moment of evaluating a shape.
    pub global_phase: f64,
    pub last_update: Option<Instant>,
    /// `+1.0` for forward, `-1.0` for reverse. PingPong toggles this at
    /// each loop boundary so it stays consistent across frames.
    pub direction_sign: f64,
}

impl Default for MovementRuntime {
    fn default() -> Self {
        Self {
            global_phase: 0.0,
            last_update: None,
            direction_sign: 1.0,
        }
    }
}

/// One generator + its derived state, kept together so we can preserve
/// the runtime phase across config edits via id lookup.
#[derive(Debug)]
struct MovementEntry {
    config: MovementGenerator,
    resolved: Vec<ResolvedSlot>,
    runtime: MovementRuntime,
}

#[derive(Debug, Default)]
pub struct MovementEngine {
    entries: Vec<MovementEntry>,
    fixtures: Vec<FixtureInstance>,
    library: HashMap<String, FixtureDefinition>,
    /// External phase anchor. Same role as the chaser engine's: when
    /// `Some`, advance_phase derives the cycle position from VDJ's
    /// beat grid; when `None`, free-runs on the generator's own
    /// tempo. The poller writes both engines in lockstep so they stay
    /// coherent with each other.
    beat_anchor: Option<BeatAnchor>,
}

impl MovementEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the show context. Re-resolves slot offsets without touching
    /// runtime, so a fixture move doesn't reset the global phase.
    pub fn update_show_context(
        &mut self,
        fixtures: Vec<FixtureInstance>,
        library: HashMap<String, FixtureDefinition>,
    ) {
        self.fixtures = fixtures;
        self.library = library;
        self.resolve_all();
    }

    /// Replace the entire set of generators. Preserves runtime phase
    /// for ids that survived the swap (mirrors `ChaserEngine`) so an
    /// edit to one generator doesn't reset the others' timing.
    pub fn replace_generators(&mut self, generators: Vec<MovementGenerator>) {
        let mut existing: HashMap<String, MovementRuntime> = self
            .entries
            .drain(..)
            .map(|e| (e.config.id, e.runtime))
            .collect();
        self.entries = generators
            .into_iter()
            .map(|cfg| MovementEntry {
                resolved: resolve_slots(&cfg, &self.fixtures, &self.library),
                runtime: existing.remove(&cfg.id).unwrap_or_default(),
                config: cfg,
            })
            .collect();
    }

    pub fn list(&self) -> Vec<MovementGenerator> {
        self.entries.iter().map(|e| e.config.clone()).collect()
    }

    /// Install or clear the external phase anchor. Mirrors
    /// `ChaserEngine::set_beat_anchor` — the VDJ poller calls both
    /// engines so chases and movement stay locked to the same beat
    /// grid as the music.
    pub fn set_beat_anchor(&mut self, anchor: Option<BeatAnchor>) {
        self.beat_anchor = anchor;
    }

    fn resolve_all(&mut self) {
        for entry in &mut self.entries {
            entry.resolved = resolve_slots(&entry.config, &self.fixtures, &self.library);
        }
    }

    /// Run one frame: tick every enabled generator and merge their writes
    /// into the overlay. Exclusivity (one enabled at a time) is enforced
    /// at the toggle command, so in practice at most one entry contributes
    /// per tick — but if two were briefly enabled the later iteration
    /// would simply overwrite the earlier one's pan/tilt cells.
    pub fn tick(&mut self, now: Instant, overall_bpm: Option<f32>) -> HashMap<u16, ChannelOverlay> {
        let mut overlay: HashMap<u16, ChannelOverlay> = HashMap::new();
        // Same single-derivation pattern as ChaserEngine: one anchor
        // read per frame, shared across every enabled generator.
        let current_beat = self.beat_anchor.as_ref().map(|a| a.beat_at(now));
        for entry in &mut self.entries {
            if !entry.config.enabled || entry.resolved.is_empty() {
                continue;
            }
            advance_phase(
                &mut entry.runtime,
                &entry.config,
                now,
                overall_bpm,
                current_beat,
            );
            for r in &entry.resolved {
                let (x, y) = evaluate_slot(
                    entry.runtime.global_phase,
                    &r.slot,
                    &entry.config.shape,
                    entry.config.rotation_deg,
                    entry.config.size_x,
                    entry.config.size_y,
                    entry.config.center_x,
                    entry.config.center_y,
                );
                write_axis(
                    &mut overlay,
                    r.universe,
                    r.pan_coarse,
                    r.pan_fine,
                    x,
                    &r.pan_range,
                );
                write_axis(
                    &mut overlay,
                    r.universe,
                    r.tilt_coarse,
                    r.tilt_fine,
                    y,
                    &r.tilt_range,
                );
            }
        }
        overlay
    }
}

fn resolve_slots(
    cfg: &MovementGenerator,
    fixtures: &[FixtureInstance],
    library: &HashMap<String, FixtureDefinition>,
) -> Vec<ResolvedSlot> {
    let mut out = Vec::with_capacity(cfg.fixtures.len());
    for slot in &cfg.fixtures {
        if let Some(r) = resolve_one(slot, fixtures, library) {
            out.push(r);
        }
    }
    out
}

/// Update `runtime.global_phase` based on wall-clock elapsed since
/// `last_update`. Direction reverses or ping-pongs as configured.
///
/// Forward and Reverse just wrap the phase modulo 1. PingPong reflects at
/// the boundaries so the same shape traces out and back without ever
/// jumping — vital for a moving head where a phase wrap means a fast
/// snap across the rig.
fn advance_phase(
    runtime: &mut MovementRuntime,
    cfg: &MovementGenerator,
    now: Instant,
    overall_bpm: Option<f32>,
    current_beat: Option<f64>,
) {
    // Same override semantics as the chaser side — keep both effects
    // locked to one tempo when the operator is driving the rig from
    // the header's overall BPM.
    let _bpm = overall_bpm.unwrap_or(match cfg.tempo {
        TempoSource::Fixed { bpm } => bpm,
    });
    let beats_per_loop = beats_per_loop(cfg.subdivision);

    // Phase-sync path: when VDJ tells us "right now you are at beat
    // B", derive the cycle position deterministically. Forward and
    // Reverse get a clean modulo of the beats-per-loop; PingPong
    // unwraps into a triangle wave that the eval pipeline reads the
    // same way as the free-run version.
    if let Some(beat) = current_beat {
        runtime.last_update = Some(now);
        let beat_safe = beat.max(0.0);
        let cycles = beat_safe / beats_per_loop;
        match cfg.direction {
            Direction::Forward => {
                runtime.direction_sign = 1.0;
                runtime.global_phase = cycles.rem_euclid(1.0);
            }
            Direction::Reverse => {
                runtime.direction_sign = -1.0;
                runtime.global_phase = (-cycles).rem_euclid(1.0);
            }
            Direction::PingPong => {
                // Triangle wave: cycles mod 2 in [0..1) means going up,
                // in [1..2) means coming back. Reflect the second half
                // so global_phase tracks 0 → 1 → 0 → 1, never wrapping.
                let tri = cycles.rem_euclid(2.0);
                if tri < 1.0 {
                    runtime.global_phase = tri;
                    runtime.direction_sign = 1.0;
                } else {
                    runtime.global_phase = 2.0 - tri;
                    runtime.direction_sign = -1.0;
                }
            }
        }
        return;
    }

    // Free-run path (no anchor): existing behaviour, unchanged.
    let beats_per_sec = (_bpm.max(1.0) as f64) / 60.0;
    let cycles_per_sec = beats_per_sec / beats_per_loop;

    let delta = match runtime.last_update {
        Some(prev) => now.saturating_duration_since(prev).as_secs_f64(),
        None => 0.0,
    };
    runtime.last_update = Some(now);

    match cfg.direction {
        Direction::Forward => {
            runtime.direction_sign = 1.0;
            runtime.global_phase = (runtime.global_phase + delta * cycles_per_sec).rem_euclid(1.0);
        }
        Direction::Reverse => {
            runtime.direction_sign = -1.0;
            runtime.global_phase = (runtime.global_phase - delta * cycles_per_sec).rem_euclid(1.0);
        }
        Direction::PingPong => {
            // Bounce off the [0, 1] interval. We reflect once per frame —
            // for sane delta values that's plenty (it would take ~30 ms
            // per loop at 15 fps to need a second reflection).
            let mut next = runtime.global_phase + runtime.direction_sign * delta * cycles_per_sec;
            if next > 1.0 {
                next = 2.0 - next;
                runtime.direction_sign = -1.0;
            } else if next < 0.0 {
                next = -next;
                runtime.direction_sign = 1.0;
            }
            runtime.global_phase = next.clamp(0.0, 1.0);
        }
    }
}

/// Beats-per-loop interpretation of the chaser's Subdivision enum, used
/// here so the same UI control means "more subdivisions = faster" in both
/// modules.
fn beats_per_loop(s: Subdivision) -> f64 {
    match s {
        Subdivision::Quarter => 0.25,
        Subdivision::Half => 0.5,
        Subdivision::One => 1.0,
        Subdivision::Two => 2.0,
        Subdivision::Four => 4.0,
        Subdivision::Eight => 8.0,
    }
}

fn write_axis(
    overlay: &mut HashMap<u16, ChannelOverlay>,
    universe: u16,
    coarse: Option<usize>,
    fine: Option<usize>,
    normalised: f32,
    range: &PanTiltRange,
) {
    let (hi, lo) = map_to_dmx(normalised, range);
    if let Some(off) = coarse {
        set_overlay(overlay, universe, off, hi);
    }
    if let Some(off) = fine {
        set_overlay(overlay, universe, off, lo);
    }
}

fn set_overlay(
    overlay: &mut HashMap<u16, ChannelOverlay>,
    universe: u16,
    channel: usize,
    value: u8,
) {
    if channel >= DMX_CHANNELS {
        return;
    }
    overlay.entry(universe).or_insert_with(empty_overlay)[channel] = Some(value);
}

fn resolve_one(
    slot: &MovementSlot,
    fixtures: &[FixtureInstance],
    library: &HashMap<String, FixtureDefinition>,
) -> Option<ResolvedSlot> {
    let fixture = fixtures.iter().find(|f| f.id == slot.fixture_id)?;
    let def = library.get(&fixture.definition_id)?;
    let mode = def.modes.get(fixture.mode_index as usize)?;
    let base = (fixture.address as usize).saturating_sub(1);
    let pan =
        role_offset(mode.channels.iter().map(|c| &c.role), &ChannelRole::Pan).map(|i| base + i);
    let pan_fine =
        role_offset(mode.channels.iter().map(|c| &c.role), &ChannelRole::PanFine).map(|i| base + i);
    let tilt =
        role_offset(mode.channels.iter().map(|c| &c.role), &ChannelRole::Tilt).map(|i| base + i);
    let tilt_fine = role_offset(
        mode.channels.iter().map(|c| &c.role),
        &ChannelRole::TiltFine,
    )
    .map(|i| base + i);
    if pan.is_none() && tilt.is_none() {
        // Nothing for movement to drive — drop the slot.
        return None;
    }
    Some(ResolvedSlot {
        slot: slot.clone(),
        universe: fixture.universe,
        pan_coarse: pan,
        pan_fine,
        tilt_coarse: tilt,
        tilt_fine,
        pan_range: mode.pan_range_or_default(),
        tilt_range: mode.tilt_range_or_default(),
    })
}

fn role_offset<'a, I>(iter: I, role: &ChannelRole) -> Option<usize>
where
    I: Iterator<Item = &'a ChannelRole>,
{
    iter.enumerate().find_map(|(i, r)| (r == role).then_some(i))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::show::fixture::{ChannelDefinition, FixtureMode};
    use std::time::Duration;

    fn mover_def() -> FixtureDefinition {
        FixtureDefinition {
            id: "mover".into(),
            manufacturer: "T".into(),
            name: "Mover".into(),
            image: None,
            modes: vec![FixtureMode {
                name: "16bit".into(),
                pan_range: None,
                tilt_range: None,
                channels: vec![
                    ChannelDefinition::new(ChannelRole::Pan, 0),
                    ChannelDefinition::new(ChannelRole::PanFine, 0),
                    ChannelDefinition::new(ChannelRole::Tilt, 0),
                    ChannelDefinition::new(ChannelRole::TiltFine, 0),
                ],
            }],
        }
    }

    fn mover_instance(id: &str, address: u16) -> FixtureInstance {
        FixtureInstance {
            id: id.into(),
            definition_id: "mover".into(),
            mode_index: 0,
            universe: 0,
            address,
            label: None,
            position: [0.0, 0.0],
        }
    }

    fn slot(fixture_id: &str) -> MovementSlot {
        MovementSlot {
            fixture_id: fixture_id.into(),
            phase_offset: 0.0,
            invert_pan: false,
            invert_tilt: false,
        }
    }

    fn lib_with_mover() -> HashMap<String, FixtureDefinition> {
        let mut m = HashMap::new();
        m.insert("mover".into(), mover_def());
        m
    }

    #[test]
    fn disabled_generator_writes_nothing() {
        let mut e = MovementEngine::new();
        e.update_show_context(vec![mover_instance("m1", 1)], lib_with_mover());
        let mut g = MovementGenerator::default_disabled();
        g.fixtures = vec![slot("m1")];
        e.replace_generators(vec![g]);
        let ov = e.tick(Instant::now(), None);
        assert!(ov.is_empty(), "disabled generator should not write");
    }

    #[test]
    fn enabled_circle_writes_pan_and_tilt_at_start() {
        let mut e = MovementEngine::new();
        e.update_show_context(vec![mover_instance("m1", 1)], lib_with_mover());
        let mut g = MovementGenerator::default_disabled();
        g.enabled = true;
        g.fixtures = vec![slot("m1")];
        e.replace_generators(vec![g]);
        let ov = e.tick(Instant::now(), None);
        let u = ov.get(&0).expect("universe 0 written");
        // Phase 0: circle at (1, 0). Pan = +1 → DMX 0xFFFF. Tilt = 0 →
        // midpoint 0x8000 (high byte 0x80, low 0x00).
        assert_eq!(u[0], Some(0xFF), "pan coarse");
        assert_eq!(u[1], Some(0xFF), "pan fine");
        assert_eq!(u[2], Some(0x80), "tilt coarse");
        // Tilt fine: 32_768 mod 256 = 0, but the rounding can land at 32_768
        // exactly so accept either byte 0 or byte 1 to absorb fp drift.
        let tilt_fine = u[3].unwrap_or(0);
        assert!(
            tilt_fine == 0 || tilt_fine == 1,
            "tilt fine drift: got {tilt_fine}"
        );
    }

    #[test]
    fn phase_advances_with_wall_clock() {
        let mut e = MovementEngine::new();
        e.update_show_context(vec![mover_instance("m1", 1)], lib_with_mover());
        let mut g = MovementGenerator::default_disabled();
        g.enabled = true;
        g.fixtures = vec![slot("m1")];
        // 60 BPM × Subdivision::One = 1 beat per loop = 1 cycle per second.
        g.tempo = TempoSource::Fixed { bpm: 60.0 };
        g.subdivision = Subdivision::One;
        e.replace_generators(vec![g]);

        let t0 = Instant::now();
        e.tick(t0, None); // initialises last_update; phase still 0
        e.tick(t0 + Duration::from_millis(250), None);
        let phase = e
            .entries
            .first()
            .map(|en| en.runtime.global_phase)
            .unwrap_or(0.0);
        // 250 ms at 1 cps = 0.25 phase.
        assert!((phase - 0.25).abs() < 0.01, "got {phase}");
    }

    #[test]
    fn missing_fixture_drops_slot_silently() {
        let mut e = MovementEngine::new();
        e.update_show_context(vec![], lib_with_mover());
        let mut g = MovementGenerator::default_disabled();
        g.enabled = true;
        g.fixtures = vec![slot("nope")];
        e.replace_generators(vec![g]);
        let ov = e.tick(Instant::now(), None);
        assert!(ov.is_empty());
    }

    #[test]
    fn fixture_without_pan_tilt_drops_slot() {
        // RGB-only fixture; movement has nothing to drive.
        let mut def = mover_def();
        def.modes[0].channels = vec![ChannelDefinition::new(ChannelRole::Red, 0)];
        let mut lib = HashMap::new();
        lib.insert("mover".into(), def);

        let mut e = MovementEngine::new();
        e.update_show_context(vec![mover_instance("m1", 1)], lib);
        let mut g = MovementGenerator::default_disabled();
        g.enabled = true;
        g.fixtures = vec![slot("m1")];
        e.replace_generators(vec![g]);
        let ov = e.tick(Instant::now(), None);
        assert!(ov.is_empty());
    }

    #[test]
    fn beat_anchor_snaps_global_phase() {
        // Anchor at (now, beat=2.0, bpm=120) with Subdivision::One
        // (1 beat per loop) → cycles = 2.0 → phase = 0.0 (mod 1).
        // Same setup with beat=2.5 → phase = 0.5.
        let mut e = MovementEngine::new();
        e.update_show_context(vec![mover_instance("m1", 1)], lib_with_mover());
        let mut g = MovementGenerator::default_disabled();
        g.enabled = true;
        g.fixtures = vec![slot("m1")];
        g.tempo = TempoSource::Fixed { bpm: 120.0 };
        g.subdivision = Subdivision::One;
        e.replace_generators(vec![g]);

        let t0 = Instant::now();
        e.set_beat_anchor(Some(BeatAnchor {
            set_at: t0,
            beat_at_set: 2.0,
            bpm: 120.0,
        }));
        e.tick(t0, None);
        let p0 = e.entries[0].runtime.global_phase;
        assert!((p0 - 0.0).abs() < 1e-6, "beat 2.0 → phase 0.0, got {p0}");

        e.set_beat_anchor(Some(BeatAnchor {
            set_at: t0,
            beat_at_set: 2.5,
            bpm: 120.0,
        }));
        e.tick(t0, None);
        let p1 = e.entries[0].runtime.global_phase;
        assert!((p1 - 0.5).abs() < 1e-6, "beat 2.5 → phase 0.5, got {p1}");
    }

    #[test]
    fn beat_anchor_pingpong_triangle_wave() {
        // PingPong: cycles % 2 in [0,1) → going up, in [1,2) → going down.
        let mut e = MovementEngine::new();
        e.update_show_context(vec![mover_instance("m1", 1)], lib_with_mover());
        let mut g = MovementGenerator::default_disabled();
        g.enabled = true;
        g.fixtures = vec![slot("m1")];
        g.tempo = TempoSource::Fixed { bpm: 120.0 };
        g.subdivision = Subdivision::One;
        g.direction = Direction::PingPong;
        e.replace_generators(vec![g]);

        let t0 = Instant::now();
        // beat 0.5 (in cycle 0, half-way up) → phase 0.5, going forward
        e.set_beat_anchor(Some(BeatAnchor {
            set_at: t0,
            beat_at_set: 0.5,
            bpm: 120.0,
        }));
        e.tick(t0, None);
        let p_up = e.entries[0].runtime.global_phase;
        let dir_up = e.entries[0].runtime.direction_sign;
        assert!((p_up - 0.5).abs() < 1e-6, "got {p_up}");
        assert_eq!(dir_up, 1.0);

        // beat 1.5 (in cycle 1, half-way back) → phase 0.5, going reverse
        e.set_beat_anchor(Some(BeatAnchor {
            set_at: t0,
            beat_at_set: 1.5,
            bpm: 120.0,
        }));
        e.tick(t0, None);
        let p_down = e.entries[0].runtime.global_phase;
        let dir_down = e.entries[0].runtime.direction_sign;
        assert!((p_down - 0.5).abs() < 1e-6, "got {p_down}");
        assert_eq!(dir_down, -1.0);
    }

    #[test]
    fn beat_anchor_cleared_falls_back_to_free_run() {
        let mut e = MovementEngine::new();
        e.update_show_context(vec![mover_instance("m1", 1)], lib_with_mover());
        let mut g = MovementGenerator::default_disabled();
        g.enabled = true;
        g.fixtures = vec![slot("m1")];
        g.tempo = TempoSource::Fixed { bpm: 60.0 };
        g.subdivision = Subdivision::One;
        e.replace_generators(vec![g]);

        let t0 = Instant::now();
        e.set_beat_anchor(Some(BeatAnchor {
            set_at: t0,
            beat_at_set: 0.5,
            bpm: 60.0,
        }));
        e.tick(t0, None); // snap to 0.5
        e.set_beat_anchor(None);
        e.tick(t0, None); // free-run init last_update
                          // 250 ms at 60 BPM × Subdivision::One = 1 cps → +0.25 phase
        e.tick(t0 + Duration::from_millis(250), None);
        let p = e.entries[0].runtime.global_phase;
        // Free-run advanced 0.25 from previous 0.5 → 0.75
        assert!((p - 0.75).abs() < 0.01, "got {p}");
    }
}
