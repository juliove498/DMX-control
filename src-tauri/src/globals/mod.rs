//! "Global" buttons that aren't tied to a single page: Blackout and Blind.
//!
//! Both share an interpolating runtime (separate fade-in / fade-out times
//! per direction) so a punch on the button reads physically — fast in, slow
//! out is the classic concert blinder feel.
//!
//! - **Blackout**: latching toggle. Persisted target (`active`) so reopening
//!   a show with blackout left on resumes blacked-out.
//! - **Blind**: momentary hold. The pressed state is *not* persisted — it
//!   only exists while the user holds the button. Halogen-blinder colour
//!   ramp written to a configured fixture set.

pub mod runtime;

use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct BlackoutConfig {
    /// Target state. Persisted so a show that ships with blackout still
    /// boots blacked-out the next time the operator opens it.
    pub active: bool,
    /// Fade-to-black time when the user toggles blackout *on*. ms.
    pub fade_in_ms: u32,
    /// Fade-from-black time when the user toggles blackout *off*. ms.
    pub fade_out_ms: u32,
}

impl Default for BlackoutConfig {
    fn default() -> Self {
        Self {
            active: false,
            fade_in_ms: 200,
            fade_out_ms: 800,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct BlindFixture {
    pub fixture_id: String,
    /// Optional list of role names the blind should drive to 255 when the
    /// button is held. Standard roles (`"intensity"`, `"red"`, `"strobe"`,
    /// …) plus the inner string of any `Other("foo")` channel — same
    /// labels the UI shows next to each slider. Empty means: fall back to
    /// the default halogen behaviour (warm-white on intensity + RGB).
    #[serde(default)]
    pub channels_at_full: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct BlindConfig {
    /// Time to ramp up while the button is held. Short by default —
    /// halogens turn on fast.
    pub fade_in_ms: u32,
    /// Cool-down time after the button is released. Long by default —
    /// halogens take ages to fade out.
    pub fade_out_ms: u32,
    /// Fixtures affected by blind. Each entry can list channel role names
    /// to drive at 255 when pressed; if no roles are listed we use the
    /// default halogen warm-white behaviour. Empty list = blind does
    /// nothing (safe default before the user has configured anything).
    #[serde(default, deserialize_with = "deserialize_blind_fixtures")]
    pub fixtures: Vec<BlindFixture>,
}

/// Accept both the legacy `["fixture-id", …]` and the new
/// `[{ "fixture_id": "…", "channels_at_full": [...] }, …]` shapes when
/// deserialising the blind fixtures list. Legacy entries get the default
/// halogen behaviour by leaving `channels_at_full` empty, so old shows
/// keep working without a migration step.
fn deserialize_blind_fixtures<'de, D>(deserializer: D) -> Result<Vec<BlindFixture>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Legacy(String),
        New(BlindFixture),
    }
    let raw: Vec<Entry> = Vec::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .map(|e| match e {
            Entry::Legacy(fixture_id) => BlindFixture {
                fixture_id,
                channels_at_full: Vec::new(),
            },
            Entry::New(b) => b,
        })
        .collect())
}

impl Default for BlindConfig {
    fn default() -> Self {
        Self {
            fade_in_ms: 80,
            fade_out_ms: 1500,
            fixtures: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct GlobalsConfig {
    #[serde(default)]
    pub blackout: BlackoutConfig,
    #[serde(default)]
    pub blind: BlindConfig,
}
