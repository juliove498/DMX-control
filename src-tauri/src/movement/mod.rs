//! Movement Generator module.
//!
//! Parametric pan/tilt animation that lives in the same low-priority
//! "effects" overlay as the Ambient Chaser. The engine reuses the chaser's
//! `TempoSource` and `Subdivision` so a tap-tempo or BPM change in one
//! affects neither — they are independent instances of the same type.
//!
//! Sub-fase A scope: Shape::Circle only, transforms (size/center/rotation)
//! plumbed through, no canon (SpreadMode::None), no SineCombo, no preview.
//! All other Shape variants exist in the data model so old shows survive
//! across sub-phases without migration; their evaluators land in C and D.

pub mod engine;
pub mod pipeline;
pub mod shape;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use crate::chaser::{Subdivision, TempoSource};

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct MovementSlot {
    pub fixture_id: String,
    /// 0.0..=1.0 phase shift applied to this fixture's place in the loop.
    /// `SpreadMode::Even` overwrites this on every fixture-list change; in
    /// `Manual` mode the user owns it.
    #[serde(default)]
    pub phase_offset: f32,
    /// Mirror this fixture horizontally relative to the shape's natural
    /// pan output. Useful when half a rig is hung facing the other way.
    #[serde(default)]
    pub invert_pan: bool,
    #[serde(default)]
    pub invert_tilt: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Shape {
    /// Unit circle, the canonical pan/tilt move.
    Circle,
    /// Regular n-gon. `sides >= 3`. Sub-fase C.
    Polygon { sides: u32 },
    /// N-pointed star with `inner_ratio in (0.0, 1.0)`. Sub-fase C.
    Star { points: u32, inner_ratio: f32 },
    /// Lemniscate-style figure of eight. Sub-fase C.
    FigureEight,
    /// Triangle wave on pan, tilt held at 0. Sub-fase C.
    LineHorizontal,
    /// Triangle wave on tilt, pan held at 0. Sub-fase C.
    LineVertical,
    /// Independent waveforms on pan and tilt — Lissajous heaven. Sub-fase D.
    SineCombo {
        pan: WaveFunction,
        tilt: WaveFunction,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum Waveform {
    Sine,
    Cosine,
    Triangle,
    Square,
    Sawtooth,
    RampUp,
    RampDown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct WaveFunction {
    pub waveform: Waveform,
    /// Multiplier on phase. `1.0` = one full cycle per loop, `2.0` = two.
    pub frequency: f32,
    /// Phase shift `0.0..=1.0` applied before the waveform is evaluated.
    pub phase_shift: f32,
    /// `0.0..=1.0` scale on the waveform's natural `[-1, 1]` output.
    pub amplitude: f32,
    /// Constant added after amplitude — useful for biasing one axis.
    pub offset: f32,
}

impl WaveFunction {
    pub fn cosine_one() -> Self {
        Self {
            waveform: Waveform::Cosine,
            frequency: 1.0,
            phase_shift: 0.0,
            amplitude: 1.0,
            offset: 0.0,
        }
    }
    pub fn sine_one() -> Self {
        Self {
            waveform: Waveform::Sine,
            frequency: 1.0,
            phase_shift: 0.0,
            amplitude: 1.0,
            offset: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum SpreadMode {
    /// All fixtures in phase — they all chase the same point.
    None,
    /// Equally spaced offsets `0, 1/N, 2/N, ...`.
    Even,
    /// Mirror-symmetric pairs from the centre out.
    Symmetric,
    /// Two-by-two: `0, 0, 1/2, 1/2, 1/4, 1/4, ...`.
    Pairs,
    /// User-edited `phase_offset` per slot.
    Manual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Forward,
    Reverse,
    /// Alternates direction each completed loop. Sub-fase F.
    PingPong,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct MovementGenerator {
    /// Stable id, used to preserve runtime phase across edits and to
    /// dispatch toggle actions from surfaces (Launchpad, future scripts).
    /// Defaulted on deserialise so legacy show files without an id are
    /// migrated transparently — the loader assigns a fresh uuid on first
    /// load and saves it back on the next persist.
    #[serde(default = "default_movement_id")]
    pub id: String,
    /// User-visible label, like "Slow circle" or "Strobe sweep".
    #[serde(default = "default_movement_name")]
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub fixtures: Vec<MovementSlot>,
    pub shape: Shape,
    /// `0.0..=1.0` width scale applied after the shape produces normalised
    /// `[-1, 1]` output.
    pub size_x: f32,
    pub size_y: f32,
    /// `-1.0..=1.0` centre offset within the normalised pan/tilt space.
    pub center_x: f32,
    pub center_y: f32,
    /// 2D rotation in degrees applied before size/centre.
    pub rotation_deg: f32,
    pub spread_mode: SpreadMode,
    pub tempo: TempoSource,
    pub subdivision: Subdivision,
    pub direction: Direction,
}

fn default_movement_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn default_movement_name() -> String {
    "Movement".to_string()
}

impl MovementGenerator {
    pub fn default_disabled() -> Self {
        Self {
            id: default_movement_id(),
            name: default_movement_name(),
            enabled: false,
            fixtures: Vec::new(),
            shape: Shape::Circle,
            size_x: 1.0,
            size_y: 1.0,
            center_x: 0.0,
            center_y: 0.0,
            rotation_deg: 0.0,
            spread_mode: SpreadMode::None,
            tempo: TempoSource::Fixed { bpm: 60.0 },
            subdivision: Subdivision::Four,
            direction: Direction::Forward,
        }
    }

    /// Recompute every slot's `phase_offset` according to `spread_mode`.
    /// `Manual` is a no-op — the user owns the offsets in that mode.
    pub fn apply_spread(&mut self) {
        apply_spread(&mut self.fixtures, self.spread_mode);
    }
}

/// Compute the canon `phase_offset` for every slot under `mode`. Lives at
/// module level so commands can call it without owning a `&mut
/// MovementGenerator` (e.g. when only the slot list changes).
pub fn apply_spread(slots: &mut [MovementSlot], mode: SpreadMode) {
    let n = slots.len();
    if n == 0 {
        return;
    }
    match mode {
        SpreadMode::None => {
            for s in slots {
                s.phase_offset = 0.0;
            }
        }
        SpreadMode::Even => {
            for (i, s) in slots.iter_mut().enumerate() {
                s.phase_offset = i as f32 / n as f32;
            }
        }
        SpreadMode::Symmetric => {
            // Mirror from the centre. For 6 slots the offsets ramp
            // 0, 1/6, 2/6, 2/6, 1/6, 0 — fixtures equidistant from the
            // ends share an offset.
            let half = (n + 1) / 2;
            let denom = (2 * half) as f32;
            for (i, s) in slots.iter_mut().enumerate() {
                let pair = i.min(n - 1 - i);
                s.phase_offset = pair as f32 / denom;
            }
        }
        SpreadMode::Pairs => {
            // Two-by-two: [0, 0, 1/k, 1/k, 2/k, 2/k, …] where k =
            // ceil(n / 2). For odd n the last slot pairs with itself.
            let pair_count = ((n + 1) / 2) as f32;
            for (i, s) in slots.iter_mut().enumerate() {
                let pair_idx = (i / 2) as f32;
                s.phase_offset = pair_idx / pair_count;
            }
        }
        SpreadMode::Manual => {
            // No-op: user owns the offsets.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_movement_round_trips_serde() {
        let m = MovementGenerator::default_disabled();
        let json = serde_json::to_string(&m).unwrap();
        let back: MovementGenerator = serde_json::from_str(&json).unwrap();
        assert_eq!(back.shape, Shape::Circle);
        assert_eq!(back.spread_mode, SpreadMode::None);
        assert_eq!(back.direction, Direction::Forward);
    }

    fn make_slots(n: usize) -> Vec<MovementSlot> {
        (0..n)
            .map(|i| MovementSlot {
                fixture_id: format!("f{i}"),
                phase_offset: 0.0,
                invert_pan: false,
                invert_tilt: false,
            })
            .collect()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn spread_none_zeroes_all() {
        let mut slots = make_slots(4);
        for s in &mut slots {
            s.phase_offset = 0.42;
        }
        apply_spread(&mut slots, SpreadMode::None);
        for s in &slots {
            assert_eq!(s.phase_offset, 0.0);
        }
    }

    #[test]
    fn spread_even_distributes_evenly() {
        let mut slots = make_slots(4);
        apply_spread(&mut slots, SpreadMode::Even);
        let expected = [0.0, 0.25, 0.5, 0.75];
        for (i, s) in slots.iter().enumerate() {
            assert!(close(s.phase_offset, expected[i]), "slot {i}");
        }
    }

    #[test]
    fn spread_symmetric_mirrors_from_centre() {
        let mut slots = make_slots(6);
        apply_spread(&mut slots, SpreadMode::Symmetric);
        // half = (6+1)/2 = 3, denom = 6 → pair_idx 0,1,2,2,1,0 / 6
        let expected = [
            0.0,
            1.0 / 6.0,
            2.0 / 6.0,
            2.0 / 6.0,
            1.0 / 6.0,
            0.0,
        ];
        for (i, s) in slots.iter().enumerate() {
            assert!(
                close(s.phase_offset, expected[i]),
                "slot {i}: got {}",
                s.phase_offset
            );
        }
    }

    #[test]
    fn spread_pairs_groups_by_two() {
        let mut slots = make_slots(6);
        apply_spread(&mut slots, SpreadMode::Pairs);
        // pair_count = 3. offsets = [0/3, 0/3, 1/3, 1/3, 2/3, 2/3]
        let expected = [0.0, 0.0, 1.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0];
        for (i, s) in slots.iter().enumerate() {
            assert!(close(s.phase_offset, expected[i]), "slot {i}");
        }
    }

    #[test]
    fn spread_manual_is_no_op() {
        let mut slots = make_slots(3);
        slots[0].phase_offset = 0.1;
        slots[1].phase_offset = 0.2;
        slots[2].phase_offset = 0.3;
        apply_spread(&mut slots, SpreadMode::Manual);
        assert_eq!(slots[0].phase_offset, 0.1);
        assert_eq!(slots[1].phase_offset, 0.2);
        assert_eq!(slots[2].phase_offset, 0.3);
    }
}
