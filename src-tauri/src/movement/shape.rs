//! Pure shape evaluator — `phase ∈ [0, 1) → (x, y) ∈ [-1, 1]²`.
//!
//! No clock, no state. The runtime resolves `phase` from the wall-clock and
//! asks the shape what point each fixture should target. Sub-fase A only
//! implements `Circle`; the rest are stubbed to fall back on Circle so the
//! data model survives across sub-phases without migration.

use std::f32::consts::TAU;

use super::{Shape, WaveFunction, Waveform};

/// Evaluate a shape at the given phase, returning normalised `(x, y)` in
/// `[-1, 1]²`. The runtime is responsible for wrapping `phase` into `[0, 1)`
/// before calling.
pub fn evaluate(shape: &Shape, phase: f32) -> (f32, f32) {
    match shape {
        Shape::Circle => circle(phase),
        Shape::Polygon { sides } => polygon(phase, (*sides).max(3)),
        Shape::Star { points, inner_ratio } => {
            star(phase, (*points).max(3), inner_ratio.clamp(0.05, 0.95))
        }
        Shape::FigureEight => figure_eight(phase),
        Shape::LineHorizontal => line_horizontal(phase),
        Shape::LineVertical => line_vertical(phase),
        Shape::SineCombo { pan, tilt } => {
            (evaluate_wave(pan, phase), evaluate_wave(tilt, phase))
        }
    }
}

/// Unit circle, traversed counter-clockwise once per `phase ∈ [0, 1)`.
/// `phase = 0` lands at `(1, 0)` (the canonical "3 o'clock" start).
pub fn circle(phase: f32) -> (f32, f32) {
    let theta = phase * TAU;
    (theta.cos(), theta.sin())
}

/// Regular polygon inscribed in the unit circle. `phase ∈ [0, 1)` is the
/// progress around the perimeter, with each edge taking `1 / sides`.
/// Vertices land on the unit circle at angles `k/sides * TAU`.
pub fn polygon(phase: f32, sides: u32) -> (f32, f32) {
    let total = sides.max(3) as f32;
    let scaled = phase * total;
    let current = (scaled.floor() as i64).rem_euclid(sides as i64) as u32;
    let local = scaled - scaled.floor();
    let angle_a = (current as f32 / total) * TAU;
    let angle_b = ((current + 1) as f32 / total) * TAU;
    let (xa, ya) = (angle_a.cos(), angle_a.sin());
    let (xb, yb) = (angle_b.cos(), angle_b.sin());
    (xa + (xb - xa) * local, ya + (yb - ya) * local)
}

/// `points`-pointed star with outer radius 1 and inner radius
/// `inner_ratio`. Alternates between outer and inner vertices, so the
/// total perimeter is split into `2 * points` segments.
pub fn star(phase: f32, points: u32, inner_ratio: f32) -> (f32, f32) {
    let total_segments = (points * 2) as f32;
    let scaled = phase * total_segments;
    let segment = (scaled.floor() as i64).rem_euclid(total_segments as i64) as u32;
    let local = scaled - scaled.floor();
    let angle_a = (segment as f32 / total_segments) * TAU;
    let angle_b = ((segment + 1) as f32 / total_segments) * TAU;
    let radius_a = if segment % 2 == 0 { 1.0 } else { inner_ratio };
    let radius_b = if segment % 2 == 0 { inner_ratio } else { 1.0 };
    let xa = angle_a.cos() * radius_a;
    let ya = angle_a.sin() * radius_a;
    let xb = angle_b.cos() * radius_b;
    let yb = angle_b.sin() * radius_b;
    (xa + (xb - xa) * local, ya + (yb - ya) * local)
}

/// Lissajous 1:2 — `(sin(t), sin(2t))` — gives a clean horizontal figure
/// 8 reaching `±1` on both axes. Smoother and more "stage-friendly" than a
/// strict Bernoulli lemniscate.
pub fn figure_eight(phase: f32) -> (f32, f32) {
    let theta = phase * TAU;
    (theta.sin(), (2.0 * theta).sin())
}

/// Triangle wave on pan, tilt held at 0. Travels `-1 → +1 → -1` over a
/// full loop so a single fixture sweeps left-right at the BPM tempo.
pub fn line_horizontal(phase: f32) -> (f32, f32) {
    let x = 1.0 - 2.0 * (2.0 * phase - 1.0).abs();
    (x, 0.0)
}

/// Triangle wave on tilt, pan held at 0.
pub fn line_vertical(phase: f32) -> (f32, f32) {
    let y = 1.0 - 2.0 * (2.0 * phase - 1.0).abs();
    (0.0, y)
}

/// Evaluate a single waveform component at the given phase. Output is
/// `[-1, 1]` (before amplitude/offset are applied) for every waveform.
pub fn evaluate_wave(wave: &WaveFunction, phase: f32) -> f32 {
    let t = (phase * wave.frequency + wave.phase_shift).rem_euclid(1.0);
    let raw = match wave.waveform {
        Waveform::Sine => (t * TAU).sin(),
        Waveform::Cosine => (t * TAU).cos(),
        Waveform::Triangle => 1.0 - 2.0 * (2.0 * t - 1.0).abs(),
        Waveform::Square => {
            if t < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        // Sawtooth and RampUp produce the same shape on a wrapped `t`;
        // the spec keeps them as separate names so user-facing labels can
        // differentiate intent (gradual rise vs. snap-back) even though
        // mathematically they're identical here.
        Waveform::Sawtooth | Waveform::RampUp => 2.0 * t - 1.0,
        Waveform::RampDown => 1.0 - 2.0 * t,
    };
    raw * wave.amplitude + wave.offset
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn circle_starts_at_three_oclock() {
        let (x, y) = circle(0.0);
        assert!(close(x, 1.0));
        assert!(close(y, 0.0));
    }

    #[test]
    fn circle_quarter_is_top() {
        let (x, y) = circle(0.25);
        assert!(close(x, 0.0), "x = {}", x);
        assert!(close(y, 1.0), "y = {}", y);
    }

    #[test]
    fn circle_half_is_left() {
        let (x, y) = circle(0.5);
        assert!(close(x, -1.0));
        assert!(close(y, 0.0));
    }

    #[test]
    fn circle_three_quarters_is_bottom() {
        let (x, y) = circle(0.75);
        assert!(close(x, 0.0));
        assert!(close(y, -1.0));
    }

    #[test]
    fn circle_full_loop_returns_to_start() {
        let (x, y) = circle(1.0);
        assert!(close(x, 1.0));
        assert!(close(y, 0.0));
    }

    #[test]
    fn circle_stays_on_unit_circle() {
        // r² = x² + y² should be 1 across the whole loop.
        for i in 0..200 {
            let phase = i as f32 / 200.0;
            let (x, y) = circle(phase);
            let r2 = x * x + y * y;
            assert!(close(r2, 1.0), "phase {phase}: r² = {r2}");
        }
    }

    // ----- polygon --------------------------------------------------------

    #[test]
    fn polygon_vertices_sit_on_unit_circle() {
        for sides in 3..=8 {
            for k in 0..sides {
                let phase = k as f32 / sides as f32;
                let (x, y) = polygon(phase, sides);
                let r = (x * x + y * y).sqrt();
                assert!(close(r, 1.0), "sides {sides} k {k}: r = {r}");
            }
        }
    }

    #[test]
    fn polygon_midpoints_inside_unit_circle() {
        // Midpoint of an edge of an inscribed regular polygon is at the
        // apothem, which is < 1 for sides > 2.
        for sides in 3..=8 {
            for k in 0..sides {
                let phase = (k as f32 + 0.5) / sides as f32;
                let (x, y) = polygon(phase, sides);
                let r = (x * x + y * y).sqrt();
                assert!(r < 1.0, "sides {sides} k {k}: r = {r}");
                assert!(r > 0.0);
            }
        }
    }

    #[test]
    fn polygon_full_loop_returns_to_start() {
        for sides in 3..=6 {
            let (x0, y0) = polygon(0.0, sides);
            let (x1, y1) = polygon(1.0, sides);
            assert!(close(x0, x1), "sides {sides}");
            assert!(close(y0, y1), "sides {sides}");
        }
    }

    // ----- star -----------------------------------------------------------

    #[test]
    fn star_outer_vertices_at_radius_one() {
        // Outer vertices land at segment indices 0, 2, 4, … which means
        // phases `0/2N, 2/2N, 4/2N, …`.
        let points = 5;
        let inner = 0.4;
        for i in 0..points {
            let phase = (2 * i) as f32 / (2 * points) as f32;
            let (x, y) = star(phase, points, inner);
            let r = (x * x + y * y).sqrt();
            assert!(close(r, 1.0), "outer {i}: r = {r}");
        }
    }

    #[test]
    fn star_inner_vertices_at_inner_ratio() {
        let points = 5;
        let inner = 0.4;
        for i in 0..points {
            let phase = (2 * i + 1) as f32 / (2 * points) as f32;
            let (x, y) = star(phase, points, inner);
            let r = (x * x + y * y).sqrt();
            assert!(close(r, inner), "inner {i}: r = {r}");
        }
    }

    // ----- figure_eight ---------------------------------------------------

    #[test]
    fn figure_eight_passes_through_origin_at_quarters() {
        // sin(2t) = 0 at phase 0 and 0.5 (theta = 0, π).
        let (_, y0) = figure_eight(0.0);
        let (_, y_half) = figure_eight(0.5);
        assert!(close(y0, 0.0));
        assert!(close(y_half, 0.0));
        // sin(t) = 0 at phase 0, 0.5.
        let (x0, _) = figure_eight(0.0);
        let (x_half, _) = figure_eight(0.5);
        assert!(close(x0, 0.0));
        assert!(close(x_half, 0.0));
    }

    #[test]
    fn figure_eight_reaches_unit_extremes() {
        // sin(t) hits ±1 at phase 0.25 and 0.75.
        let (x_q, _) = figure_eight(0.25);
        let (x_3q, _) = figure_eight(0.75);
        assert!(close(x_q, 1.0));
        assert!(close(x_3q, -1.0));
    }

    // ----- lines ----------------------------------------------------------

    #[test]
    fn line_horizontal_traces_back_and_forth() {
        // -1 at the ends, +1 in the middle.
        let (x0, _) = line_horizontal(0.0);
        let (x_q, _) = line_horizontal(0.25);
        let (x_h, _) = line_horizontal(0.5);
        let (x_3q, _) = line_horizontal(0.75);
        assert!(close(x0, -1.0));
        assert!(close(x_q, 0.0));
        assert!(close(x_h, 1.0));
        assert!(close(x_3q, 0.0));
        // tilt is always 0.
        let (_, y) = line_horizontal(0.3);
        assert!(close(y, 0.0));
    }

    #[test]
    fn line_vertical_swaps_axes() {
        let (x, y) = line_vertical(0.5);
        assert!(close(x, 0.0));
        assert!(close(y, 1.0));
    }

    // ----- evaluate_wave --------------------------------------------------

    fn wave(waveform: Waveform) -> WaveFunction {
        WaveFunction {
            waveform,
            frequency: 1.0,
            phase_shift: 0.0,
            amplitude: 1.0,
            offset: 0.0,
        }
    }

    #[test]
    fn sine_wave_quarter_points() {
        let w = wave(Waveform::Sine);
        assert!(close(evaluate_wave(&w, 0.0), 0.0));
        assert!(close(evaluate_wave(&w, 0.25), 1.0));
        assert!(close(evaluate_wave(&w, 0.5), 0.0));
        assert!(close(evaluate_wave(&w, 0.75), -1.0));
    }

    #[test]
    fn cosine_wave_quarter_points() {
        let w = wave(Waveform::Cosine);
        assert!(close(evaluate_wave(&w, 0.0), 1.0));
        assert!(close(evaluate_wave(&w, 0.25), 0.0));
        assert!(close(evaluate_wave(&w, 0.5), -1.0));
        assert!(close(evaluate_wave(&w, 0.75), 0.0));
    }

    #[test]
    fn triangle_wave_apex_in_middle() {
        let w = wave(Waveform::Triangle);
        assert!(close(evaluate_wave(&w, 0.0), -1.0));
        assert!(close(evaluate_wave(&w, 0.5), 1.0));
        // closing back to -1 by phase 1 (which wraps).
    }

    #[test]
    fn square_wave_jumps_at_half() {
        let w = wave(Waveform::Square);
        assert!(close(evaluate_wave(&w, 0.0), 1.0));
        assert!(close(evaluate_wave(&w, 0.49), 1.0));
        assert!(close(evaluate_wave(&w, 0.5), -1.0));
        assert!(close(evaluate_wave(&w, 0.99), -1.0));
    }

    #[test]
    fn ramp_up_and_sawtooth_match() {
        let up = wave(Waveform::RampUp);
        let saw = wave(Waveform::Sawtooth);
        for i in 0..20 {
            let p = i as f32 / 20.0;
            assert!(close(evaluate_wave(&up, p), evaluate_wave(&saw, p)));
        }
    }

    #[test]
    fn ramp_down_inverts_ramp_up() {
        let up = wave(Waveform::RampUp);
        let down = wave(Waveform::RampDown);
        for i in 0..20 {
            let p = i as f32 / 20.0;
            let a = evaluate_wave(&up, p);
            let b = evaluate_wave(&down, p);
            assert!(close(a, -b), "phase {p}: a {a} b {b}");
        }
    }

    #[test]
    fn wave_amplitude_and_offset_apply() {
        let w = WaveFunction {
            waveform: Waveform::Sine,
            frequency: 1.0,
            phase_shift: 0.0,
            amplitude: 0.5,
            offset: 0.25,
        };
        // sin(0.25 * TAU) = 1 → out = 1 * 0.5 + 0.25 = 0.75
        assert!(close(evaluate_wave(&w, 0.25), 0.75));
    }

    #[test]
    fn wave_frequency_doubles_cycle() {
        let w = WaveFunction {
            waveform: Waveform::Sine,
            frequency: 2.0,
            phase_shift: 0.0,
            amplitude: 1.0,
            offset: 0.0,
        };
        // freq=2: sin completes a full cycle by phase 0.5.
        assert!(close(evaluate_wave(&w, 0.0), 0.0));
        assert!(close(evaluate_wave(&w, 0.125), 1.0));
        assert!(close(evaluate_wave(&w, 0.25), 0.0));
        assert!(close(evaluate_wave(&w, 0.5), 0.0));
    }
}
