//! Runtime side of the snapshot feature: which snapshot is applied
//! right now, and the rig state captured the instant it was first
//! activated so deactivation can put everything back.
//!
//! The persisted shape (`crate::show::snapshot::Snapshot`) doubles as
//! the restore payload: activating captures the live rig into the same
//! struct before applying the chosen snapshot. Switching directly from
//! snapshot A to snapshot B keeps A's restore point — deactivating
//! always returns to "before any snapshot was active", which is what
//! the operator means by "como si nada hubiera pasado".

use std::sync::Arc;

use parking_lot::Mutex;

use crate::show::snapshot::Snapshot;

#[derive(Debug, Default)]
pub struct SnapshotRuntime {
    /// id of the snapshot currently applied. `None` = normal operation.
    active_id: Option<String>,
    /// Rig state captured just before the *first* activation. Consumed
    /// by deactivate; deliberately not replaced when switching between
    /// snapshots while one is already active.
    saved: Option<Snapshot>,
}

impl SnapshotRuntime {
    pub fn active_id(&self) -> Option<&str> {
        self.active_id.as_deref()
    }

    pub fn is_active(&self) -> bool {
        self.active_id.is_some()
    }

    /// Mark `id` as the applied snapshot. `pre` is stored as the restore
    /// point only when there isn't one already (first activation).
    pub fn set_active(&mut self, id: String, pre: Option<Snapshot>) {
        if self.saved.is_none() {
            self.saved = pre;
        }
        self.active_id = Some(id);
    }

    /// Consume the restore point and drop the active marker. Returns
    /// what should be re-applied to the rig (None if nothing was saved,
    /// e.g. deactivate without a prior activate).
    pub fn take_saved(&mut self) -> Option<Snapshot> {
        self.active_id = None;
        self.saved.take()
    }

    /// Forget the active snapshot *without* returning a restore payload.
    /// Used when the active snapshot is deleted: the rig keeps its
    /// current look, the runtime just stops tracking.
    pub fn clear(&mut self) {
        self.active_id = None;
        self.saved = None;
    }
}

pub type SharedSnapshotRuntime = Arc<Mutex<SnapshotRuntime>>;

pub fn shared_snapshot_runtime() -> SharedSnapshotRuntime {
    Arc::new(Mutex::new(SnapshotRuntime::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::show::scene::SceneFxState;

    fn dummy(id: &str) -> Snapshot {
        Snapshot {
            id: id.to_string(),
            name: id.to_string(),
            universes: Vec::new(),
            master: 255,
            chaser_state: SceneFxState::Disabled,
            movement_state: SceneFxState::Disabled,
            chaser_master: None,
            active_scene_id: None,
            active_loop_group_id: None,
            blackout: false,
            overall_bpm_enabled: false,
            overall_bpm: 120.0,
        }
    }

    #[test]
    fn switching_snapshots_keeps_first_restore_point() {
        let mut rt = SnapshotRuntime::default();
        rt.set_active("a".into(), Some(dummy("pre")));
        // Switching to B while A is active: no fresh capture is passed
        // (the caller only captures when nothing is active), and even a
        // Some() here must not clobber the original restore point.
        rt.set_active("b".into(), Some(dummy("mid")));
        assert_eq!(rt.active_id(), Some("b"));
        let saved = rt.take_saved().expect("restore point");
        assert_eq!(saved.id, "pre");
        assert!(!rt.is_active());
    }

    #[test]
    fn deactivate_without_activate_is_noop() {
        let mut rt = SnapshotRuntime::default();
        assert!(rt.take_saved().is_none());
        assert!(!rt.is_active());
    }
}
