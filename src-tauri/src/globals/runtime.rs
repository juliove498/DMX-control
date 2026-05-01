//! Live runtime for blackout + blind. Holds:
//!
//! - The user's last-set target (`blackout_target`, `blind_target`).
//! - The currently-applied factor (animated 0..1).
//!
//! Each frame `tick()` advances the factors toward their targets at
//! direction-specific rates, builds the blind overlay, and reports the
//! resulting state so the engine can compose it into the snapshot.

use std::collections::HashMap;
use std::time::Instant;

use crate::engine::{empty_overlay, ChannelOverlay, DMX_CHANNELS};
use crate::show::fixture::{ChannelRole, FixtureDefinition, FixtureInstance};

use super::{BlindConfig, GlobalsConfig};

/// Halogen "cold" tip — what blinders look like just as they wake.
const HALOGEN_AMBER: (f32, f32, f32) = (255.0, 80.0, 0.0);
/// Halogen "hot" peak — fully ramped warm white.
const HALOGEN_WARM: (f32, f32, f32) = (255.0, 230.0, 180.0);

#[derive(Debug)]
pub struct GlobalsRuntime {
    pub config: GlobalsConfig,
    pub blackout_target: f32,
    pub blackout_factor: f32,
    pub blind_target: f32,
    pub blind_factor: f32,
    pub last_update: Option<Instant>,
    /// Snapshot of fixtures + library used to resolve blind slot writes.
    fixtures: Vec<FixtureInstance>,
    library: HashMap<String, FixtureDefinition>,
}

impl Default for GlobalsRuntime {
    fn default() -> Self {
        Self {
            config: GlobalsConfig::default(),
            blackout_target: 0.0,
            blackout_factor: 0.0,
            blind_target: 0.0,
            blind_factor: 0.0,
            last_update: None,
            fixtures: Vec::new(),
            library: HashMap::new(),
        }
    }
}

impl GlobalsRuntime {
    /// Replace the show context so blind slot resolution sees the latest
    /// patch. Doesn't touch interpolation state.
    pub fn update_show_context(
        &mut self,
        fixtures: Vec<FixtureInstance>,
        library: HashMap<String, FixtureDefinition>,
    ) {
        self.fixtures = fixtures;
        self.library = library;
    }

    /// Replace the persisted config (fade times, blind fixture list, the
    /// blackout target). Doesn't snap the runtime factors — the next tick
    /// fades to the new target naturally.
    pub fn replace_config(&mut self, config: GlobalsConfig) {
        self.blackout_target = if config.blackout.active { 1.0 } else { 0.0 };
        self.config = config;
    }

    pub fn set_blackout(&mut self, active: bool) {
        self.blackout_target = if active { 1.0 } else { 0.0 };
        self.config.blackout.active = active;
    }

    pub fn set_blind(&mut self, pressed: bool) {
        self.blind_target = if pressed { 1.0 } else { 0.0 };
    }

    /// Per-frame update. Returns the rendered blind overlay so the output
    /// thread can hand it to the engine. The blackout factor is read off
    /// `self.blackout_factor` afterwards.
    pub fn tick(&mut self, now: Instant) -> HashMap<u16, ChannelOverlay> {
        let delta_ms = match self.last_update {
            Some(prev) => now.saturating_duration_since(prev).as_secs_f32() * 1000.0,
            None => 0.0,
        };
        self.last_update = Some(now);

        self.blackout_factor = advance(
            self.blackout_factor,
            self.blackout_target,
            self.config.blackout.fade_in_ms,
            self.config.blackout.fade_out_ms,
            delta_ms,
        );
        self.blind_factor = advance(
            self.blind_factor,
            self.blind_target,
            self.config.blind.fade_in_ms,
            self.config.blind.fade_out_ms,
            delta_ms,
        );

        if self.blind_factor <= 0.0001 || self.config.blind.fixtures.is_empty() {
            return HashMap::new();
        }
        build_blind_overlay(&self.config.blind, self.blind_factor, &self.fixtures, &self.library)
    }
}

/// Advance `current` toward `target` by `delta_ms`. Picks `fade_in_ms`
/// when ramping up (target > current) and `fade_out_ms` when ramping down.
/// Returns the new value clamped to `[0, 1]`.
fn advance(current: f32, target: f32, fade_in_ms: u32, fade_out_ms: u32, delta_ms: f32) -> f32 {
    if (current - target).abs() < 0.0001 {
        return target;
    }
    let going_up = target > current;
    let ms = if going_up {
        fade_in_ms.max(1) as f32
    } else {
        fade_out_ms.max(1) as f32
    };
    let step = delta_ms / ms;
    let next = if going_up {
        (current + step).min(target)
    } else {
        (current - step).max(target)
    };
    next.clamp(0.0, 1.0)
}

/// Build the blind overlay for the assigned fixtures. The *full* target
/// goes here; the engine handles visibility via `blind_factor` (lerp
/// against the underlying state) so a release crossfades back to whatever
/// the chaser/manual writes had instead of dipping through black.
///
/// Each `BlindFixture` chooses how to be lit:
/// - empty `channels_at_full` → default halogen (warm-white on intensity
///   + RGB, chromaticity warming up amber→white with `factor`).
/// - non-empty `channels_at_full` → those role names get driven to 255
///   verbatim, no halogen colour logic. Use this for movers where you
///   want shutter / strobe / a custom flash channel slammed open.
fn build_blind_overlay(
    cfg: &BlindConfig,
    factor: f32,
    fixtures: &[FixtureInstance],
    library: &HashMap<String, FixtureDefinition>,
) -> HashMap<u16, ChannelOverlay> {
    let mut overlay: HashMap<u16, ChannelOverlay> = HashMap::new();
    for entry in &cfg.fixtures {
        let Some(fixture) = fixtures.iter().find(|f| f.id == entry.fixture_id) else {
            continue;
        };
        let Some(def) = library.get(&fixture.definition_id) else {
            continue;
        };
        let Some(mode) = def.modes.get(fixture.mode_index as usize) else {
            continue;
        };
        let base = (fixture.address as usize).saturating_sub(1);
        if entry.channels_at_full.is_empty() {
            write_halogen_default(&mut overlay, fixture, mode, base, factor);
        } else {
            write_user_channels_at_full(&mut overlay, fixture, mode, base, &entry.channels_at_full);
        }
    }
    overlay
}

fn write_halogen_default(
    overlay: &mut HashMap<u16, ChannelOverlay>,
    fixture: &FixtureInstance,
    mode: &crate::show::fixture::FixtureMode,
    base: usize,
    factor: f32,
) {
    let chrom = halogen_chromaticity(factor);
    let intensity_off =
        role_offset(mode.channels.iter().map(|c| &c.role), &ChannelRole::Intensity);
    let r_off = role_offset(mode.channels.iter().map(|c| &c.role), &ChannelRole::Red);
    let g_off = role_offset(mode.channels.iter().map(|c| &c.role), &ChannelRole::Green);
    let b_off = role_offset(mode.channels.iter().map(|c| &c.role), &ChannelRole::Blue);
    let r_byte = clamp_u8(chrom.0);
    let g_byte = clamp_u8(chrom.1);
    let b_byte = clamp_u8(chrom.2);
    if let Some(off) = intensity_off {
        set_overlay(overlay, fixture.universe, base + off, 255);
    }
    if let (Some(r), Some(g), Some(b)) = (r_off, g_off, b_off) {
        set_overlay(overlay, fixture.universe, base + r, r_byte);
        set_overlay(overlay, fixture.universe, base + g, g_byte);
        set_overlay(overlay, fixture.universe, base + b, b_byte);
    }
}

fn write_user_channels_at_full(
    overlay: &mut HashMap<u16, ChannelOverlay>,
    fixture: &FixtureInstance,
    mode: &crate::show::fixture::FixtureMode,
    base: usize,
    channels: &[String],
) {
    // For each requested role label, slam every matching channel to 255.
    // Multiple matches (rare — duplicate roles in one mode) all fire.
    for role_name in channels {
        for (i, ch) in mode.channels.iter().enumerate() {
            if ch.role.label() == role_name.as_str() {
                set_overlay(overlay, fixture.universe, base + i, 255);
            }
        }
    }
}

/// Lerp between cold-amber and warm-white based on `t`. Returns RGB at
/// "full magnitude" — the caller decides whether to scale by `t` or hand
/// the magnitude off to a separate intensity channel.
fn halogen_chromaticity(t: f32) -> (f32, f32, f32) {
    let t = t.clamp(0.0, 1.0);
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    (
        lerp(HALOGEN_AMBER.0, HALOGEN_WARM.0),
        lerp(HALOGEN_AMBER.1, HALOGEN_WARM.1),
        lerp(HALOGEN_AMBER.2, HALOGEN_WARM.2),
    )
}

fn clamp_u8(v: f32) -> u8 {
    v.clamp(0.0, 255.0).round() as u8
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

fn role_offset<'a, I>(iter: I, role: &ChannelRole) -> Option<usize>
where
    I: Iterator<Item = &'a ChannelRole>,
{
    iter.enumerate().find_map(|(i, r)| (r == role).then_some(i))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn fade_runtime() -> GlobalsRuntime {
        let mut r = GlobalsRuntime::default();
        r.config.blackout.fade_in_ms = 100;
        r.config.blackout.fade_out_ms = 200;
        r.config.blind.fade_in_ms = 50;
        r.config.blind.fade_out_ms = 500;
        r
    }

    #[test]
    fn blackout_fades_in_at_configured_speed() {
        let mut r = fade_runtime();
        let t0 = Instant::now();
        r.tick(t0); // initialises last_update
        r.set_blackout(true);
        // At t0 + 50 ms with fade_in_ms = 100 we should be ~halfway.
        r.tick(t0 + Duration::from_millis(50));
        assert!(
            r.blackout_factor > 0.4 && r.blackout_factor < 0.6,
            "got {}",
            r.blackout_factor
        );
        // After full fade we're at 1.
        r.tick(t0 + Duration::from_millis(150));
        assert!((r.blackout_factor - 1.0).abs() < 0.01);
    }

    #[test]
    fn blackout_fades_out_with_separate_speed() {
        let mut r = fade_runtime();
        r.blackout_factor = 1.0;
        r.config.blackout.active = true;
        r.blackout_target = 1.0;

        let t0 = Instant::now();
        r.tick(t0);
        r.set_blackout(false);
        // fade_out_ms = 200, halfway at 100ms.
        r.tick(t0 + Duration::from_millis(100));
        assert!(
            r.blackout_factor > 0.4 && r.blackout_factor < 0.6,
            "got {}",
            r.blackout_factor
        );
        r.tick(t0 + Duration::from_millis(220));
        assert!(r.blackout_factor < 0.05);
    }

    #[test]
    fn blind_does_nothing_when_no_fixtures_assigned() {
        let mut r = fade_runtime();
        r.set_blind(true);
        let ov = r.tick(Instant::now() + Duration::from_millis(60));
        assert!(ov.is_empty());
    }

    #[test]
    fn halogen_chromaticity_starts_amber_ends_warm() {
        let cold = halogen_chromaticity(0.0);
        let hot = halogen_chromaticity(1.0);
        // R is constant (255 in both), G climbs, B climbs.
        assert!((cold.0 - 255.0).abs() < 0.5);
        assert!((hot.0 - 255.0).abs() < 0.5);
        assert!(hot.1 > cold.1, "G should climb");
        assert!(hot.2 > cold.2, "B should climb");
    }
}
