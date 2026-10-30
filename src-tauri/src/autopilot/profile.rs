//! What each participating fixture *is*, derived from the library's channel
//! roles: a colour wash, a moving head, a strobe… plus the absolute DMX
//! offsets the generator writes to. Resolved once per show-context change
//! so the per-frame evaluation is pure arithmetic.

use std::collections::HashMap;

use serde::Serialize;
use ts_rs::TS;

use crate::show::fixture::{ChannelRole, FixtureDefinition, FixtureInstance, PanTiltRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum FixtureKind {
    /// Colour/intensity wash (PAR, bar, blinder with colour…).
    Par,
    /// Has pan/tilt.
    Mover,
    /// Strobe-only device (no colour, no pan/tilt, a strobe channel).
    Strobe,
    /// Something we can only dim (or nothing we understand).
    Other,
}

/// One participating fixture with its resolved channel map. Offsets are
/// absolute 0-based channel indexes inside `universe`.
#[derive(Debug, Clone)]
pub struct FixtureProfile {
    pub id: String,
    pub label: String,
    pub kind: FixtureKind,
    pub universe: u16,
    pub dimmer: Option<usize>,
    pub red: Option<usize>,
    pub green: Option<usize>,
    pub blue: Option<usize>,
    pub white: Option<usize>,
    pub amber: Option<usize>,
    pub uv: Option<usize>,
    pub strobe: Option<usize>,
    pub shutter: Option<usize>,
    pub pan: Option<usize>,
    pub pan_fine: Option<usize>,
    pub tilt: Option<usize>,
    pub tilt_fine: Option<usize>,
    pub pan_range: PanTiltRange,
    pub tilt_range: PanTiltRange,
    /// Stage position from the patch (normalised layout coords).
    pub position: [f32; 2],
    /// Left-to-right rank among the participants (0 = leftmost), used by
    /// spatial programs (sweeps, chases).
    pub index: usize,
}

impl FixtureProfile {
    pub fn has_color(&self) -> bool {
        self.red.is_some() && self.green.is_some() && self.blue.is_some()
    }
    pub fn is_mover(&self) -> bool {
        self.pan.is_some() || self.tilt.is_some()
    }
}

/// Lightweight description for the UI's "who takes part" picker.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct RigFixture {
    pub id: String,
    pub label: String,
    pub kind: FixtureKind,
    pub has_color: bool,
    pub has_dimmer: bool,
    pub has_strobe: bool,
}

fn role_offset<'a, I>(iter: I, role: &ChannelRole) -> Option<usize>
where
    I: Iterator<Item = &'a ChannelRole>,
{
    iter.enumerate().find(|(_, r)| *r == role).map(|(i, _)| i)
}

fn label_of(f: &FixtureInstance, def: &FixtureDefinition) -> String {
    match &f.label {
        Some(l) if !l.trim().is_empty() => l.clone(),
        _ => format!("{} @{}", def.name, f.address),
    }
}

fn classify(p: &FixtureProfile) -> FixtureKind {
    if p.is_mover() {
        FixtureKind::Mover
    } else if p.has_color() || p.dimmer.is_some() && (p.white.is_some() || p.amber.is_some()) {
        FixtureKind::Par
    } else if p.strobe.is_some() && p.dimmer.is_none() {
        FixtureKind::Strobe
    } else if p.dimmer.is_some() {
        FixtureKind::Par
    } else {
        FixtureKind::Other
    }
}

fn resolve_one(
    f: &FixtureInstance,
    library: &HashMap<String, FixtureDefinition>,
) -> Option<FixtureProfile> {
    let def = library.get(&f.definition_id)?;
    let mode = def.modes.get(f.mode_index as usize)?;
    let base = (f.address as usize).saturating_sub(1);
    let off = |role: ChannelRole| {
        role_offset(mode.channels.iter().map(|c| &c.role), &role).map(|i| base + i)
    };
    let mut p = FixtureProfile {
        id: f.id.clone(),
        label: label_of(f, def),
        kind: FixtureKind::Other,
        universe: f.universe,
        dimmer: off(ChannelRole::Intensity),
        red: off(ChannelRole::Red),
        green: off(ChannelRole::Green),
        blue: off(ChannelRole::Blue),
        white: off(ChannelRole::White),
        amber: off(ChannelRole::Amber),
        uv: off(ChannelRole::Uv),
        strobe: off(ChannelRole::Strobe),
        shutter: off(ChannelRole::Shutter),
        pan: off(ChannelRole::Pan),
        pan_fine: off(ChannelRole::PanFine),
        tilt: off(ChannelRole::Tilt),
        tilt_fine: off(ChannelRole::TiltFine),
        pan_range: mode.pan_range_or_default(),
        tilt_range: mode.tilt_range_or_default(),
        position: f.position,
        index: 0,
    };
    p.kind = classify(&p);
    Some(p)
}

/// Profiles for the chosen participants, ranked left-to-right by stage x.
pub fn resolve(
    fixtures: &[FixtureInstance],
    library: &HashMap<String, FixtureDefinition>,
    ids: &[String],
) -> Vec<FixtureProfile> {
    let mut out: Vec<FixtureProfile> = fixtures
        .iter()
        .filter(|f| f.in_use && ids.iter().any(|id| id == &f.id))
        .filter_map(|f| resolve_one(f, library))
        .collect();
    out.sort_by(|a, b| {
        a.position[0]
            .partial_cmp(&b.position[0])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    for (i, p) in out.iter_mut().enumerate() {
        p.index = i;
    }
    out
}

/// Every patched fixture, classified, for the participant picker.
pub fn rig_summary(
    fixtures: &[FixtureInstance],
    library: &HashMap<String, FixtureDefinition>,
) -> Vec<RigFixture> {
    fixtures
        .iter()
        .filter(|f| f.in_use)
        .filter_map(|f| resolve_one(f, library))
        .map(|p| RigFixture {
            id: p.id.clone(),
            label: p.label.clone(),
            kind: p.kind,
            has_color: p.has_color(),
            has_dimmer: p.dimmer.is_some(),
            has_strobe: p.strobe.is_some(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::show::fixture::{ChannelDefinition, FixtureMode};

    fn def(id: &str, roles: Vec<ChannelRole>) -> FixtureDefinition {
        FixtureDefinition {
            id: id.into(),
            manufacturer: "t".into(),
            name: id.into(),
            modes: vec![FixtureMode {
                name: "m".into(),
                channels: roles
                    .into_iter()
                    .map(|r| ChannelDefinition::new(r, 0))
                    .collect(),
                pan_range: None,
                tilt_range: None,
                presets: vec![],
            }],
            image: None,
        }
    }

    fn inst(id: &str, def: &str, addr: u16, x: f32) -> FixtureInstance {
        FixtureInstance {
            id: id.into(),
            definition_id: def.into(),
            mode_index: 0,
            universe: 0,
            address: addr,
            label: None,
            position: [x, 0.0],
            in_use: true,
        }
    }

    #[test]
    fn classifies_and_ranks_left_to_right() {
        let mut lib = HashMap::new();
        lib.insert(
            "par".to_string(),
            def(
                "par",
                vec![
                    ChannelRole::Intensity,
                    ChannelRole::Red,
                    ChannelRole::Green,
                    ChannelRole::Blue,
                ],
            ),
        );
        lib.insert(
            "head".to_string(),
            def(
                "head",
                vec![ChannelRole::Pan, ChannelRole::Tilt, ChannelRole::Intensity],
            ),
        );
        lib.insert(
            "strobe".to_string(),
            def("strobe", vec![ChannelRole::Strobe]),
        );
        let fixtures = vec![
            inst("b", "par", 10, 0.8),
            inst("a", "par", 1, 0.1),
            inst("h", "head", 20, 0.5),
            inst("s", "strobe", 40, 0.9),
        ];
        let ids: Vec<String> = ["a", "b", "h", "s"].iter().map(|s| s.to_string()).collect();
        let profiles = resolve(&fixtures, &lib, &ids);
        let order: Vec<&str> = profiles.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(order, vec!["a", "h", "b", "s"]);
        assert_eq!(profiles[0].kind, FixtureKind::Par);
        assert_eq!(profiles[1].kind, FixtureKind::Mover);
        assert_eq!(profiles[3].kind, FixtureKind::Strobe);
        // Absolute offsets: fixture "b" at address 10 → red is channel 10 (0-based).
        assert_eq!(profiles[2].red, Some(10));
        assert_eq!(profiles[2].dimmer, Some(9));
    }
}
