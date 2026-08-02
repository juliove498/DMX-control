//! Owns the live set of chasers and their runtimes. Built so it can be
//! swapped in/out atomically from the UI thread (config edits) while the
//! output thread ticks it every frame.
//!
//! Sub-fase A scope: AllTogether pattern, Single colour, Fixed BPM, no fade.
//! Other patterns/colour modes are stubbed in `pattern.rs` so the data model
//! survives across sub-phases without migrations; their actual behaviour
//! lands in B/C/D.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::engine::beatgrid::BeatAnchor;
use crate::engine::{empty_overlay, ChannelOverlay, DMX_CHANNELS};
use crate::globals::TempoPattern;
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
    /// External phase anchor (currently set by the VDJ poller). When
    /// `Some`, `advance_step` derives the current step deterministically
    /// from VDJ's beat grid instead of free-running on wall-clock. When
    /// `None`, behaviour is identical to pre-sync — used by every show
    /// that isn't talking to a DAW.
    beat_anchor: Option<BeatAnchor>,
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

    /// Install or clear the external phase anchor. `Some(anchor)` makes
    /// the next `advance_step` snap to VDJ's beat grid; `None` falls
    /// back to free-running on the chaser's own tempo. Called from
    /// the VDJ poller and from `stop_poller`.
    pub fn set_beat_anchor(&mut self, anchor: Option<BeatAnchor>) {
        self.beat_anchor = anchor;
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
    /// universe. `overall_bpm`, when `Some`, replaces every chaser's
    /// `tempo` for the duration of this tick — see `advance_step`.
    ///
    /// `tempo_pattern`, when `Some` together with an `overall_bpm`, turns
    /// every chaser into a pattern-follower: instead of advancing at uniform
    /// subdivisions, each chaser advances exactly one step per pattern hit.
    /// This is what lets a recorded clave / cha-cha drive the rig
    /// rhythmically. If `overall_bpm` is `None` the pattern is ignored —
    /// the operator must arm the global tempo for the pattern to play.
    pub fn tick(
        &mut self,
        now: Instant,
        overall_bpm: Option<f32>,
        tempo_pattern: Option<(&TempoPattern, Instant)>,
    ) -> HashMap<u16, ChannelOverlay> {
        let mut overlay: HashMap<u16, ChannelOverlay> = HashMap::new();
        // Derive the current beat-grid position once per frame. All
        // enabled chasers in this tick share the same view of the
        // beat, which keeps them locked to each other even if the
        // anchor refreshes between calls.
        let current_beat = self.beat_anchor.as_ref().map(|a| a.beat_at(now));
        // Pattern only kicks in when the operator has armed the overall
        // BPM — the pattern needs a tempo to ride. Without it, fall
        // through to plain subdivision-driven behaviour so a stale
        // pattern doesn't silently override a manually-set chaser tempo.
        let pattern_active = match (overall_bpm, tempo_pattern) {
            (Some(bpm), Some((p, anchor))) if !p.hits.is_empty() => Some((bpm, p, anchor)),
            _ => None,
        };
        for entry in &mut self.entries {
            if !entry.config.enabled {
                continue;
            }
            advance_step(
                &mut entry.runtime,
                &entry.config,
                now,
                overall_bpm,
                current_beat,
                pattern_active,
            );
            apply_chaser(entry, &mut overlay, now, pattern_active);
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
fn advance_step(
    runtime: &mut ChaserRuntime,
    config: &AmbientChaser,
    now: Instant,
    overall_bpm: Option<f32>,
    current_beat: Option<f64>,
    pattern: Option<(f32, &TempoPattern, Instant)>,
) {
    // Overall BPM, when active, wins over the chaser's own configured
    // tempo. The *configuration* isn't mutated — disabling the override
    // restores the chaser's previous tempo with no setup needed.
    let bpm = overall_bpm.unwrap_or(match config.tempo {
        TempoSource::Fixed { bpm } => bpm,
    });

    // Pattern path: advance one step per pattern hit, ignoring the
    // chaser's own subdivision. The pattern is global to every chaser
    // that runs under the override, so a clave on the rig keeps every
    // dimmer / blinder / par in lockstep. We compute the absolute step
    // count from "beats since anchor", which is monotonic and resilient
    // to frame stutters (a late frame just jumps several steps at once,
    // same as the free-run path).
    if let Some((pbpm, p, anchor)) = pattern {
        let elapsed_secs = now.saturating_duration_since(anchor).as_secs_f64();
        let beats = elapsed_secs * (pbpm.max(1.0) as f64) / 60.0;
        let (abs_step, step_started_beat, step_dur_beats) = pattern_step_at(p, beats);
        let step_started_secs = (step_started_beat * 60.0) / (pbpm.max(1.0) as f64);
        let step_started_at = anchor
            .checked_add(Duration::from_secs_f64(step_started_secs.max(0.0)))
            .unwrap_or(now);
        if runtime.current_step != abs_step {
            runtime.fade_from = runtime.last_emitted.clone();
            runtime.current_step = abs_step;
        }
        runtime.last_step_at = Some(step_started_at);
        runtime.step_started_at = Some(step_started_at);
        // Cache the upcoming step duration on the runtime so the fade
        // code reads it cheaply. Stored as ms in `pattern_step_ms` (see
        // `ChaserRuntime`).
        let step_ms = (step_dur_beats * 60_000.0 / (pbpm.max(1.0) as f64)) as f32;
        runtime.pattern_step_ms = Some(step_ms.max(1.0));
        return;
    } else {
        runtime.pattern_step_ms = None;
    }

    // Phase-sync path: when an external clock (VDJ via beat anchor)
    // tells us "right now you are at beat B", derive the step
    // deterministically. This is the whole point of sync — no drift
    // across long songs, downbeats line up with the music.
    if let Some(beat) = current_beat {
        let beats_per_step = config.subdivision.beats() as f64;
        if beats_per_step > 0.0 {
            // Negative beat (rare, can happen if anchor was set during
            // a track-position seek) — clamp to 0 so we don't tip into
            // exotic u64 wrap territory.
            let beat_clamped = beat.max(0.0);
            let abs_step = (beat_clamped / beats_per_step).floor() as u64;
            // step_started_at = wall clock at which the current step
            // began. Derived from "how far into the current step we
            // are, in seconds" — needed by the fade logic which uses
            // `now - step_started_at` to interpolate.
            let beats_into_step = beat_clamped - (abs_step as f64) * beats_per_step;
            let secs_into_step = beats_into_step * 60.0 / bpm.max(1.0) as f64;
            let step_started_at = now
                .checked_sub(Duration::from_secs_f64(secs_into_step.max(0.0)))
                .unwrap_or(now);

            if runtime.current_step != abs_step {
                // Real step transition: snapshot the last emitted
                // output so the fade interpolates from a real value,
                // not from a recomputed ideal.
                runtime.fade_from = runtime.last_emitted.clone();
                runtime.current_step = abs_step;
            }
            // last_step_at + step_started_at converge on the same wall
            // clock under sync mode (both = the moment we just entered
            // this step). We re-derive them each frame so an anchor
            // refresh that shifts the perceived beat snaps the chaser
            // cleanly.
            runtime.last_step_at = Some(step_started_at);
            runtime.step_started_at = Some(step_started_at);
            return;
        }
    }

    // Free-run path (no anchor): existing behaviour, unchanged.
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

/// Resolve where the playback head sits in a tempo pattern given
/// `beats` since the pattern anchor. Returns:
/// - `abs_step`: monotonically increasing 0-based step counter
/// - `step_started_beat`: beat of the most-recent pattern hit (used
///   to derive `step_started_at` for the fade engine)
/// - `step_dur_beats`: distance in beats to the next hit (used for
///   fade duration). Never zero.
///
/// `pattern.hits` is sorted ascending with `hits[0] == 0` (the
/// quantiser guarantees this), so the first hit of each cycle always
/// lands on the cycle's downbeat. Negative `beats` clamps to zero —
/// can happen briefly if the anchor was set in the future by a clock
/// hiccup.
fn pattern_step_at(pattern: &TempoPattern, beats: f64) -> (u64, f64, f64) {
    let len = pattern.hits.len() as u64;
    let steps_per_quarter = pattern.steps_per_bar as f64 / 4.0;
    let beats_per_cycle = pattern.bars as f64 * 4.0;
    let beats_clamped = beats.max(0.0);
    let cycle_idx = (beats_clamped / beats_per_cycle).floor() as u64;
    let cycle_beat = beats_clamped - (cycle_idx as f64) * beats_per_cycle;
    let pos_in_cycle = cycle_beat * steps_per_quarter;

    // Find the index of the most-recent hit that has fired this cycle.
    // `hits[0] == 0` (quantiser invariant) → there's always at least one
    // hit consumed once `cycle_beat >= 0`.
    let mut consumed = 0usize;
    for (i, h) in pattern.hits.iter().enumerate() {
        if (*h as f64) <= pos_in_cycle + 1e-9 {
            consumed = i + 1;
        } else {
            break;
        }
    }
    // Defensive: if somehow no hit was consumed (e.g. floating-point on
    // a totally fresh pattern at beat 0 with hits[0] != 0), we stay on
    // the *last* hit of the previous cycle to avoid an underflow.
    let (cycle_idx_eff, consumed_eff) = if consumed == 0 {
        (cycle_idx.saturating_sub(1), pattern.hits.len())
    } else {
        (cycle_idx, consumed)
    };
    let abs_step = cycle_idx_eff * len + (consumed_eff as u64) - 1;
    let last_hit_grid = pattern.hits[consumed_eff - 1] as f64;
    let step_started_beat = (cycle_idx_eff as f64) * beats_per_cycle + last_hit_grid / steps_per_quarter;

    // Distance to the next hit (wraps into the next cycle when we're
    // past the last hit of the current one).
    let next_hit_beat = if consumed_eff < pattern.hits.len() {
        let next_grid = pattern.hits[consumed_eff] as f64;
        (cycle_idx_eff as f64) * beats_per_cycle + next_grid / steps_per_quarter
    } else {
        // Wrap: the next hit is hits[0] of the following cycle.
        let next_grid = pattern.hits[0] as f64;
        ((cycle_idx_eff + 1) as f64) * beats_per_cycle + next_grid / steps_per_quarter
    };
    let step_dur_beats = (next_hit_beat - step_started_beat).max(1e-3);
    (abs_step, step_started_beat, step_dur_beats)
}

fn apply_chaser(
    entry: &mut ChaserEntry,
    overlay: &mut HashMap<u16, ChannelOverlay>,
    now: Instant,
    pattern: Option<(f32, &TempoPattern, Instant)>,
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
        // Pattern mode: step durations are non-uniform, so reach for the
        // value `advance_step` cached on the runtime. Subdivision mode:
        // compute it from BPM × subdivision like before.
        let step_ms = if let Some(p_step) = entry.runtime.pattern_step_ms {
            p_step
        } else {
            let bpm = pattern
                .map(|(b, _, _)| b)
                .unwrap_or_else(|| match entry.config.tempo {
                    TempoSource::Fixed { bpm } => bpm,
                });
            step_duration_ms(bpm, entry.config.subdivision).max(1.0)
        };
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
    overlay.entry(universe).or_insert_with(empty_overlay)[channel] = Some(value);
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
        let intensity_offset = role_offset(
            mode.channels.iter().map(|c| &c.role),
            &ChannelRole::Intensity,
        )
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
                    ChannelDefinition::new(ChannelRole::Red, 0),
                    ChannelDefinition::new(ChannelRole::Green, 0),
                    ChannelDefinition::new(ChannelRole::Blue, 0),
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
                channels: vec![ChannelDefinition::new(ChannelRole::Intensity, 0)],
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
        let ov0 = engine.tick(t0, None, None);
        let u0 = ov0.get(&0).unwrap();
        assert_eq!(u0[0], Some(255));

        // 120 BPM, subdivision One = 500 ms per step. Tick a tick later than
        // half a second → step 1 → Off.
        let ov1 = engine.tick(t0 + Duration::from_millis(510), None, None);
        let u1 = ov1.get(&0).unwrap();
        assert_eq!(u1[0], Some(0));

        // Another half-second → step 2 → On.
        let ov2 = engine.tick(t0 + Duration::from_millis(1010), None, None);
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

        let ov = engine.tick(Instant::now(), None, None);
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
        let on = engine.tick(t0, None, None);
        // Step 0 → on → full red.
        assert_eq!(on.get(&0).unwrap()[0], Some(255));
        // Step 1 → off, but background 64 → dim red.
        let off = engine.tick(t0 + Duration::from_millis(510), None, None);
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

        let ov = engine.tick(Instant::now(), None, None);
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
        let ov = engine.tick(Instant::now(), None, None);
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
        engine.tick(t0, None, None); // step 0 → 255 cached as last_emitted
                               // Cross into step 1 at t = 510 ms. fade_from snapshots 255.
        let ov_at_transition = engine.tick(t0 + Duration::from_millis(510), None, None);
        // At t=510 we are 0 ms into the fade → still ~255 (linear curve, t=0).
        assert!(
            ov_at_transition.get(&0).unwrap()[0].unwrap() > 200,
            "got {:?}",
            ov_at_transition.get(&0).unwrap()[0]
        );
        // Halfway through the 250 ms fade → ~127.
        let ov_mid = engine.tick(t0 + Duration::from_millis(510 + 125), None, None);
        let v = ov_mid.get(&0).unwrap()[0].unwrap();
        assert!((90..=160).contains(&(v as i32)), "expected ~127, got {v}");
        // Past the fade end → 0.
        let ov_done = engine.tick(t0 + Duration::from_millis(510 + 260), None, None);
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
        engine.tick(t0, None, None);
        // 1 ms into step 1 should already be the new value (Off = 0).
        let ov = engine.tick(t0 + Duration::from_millis(501), None, None);
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
        engine.tick(t0, None, None); // step 0
                               // Jump 5 seconds (= 10 half-second steps) → step 10 → On.
        let ov = engine.tick(t0 + Duration::from_millis(5_001), None, None);
        assert_eq!(ov.get(&0).unwrap()[0], Some(255));
    }

    #[test]
    fn beat_anchor_snaps_current_step_to_beat_grid() {
        // With anchor at (now, beat=4, bpm=120) the chaser using
        // Subdivision::One (1 step per beat) and AllTogether pattern
        // should land on step 4 → On. With beat=5 → step 5 → Off.
        let mut lib = HashMap::new();
        lib.insert("dimmer".into(), dimmer_def("dimmer"));
        let fixtures = vec![fixture("d1", "dimmer", 1)];

        let mut engine = ChaserEngine::new();
        engine.update_show_context(fixtures, lib);
        engine.replace_chasers(vec![chaser("c", vec!["d1"])]);

        let t0 = Instant::now();
        engine.set_beat_anchor(Some(BeatAnchor {
            set_at: t0,
            beat_at_set: 4.0,
            bpm: 120.0,
        }));
        let ov = engine.tick(t0, None, None);
        assert_eq!(ov.get(&0).unwrap()[0], Some(255), "beat 4 → step 4 → On");

        engine.set_beat_anchor(Some(BeatAnchor {
            set_at: t0,
            beat_at_set: 5.0,
            bpm: 120.0,
        }));
        let ov = engine.tick(t0, None, None);
        assert_eq!(ov.get(&0).unwrap()[0], Some(0), "beat 5 → step 5 → Off");
    }

    #[test]
    fn beat_anchor_interpolates_forward() {
        // Anchor set at (t0, beat=0). At t0+500ms with BPM=120 we
        // expect the engine to have advanced to beat 1 (= step 1
        // with Subdivision::One) → Off.
        let mut lib = HashMap::new();
        lib.insert("dimmer".into(), dimmer_def("dimmer"));
        let fixtures = vec![fixture("d1", "dimmer", 1)];

        let mut engine = ChaserEngine::new();
        engine.update_show_context(fixtures, lib);
        engine.replace_chasers(vec![chaser("c", vec!["d1"])]);

        let t0 = Instant::now();
        engine.set_beat_anchor(Some(BeatAnchor {
            set_at: t0,
            beat_at_set: 0.0,
            bpm: 120.0,
        }));
        // 500ms at 120 BPM = exactly 1 beat
        let ov = engine.tick(t0 + Duration::from_millis(500), None, None);
        assert_eq!(ov.get(&0).unwrap()[0], Some(0));
    }

    #[test]
    fn pattern_drives_chaser_step_per_hit() {
        // 5-hit clave 3-2 at 120 BPM. Chaser pattern is AllTogether so
        // even steps are On and odd steps are Off. Steps fire at grid
        // positions 0, 3, 6, 10, 12 (16ths) → in seconds: 0, 0.375,
        // 0.75, 1.25, 1.5.
        let mut lib = HashMap::new();
        lib.insert("dimmer".into(), dimmer_def("dimmer"));
        let fixtures = vec![fixture("d1", "dimmer", 1)];

        let mut engine = ChaserEngine::new();
        engine.update_show_context(fixtures, lib);
        engine.replace_chasers(vec![chaser("c", vec!["d1"])]);

        let pattern = crate::globals::TempoPattern {
            bars: 1,
            steps_per_bar: 16,
            hits: vec![0, 3, 6, 10, 12],
        };
        let t0 = Instant::now();

        // Hit 1 (step 0): On.
        let ov = engine.tick(t0, Some(120.0), Some((&pattern, t0)));
        assert_eq!(ov.get(&0).unwrap()[0], Some(255), "step 0 on");

        // Between hit 1 and 2 (still step 0).
        let ov = engine.tick(t0 + Duration::from_millis(200), Some(120.0), Some((&pattern, t0)));
        assert_eq!(ov.get(&0).unwrap()[0], Some(255), "still step 0 mid-gap");

        // Just past hit 2 (step 1): Off.
        let ov = engine.tick(t0 + Duration::from_millis(380), Some(120.0), Some((&pattern, t0)));
        assert_eq!(ov.get(&0).unwrap()[0], Some(0), "step 1 off after 2nd hit");

        // Just past hit 5 (step 4): On (step 4 is even under AllTogether).
        let ov = engine.tick(t0 + Duration::from_millis(1_510), Some(120.0), Some((&pattern, t0)));
        assert_eq!(ov.get(&0).unwrap()[0], Some(255), "step 4 on");

        // After the cycle wraps (>= 2 s) we're on step 5 = first hit of
        // the next cycle → Off.
        let ov = engine.tick(t0 + Duration::from_millis(2_010), Some(120.0), Some((&pattern, t0)));
        assert_eq!(ov.get(&0).unwrap()[0], Some(0), "step 5 off (cycle wrap)");
    }

    #[test]
    fn pattern_ignored_when_overall_bpm_disabled() {
        // Even if a pattern is passed in, with overall_bpm = None the
        // chaser must fall back to its own subdivision-driven timing.
        let mut lib = HashMap::new();
        lib.insert("dimmer".into(), dimmer_def("dimmer"));
        let fixtures = vec![fixture("d1", "dimmer", 1)];
        let mut engine = ChaserEngine::new();
        engine.update_show_context(fixtures, lib);
        engine.replace_chasers(vec![chaser("c", vec!["d1"])]);

        let pattern = crate::globals::TempoPattern {
            bars: 1,
            steps_per_bar: 16,
            hits: vec![0, 3, 6, 10, 12],
        };
        let t0 = Instant::now();
        engine.tick(t0, None, Some((&pattern, t0)));
        // 510 ms later at the chaser's default 120 BPM × Subdivision::One
        // (= 500 ms per step) we should be at step 1 → Off. If the
        // pattern had taken effect we'd be at step 2 (after the 2nd hit
        // at 375 ms) → On. So Off confirms the gating.
        let ov = engine.tick(t0 + Duration::from_millis(510), None, Some((&pattern, t0)));
        assert_eq!(ov.get(&0).unwrap()[0], Some(0));
    }

    #[test]
    fn pattern_step_at_clave_positions() {
        let p = crate::globals::TempoPattern {
            bars: 1,
            steps_per_bar: 16,
            hits: vec![0, 3, 6, 10, 12],
        };
        let (s0, _, _) = pattern_step_at(&p, 0.0);
        assert_eq!(s0, 0);
        let (s1, _, _) = pattern_step_at(&p, 0.8);
        assert_eq!(s1, 1);
        let (s2, _, _) = pattern_step_at(&p, 4.0);
        assert_eq!(s2, 5, "cycle wrap lands on step 5");
    }

    #[test]
    fn dropping_anchor_falls_back_to_free_run() {
        // After we install + clear the anchor, the chaser should
        // resume free-running on its own tempo. We verify by ticking
        // far enough that the absolute step count differs from the
        // anchor's frozen value — if free-run is broken, the chaser
        // would still be glued to "step 4" from the anchor.
        let mut lib = HashMap::new();
        lib.insert("dimmer".into(), dimmer_def("dimmer"));
        let fixtures = vec![fixture("d1", "dimmer", 1)];

        let mut engine = ChaserEngine::new();
        engine.update_show_context(fixtures, lib);
        engine.replace_chasers(vec![chaser("c", vec!["d1"])]);

        let t0 = Instant::now();
        engine.set_beat_anchor(Some(BeatAnchor {
            set_at: t0,
            beat_at_set: 4.0,
            bpm: 120.0,
        }));
        engine.tick(t0, None, None); // snaps to step 4
        engine.set_beat_anchor(None);
        // 501ms at 120 BPM (500ms/step) → free-run advances by 1
        let ov = engine.tick(t0 + Duration::from_millis(501), None, None);
        // step 4 → step 5 → Off
        assert_eq!(ov.get(&0).unwrap()[0], Some(0));
    }
}
