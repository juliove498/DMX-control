//! Programmer: lightweight "touched fixtures" tracker.
//!
//! Phase 4 iteration 2 scope: instead of a full Avolites-style override
//! layer (which would mean rerouting every manual write away from
//! `Universe.data`), we track only *which fixtures the user has touched*
//! since the last Clear/Record. That's enough to power the two flows
//! the operator actually wants:
//!
//! - **Record from touched** — capture only the fixtures with active
//!   edits into a new scene. No more scrolling through 30 chips to
//!   pick the 4 you just dialed.
//! - **Update scene** — refresh an existing scene with the current
//!   state of its fixtures (or just the touched ones), so the
//!   "tweak the recall and re-save" loop is one click.
//!
//! The full-blown LTP/HTP merge layer is parked for a future iteration.
//! Until then, manual writes still go straight to `Universe.data` (so
//! the engine snapshot stays correct without architectural surgery)
//! and Clear is purely a marker reset.
//!
//! Why minimal? Because the architectural change of routing every
//! manual write through a programmer overlay touches `set_channel`,
//! the Direct Output view, the engine merge order, and the engine
//! autosave semantics. Rolling all of that in one go is high-risk.
//! Shipping touched-tracking unlocks the workflow win immediately and
//! keeps the door open for the bigger refactor when there's time.

use std::collections::BTreeSet;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Default)]
pub struct Programmer {
    /// Fixture ids the user has edited since the last Clear or Record.
    /// `BTreeSet` so the iteration order is stable for the UI without
    /// a separate sort step.
    touched: BTreeSet<String>,
}

impl Programmer {
    pub fn touch(&mut self, fixture_id: impl Into<String>) {
        self.touched.insert(fixture_id.into());
    }

    pub fn clear(&mut self) {
        self.touched.clear();
    }

    /// Stop tracking just one fixture (e.g. when it's removed from the
    /// patch). Doesn't reset its DMX values — those belong to the
    /// engine; we only own the marker.
    pub fn untouch(&mut self, fixture_id: &str) {
        self.touched.remove(fixture_id);
    }

    pub fn is_empty(&self) -> bool {
        self.touched.is_empty()
    }

    pub fn touched_ids(&self) -> Vec<String> {
        self.touched.iter().cloned().collect()
    }

    pub fn snapshot(&self) -> ProgrammerStatus {
        ProgrammerStatus {
            touched: self.touched_ids(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ProgrammerStatus {
    pub touched: Vec<String>,
}

pub type SharedProgrammer = Arc<Mutex<Programmer>>;

pub fn shared_programmer() -> SharedProgrammer {
    Arc::new(Mutex::new(Programmer::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touched_ids_dedupe_and_sort() {
        let mut p = Programmer::default();
        p.touch("zeta");
        p.touch("alpha");
        p.touch("zeta");
        p.touch("beta");
        assert_eq!(p.touched_ids(), vec!["alpha", "beta", "zeta"]);
    }

    #[test]
    fn clear_empties_the_set() {
        let mut p = Programmer::default();
        p.touch("a");
        p.touch("b");
        assert!(!p.is_empty());
        p.clear();
        assert!(p.is_empty());
    }

    #[test]
    fn untouch_drops_one_fixture() {
        let mut p = Programmer::default();
        p.touch("a");
        p.touch("b");
        p.untouch("a");
        assert_eq!(p.touched_ids(), vec!["b"]);
    }
}
