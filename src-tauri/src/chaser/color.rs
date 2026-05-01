//! Pure colour resolver — `(mode, step, slot, total) -> Option<Rgb>`.
//!
//! Returns `None` for `ColorMode::Disabled` so the engine knows to skip
//! writing colour channels at all (rather than writing white). The function
//! is pure: same inputs → same output, no clock, no state.

use super::{Cadence, ColorMode, PaletteRotation, Rgb};

pub fn color_for_slot(mode: &ColorMode, step: u64, slot: usize, total: usize) -> Option<Rgb> {
    match mode {
        ColorMode::Disabled => None,
        ColorMode::Single { color } => Some(*color),
        ColorMode::TwoColorCadence {
            color_a,
            color_b,
            cadence,
        } => {
            if pick_a(cadence, step, slot, total) {
                Some(*color_a)
            } else {
                Some(*color_b)
            }
        }
        ColorMode::Palette { colors, rotation } => {
            if colors.is_empty() {
                return Some(Rgb::BLACK);
            }
            let idx = palette_index(rotation, step, slot, total, colors.len());
            Some(colors[idx])
        }
        ColorMode::Rainbow { speed, spread } => {
            Some(rainbow_color(step, slot, total, *speed, *spread))
        }
    }
}

fn pick_a(cadence: &Cadence, step: u64, slot: usize, total: usize) -> bool {
    match cadence {
        Cadence::EveryStep => step.is_multiple_of(2),
        Cadence::EveryNSteps { n } => {
            let n = (*n).max(1) as u64;
            (step / n).is_multiple_of(2)
        }
        Cadence::PerSlot => {
            // First half (rounded up) of the slots get A, the rest get B.
            let half = total.div_ceil(2);
            slot < half
        }
        Cadence::AlternateSlots => slot.is_multiple_of(2),
        Cadence::ChasePerColor => {
            let cycle = (total.max(1)) as u64;
            (step / cycle).is_multiple_of(2)
        }
    }
}

fn palette_index(
    rotation: &PaletteRotation,
    step: u64,
    slot: usize,
    total: usize,
    len: usize,
) -> usize {
    match rotation {
        PaletteRotation::PerStep => (step as usize) % len,
        PaletteRotation::PerCycle => {
            let cycle = total.max(1) as u64;
            ((step / cycle) as usize) % len
        }
        PaletteRotation::PerSlot => slot % len,
    }
}

fn rainbow_color(step: u64, slot: usize, total: usize, speed: f32, spread: f32) -> Rgb {
    // `speed` = degrees/step. Default-ish 30 → quarter-revolution every 3 steps.
    // `spread` = how much hue offset across slots; 1.0 means a full rainbow
    // mapped over `total` slots. Negative is fine (reverses direction).
    let per_slot = if total <= 1 {
        0.0
    } else {
        (slot as f32 / total as f32) * spread * 360.0
    };
    let raw = step as f32 * speed + per_slot;
    let hue = raw.rem_euclid(360.0);
    hsv_to_rgb(hue, 1.0, 1.0)
}

/// Standard HSV→RGB conversion. `h` in degrees `[0, 360)`, `s` and `v` in
/// `[0.0, 1.0]`. Output channels clamped to `[0, 255]`.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Rgb {
    let c = v * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match h_prime as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    Rgb {
        r: (((r1 + m) * 255.0).clamp(0.0, 255.0)) as u8,
        g: (((g1 + m) * 255.0).clamp(0.0, 255.0)) as u8,
        b: (((b1 + m) * 255.0).clamp(0.0, 255.0)) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    #[test]
    fn disabled_returns_none() {
        assert_eq!(color_for_slot(&ColorMode::Disabled, 0, 0, 4), None);
    }

    #[test]
    fn single_returns_constant_color() {
        let c = ColorMode::Single {
            color: rgb(10, 20, 30),
        };
        for step in 0..10u64 {
            for slot in 0..4 {
                assert_eq!(color_for_slot(&c, step, slot, 4), Some(rgb(10, 20, 30)));
            }
        }
    }

    #[test]
    fn two_color_every_step_swaps_each_step() {
        let c = ColorMode::TwoColorCadence {
            color_a: rgb(255, 0, 0),
            color_b: rgb(0, 0, 255),
            cadence: Cadence::EveryStep,
        };
        assert_eq!(color_for_slot(&c, 0, 0, 4), Some(rgb(255, 0, 0)));
        assert_eq!(color_for_slot(&c, 1, 0, 4), Some(rgb(0, 0, 255)));
        assert_eq!(color_for_slot(&c, 2, 0, 4), Some(rgb(255, 0, 0)));
    }

    #[test]
    fn two_color_every_n_steps() {
        let c = ColorMode::TwoColorCadence {
            color_a: rgb(255, 0, 0),
            color_b: rgb(0, 0, 255),
            cadence: Cadence::EveryNSteps { n: 4 },
        };
        // 4 steps red, 4 steps blue.
        for s in 0..4 {
            assert_eq!(
                color_for_slot(&c, s, 0, 1),
                Some(rgb(255, 0, 0)),
                "step {s}"
            );
        }
        for s in 4..8 {
            assert_eq!(
                color_for_slot(&c, s, 0, 1),
                Some(rgb(0, 0, 255)),
                "step {s}"
            );
        }
    }

    #[test]
    fn two_color_per_slot_static_split() {
        let c = ColorMode::TwoColorCadence {
            color_a: rgb(1, 0, 0),
            color_b: rgb(0, 1, 0),
            cadence: Cadence::PerSlot,
        };
        // total=6 → first 3 = A, last 3 = B.
        let total = 6;
        for slot in 0..3 {
            assert_eq!(color_for_slot(&c, 99, slot, total), Some(rgb(1, 0, 0)));
        }
        for slot in 3..6 {
            assert_eq!(color_for_slot(&c, 99, slot, total), Some(rgb(0, 1, 0)));
        }
    }

    #[test]
    fn two_color_alternate_slots_zebra() {
        let c = ColorMode::TwoColorCadence {
            color_a: rgb(1, 0, 0),
            color_b: rgb(0, 1, 0),
            cadence: Cadence::AlternateSlots,
        };
        // Static: 0=A, 1=B, 2=A, 3=B regardless of step.
        for step in 0..3 {
            assert_eq!(color_for_slot(&c, step, 0, 4), Some(rgb(1, 0, 0)));
            assert_eq!(color_for_slot(&c, step, 1, 4), Some(rgb(0, 1, 0)));
            assert_eq!(color_for_slot(&c, step, 2, 4), Some(rgb(1, 0, 0)));
            assert_eq!(color_for_slot(&c, step, 3, 4), Some(rgb(0, 1, 0)));
        }
    }

    #[test]
    fn two_color_chase_per_color_swaps_each_cycle() {
        let c = ColorMode::TwoColorCadence {
            color_a: rgb(1, 0, 0),
            color_b: rgb(0, 1, 0),
            cadence: Cadence::ChasePerColor,
        };
        let total = 4;
        // Steps 0..3 = A, 4..7 = B, 8..11 = A.
        for s in 0..4 {
            assert_eq!(color_for_slot(&c, s, 0, total), Some(rgb(1, 0, 0)));
        }
        for s in 4..8 {
            assert_eq!(color_for_slot(&c, s, 0, total), Some(rgb(0, 1, 0)));
        }
        for s in 8..12 {
            assert_eq!(color_for_slot(&c, s, 0, total), Some(rgb(1, 0, 0)));
        }
    }

    #[test]
    fn palette_per_step_walks_through() {
        let c = ColorMode::Palette {
            colors: vec![rgb(1, 0, 0), rgb(0, 1, 0), rgb(0, 0, 1)],
            rotation: PaletteRotation::PerStep,
        };
        assert_eq!(color_for_slot(&c, 0, 0, 4), Some(rgb(1, 0, 0)));
        assert_eq!(color_for_slot(&c, 1, 0, 4), Some(rgb(0, 1, 0)));
        assert_eq!(color_for_slot(&c, 2, 0, 4), Some(rgb(0, 0, 1)));
        assert_eq!(color_for_slot(&c, 3, 0, 4), Some(rgb(1, 0, 0)));
    }

    #[test]
    fn palette_per_slot_assigns_each_slot() {
        let c = ColorMode::Palette {
            colors: vec![rgb(1, 0, 0), rgb(0, 1, 0)],
            rotation: PaletteRotation::PerSlot,
        };
        // Static across steps; slots cycle through palette.
        for step in 0..4 {
            assert_eq!(color_for_slot(&c, step, 0, 4), Some(rgb(1, 0, 0)));
            assert_eq!(color_for_slot(&c, step, 1, 4), Some(rgb(0, 1, 0)));
            assert_eq!(color_for_slot(&c, step, 2, 4), Some(rgb(1, 0, 0)));
            assert_eq!(color_for_slot(&c, step, 3, 4), Some(rgb(0, 1, 0)));
        }
    }

    #[test]
    fn palette_per_cycle_swaps_each_chase_cycle() {
        let c = ColorMode::Palette {
            colors: vec![rgb(1, 0, 0), rgb(0, 1, 0)],
            rotation: PaletteRotation::PerCycle,
        };
        let total = 3;
        for s in 0..3 {
            assert_eq!(color_for_slot(&c, s, 0, total), Some(rgb(1, 0, 0)));
        }
        for s in 3..6 {
            assert_eq!(color_for_slot(&c, s, 0, total), Some(rgb(0, 1, 0)));
        }
    }

    #[test]
    fn empty_palette_returns_black() {
        let c = ColorMode::Palette {
            colors: vec![],
            rotation: PaletteRotation::PerStep,
        };
        assert_eq!(color_for_slot(&c, 0, 0, 4), Some(Rgb::BLACK));
    }

    #[test]
    fn hsv_pure_red_at_zero() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), rgb(255, 0, 0));
    }

    #[test]
    fn hsv_pure_green_at_120() {
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), rgb(0, 255, 0));
    }

    #[test]
    fn hsv_pure_blue_at_240() {
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), rgb(0, 0, 255));
    }

    #[test]
    fn rainbow_advances_with_step() {
        let c = ColorMode::Rainbow {
            speed: 60.0,
            spread: 0.0,
        };
        // speed=60 deg/step, spread=0 (all slots same hue).
        // step 0 → 0 deg = red; step 2 → 120 deg = green; step 4 → 240 deg = blue.
        assert_eq!(color_for_slot(&c, 0, 0, 1), Some(rgb(255, 0, 0)));
        assert_eq!(color_for_slot(&c, 2, 0, 1), Some(rgb(0, 255, 0)));
        assert_eq!(color_for_slot(&c, 4, 0, 1), Some(rgb(0, 0, 255)));
    }

    #[test]
    fn rainbow_spreads_across_slots() {
        let c = ColorMode::Rainbow {
            speed: 0.0,
            spread: 1.0,
        };
        // step=0, spread=1 → slot 0 hue 0, slot 1 hue 90, slot 2 hue 180, slot 3 hue 270 (4 slots).
        let total = 4;
        assert_eq!(color_for_slot(&c, 0, 0, total), Some(rgb(255, 0, 0))); // 0deg
                                                                           // 90deg: yellow-green
        let s1 = color_for_slot(&c, 0, 1, total).unwrap();
        assert!(s1.r > 100 && s1.g == 255 && s1.b == 0, "got {:?}", s1);
        // 180deg: cyan
        assert_eq!(color_for_slot(&c, 0, 2, total), Some(rgb(0, 255, 255)));
    }
}
