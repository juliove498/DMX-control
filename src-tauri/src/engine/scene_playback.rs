//! Multi-step scene playback with automatic looping.
//!
//! When a scene is recalled, the playback engine resolves every step
//! into a `(universe, channel) → value` map. Each tick:
//!
//! 1. If the active step is **fading in**, lerp from the captured
//!    `fade_from` toward that step's targets and emit the per-channel
//!    updates.
//! 2. Once the fade lands at 1.0, transition into **hold**: stop
//!    emitting updates (the values now sit in `Universe.data` as the
//!    base) and start the hold timer.
//! 3. When the hold timer expires, snapshot the current target into a
//!    new `fade_from`, advance to the next step (wrapping back to 0
//!    after the last), and start the next fade.
//!
//! Single-step scenes still work: `hold_ms` is ignored at the end of
//! the only step (we just sit there until the operator recalls another
//! scene or releases).
//!
//! Recall mid-fade is supported: the new scene's `fade_from` is the
//! universe's current state at recall time, so the lerp is continuous.
//! Caveat (still true from the MVP): a manual `set_channel` *during a
//! fade* gets overwritten on the next tick. That's the trade-off of
//! writing into the base; a future LTP overlay programmer fixes it.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::Sender;
use parking_lot::Mutex;

use crate::show::scene::SceneFxState;

/// FX activation request the playback emits whenever a step transition
/// crosses (or when a release should restore the pre-recall context).
/// A separate consumer thread drains these and dispatches via the same
/// `toggle_chaser_impl` / `toggle_movement_impl` helpers the UI uses,
/// keeping disk persistence + Tauri events off the hot output thread.
#[derive(Debug, Clone)]
pub struct SceneFxApply {
    pub chaser: SceneFxState,
    pub movement: SceneFxState,
    /// `true` for the request emitted on release — purely informational
    /// for tracing/logs, not used for behaviour.
    pub is_release: bool,
}

pub type FxSender = Sender<SceneFxApply>;

#[derive(Debug, Default)]
pub struct ScenePlayback {
    /// id of the currently-recalled scene. Held even between fade and
    /// hold phases so the UI/Launchpad can highlight the active row.
    /// `None` = idle.
    active_scene_id: Option<String>,
    /// Pre-resolved per-step targets in DMX coordinates. One entry per
    /// step in the scene. Empty = no scene loaded.
    steps: Vec<ResolvedStep>,
    /// Index into `steps` that's currently driving playback.
    current_step: usize,
    /// State machine for the current step.
    phase: Phase,
    /// Per-channel starting value for the current fade. Captured at
    /// recall time (from the engine's universe data) for the first
    /// step; for subsequent steps we capture the previous step's
    /// targets so the lerp is continuous.
    fade_from: HashMap<(u16, u16), u8>,
    /// Where to send FX activation requests. Set once at app boot via
    /// `set_fx_sender`. `None` = playback runs values-only (used in
    /// unit tests and during boot before the consumer thread spins up).
    fx_tx: Option<FxSender>,
    /// Snapshot of the chaser / movement state taken at recall time so
    /// release can restore "what was running before the scene started".
    /// `None` once a release has consumed it.
    pre_recall_fx: Option<(SceneFxState, SceneFxState)>,
    /// Per-channel snapshot taken at recall time for *every* channel
    /// any step touches. Used as the target on `release` so the rig
    /// fades back to whatever was on the wire before the recall. Cleared
    /// once a release has finished its fade.
    pre_recall_values: HashMap<(u16, u16), u8>,
    /// Tracks the last value emitted per channel during normal playback.
    /// On release we lerp **from** these toward `pre_recall_values`,
    /// so a release in the middle of a fade picks up where the lerp
    /// was rather than snapping back to the step's full target.
    last_emitted: HashMap<(u16, u16), u8>,
}

#[derive(Debug, Clone)]
struct ResolvedStep {
    fade_in_ms: u32,
    hold_ms: u32,
    targets: HashMap<(u16, u16), u8>,
    /// FX state captured into this step. Sent as an `SceneFxApply` to
    /// the consumer thread the instant playback transitions into the
    /// step (i.e., the start of its fade).
    chaser: SceneFxState,
    movement: SceneFxState,
}

#[derive(Debug, Default, Clone, Copy)]
enum Phase {
    #[default]
    Idle,
    /// Fading in to the current step's targets. `started` = when the
    /// fade began.
    Fade { started: Instant },
    /// Holding at the current step's targets. `started` = when the
    /// hold timer began (i.e. when the fade reached 1.0).
    Hold { started: Instant },
    /// Fading out: lerping from `last_emitted` toward `pre_recall_values`
    /// after a release. Once `factor >= 1.0`, the playback drops to
    /// `Idle` and clears state for good.
    Release { started: Instant, fade_ms: u32 },
}

/// Default release fade when the scene's first step was a snap (fade=0)
/// or the operator hasn't tuned anything else. Long enough that the
/// rig doesn't pop, short enough that "Liberar" still feels responsive.
const DEFAULT_RELEASE_FADE_MS: u32 = 800;
const MIN_RELEASE_FADE_MS: u32 = 200;

/// Resolve a step's `(fade_ms, hold_ms)` for this tick.
///
/// Without an Overall BPM override the step's authored values pass
/// through unchanged. With an override every multi-step scene is
/// quantised to one beat per step: total step duration = `60_000 / bpm`,
/// split between fade and hold using the same ratio the step was
/// authored with so a "smooth fade" step stays smooth and a "snap then
/// hold" step stays snappy — just stretched or compressed to land on
/// the beat. A single-step scene with `hold_ms = 0` is a "stay forever"
/// cue and is left alone (it has no concept of "next beat").
fn step_durations(step: &ResolvedStep, overall_bpm: Option<f32>, single_step: bool) -> (u32, u32) {
    let Some(bpm) = overall_bpm else {
        return (step.fade_in_ms, step.hold_ms);
    };
    if single_step && step.hold_ms == 0 {
        return (step.fade_in_ms, step.hold_ms);
    }
    let beat_ms = (60_000.0 / bpm.max(1.0)).round() as u32;
    let beat_ms = beat_ms.max(2); // floor guards a /0 in the fade phase
    let total = step.fade_in_ms.saturating_add(step.hold_ms).max(1);
    let fade_ratio = (step.fade_in_ms as f32 / total as f32).clamp(0.0, 1.0);
    let mut new_fade = ((beat_ms as f32 * fade_ratio).round() as u32).max(1);
    if new_fade >= beat_ms {
        // Pure-fade step in a multi-step scene: never starve the hold
        // entirely or the engine never advances past the fade frame's
        // factor==1.0 trigger. Reserve a single-ms hold tick.
        new_fade = beat_ms.saturating_sub(1).max(1);
    }
    let new_hold = beat_ms.saturating_sub(new_fade);
    (new_fade, new_hold)
}

impl ScenePlayback {
    /// Wire the channel that carries FX-apply requests to the consumer
    /// thread. Call once at app startup; subsequent recalls and the
    /// release path will use it.
    pub fn set_fx_sender(&mut self, tx: FxSender) {
        self.fx_tx = Some(tx);
    }

    /// Start a new playback. `pre_recall_values` should hold the
    /// universe's current values for **every** key any step will write.
    /// The engine reads it as the lerp source for step 0's fade *and*
    /// keeps it as the target for the eventual release fade — so when
    /// the operator hits "Liberar", the rig fades back to whatever was
    /// on the wire just before the recall. `pre_recall_fx` is the
    /// matching chaser/movement context. Steps beyond the first
    /// regenerate `fade_from` on the fly from the previous target.
    pub fn recall(
        &mut self,
        scene_id: String,
        steps: Vec<ResolvedStepInput>,
        pre_recall_values: HashMap<(u16, u16), u8>,
        pre_recall_fx: (SceneFxState, SceneFxState),
        now: Instant,
    ) {
        let resolved: Vec<ResolvedStep> = steps
            .into_iter()
            .map(|s| ResolvedStep {
                fade_in_ms: s.fade_in_ms.max(1),
                hold_ms: s.hold_ms,
                targets: s.targets,
                chaser: s.chaser_state,
                movement: s.movement_state,
            })
            .collect();
        // Refresh pre-recall snapshots: a brand-new recall always
        // captures the rig's CURRENT FX context AND its current
        // per-channel values, even mid-playback of another scene. That
        // keeps "release goes back to whatever was running just before
        // this recall" consistent across both FX state and DMX values.
        self.pre_recall_fx = Some(pre_recall_fx);
        self.pre_recall_values = pre_recall_values.clone();
        // last_emitted starts as the pre-recall snapshot. The Fade
        // phase will overwrite per-key entries as it emits, so a
        // release mid-fade still has a reasonable "from" for keys
        // we never actually moved.
        self.last_emitted = pre_recall_values.clone();
        if resolved.is_empty() {
            // Nothing to play. Leave the active id set so the UI knows
            // the scene was "selected" but emit no updates.
            self.steps.clear();
            self.fade_from.clear();
            self.current_step = 0;
            self.phase = Phase::Idle;
            self.active_scene_id = Some(scene_id);
            return;
        }
        self.steps = resolved;
        self.fade_from = pre_recall_values;
        self.current_step = 0;
        self.phase = Phase::Fade { started: now };
        self.active_scene_id = Some(scene_id);
        // Send step 0's FX context. Consumer applies it asynchronously.
        self.send_fx_for_current_step(false);
    }

    /// Stop tracking any scene and start a fade-out toward the
    /// pre-recall snapshot (both DMX values and FX state). If no
    /// playback is active — e.g. release without a preceding recall —
    /// emit nothing and stay idle.
    pub fn release(&mut self, now: Instant) {
        if let Some((chaser, movement)) = self.pre_recall_fx.take() {
            self.send_fx(chaser, movement, true);
        }
        // No values were ever emitted (recall on an empty scene, or
        // release-before-recall): just go idle.
        if self.last_emitted.is_empty() || self.pre_recall_values.is_empty() {
            self.clear_to_idle();
            return;
        }
        // Fade duration: prefer the current step's fade_in_ms (so a
        // slow scene fades out gently and a snap scene snaps back),
        // floored to MIN_RELEASE_FADE_MS, with DEFAULT as fallback if
        // we somehow have no current step.
        let fade_ms = self
            .steps
            .get(self.current_step)
            .map(|s| s.fade_in_ms)
            .filter(|&v| v > 0)
            .unwrap_or(DEFAULT_RELEASE_FADE_MS)
            .max(MIN_RELEASE_FADE_MS);
        self.active_scene_id = None;
        self.steps.clear();
        self.fade_from.clear();
        self.current_step = 0;
        self.phase = Phase::Release {
            started: now,
            fade_ms,
        };
    }

    /// Drop any active playback immediately — no release fade, no FX
    /// restore request. Used by snapshot activation, which replaces the
    /// whole rig state (base values *and* FX layers) wholesale right
    /// after clearing; letting the normal release machinery run would
    /// only have its fade-out fight the incoming snapshot values.
    pub fn clear_hard(&mut self) {
        self.pre_recall_fx = None;
        self.clear_to_idle();
    }

    fn clear_to_idle(&mut self) {
        self.active_scene_id = None;
        self.steps.clear();
        self.fade_from.clear();
        self.current_step = 0;
        self.phase = Phase::Idle;
        self.pre_recall_values.clear();
        self.last_emitted.clear();
    }

    fn send_fx_for_current_step(&self, is_release: bool) {
        let Some(step) = self.steps.get(self.current_step) else {
            return;
        };
        self.send_fx(step.chaser.clone(), step.movement.clone(), is_release);
    }

    fn send_fx(&self, chaser: SceneFxState, movement: SceneFxState, is_release: bool) {
        if matches!(chaser, SceneFxState::Inherit) && matches!(movement, SceneFxState::Inherit) {
            // Both Inherit = nothing to do, don't bother the consumer.
            return;
        }
        if let Some(tx) = &self.fx_tx {
            let _ = tx.send(SceneFxApply {
                chaser,
                movement,
                is_release,
            });
        }
    }

    pub fn active_scene_id(&self) -> Option<&str> {
        self.active_scene_id.as_deref()
    }

    /// Returns the index of the step currently playing back, if any.
    /// UIs use this to highlight which step in the active scene is
    /// "live" right now. `None` once playback has been released.
    pub fn current_step_index(&self) -> Option<usize> {
        if matches!(self.phase, Phase::Idle) {
            None
        } else {
            Some(self.current_step)
        }
    }

    /// Compute the values to write into the universe for this frame.
    /// Returns empty when in `Idle` or `Hold` (during hold the values
    /// already sit in `Universe.data`; nothing to refresh).
    ///
    /// `overall_bpm`, when `Some`, replaces every multi-step scene's
    /// per-step fade + hold timings with one beat per step (fade and
    /// hold split by the same ratio the step was authored with). A
    /// single-step scene with `hold_ms = 0` is left alone because
    /// "stay here forever" has no beat semantics.
    pub fn tick(&mut self, now: Instant, overall_bpm: Option<f32>) -> Vec<((u16, u16), u8)> {
        let single_step = self.steps.len() == 1;
        loop {
            // Borrow the phase by value to keep the match-arm logic
            // shorter; we'll write back as needed.
            match self.phase {
                Phase::Idle => return Vec::new(),
                Phase::Fade { started } => {
                    let Some(step) = self.steps.get(self.current_step) else {
                        // Defensive: bad state, drop playback.
                        self.clear_to_idle();
                        return Vec::new();
                    };
                    let (fade_ms_u32, _) = step_durations(step, overall_bpm, single_step);
                    let fade_ms = fade_ms_u32 as f32;
                    let elapsed = now.saturating_duration_since(started).as_secs_f32() * 1000.0;
                    let factor = (elapsed / fade_ms.max(1.0)).clamp(0.0, 1.0);
                    let mut out = Vec::with_capacity(step.targets.len());
                    for (key, &target_v) in &step.targets {
                        let from_v = self.fade_from.get(key).copied().unwrap_or(0) as f32;
                        let v = from_v + (target_v as f32 - from_v) * factor;
                        let emitted = v.round().clamp(0.0, 255.0) as u8;
                        // Track what we put on the wire so a release
                        // mid-fade lerps from the actual current value
                        // rather than snapping back to the step's full
                        // target.
                        self.last_emitted.insert(*key, emitted);
                        out.push((*key, emitted));
                    }
                    if factor >= 1.0 {
                        // Transition from fade to hold. Don't loop —
                        // emit the final fade frame this tick so the
                        // engine writes the exact targets.
                        self.phase = Phase::Hold { started: now };
                    }
                    return out;
                }
                Phase::Hold { started } => {
                    let Some(step) = self.steps.get(self.current_step) else {
                        self.clear_to_idle();
                        return Vec::new();
                    };
                    if step.hold_ms == 0 && single_step {
                        // Single-step scene with no hold means "stay
                        // here forever". Nothing to emit until the next
                        // recall releases us. Overall BPM doesn't apply
                        // because there's nothing to advance to.
                        return Vec::new();
                    }
                    let (_, hold_ms_u32) = step_durations(step, overall_bpm, single_step);
                    let elapsed = now.saturating_duration_since(started).as_secs_f32() * 1000.0;
                    if elapsed < hold_ms_u32 as f32 {
                        // Still inside hold, no updates needed.
                        return Vec::new();
                    }
                    // Hold complete → advance to the next step. Use the
                    // current targets as the lerp source, then loop the
                    // outer `loop` so we re-enter `Fade` and emit the
                    // first frame of the new fade in this same tick.
                    self.fade_from = step.targets.clone();
                    self.current_step = (self.current_step + 1) % self.steps.len();
                    self.phase = Phase::Fade { started: now };
                    // Fire the new step's FX context as we cross the
                    // boundary. The consumer thread picks it up next
                    // tick and applies asynchronously, so the DMX hot
                    // path stays uncontended.
                    self.send_fx_for_current_step(false);
                    // continue → next iteration handles Fade
                }
                Phase::Release { started, fade_ms } => {
                    let elapsed = now.saturating_duration_since(started).as_secs_f32() * 1000.0;
                    let factor = (elapsed / (fade_ms as f32).max(1.0)).clamp(0.0, 1.0);
                    // Build the union of keys we've touched and keys
                    // we want to restore — covers the common case but
                    // also a release that fires before any fade tick
                    // ever ran.
                    let mut out = Vec::with_capacity(self.last_emitted.len());
                    for (key, &from_v) in &self.last_emitted {
                        let target_v = self.pre_recall_values.get(key).copied().unwrap_or(0) as f32;
                        let v = from_v as f32 + (target_v - from_v as f32) * factor;
                        out.push((*key, v.round().clamp(0.0, 255.0) as u8));
                    }
                    // Pre-recall keys that were never emitted should
                    // also fade — e.g. a release that fires immediately
                    // after recall before the first fade tick ran.
                    for (key, &target_v) in &self.pre_recall_values {
                        if self.last_emitted.contains_key(key) {
                            continue;
                        }
                        // No prior emission → just snap to target. It
                        // matches what was already on the wire so this
                        // is a no-op for the engine but keeps the
                        // semantics tidy.
                        out.push((*key, target_v));
                    }
                    if factor >= 1.0 {
                        self.clear_to_idle();
                    }
                    return out;
                }
            }
        }
    }
}

/// Per-step input for [`ScenePlayback::recall`]. Decoupled from the
/// internal `ResolvedStep` so callers don't have to think about the
/// fade-ms minimum the playback enforces.
pub struct ResolvedStepInput {
    pub fade_in_ms: u32,
    pub hold_ms: u32,
    pub targets: HashMap<(u16, u16), u8>,
    pub chaser_state: SceneFxState,
    pub movement_state: SceneFxState,
}

pub type SharedScenePlayback = Arc<Mutex<ScenePlayback>>;

pub fn shared_scene_playback() -> SharedScenePlayback {
    Arc::new(Mutex::new(ScenePlayback::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn step(fade: u32, hold: u32, kv: &[((u16, u16), u8)]) -> ResolvedStepInput {
        ResolvedStepInput {
            fade_in_ms: fade,
            hold_ms: hold,
            targets: kv.iter().copied().collect(),
            chaser_state: SceneFxState::Inherit,
            movement_state: SceneFxState::Inherit,
        }
    }

    fn no_fx() -> (SceneFxState, SceneFxState) {
        (SceneFxState::Inherit, SceneFxState::Inherit)
    }

    #[test]
    fn idle_emits_nothing() {
        let mut p = ScenePlayback::default();
        assert!(p.tick(Instant::now(), None).is_empty());
    }

    #[test]
    fn single_step_full_fade_lands_on_target() {
        let mut p = ScenePlayback::default();
        let t0 = Instant::now();
        p.recall(
            "a".into(),
            vec![step(100, 0, &[((0, 0), 200)])],
            HashMap::from([((0, 0), 0)]),
            no_fx(),
            t0,
        );
        let out = p.tick(t0 + Duration::from_millis(150), None);
        assert_eq!(out, vec![((0, 0), 200)]);
        // Next tick we're in hold (single-step + hold=0 = sit forever).
        assert!(p.tick(t0 + Duration::from_millis(200), None).is_empty());
    }

    #[test]
    fn multi_step_advances_through_loop() {
        let mut p = ScenePlayback::default();
        let t0 = Instant::now();
        p.recall(
            "loop".into(),
            vec![
                step(50, 100, &[((0, 0), 100)]),
                step(50, 100, &[((0, 0), 200)]),
            ],
            HashMap::from([((0, 0), 0)]),
            no_fx(),
            t0,
        );
        // After 60 ms: step 0 fade complete, target 100.
        let out = p.tick(t0 + Duration::from_millis(60), None);
        assert_eq!(out.first().map(|x| x.1), Some(100));
        assert_eq!(p.current_step_index(), Some(0));

        // After 60 + 110 ms: hold expired, step 1 fade started but only
        // a sliver complete. We should be on step 1 now.
        // Hold ran from ~50ms to ~150ms. At 170ms we're in step 1's
        // fade, ~20ms in (40% of 50ms).
        let _ = p.tick(t0 + Duration::from_millis(170), None);
        assert_eq!(p.current_step_index(), Some(1));

        // After step 1 fade completes (200 ms target by 220 ms).
        let out = p.tick(t0 + Duration::from_millis(220), None);
        assert_eq!(out.first().map(|x| x.1), Some(200));

        // After step 1 hold expires (100 ms hold, so by 330 ms) we're
        // back on step 0.
        let _ = p.tick(t0 + Duration::from_millis(360), None);
        assert_eq!(p.current_step_index(), Some(0));
    }

    #[test]
    fn release_clears_state() {
        let mut p = ScenePlayback::default();
        p.recall(
            "x".into(),
            vec![step(100, 0, &[])],
            HashMap::new(),
            no_fx(),
            Instant::now(),
        );
        assert_eq!(p.active_scene_id(), Some("x"));
        // Empty pre_recall_values + no emissions → release jumps
        // straight to idle.
        p.release(Instant::now());
        assert!(p.active_scene_id().is_none());
        assert!(p.current_step_index().is_none());
    }

    #[test]
    fn empty_steps_marks_active_but_emits_nothing() {
        let mut p = ScenePlayback::default();
        p.recall(
            "empty".into(),
            Vec::new(),
            HashMap::new(),
            no_fx(),
            Instant::now(),
        );
        assert_eq!(p.active_scene_id(), Some("empty"));
        assert!(p.tick(Instant::now(), None).is_empty());
        // No current step since we never started a fade.
        assert!(p.current_step_index().is_none());
    }

    #[test]
    fn release_without_pre_recall_emits_nothing() {
        // Defensive: if release is called without a prior recall, the
        // pre-recall slot is None and we should *not* fire a stale FX
        // restoration request. This guards a regression where an early
        // `release_scene` command could push garbage to the consumer.
        let (tx, rx) = crossbeam_channel::unbounded::<SceneFxApply>();
        let mut p = ScenePlayback::default();
        p.set_fx_sender(tx);
        p.release(Instant::now());
        assert!(
            rx.try_recv().is_err(),
            "no FX request should have been sent"
        );
    }

    #[test]
    fn release_after_recall_restores_pre_recall_fx() {
        let (tx, rx) = crossbeam_channel::unbounded::<SceneFxApply>();
        let mut p = ScenePlayback::default();
        p.set_fx_sender(tx);
        p.recall(
            "x".into(),
            vec![step(100, 0, &[])],
            HashMap::new(),
            (
                SceneFxState::Enabled {
                    id: "chaser-pre".into(),
                },
                SceneFxState::Disabled,
            ),
            Instant::now(),
        );
        // Drain the recall-time send (step 0 was Inherit so no message
        // for that, but a non-Inherit step would fire one).
        while rx.try_recv().is_ok() {}
        p.release(Instant::now());
        let req = rx.try_recv().expect("release should send the restore");
        assert!(matches!(req.chaser, SceneFxState::Enabled { ref id } if id == "chaser-pre"));
        assert!(matches!(req.movement, SceneFxState::Disabled));
        assert!(req.is_release);
    }

    #[test]
    fn release_fades_dmx_back_to_pre_recall_values() {
        // Recall a scene that drives ch0=200 from a starting point of 0,
        // let it fully land, then release and verify the engine emits a
        // lerp back toward 0 over the release fade.
        let mut p = ScenePlayback::default();
        let t0 = Instant::now();
        p.recall(
            "fade-back".into(),
            vec![step(100, 0, &[((0, 0), 200)])],
            HashMap::from([((0, 0), 0)]),
            no_fx(),
            t0,
        );
        // Land on the target (factor=1).
        let out = p.tick(t0 + Duration::from_millis(150), None);
        assert_eq!(out, vec![((0, 0), 200)]);
        // Release: should switch to Phase::Release with last_emitted=200,
        // pre_recall_values=0. Halfway through MIN_RELEASE_FADE_MS we
        // should be ~halfway between 200 and 0.
        let t_rel = t0 + Duration::from_millis(200);
        p.release(t_rel);
        // Step 0's fade was 100 ms which is below MIN_RELEASE_FADE_MS
        // (200), so the floor kicks in.
        let half = p.tick(t_rel + Duration::from_millis(100), None);
        assert_eq!(half.len(), 1);
        let v = half[0].1 as i32;
        assert!(
            (90..=110).contains(&v),
            "expected ~100 mid-release, got {v}"
        );
        // After the full release fade, we should land at 0 and go idle.
        let done = p.tick(t_rel + Duration::from_millis(300), None);
        assert_eq!(done, vec![((0, 0), 0)]);
        assert!(p.active_scene_id().is_none());
        assert!(p.tick(t_rel + Duration::from_millis(400), None).is_empty());
    }
}
