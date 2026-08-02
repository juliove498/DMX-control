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
    /// Fixtures affected by blackout, with the specific channel roles
    /// that should be driven to 0 when the button is engaged. Empty
    /// list = "auto mode": every patched fixture in the show gets the
    /// sensible default (intensity → 0, or RGB → 0 for fixtures without
    /// an intensity channel, plus strobe → 0). Pan/tilt/zoom etc. are
    /// never touched by auto mode — a moving head should stay parked
    /// where it was, just dark.
    #[serde(default)]
    pub fixtures: Vec<BlackoutFixture>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct BlackoutFixture {
    pub fixture_id: String,
    /// Role labels (`"intensity"`, `"red"`, `"strobe"`, …) that should
    /// be lerped to 0 when blackout is on. Empty list = auto behaviour
    /// for this fixture (intensity-or-RGB + strobe).
    #[serde(default)]
    pub channels_to_zero: Vec<String>,
}

impl Default for BlackoutConfig {
    fn default() -> Self {
        Self {
            active: false,
            fade_in_ms: 200,
            fade_out_ms: 800,
            fixtures: Vec::new(),
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

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct GlobalsConfig {
    #[serde(default)]
    pub blackout: BlackoutConfig,
    #[serde(default)]
    pub blind: BlindConfig,
    /// Which fixture channels the master fader scales. Default behaviour
    /// (`MasterConfig::default()` with `fixtures` empty) is "auto":
    /// scale only the intensity-or-RGB channel of every patched fixture,
    /// leaving pan/tilt/zoom/colour-wheel/etc. untouched. The per-
    /// fixture override lets the operator extend or narrow that set.
    #[serde(default)]
    pub master: MasterConfig,
    /// When `true`, every chaser and movement generator ignores its own
    /// configured `tempo` and runs at `overall_bpm` instead. Lets the
    /// operator drive the entire rig from one TAP button on the header
    /// without fiddling with each effect's BPM individually. The bpm
    /// value is preserved across toggles so disabling and re-enabling
    /// snaps back to the same tempo.
    #[serde(default)]
    pub overall_bpm_enabled: bool,
    #[serde(default = "default_overall_bpm")]
    pub overall_bpm: f32,
    /// Optional syncopated rhythm captured by the operator. When `Some`
    /// AND `overall_bpm_enabled` is true, every chaser ignores its own
    /// subdivision and advances one step per pattern hit instead — so a
    /// clave / cha-cha / son rhythm drives the rig directly. `None`
    /// leaves chasers on uniform subdivisions, which is the default.
    #[serde(default)]
    pub tempo_pattern: Option<TempoPattern>,
}

/// Quantised syncopated rhythm. `bars` × `steps_per_bar` defines a grid;
/// each entry in `hits` is a grid position (0 ≤ pos < bars * steps_per_bar)
/// where a "hit" should fire. The pattern loops every `bars * steps_per_bar`
/// grid units, scaled by the global BPM.
///
/// Quantising on sixteenth notes (`steps_per_bar = 16`) is the standard for
/// Afro-Cuban / Latin patterns: clave, son, rumba, cha-cha all sit on a 16-
/// step grid in 4/4. The data model also supports two-bar patterns (clave
/// 3-2 vs 2-3 is one bar, but two-bar phrases like clave-with-pickup show
/// up in some folk styles).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct TempoPattern {
    /// Number of 4/4 bars the pattern spans. Capped at 2 in the
    /// quantiser because longer phrases get hard to play by hand and the
    /// 16-step grid loses resolution past two bars.
    pub bars: u8,
    /// Resolution of the grid, in steps per bar. Always 16 today
    /// (sixteenth notes). Stored so the JSON is self-describing — a
    /// future "32nd-note" mode wouldn't break old shows.
    pub steps_per_bar: u8,
    /// Grid positions where a hit fires, sorted ascending and unique.
    /// Position 0 is always the first hit; subsequent positions are
    /// offsets in sixteenth-notes from there.
    pub hits: Vec<u8>,
}

impl Default for GlobalsConfig {
    fn default() -> Self {
        Self {
            blackout: BlackoutConfig::default(),
            blind: BlindConfig::default(),
            master: MasterConfig::default(),
            overall_bpm_enabled: false,
            overall_bpm: default_overall_bpm(),
            tempo_pattern: None,
        }
    }
}

fn default_overall_bpm() -> f32 {
    120.0
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct MasterConfig {
    /// Fixtures with custom master scaling. Empty = "auto mode" applies
    /// to every patched fixture. Auto mode means: scale intensity (or
    /// RGB if there's no intensity channel). Strobe is intentionally
    /// *not* included in auto — operators expect the master to dim the
    /// look, not slow strobes.
    #[serde(default)]
    pub fixtures: Vec<MasterFixture>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct MasterFixture {
    pub fixture_id: String,
    /// Role labels the master should scale on this fixture. Empty list
    /// = "use the auto channel set for THIS fixture" (intensity-or-RGB).
    /// Operators tick e.g. `["intensity", "white"]` to also dim the
    /// extra LED chips on an RGBWAP par.
    #[serde(default)]
    pub channels_to_scale: Vec<String>,
}
