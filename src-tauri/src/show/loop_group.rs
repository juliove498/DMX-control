//! Sequence loop groups (UI: "Listas de loop" / "Loop sequences").
//!
//! A SceneLoopGroup is an ordered list of scene IDs that play in a
//! cycle: scene 1 → scene 2 → … → scene N → scene 1 → …
//!
//! Each scene is recalled normally (its own fade + FX context apply),
//! held for either a per-scene hold time or — when the group itself
//! sets `hold_ms_override` to non-zero — that override duration. After
//! the hold expires, the next scene is recalled.
//!
//! Difference from a multi-step Scene:
//! - A Scene's "steps" share fixture targets within one named cue and
//!   keep an explicit FX context per step. Use steps when you're
//!   animating a single look.
//! - A loop group plays a *series* of distinct scenes, each with its
//!   own programming, capture and FX state. Use groups when you want a
//!   playlist of cues that rotates on its own — e.g. a 4-scene ambient
//!   sequence the operator triggers once at the top of a song.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct SceneLoopGroup {
    pub id: String,
    pub name: String,
    /// Scene IDs in playback order. The cycle loops back to index 0
    /// after the last entry. Entries that point at scenes that no
    /// longer exist are silently skipped at playback time so deleting
    /// a scene doesn't strand the group.
    #[serde(default)]
    pub scene_ids: Vec<String>,
    /// Optional override of how long to dwell on each scene before
    /// advancing, in milliseconds. `0` (default) means "use the scene's
    /// own total cycle time" — sum of all of its steps' fade + hold —
    /// which is the natural choice for hand-crafted multi-step scenes.
    /// Set to a positive value when every scene in the group should
    /// hold for the same duration regardless of how it was recorded.
    #[serde(default)]
    pub hold_ms_override: u32,
}

impl SceneLoopGroup {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            scene_ids: Vec::new(),
            hold_ms_override: 0,
        }
    }
}
