//! Live runtime for blackout + blind. Holds:
//!
//! - The user's last-set target (`blackout_target`, `blind_target`).
//! - The currently-applied factor (animated 0..1).
//!
//! Each frame `tick()` advances the factors toward their targets at
//! direction-specific rates, builds the blind overlay, and reports the
//! resulting state so the engine can compose it into the snapshot.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::engine::{empty_mask, empty_overlay, ChannelMask, ChannelOverlay, DMX_CHANNELS};
use crate::show::fixture::{ChannelRole, FixtureDefinition, FixtureInstance};

use super::{BlackoutConfig, BlindConfig, GlobalsConfig, MasterConfig};

/// If two TAP presses are spaced wider than this, the operator is
/// almost certainly starting a new measurement — drop the previous
/// taps so a stale tempo doesn't drag the average.
const TAP_RESET_GAP: Duration = Duration::from_secs(2);

/// Bound the rolling tap window so very-old presses can't drift the
/// average even if they're spaced just under `TAP_RESET_GAP`.
const TAP_HISTORY_MAX: usize = 8;

/// Reasonable BPM ceiling/floor — anything outside is almost certainly
/// a misclick or a phantom button release. Matches the Launchpad's BPM
/// nudge clamp ([20, 300]).
const TAP_BPM_MIN: f32 = 20.0;
const TAP_BPM_MAX: f32 = 300.0;

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
    /// In-memory rolling buffer of TAP button presses. Not persisted —
    /// a tempo measured at the start of a song is meaningless across
    /// app restarts. Each entry is a wall-clock `Instant` of one tap.
    tap_history: Vec<Instant>,
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
            tap_history: Vec::new(),
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

    /// Toggle the override on/off without touching the stored BPM
    /// value. Disabling clears the tap window so the next enable starts
    /// fresh rather than averaging in stale taps from a previous song.
    pub fn set_overall_bpm_enabled(&mut self, enabled: bool) {
        self.config.overall_bpm_enabled = enabled;
        if !enabled {
            self.tap_history.clear();
        }
    }

    /// Set the BPM value directly (e.g. typing into the header input).
    /// Doesn't change the enabled flag — the operator might want to
    /// pre-set the value before turning the override on.
    pub fn set_overall_bpm(&mut self, bpm: f32) {
        self.config.overall_bpm = bpm.clamp(TAP_BPM_MIN, TAP_BPM_MAX);
    }

    /// Register a TAP press. Returns the freshly-computed BPM if at
    /// least two recent taps are available, or `None` if this was the
    /// first tap of a fresh window. The override is also turned on
    /// implicitly — operators expect TAP-then-watch behaviour without
    /// a separate "enable" click.
    pub fn tap_overall_bpm(&mut self, now: Instant) -> Option<f32> {
        // Reset on long gaps so a paused performance doesn't poison
        // the next measurement. Compares to the *last* tap so a slow
        // four-tap (e.g. 50 BPM = 1.2 s/beat) survives within the
        // 2 s threshold.
        if let Some(last) = self.tap_history.last() {
            if now.saturating_duration_since(*last) > TAP_RESET_GAP {
                self.tap_history.clear();
            }
        }
        self.tap_history.push(now);
        if self.tap_history.len() > TAP_HISTORY_MAX {
            let drop = self.tap_history.len() - TAP_HISTORY_MAX;
            self.tap_history.drain(0..drop);
        }
        if self.tap_history.len() < 2 {
            // First tap: enable so the next press lands on a hot
            // override. Don't touch the bpm value yet.
            self.config.overall_bpm_enabled = true;
            return None;
        }
        // Mean of inter-tap intervals, in seconds. Mean (vs median)
        // because four-tap measurements are short and a median would
        // throw away half the data; if the operator drifts they can
        // simply tap a few more times.
        let intervals_secs: Vec<f64> = self
            .tap_history
            .windows(2)
            .map(|w| w[1].saturating_duration_since(w[0]).as_secs_f64())
            .collect();
        let avg = intervals_secs.iter().sum::<f64>() / intervals_secs.len() as f64;
        if avg < 0.05 {
            // <50 ms between taps means a ghost click or stuck button.
            // Bail out without polluting the stored value.
            return None;
        }
        let bpm = ((60.0 / avg) as f32).clamp(TAP_BPM_MIN, TAP_BPM_MAX);
        self.config.overall_bpm = bpm;
        self.config.overall_bpm_enabled = true;
        Some(bpm)
    }

    /// The BPM that should override every chaser/movement on this
    /// frame, or `None` if the operator hasn't enabled the override.
    /// Read every tick by the engine — keep cheap.
    pub fn current_overall_bpm(&self) -> Option<f32> {
        if self.config.overall_bpm_enabled {
            Some(self.config.overall_bpm)
        } else {
            None
        }
    }

    /// Per-frame update. Returns the blind + blackout overlays so the
    /// output thread can hand them to the engine. Each overlay's
    /// visibility is animated by its respective `*_factor` (which the
    /// engine reads off this struct directly).
    pub fn tick(&mut self, now: Instant) -> GlobalsTickOutput {
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

        let blind = if self.blind_factor <= 0.0001 || self.config.blind.fixtures.is_empty() {
            HashMap::new()
        } else {
            build_blind_overlay(
                &self.config.blind,
                self.blind_factor,
                &self.fixtures,
                &self.library,
            )
        };
        let blackout = if self.blackout_factor <= 0.0001 {
            HashMap::new()
        } else {
            build_blackout_overlay(&self.config.blackout, &self.fixtures, &self.library)
        };
        // Master mask is rebuilt unconditionally — even when the master
        // fader is at 255 (engine skips the multiply anyway), we want
        // the mask to be ready the moment the user starts pulling so
        // the first frame off-full already respects the channel
        // restrictions.
        let master = build_master_mask(&self.config.master, &self.fixtures, &self.library);
        GlobalsTickOutput {
            blind,
            blackout,
            master,
        }
    }
}

/// Output of one tick of the globals runtime: separate overlays for
/// blind (warm halogen punch) and blackout (channels driven to 0).
/// Both are crossfaded against the underlying base + effects state by
/// the engine using `blind_factor` / `blackout_factor`.
pub struct GlobalsTickOutput {
    pub blind: HashMap<u16, ChannelOverlay>,
    pub blackout: HashMap<u16, ChannelOverlay>,
    /// Bitmask of channels the master fader is allowed to scale, per
    /// universe. Built every tick from `MasterConfig` + the patched
    /// fixtures. The engine reads this in `snapshot_universe`.
    pub master: HashMap<u16, ChannelMask>,
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
    let intensity_off = role_offset(
        mode.channels.iter().map(|c| &c.role),
        &ChannelRole::Intensity,
    );
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

/// Build the blackout overlay. Targets are *zero* — the engine lerps
/// the underlying state down to zero on the listed channels, leaving
/// pan/tilt/zoom etc. untouched (a moving head goes dark but stays
/// parked where it was).
///
/// Two modes:
/// - `cfg.fixtures` is empty → auto: every patched fixture gets its
///   intensity (or RGB if no intensity) plus strobe lerped to 0. This
///   is what the operator expects from a global blackout button.
/// - `cfg.fixtures` is non-empty → user mode: only the listed fixtures'
///   listed channels get zeroed. Per-fixture `channels_to_zero` empty
///   means "use the auto channel set for THIS fixture".
fn build_blackout_overlay(
    cfg: &BlackoutConfig,
    fixtures: &[FixtureInstance],
    library: &HashMap<String, FixtureDefinition>,
) -> HashMap<u16, ChannelOverlay> {
    let mut overlay: HashMap<u16, ChannelOverlay> = HashMap::new();
    if cfg.fixtures.is_empty() {
        for fixture in fixtures {
            let Some(def) = library.get(&fixture.definition_id) else {
                continue;
            };
            let Some(mode) = def.modes.get(fixture.mode_index as usize) else {
                continue;
            };
            let base = (fixture.address as usize).saturating_sub(1);
            write_blackout_auto(&mut overlay, fixture, mode, base);
        }
    } else {
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
            if entry.channels_to_zero.is_empty() {
                write_blackout_auto(&mut overlay, fixture, mode, base);
            } else {
                write_blackout_listed(&mut overlay, fixture, mode, base, &entry.channels_to_zero);
            }
        }
    }
    overlay
}

/// Build the per-universe master scaling mask. Channels marked `true`
/// will be multiplied by `master / 255` in the engine snapshot;
/// unmasked channels pass the master stage untouched (pan/tilt
/// continue parking, colour-wheel positions hold, etc).
///
/// Mode resolution mirrors blackout:
/// - `cfg.fixtures` empty → AUTO across every patched fixture: the
///   intensity channel, or the R/G/B channels if no dedicated
///   intensity exists. Same channel set the operator already gets
///   under "auto blackout"; consistency between the two globals
///   means they don't have to relearn the rules per button.
/// - `cfg.fixtures` non-empty → user mode: only the listed fixtures.
///   Each fixture's `channels_to_scale` empty falls back to the auto
///   set for THAT fixture; non-empty drives the named roles.
fn build_master_mask(
    cfg: &MasterConfig,
    fixtures: &[FixtureInstance],
    library: &HashMap<String, FixtureDefinition>,
) -> HashMap<u16, ChannelMask> {
    let mut mask: HashMap<u16, ChannelMask> = HashMap::new();
    if cfg.fixtures.is_empty() {
        for fixture in fixtures {
            let Some(def) = library.get(&fixture.definition_id) else {
                continue;
            };
            let Some(mode) = def.modes.get(fixture.mode_index as usize) else {
                continue;
            };
            let base = (fixture.address as usize).saturating_sub(1);
            mask_master_auto(&mut mask, fixture, mode, base);
        }
    } else {
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
            if entry.channels_to_scale.is_empty() {
                mask_master_auto(&mut mask, fixture, mode, base);
            } else {
                mask_master_listed(&mut mask, fixture, mode, base, &entry.channels_to_scale);
            }
        }
    }
    // The engine's `master_mask.get(universe)` returns `None` to mean
    // "no mask configured at all → fall back to global scaling".
    // Universes that have at least one fixture must always have an
    // entry in the map even if no channels ended up flagged, so the
    // engine does NOT regress to the global-master fallback when the
    // operator deliberately removes every channel from a fixture.
    // Insert empty masks for every universe represented in the patch.
    for fixture in fixtures {
        mask.entry(fixture.universe).or_insert_with(empty_mask);
    }
    mask
}

fn mask_master_auto(
    mask: &mut HashMap<u16, ChannelMask>,
    fixture: &FixtureInstance,
    mode: &crate::show::fixture::FixtureMode,
    base: usize,
) {
    let intensity_off = role_offset(
        mode.channels.iter().map(|c| &c.role),
        &ChannelRole::Intensity,
    );
    if let Some(off) = intensity_off {
        set_mask(mask, fixture.universe, base + off);
    } else {
        for role in [ChannelRole::Red, ChannelRole::Green, ChannelRole::Blue] {
            if let Some(off) = role_offset(mode.channels.iter().map(|c| &c.role), &role) {
                set_mask(mask, fixture.universe, base + off);
            }
        }
    }
}

fn mask_master_listed(
    mask: &mut HashMap<u16, ChannelMask>,
    fixture: &FixtureInstance,
    mode: &crate::show::fixture::FixtureMode,
    base: usize,
    channels: &[String],
) {
    for role_name in channels {
        for (i, ch) in mode.channels.iter().enumerate() {
            if ch.role.label() == role_name.as_str() {
                set_mask(mask, fixture.universe, base + i);
            }
        }
    }
}

fn set_mask(mask: &mut HashMap<u16, ChannelMask>, universe: u16, channel: usize) {
    if channel >= DMX_CHANNELS {
        return;
    }
    mask.entry(universe).or_insert_with(empty_mask)[channel] = true;
}

/// Auto blackout for one fixture: kill intensity (or R/G/B if there's
/// no intensity channel) and strobe. Everything else passes through —
/// pan/tilt should keep their position so the rig doesn't "snap home"
/// when the operator hits blackout mid-show.
fn write_blackout_auto(
    overlay: &mut HashMap<u16, ChannelOverlay>,
    fixture: &FixtureInstance,
    mode: &crate::show::fixture::FixtureMode,
    base: usize,
) {
    let intensity_off = role_offset(
        mode.channels.iter().map(|c| &c.role),
        &ChannelRole::Intensity,
    );
    if let Some(off) = intensity_off {
        set_overlay(overlay, fixture.universe, base + off, 0);
    } else {
        // No master dimmer — kill the colour channels so the fixture
        // goes dark.
        for role in [ChannelRole::Red, ChannelRole::Green, ChannelRole::Blue] {
            if let Some(off) = role_offset(mode.channels.iter().map(|c| &c.role), &role) {
                set_overlay(overlay, fixture.universe, base + off, 0);
            }
        }
    }
    if let Some(off) = role_offset(mode.channels.iter().map(|c| &c.role), &ChannelRole::Strobe) {
        set_overlay(overlay, fixture.universe, base + off, 0);
    }
}

/// User-listed channels for blackout: drive each named role to 0.
fn write_blackout_listed(
    overlay: &mut HashMap<u16, ChannelOverlay>,
    fixture: &FixtureInstance,
    mode: &crate::show::fixture::FixtureMode,
    base: usize,
    channels: &[String],
) {
    for role_name in channels {
        for (i, ch) in mode.channels.iter().enumerate() {
            if ch.role.label() == role_name.as_str() {
                set_overlay(overlay, fixture.universe, base + i, 0);
            }
        }
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
    overlay.entry(universe).or_insert_with(empty_overlay)[channel] = Some(value);
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
        assert!(ov.blind.is_empty());
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
