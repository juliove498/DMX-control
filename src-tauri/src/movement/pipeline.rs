//! Per-fixture render pipeline:
//!
//! 1. `fixture_phase = (global_phase + slot.phase_offset) mod 1.0`
//! 2. `(x, y) = shape.evaluate(fixture_phase)`         // [-1, 1]²
//! 3. rotate by `rotation_deg`                          // 2D rotation
//! 4. scale by `(size_x, size_y)`                       // non-uniform scale
//! 5. translate by `(center_x, center_y)`               // re-centre
//! 6. clamp to `[-1, 1]` and map to DMX 16-bit using fixture's pan/tilt range
//!
//! Each step is a small pure function so they're testable in isolation.

use std::f32::consts::PI;

use super::shape::evaluate as eval_shape;
use super::{MovementSlot, Shape};
use crate::show::fixture::PanTiltRange;

/// Derive a fixture's phase from the global phase + the slot offset.
/// Always wraps into `[0, 1)`.
pub fn fixture_phase(global_phase: f64, slot: &MovementSlot) -> f32 {
    (global_phase as f32 + slot.phase_offset).rem_euclid(1.0)
}

/// Rotate a 2D point by `deg` degrees counter-clockwise.
pub fn rotate(x: f32, y: f32, deg: f32) -> (f32, f32) {
    let rad = deg * PI / 180.0;
    let (s, c) = rad.sin_cos();
    (c * x - s * y, s * x + c * y)
}

pub fn scale(x: f32, y: f32, sx: f32, sy: f32) -> (f32, f32) {
    (x * sx, y * sy)
}

pub fn translate(x: f32, y: f32, cx: f32, cy: f32) -> (f32, f32) {
    (x + cx, y + cy)
}

pub fn invert(x: f32, y: f32, invert_x: bool, invert_y: bool) -> (f32, f32) {
    let nx = if invert_x { -x } else { x };
    let ny = if invert_y { -y } else { y };
    (nx, ny)
}

/// Map a normalised `[-1, 1]` value to a 16-bit DMX value within
/// `range.min..=range.max`. Returns `(coarse, fine)` so the caller can hand
/// each byte off to the right channel.
pub fn map_to_dmx(normalised: f32, range: &PanTiltRange) -> (u8, u8) {
    let clamped = normalised.clamp(-1.0, 1.0);
    let t = (clamped + 1.0) * 0.5; // 0.0..=1.0
    let lo = range.min as f32;
    let hi = range.max as f32;
    let value = (lo + t * (hi - lo)).round();
    let value_u16 = value.clamp(0.0, 65_535.0) as u16;
    let high = (value_u16 >> 8) as u8;
    let low = (value_u16 & 0xFF) as u8;
    (high, low)
}

/// Compose the whole transform pipeline starting from a normalised shape
/// output. The result is in normalised space (still `[-1, 1]` after clamp);
/// the caller hands it off to `map_to_dmx` per axis with the fixture's
/// own `PanTiltRange`.
pub fn transform(
    shape_xy: (f32, f32),
    rotation_deg: f32,
    size_x: f32,
    size_y: f32,
    center_x: f32,
    center_y: f32,
) -> (f32, f32) {
    let (x, y) = rotate(shape_xy.0, shape_xy.1, rotation_deg);
    let (x, y) = scale(x, y, size_x, size_y);
    let (x, y) = translate(x, y, center_x, center_y);
    (x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0))
}

/// Full per-slot evaluator: from `(global_phase, slot, generator config)`
/// to the final normalised `(x, y)` ready for DMX mapping. Used by the
/// engine; surfaced separately for tests so we can pin specific outputs
/// with known inputs.
#[allow(clippy::too_many_arguments)]
pub fn evaluate_slot(
    global_phase: f64,
    slot: &MovementSlot,
    shape: &Shape,
    rotation_deg: f32,
    size_x: f32,
    size_y: f32,
    center_x: f32,
    center_y: f32,
) -> (f32, f32) {
    let phase = fixture_phase(global_phase, slot);
    let xy = eval_shape(shape, phase);
    let (x, y) = invert(xy.0, xy.1, slot.invert_pan, slot.invert_tilt);
    transform((x, y), rotation_deg, size_x, size_y, center_x, center_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn slot(offset: f32) -> MovementSlot {
        MovementSlot {
            fixture_id: "f".into(),
            phase_offset: offset,
            invert_pan: false,
            invert_tilt: false,
        }
    }

    // ----- fixture_phase --------------------------------------------------

    #[test]
    fn fixture_phase_wraps_into_unit_interval() {
        let s = slot(0.7);
        // global 0.5 + offset 0.7 = 1.2 → 0.2
        let p = fixture_phase(0.5, &s);
        assert!(close(p, 0.2), "got {p}");
    }

    #[test]
    fn fixture_phase_zero_offset_is_identity() {
        let s = slot(0.0);
        for global in [0.0, 0.25, 0.5, 0.75, 0.999] {
            let p = fixture_phase(global, &s);
            assert!(close(p, global as f32), "global {global}: got {p}");
        }
    }

    // ----- rotate ---------------------------------------------------------

    #[test]
    fn rotate_90_swaps_axes() {
        let (x, y) = rotate(1.0, 0.0, 90.0);
        assert!(close(x, 0.0), "x = {}", x);
        assert!(close(y, 1.0), "y = {}", y);
    }

    #[test]
    fn rotate_180_negates() {
        let (x, y) = rotate(1.0, 0.5, 180.0);
        assert!(close(x, -1.0));
        assert!(close(y, -0.5));
    }

    #[test]
    fn rotate_zero_is_identity() {
        let (x, y) = rotate(0.7, -0.3, 0.0);
        assert!(close(x, 0.7));
        assert!(close(y, -0.3));
    }

    // ----- scale + translate ----------------------------------------------

    #[test]
    fn scale_independent_axes() {
        assert_eq!(scale(0.5, 0.5, 0.4, 1.0), (0.2, 0.5));
    }

    #[test]
    fn translate_adds_offset() {
        let (x, y) = translate(0.2, -0.4, 0.5, 0.5);
        assert!(close(x, 0.7));
        assert!(close(y, 0.1));
    }

    #[test]
    fn invert_flips_only_when_requested() {
        assert_eq!(invert(0.6, -0.3, false, false), (0.6, -0.3));
        assert_eq!(invert(0.6, -0.3, true, false), (-0.6, -0.3));
        assert_eq!(invert(0.6, -0.3, false, true), (0.6, 0.3));
        assert_eq!(invert(0.6, -0.3, true, true), (-0.6, 0.3));
    }

    // ----- map_to_dmx -----------------------------------------------------

    #[test]
    fn map_to_dmx_full_range_endpoints() {
        let r = PanTiltRange {
            min: 0,
            max: u16::MAX,
            physical_degrees: 540.0,
        };
        // -1 maps to min (0).
        assert_eq!(map_to_dmx(-1.0, &r), (0, 0));
        // +1 maps to max (65_535).
        assert_eq!(map_to_dmx(1.0, &r), (255, 255));
        // 0 lands at the midpoint (~32_768).
        let (hi, lo) = map_to_dmx(0.0, &r);
        let value = ((hi as u16) << 8) | (lo as u16);
        assert!((value as i32 - 32_768).abs() <= 1, "got {value}");
    }

    #[test]
    fn map_to_dmx_clamps_overshoot() {
        let r = PanTiltRange {
            min: 0,
            max: 1000,
            physical_degrees: 90.0,
        };
        assert_eq!(map_to_dmx(2.0, &r), (3, 232)); // 1000 = 0x03E8
        assert_eq!(map_to_dmx(-2.0, &r), (0, 0));
    }

    #[test]
    fn map_to_dmx_partial_range_centred() {
        // Range biased toward the high end: min=10000, max=20000.
        // Normalised 0.0 should land at the midpoint (15000).
        let r = PanTiltRange {
            min: 10_000,
            max: 20_000,
            physical_degrees: 180.0,
        };
        let (hi, lo) = map_to_dmx(0.0, &r);
        let value = ((hi as u16) << 8) | (lo as u16);
        assert!((value as i32 - 15_000).abs() <= 1, "got {value}");
    }

    // ----- transform composite -------------------------------------------

    #[test]
    fn transform_identity_passes_through() {
        let (x, y) = transform((0.7, -0.4), 0.0, 1.0, 1.0, 0.0, 0.0);
        assert!(close(x, 0.7));
        assert!(close(y, -0.4));
    }

    #[test]
    fn transform_size_then_centre() {
        // Half-size + centre at 0.3, 0.5 should hit (0.3 + 0.5*0.5, 0.5 + 0.5*1.0)
        // = (0.55, 1.0) but clamped → (0.55, 1.0).
        let (x, y) = transform((1.0, 1.0), 0.0, 0.5, 0.5, 0.3, 0.5);
        assert!(close(x, 0.8));
        assert!(close(y, 1.0));
    }

    #[test]
    fn transform_clamps_runaway_values() {
        // Big size + big offset shouldn't escape [-1, 1].
        let (x, y) = transform((1.0, 1.0), 0.0, 10.0, 10.0, 5.0, 5.0);
        assert_eq!(x, 1.0);
        assert_eq!(y, 1.0);
    }

    // ----- evaluate_slot end-to-end --------------------------------------

    #[test]
    fn evaluate_slot_circle_at_phase_zero() {
        let s = slot(0.0);
        let (x, y) = evaluate_slot(0.0, &s, &Shape::Circle, 0.0, 1.0, 1.0, 0.0, 0.0);
        assert!(close(x, 1.0));
        assert!(close(y, 0.0));
    }

    #[test]
    fn evaluate_slot_canon_offset_lands_quarter_ahead() {
        // Circle, offset = 0.25, global phase = 0.0 → fixture phase 0.25 →
        // top of circle.
        let s = slot(0.25);
        let (x, y) = evaluate_slot(0.0, &s, &Shape::Circle, 0.0, 1.0, 1.0, 0.0, 0.0);
        assert!(close(x, 0.0));
        assert!(close(y, 1.0));
    }

    #[test]
    fn evaluate_slot_size_zero_collapses_to_centre() {
        let s = slot(0.3);
        let (x, y) = evaluate_slot(0.0, &s, &Shape::Circle, 0.0, 0.0, 0.0, -0.5, 0.5);
        assert!(close(x, -0.5));
        assert!(close(y, 0.5));
    }
}
