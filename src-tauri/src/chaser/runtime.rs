//! Per-chaser runtime state. Lives next to the persisted config but is
//! intentionally kept off-disk: clock-derived state would just produce noisy
//! diffs in the show file and confuse "last saved at" comparisons.

use std::time::Instant;

use super::Rgb;

/// What the chaser actually output for a given slot on the previous frame.
/// Used as the lerp source for the fade engine — a step transition snapshots
/// these values into `fade_from`, and subsequent frames interpolate between
/// `fade_from` and the freshly computed target.
#[derive(Debug, Clone, Default)]
pub struct SlotOutput {
    pub intensity: Option<u8>,
    pub rgb: Option<Rgb>,
}

#[derive(Debug, Default)]
pub struct ChaserRuntime {
    /// Monotonically increasing step counter. Wraps at `u64::MAX`, which at
    /// 1 ms per step would take ~580 million years.
    pub current_step: u64,
    /// Wall-clock instant when `current_step` was first entered. `None`
    /// before the first tick or after a config change that resets timing.
    pub last_step_at: Option<Instant>,
    /// Wall-clock instant when the *current* step started. Used to derive
    /// fade progress within the step. Distinct from `last_step_at` in the
    /// pathological case where `advance_step` jumps several steps at once
    /// after a long pause — `last_step_at` is bumped forward in `step_dur`
    /// chunks, but `step_started_at` is reset to the moment we noticed.
    pub step_started_at: Option<Instant>,
    /// Outputs we wrote on the previous frame, one per slot. Persisted
    /// across frames so the fade engine can fade *from* the actual last
    /// pixel rather than from a recomputed "ideal" old target.
    pub last_emitted: Vec<SlotOutput>,
    /// Snapshot of `last_emitted` taken at the moment of the last step
    /// transition. Lerp source during the fade-in.
    pub fade_from: Vec<SlotOutput>,
    /// When a global tempo pattern drives this chaser, the *current
    /// step's duration in ms* (distance to the next pattern hit). The
    /// fade engine uses it instead of `step_duration_ms(bpm, subdiv)`
    /// because pattern steps are non-uniform — a clave's "..X" gap is
    /// longer than its "XX" pair. `None` means "free-run / subdivision
    /// path is active", which is the default.
    pub pattern_step_ms: Option<f32>,
}
