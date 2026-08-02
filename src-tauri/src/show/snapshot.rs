//! Live-state snapshots ("modo grabar snapshot").
//!
//! A snapshot is a one-step capture of the *whole* rig at a moment in
//! time: the base DMX values of every universe, the grand master, the
//! chaser/movement that was running (and the chaser's level), the
//! active scene / loop group, blackout and the Overall BPM override.
//!
//! Activating a snapshot re-applies all of that wholesale; the runtime
//! (see `crate::snapshot`) first captures the same shape from the live
//! rig so deactivating restores the pre-activation state — the operator
//! gets an "as if nothing happened" round trip.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::scene::SceneFxState;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct SnapshotUniverse {
    pub id: u16,
    /// Base-layer DMX values (`Universe.data`) at capture time — manual
    /// writes plus whatever the active scene had landed, but *no*
    /// effects/blind/master/blackout overlays. Those layers are re-created
    /// on apply from the FX/global fields below, so capturing the merged
    /// output here would double-count them.
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct Snapshot {
    pub id: String,
    pub name: String,
    pub universes: Vec<SnapshotUniverse>,
    /// Grand master fader at capture time.
    pub master: u8,
    /// Chaser running at capture time (`Disabled` = none was on).
    #[serde(default)]
    pub chaser_state: SceneFxState,
    /// Movement generator running at capture time.
    #[serde(default)]
    pub movement_state: SceneFxState,
    /// Level (0.0..=1.0) of the enabled chaser at capture time, so a
    /// re-apply restores the operator's live fader position too.
    #[serde(default)]
    pub chaser_master: Option<f32>,
    /// Scene that was playing at capture time. Re-recalled on apply so
    /// multi-step scenes resume animating (from step 0).
    #[serde(default)]
    pub active_scene_id: Option<String>,
    /// Loop group that was driving at capture time. Takes precedence
    /// over `active_scene_id` on apply (the group recalls its own
    /// scenes).
    #[serde(default)]
    pub active_loop_group_id: Option<String>,
    #[serde(default)]
    pub blackout: bool,
    #[serde(default)]
    pub overall_bpm_enabled: bool,
    #[serde(default = "default_overall_bpm")]
    pub overall_bpm: f32,
}

fn default_overall_bpm() -> f32 {
    120.0
}
