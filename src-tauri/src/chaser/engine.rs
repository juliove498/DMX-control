//! Owns the live set of chasers and their runtimes. Built so it can be
//! swapped in/out atomically from the UI thread (config edits) while the
//! output thread ticks it every frame.
//!
//! Sub-fase A scope: AllTogether pattern, Single colour, Fixed BPM, no fade.
//! Other patterns/colour modes are stubbed in `pattern.rs` so the data model
//! survives across sub-phases without migrations; their actual behaviour
//! lands in B/C/D.

use std::collections::HashMap;
use std::time::Instant;

use crate::engine::{empty_overlay, ChannelOverlay, DMX_CHANNELS};
use crate::show::fixture::{ChannelRole, FixtureDefinition, FixtureInstance};

use super::color::color_for_slot;
use super::pattern::evaluate;
use super::runtime::{ChaserRuntime, SlotOutput};
use super::{
    apply_fade_curve, step_duration_ms, AmbientChaser, ColorMode, Rgb, SlotState, TempoSource,
};

/// Pre-resolved channel offsets for a single slot. Computed once whenever
/// the show's fixtures or the chaser's slot list changes, so the per-frame
/// hot path is just array indexing.
#[derive(Debug, Clone)]
struct ResolvedSlot {
    universe: u16,
    intensity_offset: Option<usize>,
    rgb_offset: Option<(usize, usize, usize)>,
    use_intensity: bool,
    use_color: bool,
}

#[derive(Debug)]
struct ChaserEntry {
    config: AmbientChaser,
    runtime: ChaserRuntime,
    resolved: Vec<ResolvedSlot>,
}

#[derive(Debug, Default)]
pub struct ChaserEngine {
    entries: Vec<ChaserEntry>,
    /// Snapshot of fixtures + library used to resolve slot offsets. Kept so
    /// `update_chasers` can re-resolve without callers having to ferry these
    /// in every time.
    fixtures: Vec<FixtureInstance>,
    library: HashMap<String, FixtureDefinition>,
}

impl ChaserEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the show context used to resolve slots. Triggers a re-resolve
    /// of every chaser.
    pub fn update_show_context(
        &mut self,
        fixtures: Vec<FixtureInstance>,
        library: HashMap<String, FixtureDefinition>,
    ) {
        self.fixtures = fixtures;
        self.library = library;
        self.resolve_all();
    }

    /// Replace the entire set of chasers. Runtimes are preserved by `id` so
    /// editing one chaser doesn't reset the timing of the others, and a
    /// chaser that changes BPM/pattern keeps its step counter (the next
    /// `advance_step` will compute correctly under the new tempo).
    pub fn replace_chasers(&mut self, chasers: Vec<AmbientChaser>) {
        let mut existing: HashMap<String, ChaserRuntime> = self
            .entries
            .drain(..)
            .map(|e| (e.config.id, e.runtime))
            .collect();
        self.entries = chasers
            .into_iter()
            .map(|config| ChaserEntry {
                resolved: resolve_slots(&config, &self.fixtures, &self.library),
                runtime: existing.remove(&config.id).unwrap_or_default(),
                config,
            })
            .collect();
    }

    pub fn list(&self) -> Vec<AmbientChaser> {
        self.entries.iter().map(|e| e.config.clone()).collect()
    }

    /// Snapshot of the per-slot output (RGB + intensity) from the most
    /// recent tick of whichever chaser is currently enabled. Used by
    /// surfaces that mirror the chase visually, e.g. the Launchpad top
    /// row showing each "tacho" in its real colour and brightness.
    /// Returns `None` if no chaser is enabled.
    pub fn active_slot_outputs(&self) -> Option<Vec<SlotOutput>> {
        self.entries
            .iter()
            .find(|e| e.config.enabled)
            .map(|e| e.runtime.last_emitted.clone())
    }

    fn resolve_all(&mut self) {
        for e in &mut self.entries {
            e.resolved = resolve_slots(&e.config, &self.fixtures, &self.library);
        }
    }

    /// Run one frame: advance step counters as needed, evaluate the pattern
    /// for every enabled chaser, and produce the merged overlay map keyed by
    /// universe.
    pub fn tick(&mut self, now: Instant) -> HashMap<u16, ChannelOverlay> {
        let mut overlay: HashMap<u16, ChannelOverlay> = HashMap::new();
        for entry in &mut self.entries {
            if !entry.config.enabled {
                continue;
            }
            advance_step(&mut entry.runtime, &entry.config, now);
            apply_chaser(entry, &mut overlay, now);
        }
        overlay
    }
}

/// Advance `runtime.current_step` to match wall-clock `now`. `last_step_at`
/// holds the wall time at which we entered the current step. We jump as
/// many steps forward as the elapsed time allows, so a stutter or pause
/// doesn't leave the chaser drifting. When the step changes we snapshot
/// `last_emitted` into `fade_from` so the fade-in interpolates from the
/// actual previous output rather than from a recomputed ideal.
fn advance_step(runtime: &mut ChaserRuntime, config: &AmbientChaser, now: Instant) {
    let bpm = match config.tempo {
        TempoSource::Fixed { bpm } => bpm,
    };
    let step_ms = step_duration_ms(bpm, config.subdivision).max(1.0);
    let step_dur = std::time::Duration::from_secs_f32(step_ms / 1000.0);
    match runtime.last_step_at {
        None => {
            runtime.last_step_at = Some(now);
            runtime.step_started_at = Some(now);
        }
        Some(last) => {
            let elapsed = now.saturating_duration_since(last);
            if elapsed >= step_dur {
                let steps = (elapsed.as_secs_f64() / step_dur.as_secs_f64()).floor() as u64;
                runtime.current_step = runtime.current_step.wrapping_add(steps);
                runtime.last_step_at = Some(last + step_dur * steps as u32);
                // Step transition: capture what we output last frame as the
                // fade source, and reset the fade timer.
                runtime.fade_from = runtime.last_emitted.clone();
                runtime.step_started_at = Some(now);
            }
        }
    }
}

fn apply_chaser(
    entry: &mut ChaserEntry,
    overlay: &mut HashMap<u16, ChannelOverlay>,
    now: Instant,
) {
    let total = entry.resolved.len();
    if total == 0 {
        return;
    }
    let master_byte = ((entry.config.master.clamp(0.0, 1.0)) * 255.0).round() as u8;
    let color_active = !matches!(entry.config.color_mode, ColorMode::Disabled);

    // Step 1: compute raw targets per slot (where the slot WANTS to be).
    let raw_targets: Vec<SlotOutput> = entry
        .resolved
        .iter()
        .enumerate()
        .map(|(slot_index, slot)| {
            let state = evaluate(
                &entry.config.pattern,
                entry.runtime.current_step,
                slot_index,
                total,
            );
            let on = matches!(state, SlotState::On);
            let bg = entry.config.background;
            let intensity = if slot.use_intensity {
                Some(if on { master_byte } else { bg })
            } else {
                None
            };
            // Colour at "off" needs to honour `background` too, otherwise
            // the only way background affects RGB-only fixtures is "no
            // change" — the chaser just goes black between flashes. So:
            //
            //   - on                              → resolved chase colour
            //   - off + background = 0            → black (clean blink)
            //   - off + background > 0:
            //       - has intensity channel       → keep colour at full
            //         chromaticity (the intensity channel is already
            //         dimmed to `background`)
            //       - RGB-only                    → scale colour by
            //         background/255 so you see a dim version of the
            //         chase colour as the ambient floor.
            let rgb = if slot.use_color && color_active && slot.rgb_offset.is_some() {
                let chase = color_for_slot(
                    &entry.config.color_mode,
                    entry.runtime.current_step,
                    slot_index,
                    total,
                )
                .unwrap_or(Rgb::WHITE);
                let c = if on {
                    chase
                } else if bg == 0 {
                    Rgb::BLACK
                } else if slot.use_intensity && slot.intensity_offset.is_some() {
                    chase
                } else {
                    let factor = bg as f32 / 255.0;
                    Rgb {
                        r: ((chase.r as f32) * factor).round() as u8,
                        g: ((chase.g as f32) * factor).round() as u8,
                        b: ((chase.b as f32) * factor).round() as u8,
                    }
                };
                Some(c)
            } else {
                None
            };
            SlotOutput { intensity, rgb }
        })
        .collect();

    // Step 2: blend with the fade source if the user has fade enabled and
    // we have a recent transition to fade from. `fade_from` is empty when
    // the chaser just (re)started — in that case we just emit the raw.
    let emitted: Vec<SlotOutput> = if entry.config.fade.enabled
        && !entry.runtime.fade_from.is_empty()
        && entry.runtime.fade_from.len() == raw_targets.len()
    {
        let bpm = match entry.config.tempo {
            TempoSource::Fixed { bpm } => bpm,
        };
        let step_ms = step_duration_ms(bpm, entry.config.subdivision).max(1.0);
        // Cap fade at 90% of the step so each step always settles before
        // the next transition, no matter how aggressive the user gets.
        let fade_ms = step_ms * entry.config.fade.amount.clamp(0.0, 0.9);
        let progress = if let Some(started) = entry.runtime.step_started_at {
            if fade_ms <= 0.0 {
                1.0
            } else {
                (now.saturating_duration_since(started).as_secs_f32() * 1000.0 / fade_ms)
                    .clamp(0.0, 1.0)
            }
        } else {
            1.0
        };
        let eased = apply_fade_curve(entry.config.fade.curve, progress);
        raw_targets
            .iter()
            .zip(entry.runtime.fade_from.iter())
            .map(|(target, from)| lerp_output(from, target, eased))
            .collect()
    } else {
        raw_targets.clone()
    };

    // Step 3: write the emitted output to the overlay.
    for (slot, out) in entry.resolved.iter().zip(emitted.iter()) {
        if let (Some(off), Some(value)) = (slot.intensity_offset, out.intensity) {
            set_overlay(overlay, slot.universe, off, value);
        }
        if let (Some((r_off, g_off, b_off)), Some(c)) = (slot.rgb_offset, out.rgb) {
            set_overlay(overlay, slot.universe, r_off, c.r);
            set_overlay(overlay, slot.universe, g_off, c.g);
            set_overlay(overlay, slot.universe, b_off, c.b);
        }
    }

    // Step 4: cache the actual output so next step's fade has a real source.
    entry.runtime.last_emitted = emitted;
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 + (b as f32 - a as f32) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn lerp_rgb(a: Rgb, b: Rgb, t: f32) -> Rgb {
    Rgb {
        r: lerp_u8(a.r, b.r, t),
        g: lerp_u8(a.g, b.g, t),
        b: lerp_u8(a.b, b.b, t),
    }
}

fn lerp_output(from: &SlotOutput, to: &SlotOutput, t: f32) -> SlotOutput {
    SlotOutput {
        intensity: match (from.intensity, to.intensity) {
            (Some(a), Some(b)) => Some(lerp_u8(a, b, t)),
            (None, b) => b,
            // Disable mid-fade: keep the previous value until the fade is
            // mostly done, then snap to None so the chaser stops writing.
            (a, None) => {
                if t > 0.5 {
                    None
                } else {
                    a
                }
            }
        },
        rgb: match (from.rgb, to.rgb) {
            (Some(a), Some(b)) => Some(lerp_rgb(a, b, t)),
            (None, b) => b,
            (a, None) => {
                if t > 0.5 {
                    None
                } else {
                    a
                }
            }
        },
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
    overlay
        .entry(universe)
        .or_insert_with(empty_overlay)[channel] = Some(value);
}

fn resolve_slots(
    chaser: &AmbientChaser,
    fixtures: &[FixtureInstance],
    library: &HashMap<String, FixtureDefinition>,
) -> Vec<ResolvedSlot> {
    let mut out = Vec::with_capacity(chaser.slots.len());
    for slot in &chaser.slots {
        let Some(fixture) = fixtures.iter().find(|f| f.id == slot.fixture_id) else {
            continue;
        };
        let Some(def) = library.get(&fixture.definition_id) else {
            continue;
        };
        let Some(mode) = def.modes.get(fixture.mode_index as usize) else {
            continue;
        };
        let base = (fixture.address as usize).saturating_sub(1);
        let intensity_offset = role_offset(mode.channels.iter().map(|c| &c.role), &ChannelRole::Intensity)
            .map(|i| base + i);
        let r = role_offset(mode.channels.iter().map(|c| &c.role), &ChannelRole::Red);
        let g = role_offset(mode.channels.iter().map(|c| &c.role), &ChannelRole::Green);
        let b = role_offset(mode.channels.iter().map(|c| &c.role), &ChannelRole::Blue);
        let rgb_offset = match (r, g, b) {
            (Some(r), Some(g), Some(b)) => Some((base + r, base + g, base + b)),
            _ => None,
        };
        out.push(ResolvedSlot {
            universe: fixture.universe,
            intensity_offset,
            rgb_offset,
            use_intensity: slot.use_intensity,
            use_color: slot.use_color,
        });
    }
    out
}

fn role_offset<'a, I>(iter: I, role: &ChannelRole) -> Option<usize>
where
    I: Iterator<Item = &'a ChannelRole>,
{
    iter.enumerate()
        .find_map(|(i, r)| if r == role { Some(i) } else { None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::show::fixture::{ChannelDefinition, FixtureMode};
    use std::time::Duration;

    fn rgb_def(id: &str) -> FixtureDefinition {
        FixtureDefinition {
            id: id.into(),
            manufacturer: "Test".into(),
            name: "RGB".into(),
            image: None,
            modes: vec![FixtureMode {
                name: "3ch".into(),
                pan_range: None,
                tilt_range: None,
                channels: vec![
                    ChannelDefinition {
                        role: ChannelRole::Red,
                        default: 0,
                    },
                    ChannelDefinition {
                        role: ChannelRole::Green,
                        default: 0,
                    },
                    ChannelDefinition {
                        role: ChannelRole::Blue,
                        default: 0,
                    },
                ],
            }],
        }
    }

    fn dimmer_def(id: &str) -> FixtureDefinition {
        FixtureDefinition {
            id: id.into(),
            manufacturer: "Test".into(),
            name: "Dimmer".into(),
            image: None,
            modes: vec![FixtureMode {
                name: "1ch".into(),
                pan_range: None,
                tilt_range: None,
                channels: vec![ChannelDefinition {
                    role: ChannelRole::Intensity,
                    default: 0,
                }],
            }],
        }
    }

    fn fixture(id: &str, def_id: &str, address: u16) -> FixtureInstance {
        FixtureInstance {
            id: id.into(),
            definition_id: def_id.into(),
            mode_index: 0,
            universe: 0,
            address,
            label: None,
            position: [0.0, 0.0],
        }
    }

    fn chaser(id: &str, slots: Vec<&str>) -> AmbientChaser {
        let mut c = AmbientChaser::default_with_id(id.into());
        c.enabled = true;
        c.tempo = TempoSource::Fixed { bpm: 120.0 };
        c.subdivision = super::super::Subdivision::One;
        c.slots = slots
            .into_iter()
            .map(|fid| super::super::ChaserSlot {
                fixture_id: fid.into(),
                use_intensity: true,
                use_color: false,
            })
            .collect();
        c
    }

    #[test]
    fn dimmer_chaser_pulses_at_120_bpm() {
        let mut lib = HashMap::new();
        lib.insert("dimmer".into(), dimmer_def("dimmer"));
        let fixtures = vec![fixture("d1", "dimmer", 1)];

        let mut engine = ChaserEngine::new();
        engine.update_show_context(fixtures, lib);
        engine.replace_chasers(vec![chaser("c", vec!["d1"])]);

        let t0 = Instant::now();
        // Step 0 → On (255)
        let ov0 = engine.tick(t0);
        let u0 = ov0.get(&0).unwrap();
        assert_eq!(u0[0], Some(255));

        // 120 BPM, subdivision One = 500 ms per step. Tick a tick later than
        // half a second → step 1 → Off.
        let ov1 = engine.tick(t0 + Duration::from_millis(510));
        let u1 = ov1.get(&0).unwrap();
        assert_eq!(u1[0], Some(0));

        // Another half-second → step 2 → On.
        let ov2 = engine.tick(t0 + Duration::from_millis(1010));
        assert_eq!(ov2.get(&0).unwrap()[0], Some(255));
    }

    #[test]
    fn disabled_chaser_writes_nothing() {
        let mut lib = HashMap::new();
        lib.insert("dimmer".into(), dimmer_def("dimmer"));
        let fixtures = vec![fixture("d1", "dimmer", 1)];

        let mut engine = ChaserEngine::new();
        engine.update_show_context(fixtures, lib);
        let mut c = chaser("c", vec!["d1"]);
        c.enabled = false;
        engine.replace_chasers(vec![c]);

        let ov = engine.tick(Instant::now());
        assert!(ov.is_empty());
    }

    #[test]
    fn background_dims_rgb_only_fixtures_when_off() {
        // RGB par with no intensity channel + Single red. Expectation:
        // on  → (255, 0, 0)
        // off, background=0   → (0, 0, 0)
        // off, background=64  → (64, 0, 0) — dim red ambient
        let mut lib = HashMap::new();
        lib.insert("rgb".into(), rgb_def("rgb"));
        let fixtures = vec![fixture("p1", "rgb", 1)];

        let mut engine = ChaserEngine::new();
        engine.update_show_context(fixtures, lib);
        let mut c = chaser("c", vec!["p1"]);
        c.color_mode = ColorMode::Single {
            color: Rgb { r: 255, g: 0, b: 0 },
        };
        c.slots[0].use_color = true;
        c.slots[0].use_intensity = false;
        // total=1, AllTogether: step 0 = on, step 1 = off.
        c.background = 64;
        engine.replace_chasers(vec![c]);

        let t0 = Instant::now();
        let on = engine.tick(t0);
        // Step 0 → on → full red.
        assert_eq!(on.get(&0).unwrap()[0], Some(255));
        // Step 1 → off, but background 64 → dim red.
        let off = engine.tick(t0 + Duration::from_millis(510));
        let u = off.get(&0).unwrap();
        assert!(
            u[0] == Some(64) || u[0] == Some(63),
            "expected dim red ~64, got {:?}",
            u[0]
        );
        assert_eq!(u[1], Some(0));
        assert_eq!(u[2], Some(0));
    }

    #[test]
    fn rgb_single_color_writes_color_when_on() {
        let mut lib = HashMap::new();
        lib.insert("rgb".into(), rgb_def("rgb"));
        let fixtures = vec![fixture("p1", "rgb", 1)];

        let mut engine = ChaserEngine::new();
        engine.update_show_context(fixtures, lib);
        let mut c = chaser("c", vec!["p1"]);
        c.color_mode = ColorMode::Single {
            color: Rgb {
                r: 255,
                g: 0,
                b: 128,
            },
        };
        c.slots[0].use_color = true;
        c.slots[0].use_intensity = false;
        engine.replace_chasers(vec![c]);

        let ov = engine.tick(Instant::now());
        let u = ov.get(&0).unwrap();
        assert_eq!(u[0], Some(255));
        assert_eq!(u[1], Some(0));
        assert_eq!(u[2], Some(128));
    }

    #[test]
    fn missing_fixture_skipped_silently() {
        let lib = HashMap::new();
        let fixtures: Vec<FixtureInstance> = vec![];
        let mut engine = ChaserEngine::new();
        engine.update_show_context(fixtures, lib);
        engine.replace_chasers(vec![chaser("c", vec!["nonexistent"])]);
        let ov = engine.tick(Instant::now());
        assert!(ov.is_empty());
    }

    #[test]
    fn fade_interpolates_intensity_across_step() {
        // Linear fade at 50% of step. At 120 BPM with subdivision One the
        // step is 500 ms, so the fade lasts 250 ms. We emit step 0 (on,
        // 255), then step 1 (off, 0). Halfway through the fade the
        // intensity should be ~127.
        let mut lib = HashMap::new();
        lib.insert("dimmer".into(), dimmer_def("dimmer"));
        let fixtures = vec![fixture("d1", "dimmer", 1)];

        let mut engine = ChaserEngine::new();
        engine.update_show_context(fixtures, lib);
        let mut c = chaser("c", vec!["d1"]);
        c.fade = super::super::FadeConfig {
            enabled: true,
            amount: 0.5,
            curve: super::super::FadeCurve::Linear,
        };
        engine.replace_chasers(vec![c]);

        let t0 = Instant::now();
        engine.tick(t0); // step 0 → 255 cached as last_emitted
        // Cross into step 1 at t = 510 ms. fade_from snapshots 255.
        let ov_at_transition = engine.tick(t0 + Duration::from_millis(510));
        // At t=510 we are 0 ms into the fade → still ~255 (linear curve, t=0).
        assert!(
            ov_at_transition.get(&0).unwrap()[0].unwrap() > 200,
            "got {:?}",
            ov_at_transition.get(&0).unwrap()[0]
        );
        // Halfway through the 250 ms fade → ~127.
        let ov_mid = engine.tick(t0 + Duration::from_millis(510 + 125));
        let v = ov_mid.get(&0).unwrap()[0].unwrap();
        assert!(
            (90..=160).contains(&(v as i32)),
            "expected ~127, got {v}"
        );
        // Past the fade end → 0.
        let ov_done = engine.tick(t0 + Duration::from_millis(510 + 260));
        assert_eq!(ov_done.get(&0).unwrap()[0], Some(0));
    }

    #[test]
    fn fade_disabled_snaps() {
        let mut lib = HashMap::new();
        lib.insert("dimmer".into(), dimmer_def("dimmer"));
        let fixtures = vec![fixture("d1", "dimmer", 1)];

        let mut engine = ChaserEngine::new();
        engine.update_show_context(fixtures, lib);
        // Default chaser has fade.enabled = false → snap.
        engine.replace_chasers(vec![chaser("c", vec!["d1"])]);

        let t0 = Instant::now();
        engine.tick(t0);
        // 1 ms into step 1 should already be the new value (Off = 0).
        let ov = engine.tick(t0 + Duration::from_millis(501));
        assert_eq!(ov.get(&0).unwrap()[0], Some(0));
    }

    #[test]
    fn step_advances_correctly_after_long_pause() {
        // After a hiccup of 5 seconds at 120 BPM the chaser should still be
        // on the right boundary, not stuck on step 1.
        let mut lib = HashMap::new();
        lib.insert("dimmer".into(), dimmer_def("dimmer"));
        let fixtures = vec![fixture("d1", "dimmer", 1)];

        let mut engine = ChaserEngine::new();
        engine.update_show_context(fixtures, lib);
        engine.replace_chasers(vec![chaser("c", vec!["d1"])]);

        let t0 = Instant::now();
        engine.tick(t0); // step 0
                        // Jump 5 seconds (= 10 half-second steps) → step 10 → On.
        let ov = engine.tick(t0 + Duration::from_millis(5_001));
        assert_eq!(ov.get(&0).unwrap()[0], Some(255));
    }
}
