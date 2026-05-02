//! Scenes: looping multi-step snapshots that can be recalled with a
//! per-step fade and that optionally capture the chaser/movement state
//! that was running at record time.
//!
//! Iteration 3 scope:
//! - **Multi-step**: each scene holds an ordered list of `SceneStep`s.
//!   While the scene is active, playback fades into a step, holds for
//!   the configured time, then fades into the next step. After the last
//!   step, playback loops back to step 0. Single-step scenes (the
//!   common cue-style use) are just a list of one with `hold_ms = 0`.
//! - **FX capture**: a scene records "what was on" — the active chaser
//!   and movement at record time — and re-applies them on recall, so
//!   the operator gets the full look back, not just the fixture values.
//!
//! Backward compat: pre-iteration-3 saves stored `fade_in_ms` + `fixtures`
//! directly on `Scene`. Those legacy fields are still accepted on
//! deserialise (kept here as `#[serde(default)]` fields with
//! `skip_serializing_if`) and lifted into a single step on load via
//! [`Scene::migrate_legacy`]. Future saves never re-emit them.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct SceneChannel {
    pub channel_offset: u16,
    pub value: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct SceneFixture {
    pub fixture_id: String,
    pub values: Vec<SceneChannel>,
}

/// One frame of a multi-step scene. Holds for `hold_ms` after fading in
/// over `fade_in_ms`. The hold portion is what makes a step distinct
/// from the next — without hold, two steps with similar values would
/// just look like a long fade.
///
/// Each step carries its own FX context (chaser/movement) captured at
/// record time. When the playback transitions into this step, the
/// engine applies that context (toggling chasers/movement to match).
/// `Inherit` means "leave whatever's running alone" — useful when most
/// steps share the same chaser and you only want one or two steps to
/// switch it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct SceneStep {
    pub id: String,
    /// Optional human label. UI defaults to "Step N" when empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub fade_in_ms: u32,
    /// How long to sit on the step's target after the fade finishes.
    /// `0` = transition straight into the next step's fade. Single-step
    /// scenes effectively ignore this.
    #[serde(default)]
    pub hold_ms: u32,
    pub fixtures: Vec<SceneFixture>,
    #[serde(default)]
    pub chaser_state: SceneFxState,
    #[serde(default)]
    pub movement_state: SceneFxState,
}

/// Per-layer FX state baked into a scene. Captured at record time,
/// re-applied on recall, mirrors the user's mental model of "the look
/// I saved" being the full rig context, not just the dimmer/colour
/// snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
#[ts(export, export_to = "../bindings/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SceneFxState {
    /// Recall doesn't manage this layer (chasers / movement / …) at
    /// all; whatever was running keeps running.
    #[default]
    Inherit,
    /// Recall disables every entry in the layer (e.g. all chasers off).
    Disabled,
    /// Recall enables this id, implicitly disabling its siblings since
    /// chasers and movements are exclusive in this engine.
    Enabled { id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct Scene {
    pub id: String,
    pub name: String,
    /// Ordered list of frames. Playback loops back to 0 after the last.
    /// Empty list = scene is a placeholder; recall does nothing visible.
    #[serde(default)]
    pub steps: Vec<SceneStep>,
    /// Legacy: pre-iteration-4 scenes carried a single FX state at the
    /// scene level. Migrated into `steps[0]` on load via `migrate_legacy`
    /// and never re-emitted. The skip-if-inherit guard keeps fresh saves
    /// clean.
    #[serde(default, skip_serializing_if = "is_inherit_fx")]
    pub chaser_state: SceneFxState,
    #[serde(default, skip_serializing_if = "is_inherit_fx")]
    pub movement_state: SceneFxState,

    // ---- Legacy fields (pre-iteration-3) ---------------------------------
    /// Legacy: pre-multi-step scenes stored a single fade time at the
    /// scene level. Lifted into `steps[0].fade_in_ms` on load via
    /// `migrate_legacy`; never re-serialised after migration.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub fade_in_ms: u32,
    /// Legacy direct fixtures list. Same lift-and-clear semantics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixtures: Vec<SceneFixture>,
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

fn is_inherit_fx(v: &SceneFxState) -> bool {
    matches!(v, SceneFxState::Inherit)
}

impl Scene {
    pub fn default_with_id(id: String, name: String) -> Self {
        Self {
            id,
            name,
            steps: Vec::new(),
            chaser_state: SceneFxState::Inherit,
            movement_state: SceneFxState::Inherit,
            fade_in_ms: 0,
            fixtures: Vec::new(),
        }
    }

    /// Lift legacy single-step shape into `steps`, then lift legacy
    /// scene-level FX state into `steps[0]`. Idempotent. Called by the
    /// show file loader on every scene.
    pub fn migrate_legacy(&mut self) {
        // Step 1: legacy fade_in_ms + fixtures → first step.
        if self.steps.is_empty() && (!self.fixtures.is_empty() || self.fade_in_ms != 0) {
            let step = SceneStep {
                id: uuid::Uuid::new_v4().to_string(),
                name: None,
                fade_in_ms: self.fade_in_ms,
                hold_ms: 0,
                fixtures: std::mem::take(&mut self.fixtures),
                chaser_state: SceneFxState::Inherit,
                movement_state: SceneFxState::Inherit,
            };
            self.fade_in_ms = 0;
            self.steps.push(step);
        }
        // Already-migrated stragglers: keep `steps` as authoritative,
        // drop the legacy primitives so they can't diverge silently.
        self.fade_in_ms = 0;
        self.fixtures.clear();

        // Step 2: legacy scene-level FX → first step's FX. Only if the
        // first step is still on Inherit (the default for fresh
        // migrations); never overwrite an explicit per-step setting.
        if let Some(first) = self.steps.first_mut() {
            if !matches!(self.chaser_state, SceneFxState::Inherit)
                && matches!(first.chaser_state, SceneFxState::Inherit)
            {
                first.chaser_state = std::mem::take(&mut self.chaser_state);
            }
            if !matches!(self.movement_state, SceneFxState::Inherit)
                && matches!(first.movement_state, SceneFxState::Inherit)
            {
                first.movement_state = std::mem::take(&mut self.movement_state);
            }
        }
        // Either migration already happened or there was nothing there;
        // either way reset the scene-level slots so `skip_serializing_if`
        // keeps them out of new saves.
        self.chaser_state = SceneFxState::Inherit;
        self.movement_state = SceneFxState::Inherit;
    }
}

impl SceneStep {
    pub fn new(fade_in_ms: u32, fixtures: Vec<SceneFixture>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: None,
            fade_in_ms,
            hold_ms: 0,
            fixtures,
            chaser_state: SceneFxState::Inherit,
            movement_state: SceneFxState::Inherit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_legacy_lifts_single_step() {
        let mut s = Scene {
            id: "s1".into(),
            name: "Cue 1".into(),
            steps: Vec::new(),
            chaser_state: SceneFxState::Inherit,
            movement_state: SceneFxState::Inherit,
            fade_in_ms: 800,
            fixtures: vec![SceneFixture {
                fixture_id: "f1".into(),
                values: vec![SceneChannel {
                    channel_offset: 0,
                    value: 200,
                }],
            }],
        };
        s.migrate_legacy();
        assert_eq!(s.steps.len(), 1);
        assert_eq!(s.steps[0].fade_in_ms, 800);
        assert_eq!(s.steps[0].fixtures.len(), 1);
        assert_eq!(s.fade_in_ms, 0);
        assert!(s.fixtures.is_empty());
    }

    #[test]
    fn migrate_legacy_is_noop_when_already_migrated() {
        let mut s = Scene {
            id: "s1".into(),
            name: "Cue 1".into(),
            steps: vec![SceneStep::new(500, vec![])],
            chaser_state: SceneFxState::Inherit,
            movement_state: SceneFxState::Inherit,
            fade_in_ms: 0,
            fixtures: Vec::new(),
        };
        s.migrate_legacy();
        assert_eq!(s.steps.len(), 1);
        assert_eq!(s.steps[0].fade_in_ms, 500);
    }

    #[test]
    fn migrate_legacy_clears_orphan_legacy_fields() {
        // A buggy save could leave both `steps` AND legacy fields set.
        // Migration should keep `steps` as authoritative and drop the
        // legacy bits so they don't diverge silently on the next save.
        let mut s = Scene {
            id: "s1".into(),
            name: "x".into(),
            steps: vec![SceneStep::new(200, vec![])],
            chaser_state: SceneFxState::Inherit,
            movement_state: SceneFxState::Inherit,
            fade_in_ms: 999,
            fixtures: vec![SceneFixture {
                fixture_id: "stale".into(),
                values: vec![],
            }],
        };
        s.migrate_legacy();
        assert_eq!(s.fade_in_ms, 0);
        assert!(s.fixtures.is_empty());
        assert_eq!(s.steps.len(), 1);
        assert_eq!(s.steps[0].fade_in_ms, 200);
    }
}
