//! Sequence loop groups (UI: "Listas de loop" / "Loop sequences").
//!
//! A SceneLoopGroup is an ordered playlist of entries — scenes and/or
//! whole-rig snapshots — that play in a cycle:
//! entry 1 → entry 2 → … → entry N → entry 1 → …
//!
//! Scenes are recalled normally (their own fade + FX context apply);
//! snapshots are applied values-only (base DMX + master + FX + globals)
//! without touching the loop driver or the snapshot toggle runtime.
//!
//! Dwell per entry, in priority order:
//! 1. `sync_to_bpm` + Overall BPM enabled → one `subdivision` worth of
//!    beats at the global tempo (musical timing).
//! 2. `hold_ms_override` > 0 → that fixed duration.
//! 3. The scene's natural cycle (sum of step fade+hold); snapshots have
//!    no natural duration and fall back to a 2 s default.
//!
//! Difference from a multi-step Scene:
//! - A Scene's "steps" share fixture targets within one named cue and
//!   keep an explicit FX context per step. Use steps when you're
//!   animating a single look.
//! - A loop group plays a *series* of distinct cues, each with its
//!   own programming, capture and FX state. Use groups when you want a
//!   playlist that rotates on its own — e.g. a 4-scene ambient
//!   sequence the operator triggers once at the top of a song.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::chaser::Subdivision;

/// One playlist slot: a scene recall or a snapshot apply.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoopEntry {
    Scene { id: String },
    Snapshot { id: String },
}

impl LoopEntry {
    pub fn id(&self) -> &str {
        match self {
            LoopEntry::Scene { id } | LoopEntry::Snapshot { id } => id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct SceneLoopGroup {
    pub id: String,
    pub name: String,
    /// Legacy scene-only list (pre-snapshot groups). Lifted into
    /// `entries` on load via [`SceneLoopGroup::migrate_legacy`] and
    /// never re-serialised afterwards.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scene_ids: Vec<String>,
    /// Playlist entries in playback order. The cycle loops back to
    /// index 0 after the last entry. Entries whose target no longer
    /// exists are silently skipped at playback time, so deleting a
    /// scene/snapshot doesn't strand the group.
    #[serde(default)]
    pub entries: Vec<LoopEntry>,
    /// Optional override of how long to dwell on each entry before
    /// advancing, in milliseconds. `0` (default) means "use the scene's
    /// own total cycle time" (or 2 s for snapshots). Ignored while
    /// `sync_to_bpm` is active with the Overall BPM enabled.
    #[serde(default)]
    pub hold_ms_override: u32,
    /// When true AND the Overall BPM override is on, each entry dwells
    /// exactly one `subdivision` worth of beats at the global tempo, so
    /// the playlist advances on the music.
    #[serde(default)]
    pub sync_to_bpm: bool,
    /// Musical length of one entry when `sync_to_bpm` drives the dwell.
    /// Expressed in the same quarter-note multiples the chasers use
    /// (`Four` = one 4/4 bar — the default a DJ expects).
    #[serde(default = "default_loop_subdivision")]
    pub subdivision: Subdivision,
}

fn default_loop_subdivision() -> Subdivision {
    Subdivision::Four
}

impl SceneLoopGroup {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            scene_ids: Vec::new(),
            entries: Vec::new(),
            hold_ms_override: 0,
            sync_to_bpm: false,
            subdivision: default_loop_subdivision(),
        }
    }

    /// Lift the legacy `scene_ids` list into typed `entries`. Idempotent;
    /// called by the show loader on every group.
    pub fn migrate_legacy(&mut self) {
        if self.entries.is_empty() && !self.scene_ids.is_empty() {
            self.entries = self
                .scene_ids
                .drain(..)
                .map(|id| LoopEntry::Scene { id })
                .collect();
        }
        // Entries are authoritative from here on; drop any legacy
        // stragglers so the two lists can't diverge silently.
        self.scene_ids.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_lifts_scene_ids_into_entries() {
        let mut g = SceneLoopGroup::new("g".into(), "x".into());
        g.scene_ids = vec!["a".into(), "b".into()];
        g.migrate_legacy();
        assert!(g.scene_ids.is_empty());
        assert_eq!(
            g.entries,
            vec![
                LoopEntry::Scene { id: "a".into() },
                LoopEntry::Scene { id: "b".into() }
            ]
        );
    }

    #[test]
    fn migrate_keeps_existing_entries_authoritative() {
        let mut g = SceneLoopGroup::new("g".into(), "x".into());
        g.entries = vec![LoopEntry::Snapshot { id: "s".into() }];
        g.scene_ids = vec!["stale".into()];
        g.migrate_legacy();
        assert_eq!(g.entries.len(), 1);
        assert!(g.scene_ids.is_empty());
    }
}
