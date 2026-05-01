use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Hash)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum ChannelRole {
    Intensity,
    Red,
    Green,
    Blue,
    White,
    Amber,
    Uv,
    Cyan,
    Magenta,
    Yellow,
    Pan,
    PanFine,
    Tilt,
    TiltFine,
    ColorWheel,
    Gobo,
    Strobe,
    Zoom,
    Focus,
    Iris,
    Shutter,
    Other(String),
}

impl ChannelRole {
    /// Snake-case display label for the role. Standard variants return
    /// `"red"`, `"pan_fine"`, `"shutter"`, etc. — the same names the
    /// frontend uses. `Other("foo")` returns the inner `"foo"` so custom
    /// channels read meaningfully (`"function_speed"`, `"purple"`…). This
    /// is the canonical label used to match a role against user-supplied
    /// strings (e.g. blind's `channels_at_full` list).
    pub fn label(&self) -> &str {
        match self {
            ChannelRole::Intensity => "intensity",
            ChannelRole::Red => "red",
            ChannelRole::Green => "green",
            ChannelRole::Blue => "blue",
            ChannelRole::White => "white",
            ChannelRole::Amber => "amber",
            ChannelRole::Uv => "uv",
            ChannelRole::Cyan => "cyan",
            ChannelRole::Magenta => "magenta",
            ChannelRole::Yellow => "yellow",
            ChannelRole::Pan => "pan",
            ChannelRole::PanFine => "pan_fine",
            ChannelRole::Tilt => "tilt",
            ChannelRole::TiltFine => "tilt_fine",
            ChannelRole::ColorWheel => "color_wheel",
            ChannelRole::Gobo => "gobo",
            ChannelRole::Strobe => "strobe",
            ChannelRole::Zoom => "zoom",
            ChannelRole::Focus => "focus",
            ChannelRole::Iris => "iris",
            ChannelRole::Shutter => "shutter",
            ChannelRole::Other(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct ChannelDefinition {
    pub role: ChannelRole,
    #[serde(default)]
    pub default: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct PanTiltRange {
    /// 16-bit DMX value at the "negative" extreme of the normalised range.
    pub min: u16,
    /// 16-bit DMX value at the "positive" extreme of the normalised range.
    pub max: u16,
    /// Physical sweep covered by `min..=max`, used by the UI to label the
    /// preview. Not used in the DMX math itself (the math is pure-numeric).
    pub physical_degrees: f32,
}

impl PanTiltRange {
    /// Sensible defaults for a typical moving head: 540° pan / 270° tilt
    /// across the full 16-bit range. Used when a fixture mode does not
    /// declare its own ranges.
    pub fn default_pan() -> Self {
        Self {
            min: 0,
            max: u16::MAX,
            physical_degrees: 540.0,
        }
    }
    pub fn default_tilt() -> Self {
        Self {
            min: 0,
            max: u16::MAX,
            physical_degrees: 270.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct FixtureMode {
    pub name: String,
    pub channels: Vec<ChannelDefinition>,
    /// Physical pan range. `None` lets consumers pick a sensible default
    /// (`PanTiltRange::default_pan`). Once a fixture is declared with
    /// concrete ranges those win, so user-edited library JSON doesn't get
    /// overwritten by app-side defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan_range: Option<PanTiltRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tilt_range: Option<PanTiltRange>,
}

impl FixtureMode {
    pub fn channel_count(&self) -> u16 {
        self.channels.len() as u16
    }

    pub fn pan_range_or_default(&self) -> PanTiltRange {
        self.pan_range.clone().unwrap_or_else(PanTiltRange::default_pan)
    }

    pub fn tilt_range_or_default(&self) -> PanTiltRange {
        self.tilt_range.clone().unwrap_or_else(PanTiltRange::default_tilt)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct FixtureDefinition {
    pub id: String,
    pub manufacturer: String,
    pub name: String,
    pub modes: Vec<FixtureMode>,
    /// Optional path (relative to the library directory) of an image used to
    /// identify the fixture in the UI. `None` for definitions that ship without
    /// art; persisted alongside the JSON definition file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

impl FixtureDefinition {
    pub fn mode(&self, index: usize) -> Option<&FixtureMode> {
        self.modes.get(index)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct FixtureInstance {
    pub id: String,
    pub definition_id: String,
    pub mode_index: u32,
    pub universe: u16,
    /// 1-based DMX start address.
    pub address: u16,
    pub label: Option<String>,
    /// Position on the 2D stage grid in arbitrary units.
    pub position: [f32; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct PatchConflict {
    pub fixture_a: String,
    pub fixture_b: String,
    pub universe: u16,
    /// 1-based start of the overlap.
    pub overlap_start: u16,
    pub overlap_len: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct PatchProblem {
    pub fixture: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct PatchReport {
    pub conflicts: Vec<PatchConflict>,
    pub problems: Vec<PatchProblem>,
}

impl PatchReport {
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty() && self.problems.is_empty()
    }
}

/// Validate the placement of fixture instances against their definitions.
/// Detects three things:
///   1. Missing or unknown definition / mode reference.
///   2. Address out of universe bounds (would overflow channel 512).
///   3. Channel-range overlap with another fixture in the same universe.
pub fn validate_patch(
    instances: &[FixtureInstance],
    library: &HashMap<String, FixtureDefinition>,
) -> PatchReport {
    let mut report = PatchReport::default();

    // Collect resolved (id, universe, start_1based, len) for cross-comparison.
    let mut resolved: Vec<(String, u16, u16, u16)> = Vec::new();

    for inst in instances {
        if inst.address < 1 || inst.address > 512 {
            report.problems.push(PatchProblem {
                fixture: inst.id.clone(),
                message: format!("address {} out of range (1..=512)", inst.address),
            });
            continue;
        }
        let Some(def) = library.get(&inst.definition_id) else {
            report.problems.push(PatchProblem {
                fixture: inst.id.clone(),
                message: format!("unknown fixture definition {}", inst.definition_id),
            });
            continue;
        };
        let Some(mode) = def.mode(inst.mode_index as usize) else {
            report.problems.push(PatchProblem {
                fixture: inst.id.clone(),
                message: format!("unknown mode index {}", inst.mode_index),
            });
            continue;
        };
        let len = mode.channel_count();
        if len == 0 {
            report.problems.push(PatchProblem {
                fixture: inst.id.clone(),
                message: "fixture mode has no channels".to_string(),
            });
            continue;
        }
        let end = inst.address as u32 + len as u32 - 1;
        if end > 512 {
            report.problems.push(PatchProblem {
                fixture: inst.id.clone(),
                message: format!(
                    "{} channels at address {} overflows the universe (last channel {})",
                    len, inst.address, end
                ),
            });
            continue;
        }
        resolved.push((inst.id.clone(), inst.universe, inst.address, len));
    }

    for (i, a) in resolved.iter().enumerate() {
        for b in resolved.iter().skip(i + 1) {
            if a.1 != b.1 {
                continue;
            }
            let a_end = a.2 + a.3 - 1;
            let b_end = b.2 + b.3 - 1;
            let start = a.2.max(b.2);
            let end = a_end.min(b_end);
            if start <= end {
                report.conflicts.push(PatchConflict {
                    fixture_a: a.0.clone(),
                    fixture_b: b.0.clone(),
                    universe: a.1,
                    overlap_start: start,
                    overlap_len: end - start + 1,
                });
            }
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def_rgb() -> FixtureDefinition {
        FixtureDefinition {
            id: "rgb-3ch".into(),
            manufacturer: "Test".into(),
            name: "RGB".into(),
            image: None,
            modes: vec![FixtureMode {
                name: "3ch".into(),
                pan_range: None,
                tilt_range: None,
                channels: vec![
                    ChannelDefinition {
                        role: ChannelRole::Red,
                        default: 0,
                    },
                    ChannelDefinition {
                        role: ChannelRole::Green,
                        default: 0,
                    },
                    ChannelDefinition {
                        role: ChannelRole::Blue,
                        default: 0,
                    },
                ],
            }],
        }
    }

    fn library() -> HashMap<String, FixtureDefinition> {
        let mut m = HashMap::new();
        m.insert(def_rgb().id.clone(), def_rgb());
        m
    }

    fn inst(id: &str, universe: u16, address: u16) -> FixtureInstance {
        FixtureInstance {
            id: id.into(),
            definition_id: "rgb-3ch".into(),
            mode_index: 0,
            universe,
            address,
            label: None,
            position: [0.0, 0.0],
        }
    }

    #[test]
    fn no_overlap_clean() {
        let r = validate_patch(&[inst("a", 0, 1), inst("b", 0, 4)], &library());
        assert!(r.is_clean());
    }

    #[test]
    fn detects_overlap() {
        let r = validate_patch(&[inst("a", 0, 1), inst("b", 0, 2)], &library());
        assert_eq!(r.conflicts.len(), 1);
        let c = &r.conflicts[0];
        assert_eq!(c.universe, 0);
        assert_eq!(c.overlap_start, 2);
        assert_eq!(c.overlap_len, 2);
    }

    #[test]
    fn different_universes_dont_conflict() {
        let r = validate_patch(&[inst("a", 0, 1), inst("b", 1, 1)], &library());
        assert!(r.is_clean());
    }

    #[test]
    fn overflow_universe_is_problem() {
        let r = validate_patch(&[inst("a", 0, 511)], &library());
        assert_eq!(r.problems.len(), 1);
    }

    #[test]
    fn zero_address_is_problem() {
        let r = validate_patch(&[inst("a", 0, 0)], &library());
        assert_eq!(r.problems.len(), 1);
    }

    #[test]
    fn unknown_definition_is_problem() {
        let mut bad = inst("a", 0, 1);
        bad.definition_id = "nope".into();
        let r = validate_patch(&[bad], &library());
        assert_eq!(r.problems.len(), 1);
    }
}
