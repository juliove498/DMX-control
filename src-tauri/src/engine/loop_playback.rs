//! Sequence loop group playback driver.
//!
//! When the user starts a loop group, this struct remembers which
//! group is active and when the next scene change is due. A worker
//! thread polls every ~50 ms and, when the dwell timer expires, calls
//! back into `recall_scene_impl` to advance.
//!
//! Why not extend [`ScenePlayback`] instead? They run at different
//! scopes:
//! - `ScenePlayback` interpolates DMX values on the hot output thread
//!   for one scene at a time. Adding "advance to the next scene"
//!   would require a feedback loop back into command land.
//! - This struct lives off-thread, owns nothing on the hot path, and
//!   uses the existing `recall_scene` command path to swap scenes.
//!   Each recall transparently captures the right pre-recall snapshot
//!   so blending between scenes uses each scene's own fade time.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::show::loop_group::SceneLoopGroup;
use crate::show::scene::Scene;

#[derive(Debug, Default)]
pub struct LoopGroupPlayback {
    /// `Some` while a group is active.
    state: Option<ActiveState>,
}

#[derive(Debug, Clone)]
struct ActiveState {
    group_id: String,
    /// Resolved order of scene IDs at start time. We snapshot it once
    /// instead of re-reading the show every tick — keeps cycle
    /// behaviour deterministic if the user edits the group mid-loop.
    scene_ids: Vec<String>,
    current_idx: usize,
    /// When the current scene's dwell ends and the driver should
    /// recall the next one.
    advance_at: Instant,
}

impl LoopGroupPlayback {
    pub fn start(&mut self, group_id: String, scene_ids: Vec<String>, first_dwell_ms: u32, now: Instant) {
        if scene_ids.is_empty() {
            self.state = None;
            return;
        }
        self.state = Some(ActiveState {
            group_id,
            scene_ids,
            current_idx: 0,
            advance_at: now + Duration::from_millis(first_dwell_ms as u64),
        });
    }

    /// Re-compute when the current scene should advance — called by
    /// the driver after each recall so each scene gets its own dwell
    /// (which may differ because of overrides or per-scene cycle
    /// times).
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

    pub fn current_scene_id(&self) -> Option<&str> {
        self.state
            .as_ref()
            .and_then(|s| s.scene_ids.get(s.current_idx).map(String::as_str))
    }

    pub fn current_index(&self) -> Option<u32> {
        self.state.as_ref().map(|s| s.current_idx as u32)
    }

    /// Returns `Some(next_scene_id)` if it's time to advance. Mutates
    /// the index toward the next entry, wrapping back to 0 after the
    /// last. Caller is responsible for actually recalling the scene
    /// and then calling `schedule_next` with the new dwell.
    pub fn pop_if_ready(&mut self, now: Instant) -> Option<String> {
        let st = self.state.as_mut()?;
        if now < st.advance_at {
            return None;
        }
        if st.scene_ids.is_empty() {
            return None;
        }
        st.current_idx = (st.current_idx + 1) % st.scene_ids.len();
        st.scene_ids.get(st.current_idx).cloned()
    }
}

pub type SharedLoopPlayback = Arc<Mutex<LoopGroupPlayback>>;

pub fn shared_loop_playback() -> SharedLoopPlayback {
    Arc::new(Mutex::new(LoopGroupPlayback::default()))
}

/// Dwell heuristic: prefer the group's per-scene override when set,
/// else fall back to the scene's natural cycle (sum of step fade+hold).
/// Hard floor of 200 ms keeps a misconfigured group from busy-looping.
pub fn dwell_ms_for(group: &SceneLoopGroup, scene: &Scene) -> u32 {
    if group.hold_ms_override > 0 {
        return group.hold_ms_override.max(200);
    }
    let total: u32 = scene
        .steps
        .iter()
        .map(|s| s.fade_in_ms.saturating_add(s.hold_ms))
        .sum();
    total.max(200)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pop_returns_next_after_dwell() {
        let mut p = LoopGroupPlayback::default();
        let t0 = Instant::now();
        p.start("g".into(), vec!["a".into(), "b".into(), "c".into()], 100, t0);
        assert_eq!(p.current_scene_id(), Some("a"));
        // Before dwell expires: no advance.
        assert_eq!(p.pop_if_ready(t0 + Duration::from_millis(50)), None);
        // After: advances to b.
        assert_eq!(
            p.pop_if_ready(t0 + Duration::from_millis(150)).as_deref(),
            Some("b")
        );
        // Re-arm and advance again to c.
        p.schedule_next(100, t0 + Duration::from_millis(150));
        assert_eq!(
            p.pop_if_ready(t0 + Duration::from_millis(300)).as_deref(),
            Some("c")
        );
        // Wrap back to a.
        p.schedule_next(100, t0 + Duration::from_millis(300));
        assert_eq!(
            p.pop_if_ready(t0 + Duration::from_millis(450)).as_deref(),
            Some("a")
        );
    }

    #[test]
    fn stop_clears_state() {
        let mut p = LoopGroupPlayback::default();
        p.start("g".into(), vec!["a".into()], 100, Instant::now());
        p.stop();
        assert!(p.active_group_id().is_none());
    }

    #[test]
    fn empty_scene_list_doesnt_start() {
        let mut p = LoopGroupPlayback::default();
        p.start("g".into(), Vec::new(), 100, Instant::now());
        assert!(p.active_group_id().is_none());
    }
}
