//! Sequence loop group playback driver.
//!
//! When the user starts a loop group, this struct remembers which
//! group is active and when the next entry change is due. A worker
//! thread polls every ~50 ms and, when the dwell timer expires, calls
//! back into the commands layer to apply the next entry (scene recall
//! or snapshot values-apply).
//!
//! Why not extend [`ScenePlayback`] instead? They run at different
//! scopes:
//! - `ScenePlayback` interpolates DMX values on the hot output thread
//!   for one scene at a time. Adding "advance to the next entry"
//!   would require a feedback loop back into command land.
//! - This struct lives off-thread, owns nothing on the hot path, and
//!   uses the existing recall/apply command paths to swap cues.
//!   Each recall transparently captures the right pre-recall snapshot
//!   so blending between scenes uses each scene's own fade time.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::show::loop_group::{LoopEntry, SceneLoopGroup};
use crate::show::scene::Scene;

#[derive(Debug, Default)]
pub struct LoopGroupPlayback {
    /// `Some` while a group is active.
    state: Option<ActiveState>,
}

#[derive(Debug, Clone)]
struct ActiveState {
    group_id: String,
    /// Resolved entries at start time. We snapshot the list once
    /// instead of re-reading the show every tick — keeps cycle
    /// behaviour deterministic if the user edits the group mid-loop.
    entries: Vec<LoopEntry>,
    current_idx: usize,
    /// When the current entry's dwell ends and the driver should
    /// apply the next one.
    advance_at: Instant,
}

impl LoopGroupPlayback {
    pub fn start(
        &mut self,
        group_id: String,
        entries: Vec<LoopEntry>,
        first_dwell_ms: u32,
        now: Instant,
    ) {
        if entries.is_empty() {
            self.state = None;
            return;
        }
        self.state = Some(ActiveState {
            group_id,
            entries,
            current_idx: 0,
            advance_at: now + Duration::from_millis(first_dwell_ms as u64),
        });
    }

    /// Re-compute when the current entry should advance — called by
    /// the driver after each apply so each entry gets its own dwell
    /// (which may differ because of overrides, BPM sync, or per-scene
    /// cycle times).
    pub fn schedule_next(&mut self, dwell_ms: u32, now: Instant) {
        if let Some(ref mut st) = self.state {
            st.advance_at = now + Duration::from_millis(dwell_ms as u64);
        }
    }

    pub fn stop(&mut self) {
        self.state = None;
    }

    pub fn active_group_id(&self) -> Option<&str> {
        self.state.as_ref().map(|s| s.group_id.as_str())
    }

    pub fn current_entry(&self) -> Option<&LoopEntry> {
        self.state
            .as_ref()
            .and_then(|s| s.entries.get(s.current_idx))
    }

    pub fn current_index(&self) -> Option<u32> {
        self.state.as_ref().map(|s| s.current_idx as u32)
    }

    /// Returns `Some(next_entry)` if it's time to advance. Mutates
    /// the index toward the next entry, wrapping back to 0 after the
    /// last. Caller is responsible for actually applying the entry
    /// and then calling `schedule_next` with the new dwell.
    pub fn pop_if_ready(&mut self, now: Instant) -> Option<LoopEntry> {
        let st = self.state.as_mut()?;
        if now < st.advance_at {
            return None;
        }
        if st.entries.is_empty() {
            return None;
        }
        st.current_idx = (st.current_idx + 1) % st.entries.len();
        st.entries.get(st.current_idx).cloned()
    }
}

pub type SharedLoopPlayback = Arc<Mutex<LoopGroupPlayback>>;

pub fn shared_loop_playback() -> SharedLoopPlayback {
    Arc::new(Mutex::new(LoopGroupPlayback::default()))
}

/// Fallback dwell for snapshot entries: they have no fade/hold of
/// their own, so without an override or BPM sync they hold this long.
pub const SNAPSHOT_DEFAULT_DWELL_MS: u32 = 2000;

/// Dwell resolution, in priority order:
/// 1. BPM sync (when the group opts in AND the Overall BPM override is
///    on): one `subdivision` worth of quarter-note beats at `bpm`.
/// 2. The group's per-entry override.
/// 3. The entry's natural duration (`None` for snapshots → 2 s default).
///
/// Hard floor of 200 ms keeps a misconfigured group from busy-looping.
pub fn dwell_ms_for_entry(
    group: &SceneLoopGroup,
    natural_ms: Option<u32>,
    overall_bpm: Option<f32>,
) -> u32 {
    if group.sync_to_bpm {
        if let Some(bpm) = overall_bpm {
            let beat_ms = 60_000.0 / bpm.max(1.0);
            let dwell = (beat_ms * group.subdivision.beats()).round() as u32;
            return dwell.max(200);
        }
    }
    if group.hold_ms_override > 0 {
        return group.hold_ms_override.max(200);
    }
    natural_ms.unwrap_or(SNAPSHOT_DEFAULT_DWELL_MS).max(200)
}

/// Natural cycle of a scene: the sum of its steps' fade + hold.
pub fn scene_natural_ms(scene: &Scene) -> u32 {
    scene
        .steps
        .iter()
        .map(|s| s.fade_in_ms.saturating_add(s.hold_ms))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chaser::Subdivision;

    fn scene_entry(id: &str) -> LoopEntry {
        LoopEntry::Scene { id: id.into() }
    }

    #[test]
    fn pop_returns_next_after_dwell() {
        let mut p = LoopGroupPlayback::default();
        let t0 = Instant::now();
        p.start(
            "g".into(),
            vec![scene_entry("a"), scene_entry("b"), scene_entry("c")],
            100,
            t0,
        );
        assert_eq!(p.current_entry().map(|e| e.id()), Some("a"));
        // Before dwell expires: no advance.
        assert!(p.pop_if_ready(t0 + Duration::from_millis(50)).is_none());
        // After: advances to b.
        assert_eq!(
            p.pop_if_ready(t0 + Duration::from_millis(150))
                .map(|e| e.id().to_string())
                .as_deref(),
            Some("b")
        );
        // Re-arm and advance again to c.
        p.schedule_next(100, t0 + Duration::from_millis(150));
        assert_eq!(
            p.pop_if_ready(t0 + Duration::from_millis(300))
                .map(|e| e.id().to_string())
                .as_deref(),
            Some("c")
        );
        // Wrap back to a.
        p.schedule_next(100, t0 + Duration::from_millis(300));
        assert_eq!(
            p.pop_if_ready(t0 + Duration::from_millis(450))
                .map(|e| e.id().to_string())
                .as_deref(),
            Some("a")
        );
    }

    #[test]
    fn stop_clears_state() {
        let mut p = LoopGroupPlayback::default();
        p.start("g".into(), vec![scene_entry("a")], 100, Instant::now());
        p.stop();
        assert!(p.active_group_id().is_none());
    }

    #[test]
    fn empty_entry_list_doesnt_start() {
        let mut p = LoopGroupPlayback::default();
        p.start("g".into(), Vec::new(), 100, Instant::now());
        assert!(p.active_group_id().is_none());
    }

    fn group() -> SceneLoopGroup {
        SceneLoopGroup::new("g".into(), "x".into())
    }

    #[test]
    fn dwell_prefers_bpm_sync_when_enabled() {
        let mut g = group();
        g.sync_to_bpm = true;
        g.subdivision = Subdivision::Four; // one 4/4 bar
        g.hold_ms_override = 5000; // must be ignored while synced
                                   // 120 BPM → 500 ms/beat → 2000 ms/bar.
        assert_eq!(dwell_ms_for_entry(&g, Some(9999), Some(120.0)), 2000);
        // One beat at 120 BPM.
        g.subdivision = Subdivision::One;
        assert_eq!(dwell_ms_for_entry(&g, Some(9999), Some(120.0)), 500);
    }

    #[test]
    fn dwell_falls_back_when_bpm_off() {
        let mut g = group();
        g.sync_to_bpm = true; // opted in, but Overall BPM is off (None)
        g.hold_ms_override = 1500;
        assert_eq!(dwell_ms_for_entry(&g, Some(9999), None), 1500);
        g.hold_ms_override = 0;
        assert_eq!(dwell_ms_for_entry(&g, Some(750), None), 750);
        // Snapshot entry (no natural duration) → default.
        assert_eq!(
            dwell_ms_for_entry(&g, None, None),
            SNAPSHOT_DEFAULT_DWELL_MS
        );
    }

    #[test]
    fn dwell_has_a_floor() {
        let mut g = group();
        g.sync_to_bpm = true;
        g.subdivision = Subdivision::Quarter; // sixteenth notes
                                              // 300 BPM → 200 ms/beat → 50 ms/sixteenth → floored to 200.
        assert_eq!(dwell_ms_for_entry(&g, None, Some(300.0)), 200);
    }
}
