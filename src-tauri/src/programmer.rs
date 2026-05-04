//! Programmer: tracks which fixtures (and which of their channels) the
//! operator has edited since the last Clear/Record.
//!
//! Phase 4 iteration 2 scope: instead of a full Avolites-style override
//! layer (which would mean rerouting every manual write away from
//! `Universe.data`), we track what the user has touched. That's enough
//! to power the two flows the operator actually wants:
//!
//! - **Record from touched** — capture only the fixtures with active
//!   edits into a new scene. No more scrolling through 30 chips to
//!   pick the 4 you just dialed.
//! - **Update scene** — refresh an existing scene with the current
//!   state of its fixtures (or just the touched ones), so the
//!   "tweak the recall and re-save" loop is one click.
//!
//! Per-channel resolution (added in iteration 3) lets the UI tell the
//! operator *what* they touched on each fixture (e.g. "Pan, Tilt"
//! versus "Dimmer"), not just the fixture count. The recording flows
//! still operate at fixture granularity — capturing a touched fixture
//! grabs all its channels, same as before.
//!
//! The full-blown LTP/HTP merge layer is parked for a future iteration.
//! Until then, manual writes still go straight to `Universe.data` (so
//! the engine snapshot stays correct without architectural surgery)
//! and Clear is purely a marker reset.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Default)]
pub struct Programmer {
    /// Per-fixture set of channel offsets the user has edited since the
    /// last Clear or Record. `BTreeMap`/`BTreeSet` keep iteration order
    /// stable for the UI without a separate sort step.
    touched: BTreeMap<String, BTreeSet<u16>>,
}

impl Programmer {
    /// Mark a specific channel of a fixture as touched. Idempotent.
    pub fn touch(&mut self, fixture_id: impl Into<String>, channel_offset: u16) {
        self.touched
            .entry(fixture_id.into())
            .or_default()
            .insert(channel_offset);
    }

    pub fn clear(&mut self) {
        self.touched.clear();
    }

    /// Stop tracking a fixture entirely (e.g. when it's removed from the
    /// patch). Doesn't reset its DMX values — those belong to the
    /// engine; we only own the marker.
    pub fn untouch(&mut self, fixture_id: &str) {
        self.touched.remove(fixture_id);
    }

    pub fn is_empty(&self) -> bool {
        self.touched.is_empty()
    }

    /// Fixture IDs with at least one touched channel. Used by the
    /// recording flows that still capture at fixture granularity.
    pub fn touched_ids(&self) -> Vec<String> {
        self.touched.keys().cloned().collect()
    }

    pub fn snapshot(&self) -> ProgrammerStatus {
        let channels: Vec<TouchedFixture> = self
            .touched
            .iter()
            .map(|(fixture_id, offsets)| TouchedFixture {
                fixture_id: fixture_id.clone(),
                channels: offsets.iter().copied().collect(),
            })
            .collect();
        ProgrammerStatus {
            touched: self.touched.keys().cloned().collect(),
            channels,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct TouchedFixture {
    pub fixture_id: String,
    /// Channel offsets within the fixture mode (0-based) that the
    /// operator has written to since the last Clear/Record.
    pub channels: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ProgrammerStatus {
    /// IDs of touched fixtures. Kept as a flat list for the existing
    /// "fixture count" widgets that were written before per-channel
    /// tracking landed.
    pub touched: Vec<String>,
    /// Per-fixture detail: which channels are touched on each. Empty
    /// when nothing has been touched. The UI uses this to highlight
    /// just the dialed-in channels on the stage canvas.
    pub channels: Vec<TouchedFixture>,
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
        p.touch("zeta", 0);
        p.touch("alpha", 1);
        p.touch("zeta", 2);
        p.touch("beta", 0);
        assert_eq!(p.touched_ids(), vec!["alpha", "beta", "zeta"]);
    }

    #[test]
    fn touch_records_per_channel() {
        let mut p = Programmer::default();
        p.touch("a", 0);
        p.touch("a", 3);
        p.touch("a", 0); // dedupe
        let snap = p.snapshot();
        assert_eq!(snap.touched, vec!["a"]);
        assert_eq!(snap.channels.len(), 1);
        assert_eq!(snap.channels[0].fixture_id, "a");
        assert_eq!(snap.channels[0].channels, vec![0, 3]);
    }

    #[test]
    fn clear_empties_the_set() {
        let mut p = Programmer::default();
        p.touch("a", 0);
        p.touch("b", 1);
        assert!(!p.is_empty());
        p.clear();
        assert!(p.is_empty());
        assert_eq!(p.snapshot().channels.len(), 0);
    }

    #[test]
    fn untouch_drops_one_fixture() {
        let mut p = Programmer::default();
        p.touch("a", 0);
        p.touch("a", 1);
        p.touch("b", 0);
        p.untouch("a");
        assert_eq!(p.touched_ids(), vec!["b"]);
        assert_eq!(p.snapshot().channels.len(), 1);
    }
}
