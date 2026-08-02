//! Beat-grid anchor used by chasers + movement to lock their phase
//! to an external clock source (currently VirtualDJ via the
//! `get_beatpos` script; could be MIDI Clock or Ableton Link later).
//!
//! Why an anchor instead of a "current beat" number every frame: the
//! output thread runs at 44 fps but the external clock source updates
//! once every ~250 ms (HTTP poll). An anchor holds (wall_clock,
//! beat_at_that_clock, bpm) and lets every frame in between
//! interpolate cleanly:
//!
//!   current_beat(now) = beat_at_set + (now - set_at) * bpm / 60
//!
//! Set to `None` (or unset) means "free-run mode" — chasers /
//! movement use their own configured tempo without external phase
//! correction. That's the existing pre-sync behaviour, preserved as
//! the fallback for shows that don't use a DAW.

use std::time::Instant;

/// A point on the beat grid plus the tempo that was active when it
/// was sampled. The bpm is needed for inter-frame interpolation —
/// the next anchor will refresh both, so a tempo change between
/// polls just means a slightly wrong derived beat for the gap and
/// then a snap-to-correct on the next poll.
#[derive(Debug, Clone, Copy)]
pub struct BeatAnchor {
    pub set_at: Instant,
    pub beat_at_set: f64,
    pub bpm: f32,
}

impl BeatAnchor {
    /// Interpolate forward from this anchor to estimate the current
    /// beat position at `now`. Used by chasers + movement to derive
    /// per-frame phase without needing the anchor to be refreshed
    /// every frame.
    pub fn beat_at(&self, now: Instant) -> f64 {
        let elapsed_secs = now.saturating_duration_since(self.set_at).as_secs_f64();
        let bpm = self.bpm.max(1.0) as f64;
        self.beat_at_set + elapsed_secs * bpm / 60.0
    }
}
