pub mod beatgrid;
pub mod loop_playback;
pub mod output_thread;
pub mod scene_playback;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use ts_rs::TS;

pub const DMX_CHANNELS: usize = 512;

/// Per-universe overlay buffer used by the low-priority effects layer
/// (Ambient Chaser + Movement Generator). `Some(v)` means "an effect owns
/// this channel right now, output `v`"; `None` means "fall through to the
/// base universe value". Manual user writes go to the base universe, so to
/// make effects respect a manual write we'd need an explicit
/// programmer/cuelist layer above. For now effects overwrite manual writes
/// on the channels they own — the user disables the effect to take
/// control back.
pub type ChannelOverlay = [Option<u8>; DMX_CHANNELS];

/// Combine two effect-layer overlays into one. The `top` map's values
/// take priority on any channel where both maps have `Some`. In the
/// current world chasers write to colour/intensity and the movement
/// generator writes to pan/tilt, so collisions are not expected — but the
/// last-writer-wins rule is the cleanest tie-break if they ever happen.
pub fn merge_overlays(
    bottom: HashMap<u16, ChannelOverlay>,
    top: HashMap<u16, ChannelOverlay>,
) -> HashMap<u16, ChannelOverlay> {
    let mut out = bottom;
    for (universe, top_buf) in top {
        let entry = out.entry(universe).or_insert_with(empty_overlay);
        for (i, slot) in top_buf.iter().enumerate() {
            if let Some(v) = slot {
                entry[i] = Some(*v);
            }
        }
    }
    out
}

pub fn empty_overlay() -> ChannelOverlay {
    [None; DMX_CHANNELS]
}

/// Bitmask flagging which channels of a universe the master fader is
/// allowed to scale. Constructed once per frame from the user's
/// MasterConfig (via `globals::runtime`). `true` → channel goes
/// through the master multiplication; `false` → passes the master
/// stage untouched (pan/tilt/colour-wheel/etc. survive the dim).
pub type ChannelMask = [bool; DMX_CHANNELS];

pub fn empty_mask() -> ChannelMask {
    [false; DMX_CHANNELS]
}

#[derive(Clone, Debug)]
pub struct Universe {
    pub id: u16,
    pub data: [u8; DMX_CHANNELS],
}

impl Universe {
    pub fn new(id: u16) -> Self {
        Self {
            id,
            data: [0u8; DMX_CHANNELS],
        }
    }

    pub fn blackout(&mut self) {
        self.data = [0u8; DMX_CHANNELS];
    }
}

/// Crossfade of the base layer toward a full-universe target (a
/// snapshot with a fade). Ticked by the output thread each frame; a
/// scene recall or a hard snapshot cancels it.
#[derive(Debug, Clone)]
pub struct BaseFade {
    universes: Vec<BaseFadeUniverse>,
    started: Instant,
    duration: Duration,
}

#[derive(Debug, Clone)]
struct BaseFadeUniverse {
    id: u16,
    from: [u8; DMX_CHANNELS],
    to: [u8; DMX_CHANNELS],
}

#[derive(Debug)]
pub struct EngineInner {
    pub universes: Vec<Universe>,
    /// Snapshot crossfade in flight, if any (see `start_base_fade`).
    pub base_fade: Option<BaseFade>,
    pub master: u8,
    /// Animated blackout factor in `[0, 1]`. `0` = no blackout; `1` = full
    /// blackout. Driven by `globals::runtime` so the user gets configurable
    /// fade-to-black / fade-from-black times.
    pub blackout_factor: f32,
    /// Per-universe overlay written by the Ambient Chaser + Movement layers.
    /// Merged into the snapshot before master/blackout are applied, so
    /// effects ride the global master and respect blackout.
    pub effects_overlay: HashMap<u16, ChannelOverlay>,
    /// Blind/blinder overlay. Holds the *full-strength* halogen target
    /// values for each affected channel. Visibility is governed by
    /// `blind_factor` so the merge can crossfade between the underlying
    /// state (manual + chasers + movement) and the halogen target —
    /// otherwise releasing blind would briefly fade through black before
    /// snapping back to whatever the rig was doing.
    pub blind_overlay: HashMap<u16, ChannelOverlay>,
    /// `0.0..=1.0` visibility of the blind layer. `0` = blind contributes
    /// nothing; `1` = blind fully overrides. Driven by `globals::runtime`.
    pub blind_factor: f32,
    /// Blackout overlay: per-fixture, per-channel target of `0`. Same
    /// crossfade pattern as blind — channels listed in this overlay get
    /// lerped toward 0 by `blackout_factor`, while channels not listed
    /// (typically pan/tilt/zoom) pass through untouched. This replaces
    /// the older "multiply every channel by `1 - factor`" path which
    /// also dragged movers' positions to 0.
    pub blackout_overlay: HashMap<u16, ChannelOverlay>,
    /// Per-universe bitmask of channels the master fader is allowed to
    /// scale. `globals::runtime` rebuilds this each frame from the
    /// user's MasterConfig — by default it covers intensity-or-RGB on
    /// every patched fixture and leaves pan/tilt/wheel channels alone.
    /// An empty map (no entries for the universe) is treated as the
    /// pre-feature behaviour: master scales every channel. That keeps
    /// engine-only contexts (tests, mid-startup) from suddenly losing
    /// their master before the runtime has populated the mask.
    pub master_mask: HashMap<u16, ChannelMask>,
    /// Safe-boot hold: channels forced to 0 with absolute priority,
    /// after every other layer. Used while a fixture powers up — cheap
    /// moving heads read DMX during their homing sweep, and a non-zero
    /// pan/tilt frame corrupts their end-stop reference (they then slam
    /// the physical stops). Holding the fixture's whole footprint at 0
    /// reproduces what a manual console with all faders down sends,
    /// which is the condition under which these fixtures home cleanly.
    pub safe_boot_mask: HashMap<u16, ChannelMask>,
}

impl EngineInner {
    pub fn new(universe_count: u16) -> Self {
        let universes = (0..universe_count).map(Universe::new).collect();
        Self {
            universes,
            base_fade: None,
            master: 255,
            blackout_factor: 0.0,
            effects_overlay: HashMap::new(),
            blind_overlay: HashMap::new(),
            blind_factor: 0.0,
            blackout_overlay: HashMap::new(),
            master_mask: HashMap::new(),
            safe_boot_mask: HashMap::new(),
        }
    }

    pub fn with_universes(ids: &[u16]) -> Self {
        let mut universes: Vec<Universe> = ids.iter().copied().map(Universe::new).collect();
        universes.sort_by_key(|u| u.id);
        universes.dedup_by_key(|u| u.id);
        Self {
            universes,
            base_fade: None,
            master: 255,
            blackout_factor: 0.0,
            effects_overlay: HashMap::new(),
            blind_overlay: HashMap::new(),
            blind_factor: 0.0,
            blackout_overlay: HashMap::new(),
            master_mask: HashMap::new(),
            safe_boot_mask: HashMap::new(),
        }
    }

    /// Reconcile the tracked universes with `wanted`. Existing data is preserved
    /// for any universe that survives; new universes start zeroed.
    pub fn reconcile_universes(&mut self, wanted: &[u16]) {
        let mut next: Vec<Universe> = Vec::with_capacity(wanted.len());
        for &id in wanted {
            if let Some(existing) = self.universes.iter().find(|u| u.id == id) {
                next.push(existing.clone());
            } else {
                next.push(Universe::new(id));
            }
        }
        next.sort_by_key(|u| u.id);
        next.dedup_by_key(|u| u.id);
        self.universes = next;
    }

    pub fn set_channel(
        &mut self,
        universe: u16,
        channel: u16,
        value: u8,
    ) -> Result<(), EngineError> {
        let u = self
            .universes
            .iter_mut()
            .find(|u| u.id == universe)
            .ok_or(EngineError::UniverseNotFound(universe))?;
        let idx = channel as usize;
        if idx >= DMX_CHANNELS {
            return Err(EngineError::ChannelOutOfRange(channel));
        }
        u.data[idx] = value;
        Ok(())
    }

    /// Glide the base layer of the given universes from where it is now
    /// to `targets` over `duration`. Universes unknown to the engine are
    /// skipped. Replaces any fade in flight — the current values are the
    /// new start, so the glide stays continuous.
    pub fn start_base_fade(
        &mut self,
        targets: &[(u16, [u8; DMX_CHANNELS])],
        duration: Duration,
        now: Instant,
    ) {
        let universes: Vec<BaseFadeUniverse> = targets
            .iter()
            .filter_map(|(id, to)| {
                let from = self.snapshot_base(*id)?;
                Some(BaseFadeUniverse {
                    id: *id,
                    from,
                    to: *to,
                })
            })
            .collect();
        self.base_fade = (!universes.is_empty()).then_some(BaseFade {
            universes,
            started: now,
            duration,
        });
    }

    pub fn cancel_base_fade(&mut self) {
        self.base_fade = None;
    }

    pub fn base_fade_active(&self) -> bool {
        self.base_fade.is_some()
    }

    /// Advance the base fade, writing the interpolated values into the
    /// universes. Drops itself once the target has landed.
    pub fn tick_base_fade(&mut self, now: Instant) {
        let Self {
            universes,
            base_fade,
            ..
        } = self;
        let Some(fade) = base_fade.as_ref() else {
            return;
        };
        let t = if fade.duration.is_zero() {
            1.0
        } else {
            (now.saturating_duration_since(fade.started).as_secs_f32()
                / fade.duration.as_secs_f32())
            .clamp(0.0, 1.0)
        };
        for fu in &fade.universes {
            let Some(u) = universes.iter_mut().find(|u| u.id == fu.id) else {
                continue;
            };
            for i in 0..DMX_CHANNELS {
                let (a, b) = (fu.from[i], fu.to[i]);
                u.data[i] = if a == b {
                    b
                } else {
                    (a as f32 + (b as f32 - a as f32) * t).round() as u8
                };
            }
        }
        if t >= 1.0 {
            *base_fade = None;
        }
    }

    /// Raw base-layer snapshot — what `set_channel` last wrote, with
    /// no effects/blind/master/blackout overlays applied. Used by
    /// scene recall to capture "where the base values were before
    /// this recall" so release can fade base back without
    /// double-counting chaser/movement contributions, which the
    /// overlays apply independently each frame.
    pub fn snapshot_base(&self, universe: u16) -> Option<[u8; DMX_CHANNELS]> {
        let u = self.universes.iter().find(|u| u.id == universe)?;
        Some(u.data)
    }

    pub fn snapshot_universe(&self, universe: u16) -> Option<[u8; DMX_CHANNELS]> {
        let u = self.universes.iter().find(|u| u.id == universe)?;
        let mut out = u.data;
        // 1. Effects layer (chasers + movement): merge over the base.
        if let Some(overlay) = self.effects_overlay.get(&universe) {
            for (i, slot) in overlay.iter().enumerate() {
                if let Some(v) = slot {
                    out[i] = *v;
                }
            }
        }
        // 2. Blind layer: a momentary halogen punch on configured fixtures.
        // Cross-fades with the underlying state (base + effects) using
        // `blind_factor` so a release blends the halogen back to whatever
        // the rig was doing — not through black. The overlay holds the
        // *full* halogen target for each channel; the engine handles the
        // visibility ramp via this lerp.
        if self.blind_factor > 0.0 {
            if let Some(overlay) = self.blind_overlay.get(&universe) {
                let f = self.blind_factor.clamp(0.0, 1.0);
                for (i, slot) in overlay.iter().enumerate() {
                    if let Some(target) = slot {
                        let from = out[i] as f32;
                        let to = *target as f32;
                        out[i] = (from + (to - from) * f).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
        // 3. Master fader: scales channels by `self.master / 255`. The
        //    set of channels affected is the per-universe `master_mask`
        //    built each frame by `globals::runtime` from the user's
        //    MasterConfig — by default it covers intensity-or-RGB on
        //    every patched fixture and lets pan/tilt/colour-wheel
        //    survive intact (a half-master shouldn't drag mover
        //    positions toward channel 127). When the runtime hasn't
        //    populated a mask for this universe yet (e.g. boot, tests)
        //    we fall back to scaling every channel — preserves the
        //    pre-mask behaviour so the engine is never silently
        //    "muted" before the globals tick has landed once.
        if self.master != 255 {
            let m = self.master as u16;
            match self.master_mask.get(&universe) {
                Some(mask) => {
                    for (i, v) in out.iter_mut().enumerate() {
                        if mask[i] {
                            *v = ((*v as u16 * m) / 255) as u8;
                        }
                    }
                }
                None => {
                    for v in out.iter_mut() {
                        *v = ((*v as u16 * m) / 255) as u8;
                    }
                }
            }
        }
        // 4. Blackout layer: lerp the configured channels toward 0 by
        // `blackout_factor`. Channels not present in the overlay pass
        // through untouched, so a moving head holds its pan/tilt while
        // its dimmer/strobe fade out.
        if self.blackout_factor > 0.0 {
            if let Some(overlay) = self.blackout_overlay.get(&universe) {
                let f = self.blackout_factor.clamp(0.0, 1.0);
                for (i, slot) in overlay.iter().enumerate() {
                    if slot.is_some() {
                        // Target is always 0 for blackout — saves the
                        // overlay from having to spell that out and
                        // makes the lerp a single multiply.
                        out[i] = ((out[i] as f32) * (1.0 - f)).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
        // 5. Safe-boot hold: absolute. A fixture that is powering up
        // must see all-zero on its footprint no matter what scenes /
        // effects / blind are doing, so this runs after everything.
        if let Some(mask) = self.safe_boot_mask.get(&universe) {
            for (i, v) in out.iter_mut().enumerate() {
                if mask[i] {
                    *v = 0;
                }
            }
        }
        Some(out)
    }

    /// Replace the entire effects overlay (chaser ⊕ movement) for the next
    /// frame. Universes not in `next` get an empty (`None`-filled) overlay
    /// so disabled effects stop influencing output immediately.
    pub fn replace_effects_overlay(&mut self, next: HashMap<u16, ChannelOverlay>) {
        self.effects_overlay = next;
    }

    /// Replace the blind overlay (halogen punch). Same semantics as
    /// `replace_effects_overlay` but at a higher priority.
    pub fn replace_blind_overlay(&mut self, next: HashMap<u16, ChannelOverlay>) {
        self.blind_overlay = next;
    }

    pub fn replace_blackout_overlay(&mut self, next: HashMap<u16, ChannelOverlay>) {
        self.blackout_overlay = next;
    }

    pub fn replace_master_mask(&mut self, next: HashMap<u16, ChannelMask>) {
        self.master_mask = next;
    }

    /// Replace the safe-boot hold mask. Rebuilt by the commands layer
    /// from the set of fixtures currently flagged as booting.
    pub fn replace_safe_boot_mask(&mut self, next: HashMap<u16, ChannelMask>) {
        self.safe_boot_mask = next;
    }
}

#[derive(Clone)]
pub struct EngineState(Arc<RwLock<EngineInner>>);

impl EngineState {
    pub fn new(universe_count: u16) -> Self {
        Self(Arc::new(RwLock::new(EngineInner::new(universe_count))))
    }

    pub fn with_universes(ids: &[u16]) -> Self {
        Self(Arc::new(RwLock::new(EngineInner::with_universes(ids))))
    }

    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, EngineInner> {
        self.0.read()
    }

    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, EngineInner> {
        self.0.write()
    }
}

#[derive(Debug, thiserror::Error, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub enum EngineError {
    #[error("universe {0} not found")]
    UniverseNotFound(u16),
    #[error("channel {0} out of range (0..512)")]
    ChannelOutOfRange(u16),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct EngineStats {
    pub fps: f32,
    pub frames_total: u32,
    pub late_frames: u32,
    pub max_frame_micros: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_channel_writes_value() {
        let mut s = EngineInner::new(1);
        s.set_channel(0, 5, 200).unwrap();
        assert_eq!(s.universes[0].data[5], 200);
    }

    #[test]
    fn snapshot_applies_master() {
        let mut s = EngineInner::new(1);
        s.set_channel(0, 0, 200).unwrap();
        s.master = 128;
        let snap = s.snapshot_universe(0).unwrap();
        assert_eq!(snap[0], 100);
    }

    #[test]
    fn snapshot_blackout_zeros_listed_channel() {
        // Blackout now operates per-channel via an overlay (built by
        // globals::runtime). Channels in the overlay get lerped to 0;
        // channels outside it pass through. Here we list channel 0 and
        // expect channel 1 to be untouched — the regression we're
        // guarding against is "blackout drags pan/tilt to 0 even
        // though the operator only wanted dimmers killed".
        let mut s = EngineInner::new(1);
        s.set_channel(0, 0, 255).unwrap();
        s.set_channel(0, 1, 127).unwrap(); // pretend this is "pan"
        let mut overlay = empty_overlay();
        overlay[0] = Some(0);
        s.blackout_overlay.insert(0, overlay);
        s.blackout_factor = 1.0;
        let snap = s.snapshot_universe(0).unwrap();
        assert_eq!(snap[0], 0);
        assert_eq!(snap[1], 127, "blackout must not touch unlisted channels");
    }

    #[test]
    fn snapshot_blackout_partial_dims_proportionally() {
        let mut s = EngineInner::new(1);
        s.set_channel(0, 0, 200).unwrap();
        let mut overlay = empty_overlay();
        overlay[0] = Some(0);
        s.blackout_overlay.insert(0, overlay);
        s.blackout_factor = 0.5; // 50% blackout → keep 50%
        let snap = s.snapshot_universe(0).unwrap();
        assert!((snap[0] as i32 - 100).abs() <= 1, "got {}", snap[0]);
    }

    #[test]
    fn unknown_universe_errors() {
        let mut s = EngineInner::new(1);
        assert!(s.set_channel(7, 0, 1).is_err());
    }

    #[test]
    fn out_of_range_channel_errors() {
        let mut s = EngineInner::new(1);
        assert!(s.set_channel(0, 512, 1).is_err());
    }

    #[test]
    fn effects_overlay_replaces_base_on_owned_channels() {
        let mut s = EngineInner::new(1);
        s.set_channel(0, 0, 80).unwrap(); // base
        let mut overlay = empty_overlay();
        overlay[0] = Some(200);
        s.effects_overlay.insert(0, overlay);
        let snap = s.snapshot_universe(0).unwrap();
        assert_eq!(snap[0], 200);
    }

    #[test]
    fn effects_overlay_falls_through_on_unowned_channels() {
        let mut s = EngineInner::new(1);
        s.set_channel(0, 5, 77).unwrap();
        let overlay = empty_overlay(); // all None
        s.effects_overlay.insert(0, overlay);
        let snap = s.snapshot_universe(0).unwrap();
        assert_eq!(snap[5], 77);
    }

    #[test]
    fn effects_overlay_runs_under_master() {
        let mut s = EngineInner::new(1);
        let mut overlay = empty_overlay();
        overlay[0] = Some(200);
        s.effects_overlay.insert(0, overlay);
        s.master = 128;
        let snap = s.snapshot_universe(0).unwrap();
        // (200 * 128) / 255 = 100
        assert_eq!(snap[0], 100);
    }

    #[test]
    fn effects_overlay_killed_by_blackout() {
        // Effects (chasers, movement) sit under master + blackout, so
        // a chaser writing 255 to a channel listed in the blackout
        // overlay should still come through as 0 at full blackout.
        let mut s = EngineInner::new(1);
        let mut effects = empty_overlay();
        effects[0] = Some(255);
        s.effects_overlay.insert(0, effects);
        let mut blackout = empty_overlay();
        blackout[0] = Some(0);
        s.blackout_overlay.insert(0, blackout);
        s.blackout_factor = 1.0;
        let snap = s.snapshot_universe(0).unwrap();
        assert_eq!(snap[0], 0);
    }

    #[test]
    fn blind_overlay_at_full_factor_takes_over() {
        let mut s = EngineInner::new(1);
        let mut effects = empty_overlay();
        effects[0] = Some(50);
        s.effects_overlay.insert(0, effects);
        let mut blind = empty_overlay();
        blind[0] = Some(255);
        s.blind_overlay.insert(0, blind);
        s.blind_factor = 1.0;
        let snap = s.snapshot_universe(0).unwrap();
        assert_eq!(snap[0], 255);
    }

    #[test]
    fn blind_overlay_at_zero_factor_is_invisible() {
        let mut s = EngineInner::new(1);
        let mut effects = empty_overlay();
        effects[0] = Some(50);
        s.effects_overlay.insert(0, effects);
        let mut blind = empty_overlay();
        blind[0] = Some(255);
        s.blind_overlay.insert(0, blind);
        s.blind_factor = 0.0;
        let snap = s.snapshot_universe(0).unwrap();
        assert_eq!(snap[0], 50);
    }

    #[test]
    fn safe_boot_mask_zeroes_over_everything() {
        // Base at 127 (mover parked centre), an effect writing 200, and
        // blind punching 255 — the safe-boot hold must still force 0 on
        // the held channel and leave the neighbour untouched.
        let mut s = EngineInner::new(1);
        s.set_channel(0, 0, 127).unwrap();
        s.set_channel(0, 1, 127).unwrap();
        let mut effects = empty_overlay();
        effects[0] = Some(200);
        s.effects_overlay.insert(0, effects);
        let mut blind = empty_overlay();
        blind[0] = Some(255);
        s.blind_overlay.insert(0, blind);
        s.blind_factor = 1.0;
        let mut mask = empty_mask();
        mask[0] = true;
        s.safe_boot_mask.insert(0, mask);
        let snap = s.snapshot_universe(0).unwrap();
        assert_eq!(snap[0], 0, "held channel must be forced to 0");
        assert_eq!(snap[1], 127, "unheld channel must pass through");
    }

    #[test]
    fn blind_overlay_lerps_with_underlying_mid_factor() {
        // Underlying = 50 (chaser writing dim), blind target = 250.
        // At blind_factor = 0.5 the lerp should land at (50 + 250) / 2 = 150.
        let mut s = EngineInner::new(1);
        let mut effects = empty_overlay();
        effects[0] = Some(50);
        s.effects_overlay.insert(0, effects);
        let mut blind = empty_overlay();
        blind[0] = Some(250);
        s.blind_overlay.insert(0, blind);
        s.blind_factor = 0.5;
        let snap = s.snapshot_universe(0).unwrap();
        assert!((snap[0] as i32 - 150).abs() <= 1, "got {}", snap[0]);
    }
}
