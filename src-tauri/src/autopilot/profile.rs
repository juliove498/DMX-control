//! What each participating fixture *is*, derived from the library's channel
//! roles: a colour wash, a moving head, a strobe… plus the absolute DMX
//! offsets the generator writes to. Resolved once per show-context change
//! so the per-frame evaluation is pure arithmetic.

use std::collections::HashMap;

use serde::Serialize;
use ts_rs::TS;

use crate::show::fixture::{
    ChannelDefinition, ChannelRange, ChannelRole, FixtureDefinition, FixtureInstance, FixtureMode,
    PanTiltRange,
};

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
    /// Colour wheel (heads without RGB mixing): the palette colour snaps
    /// to the nearest slot.
    pub color_wheel: Option<Wheel>,
    /// Gobo wheel: static gobos only (rotation/shake ranges are skipped).
    pub gobo: Option<Wheel>,
    /// Prisms (one or two), with their rotation channel when there is one.
    pub prisms: Vec<Prism>,
}

/// A labelled slot on a wheel channel (middle of its DMX range).
#[derive(Debug, Clone, PartialEq)]
pub struct WheelSlot {
    pub value: u8,
    pub label: String,
    /// Nominal colour of a colour-wheel slot, read from its label.
    pub rgb: Option<[u8; 3]>,
}

/// A wheel channel: where open/white sits and the slots worth using.
#[derive(Debug, Clone, PartialEq)]
pub struct Wheel {
    pub channel: usize,
    pub open: u8,
    pub slots: Vec<WheelSlot>,
}

/// A prism channel with its out/in values and an optional rotation.
#[derive(Debug, Clone, PartialEq)]
pub struct Prism {
    pub channel: usize,
    pub off: u8,
    pub on: u8,
    /// (channel, value for a slow rotation).
    pub rot: Option<(usize, u8)>,
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
        color_wheel: wheel_at(mode, base, &ChannelRole::ColorWheel)
            .and_then(|(ch, def)| colour_wheel(ch, def)),
        gobo: wheel_at(mode, base, &ChannelRole::Gobo).and_then(|(ch, def)| gobo_wheel(ch, def)),
        prisms: prisms_of(mode, base),
    };
    p.kind = classify(&p);
    Some(p)
}

// ---- Wheels and prisms --------------------------------------------------

fn wheel_at<'a>(
    mode: &'a FixtureMode,
    base: usize,
    role: &ChannelRole,
) -> Option<(usize, &'a ChannelDefinition)> {
    mode.channels
        .iter()
        .enumerate()
        .find(|(_, c)| &c.role == role)
        .map(|(i, c)| (base + i, c))
}

fn mid(r: &ChannelRange) -> u8 {
    ((r.from as u16 + r.to as u16) / 2) as u8
}

/// Ranges that are effects (rotation, rainbow, shake, split colours,
/// macros) rather than a static slot.
fn is_effect_label(lower: &str) -> bool {
    if lower.contains('/') || lower.contains('-') {
        return true;
    }
    const FX: [&str; 17] = [
        "rot",
        "rotation",
        "rotacion",
        "rotación",
        "cw",
        "ccw",
        "rainbow",
        "arco",
        "iris",
        "shake",
        "macro",
        "mitad",
        "half",
        "split",
        "misc",
        "speed",
        "stop",
    ];
    lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .any(|t| FX.contains(&t))
}

/// Nominal RGB for a colour-wheel slot label, in Spanish or English.
/// Specific names come before generic ones ("light blue" before "blue").
fn colour_from_label(lower: &str) -> Option<[u8; 3]> {
    const TABLE: &[(&str, [u8; 3])] = &[
        ("light blue", [90, 170, 255]),
        ("azul claro", [90, 170, 255]),
        ("celeste", [90, 170, 255]),
        ("sky", [90, 170, 255]),
        ("dark blue", [0, 0, 220]),
        ("azul oscuro", [0, 0, 220]),
        ("deep blue", [0, 0, 220]),
        ("congo", [40, 0, 200]),
        ("light green", [120, 255, 120]),
        ("verde claro", [120, 255, 120]),
        ("amarillo claro", [255, 255, 120]),
        ("light yellow", [255, 255, 120]),
        ("white", [255, 255, 255]),
        ("blanco", [255, 255, 255]),
        ("open", [255, 255, 255]),
        ("abierto", [255, 255, 255]),
        ("red", [255, 0, 0]),
        ("rojo", [255, 0, 0]),
        ("green", [0, 255, 0]),
        ("verde", [0, 255, 0]),
        ("blue", [0, 0, 255]),
        ("azul", [0, 0, 255]),
        ("yellow", [255, 255, 0]),
        ("amarillo", [255, 255, 0]),
        ("orange", [255, 120, 0]),
        ("naranja", [255, 120, 0]),
        ("magenta", [255, 0, 255]),
        ("pink", [255, 80, 180]),
        ("rosa", [255, 80, 180]),
        ("cyan", [0, 255, 255]),
        ("cian", [0, 255, 255]),
        ("violet", [150, 0, 255]),
        ("violeta", [150, 0, 255]),
        ("purple", [150, 0, 255]),
        ("purpura", [150, 0, 255]),
        ("púrpura", [150, 0, 255]),
        ("lila", [190, 120, 255]),
        ("lavanda", [190, 120, 255]),
        ("lavender", [190, 120, 255]),
        ("uv", [120, 0, 255]),
        ("amber", [255, 170, 0]),
        ("ambar", [255, 170, 0]),
        ("ámbar", [255, 170, 0]),
        ("cto", [255, 200, 140]),
        ("ctb", [200, 220, 255]),
    ];
    TABLE
        .iter()
        .find(|(k, _)| lower.contains(k))
        .map(|(_, c)| *c)
}

fn colour_wheel(ch: usize, def: &ChannelDefinition) -> Option<Wheel> {
    let mut slots = Vec::new();
    let mut open: Option<u8> = None;
    for r in &def.ranges {
        let l = r.label.to_lowercase();
        if is_effect_label(&l) {
            continue;
        }
        let Some(rgb) = colour_from_label(&l) else {
            continue;
        };
        let v = mid(r);
        if rgb == [255, 255, 255] {
            open.get_or_insert(v);
        }
        slots.push(WheelSlot {
            value: v,
            label: r.label.clone(),
            rgb: Some(rgb),
        });
    }
    (!slots.is_empty()).then(|| Wheel {
        channel: ch,
        open: open.unwrap_or(0),
        slots,
    })
}

fn gobo_wheel(ch: usize, def: &ChannelDefinition) -> Option<Wheel> {
    let mut slots = Vec::new();
    let mut open: Option<u8> = None;
    for r in &def.ranges {
        let l = r.label.to_lowercase();
        if is_effect_label(&l) {
            continue;
        }
        let v = mid(r);
        if l.contains("open")
            || l.contains("abierto")
            || l.contains("sin gobo")
            || l.contains("no gobo")
        {
            open.get_or_insert(v);
            continue;
        }
        slots.push(WheelSlot {
            value: v,
            label: r.label.clone(),
            rgb: None,
        });
    }
    (!slots.is_empty()).then(|| Wheel {
        channel: ch,
        open: open.unwrap_or(0),
        slots,
    })
}

/// "prism2_rot" → "prism2", "prism_rot" → "prism", "prismrotation" → "prism".
fn prism_stem(role: &str) -> String {
    role.trim_end_matches("ation")
        .trim_end_matches("rot")
        .trim_end_matches(['_', '-', ' '])
        .to_string()
}

fn prism_values(c: &ChannelDefinition) -> (u8, u8) {
    if c.ranges.is_empty() {
        return (0, 255);
    }
    let is_off = |l: &str| {
        l.contains("off")
            || l.contains("sin ")
            || l.starts_with("no")
            || l.contains("open")
            || l.contains("abierto")
    };
    let off = c
        .ranges
        .iter()
        .find(|r| is_off(&r.label.to_lowercase()))
        .map(mid)
        .unwrap_or(c.ranges[0].from);
    let on = c
        .ranges
        .iter()
        .find(|r| {
            let l = r.label.to_lowercase();
            !is_off(&l) && !is_effect_label(&l)
        })
        .map(mid)
        .or_else(|| c.ranges.get(1).map(mid))
        .unwrap_or(255);
    (off, on)
}

/// A slow clockwise rotation, if the channel's ranges say where it is;
/// unknown semantics leave the rotation alone.
fn rotation_value(c: &ChannelDefinition) -> u8 {
    let slow = c.ranges.iter().find(|r| {
        let l = r.label.to_lowercase();
        (l.contains("slow") || l.contains("lent")) && !l.contains("ccw")
    });
    slow.or_else(|| {
        c.ranges.iter().find(|r| {
            let l = r.label.to_lowercase();
            l.contains("cw") && !l.contains("ccw")
        })
    })
    .map(mid)
    .unwrap_or(0)
}

fn prisms_of(mode: &FixtureMode, base: usize) -> Vec<Prism> {
    let mut out = Vec::new();
    for (i, c) in mode.channels.iter().enumerate() {
        let ChannelRole::Other(role) = &c.role else {
            continue;
        };
        let r = role.to_lowercase();
        if !r.starts_with("prism") || r.contains("rot") {
            continue;
        }
        let (off, on) = prism_values(c);
        let rot = mode
            .channels
            .iter()
            .enumerate()
            .find(|(_, rc)| {
                matches!(&rc.role, ChannelRole::Other(s) if {
                    let s = s.to_lowercase();
                    s.contains("rot") && prism_stem(&s) == r
                })
            })
            .map(|(j, rc)| (base + j, rotation_value(rc)));
        out.push(Prism {
            channel: base + i,
            off,
            on,
            rot,
        });
    }
    out
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
