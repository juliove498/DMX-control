//! Ambient Chaser module.
//!
//! "Set & forget" generative chase that runs in a low-priority overlay layer
//! beneath manual writes. See `docs/AMBIENT_CHASER.md` for the full design.
//! Sub-fase A scope: AllTogether pattern, Single colour, Fixed BPM, no fade.
//! Other variants exist in the data model so old-show migrations don't break
//! when later sub-phases land, but their runtime support is incremental.

pub mod color;
pub mod engine;
pub mod pattern;
pub mod presets;
pub mod runtime;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };
    pub const WHITE: Rgb = Rgb {
        r: 255,
        g: 255,
        b: 255,
    };
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
pub struct ChaserSlot {
    pub fixture_id: String,
    #[serde(default = "default_true")]
    pub use_intensity: bool,
    #[serde(default = "default_true")]
    pub use_color: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Pattern {
    /// Every slot blinks in unison, on at even steps.
    AllTogether,
    /// Even-indexed slots vs odd-indexed slots, swapping each step.
    Alternate,
    /// One slot lit at a time, marching forward through the slot list.
    Chase,
    /// Same as Chase but stepping right-to-left.
    ChaseReverse,
    /// Forward chase to the end, then reverse to the start, then forward.
    PingPong,
    /// One slot lit per step, picked by a deterministic hash of `step`.
    Random,
    /// Moving block of two adjacent slots marching forward.
    Wave,
    /// Moving block of two adjacent slots marching backward.
    WaveReverse,
    /// Slots accumulate one per step (0; 0,1; 0,1,2; …; all; off; reset).
    Build,
    /// Slots peel off from a fully-lit start (all; 1..=n-1; …; n-1; off).
    BuildReverse,
    /// Lit area expands outward from the centre then collapses for one step.
    CenterOut,
    /// Symmetric chase: one slot lit at each end converging to the middle.
    Symmetric,
    /// Lit area shrinks from the edges to the centre, then resets.
    /// Mirror of CenterOut. Cycle = ceil(total/2) + 1.
    OutsideIn,
    /// "Shadow chase": every slot lit *except* one, which marches
    /// forward through the strip. Reads as a moving dark spot.
    InvertedChase,
    /// Pairs of adjacent slots march together (slots 0+1 → 2+3 → 4+5 → …).
    GroupsOfTwo,
    /// Triplets of adjacent slots march together (slots 0+1+2 → 3+4+5 → …).
    GroupsOfThree,
    /// Left half on then right half on, swapping each step.
    HalfSwap,
    /// Only the first and last slots blink in unison; everything in
    /// between stays off. Useful as a downbeat accent layer.
    Edges,
    /// Single ring expanding from the centre outwards: only the slots
    /// at the current radius are lit (not cumulative like CenterOut).
    /// Reads as a sonar ping spreading.
    PulseOut,
    /// Mirror of PulseOut — single ring contracting from the edges
    /// toward the centre. Reads as a "closing in" effect.
    PulseIn,
    /// Continuous breathing: pulse rides centre→edges→centre with no
    /// off-step. Cycle = 2·ceil(total/2) − 1, no reset frame, so the
    /// motion never stalls. Great for sustained ambient pads.
    Accordion,
    /// Cumulative symmetric build from the edges inward — both ends
    /// fill toward the centre simultaneously, then a reset frame.
    /// Like a bowtie tying itself.
    Bowtie,
    /// Two synchronised heads on opposite halves of the strip,
    /// marching forward together (slot k and slot k+half). For odd
    /// totals the offset wraps around.
    DualChase,
    /// Symmetric pair traverses edges→centre→edges→centre…, in a
    /// ping-pong rather than the unidirectional Symmetric. Cycle =
    /// 2·ceil(total/2) − 2 with a centre crease for even totals.
    SymmetricBounce,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Cadence {
    /// A B A B A B… swap on every step.
    EveryStep,
    /// AAAA BBBB AAAA BBBB… swap every `n` steps.
    EveryNSteps { n: u32 },
    /// First half of slots get A, second half get B (static within the step).
    PerSlot,
    /// Slot 0=A, 1=B, 2=A, 3=B… (zebra static within the step).
    AlternateSlots,
    /// All-A for one chase cycle, then all-B for the next, repeating.
    /// "Cycle" = `total` steps.
    ChasePerColor,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum PaletteRotation {
    /// Whole palette advances by one entry per step.
    PerStep,
    /// Palette advances every `total` steps (one colour per chase cycle).
    PerCycle,
    /// Each slot gets a fixed entry from the palette (slot % len).
    PerSlot,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ColorMode {
    /// Don't write color channels — chaser only animates intensity.
    Disabled,
    /// One fixed RGB applied to every "on" slot.
    Single { color: Rgb },
    /// Two-colour cadence — the workhorse for "cyan/magenta blink" looks.
    TwoColorCadence {
        color_a: Rgb,
        color_b: Rgb,
        cadence: Cadence,
    },
    /// Cycle through a user-defined palette of colours.
    Palette {
        colors: Vec<Rgb>,
        rotation: PaletteRotation,
    },
    /// Continuous spectrum, with `speed` controlling the hue advance per step
    /// (degrees) and `spread` the hue offset between adjacent slots
    /// (1.0 = a full wheel evenly spread across all slots).
    Rainbow { speed: f32, spread: f32 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TempoSource {
    /// Manual numeric BPM.
    Fixed { bpm: f32 },
    // Sub-fases E/F: tap_tempo, sound_reactive, global_tempo.
}

/// Step length expressed as a multiple of one quarter note. The "beat" used
/// for BPM is always a quarter note, so `One` = quarter, `Half` = eighth,
/// `Quarter` = sixteenth, `Two` = half-note, `Four` = whole-note.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Hash)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum Subdivision {
    Quarter,
    Half,
    One,
    Two,
    Four,
}

impl Subdivision {
    /// Step length as a multiple of a quarter note.
    pub fn beats(self) -> f32 {
        match self {
            Subdivision::Quarter => 0.25,
            Subdivision::Half => 0.5,
            Subdivision::One => 1.0,
            Subdivision::Two => 2.0,
            Subdivision::Four => 4.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum FadeCurve {
    /// Linear `t` — constant rate of change.
    Linear,
    /// Smoothstep: slow at the edges, fast in the middle. Most musical-
    /// looking transition for "ambient" use.
    EaseInOut,
    /// `t²` — slow start, snappy end.
    EaseIn,
    /// `1 - (1-t)²` — fast start, gentle landing.
    EaseOut,
    /// Exponential ramp — looks like a capacitor charging.
    Exponential,
    /// Logarithmic ramp — quick rise that tapers off.
    Logarithmic,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct FadeConfig {
    pub enabled: bool,
    /// Fraction of the step duration spent fading. `0.0` snaps (no fade),
    /// `1.0` fades across the whole step. Clamped to `[0, 0.9]` at runtime
    /// so the fade always finishes before the next step transition.
    pub amount: f32,
    pub curve: FadeCurve,
}

impl Default for FadeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            amount: 0.5,
            curve: FadeCurve::EaseInOut,
        }
    }
}

/// Apply a curve to a normalised time `t ∈ [0, 1]`. Returns a value in
/// `[0, 1]` (modulo float rounding); callers use it as the lerp factor.
pub fn apply_fade_curve(curve: FadeCurve, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match curve {
        FadeCurve::Linear => t,
        // Smoothstep `3t² - 2t³`. Symmetric around t = 0.5.
        FadeCurve::EaseInOut => t * t * (3.0 - 2.0 * t),
        FadeCurve::EaseIn => t * t,
        FadeCurve::EaseOut => t * (2.0 - t),
        FadeCurve::Exponential => {
            if t <= 0.0 {
                0.0
            } else {
                // Maps 0→0, 1→1 with most of the change near the end.
                2.0_f32.powf(10.0 * (t - 1.0))
            }
        }
        FadeCurve::Logarithmic => {
            if t >= 1.0 {
                1.0
            } else {
                1.0 - 2.0_f32.powf(-10.0 * t)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct AmbientChaser {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub slots: Vec<ChaserSlot>,
    pub pattern: Pattern,
    pub color_mode: ColorMode,
    pub tempo: TempoSource,
    pub subdivision: Subdivision,
    /// 0.0..=1.0 master multiplier on intensity.
    pub master: f32,
    /// Dim level applied when a slot is "off". 0 = blackout between flashes,
    /// >0 = constant ambient with brighter "on" pulses.
    pub background: u8,
    /// Crossfade between successive steps. `enabled = false` snaps; otherwise
    /// the chaser interpolates intensity and colour between frames.
    /// `serde(default)` so old shows load with no fade.
    #[serde(default)]
    pub fade: FadeConfig,
}

impl AmbientChaser {
    pub fn default_with_id(id: String) -> Self {
        Self {
            id,
            name: "Chaser".to_string(),
            enabled: false,
            slots: Vec::new(),
            pattern: Pattern::AllTogether,
            color_mode: ColorMode::Disabled,
            tempo: TempoSource::Fixed { bpm: 120.0 },
            subdivision: Subdivision::One,
            master: 1.0,
            background: 0,
            fade: FadeConfig::default(),
        }
    }
}

/// The state of a slot at a given step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    On,
    Off,
}

/// Compute the step duration in milliseconds from BPM and subdivision.
pub fn step_duration_ms(bpm: f32, subdivision: Subdivision) -> f32 {
    let safe_bpm = bpm.max(1.0);
    (60_000.0 / safe_bpm) * subdivision.beats()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_duration_120bpm_quarter_is_125ms() {
        // 120 bpm = 500 ms per beat, * 0.25 (sixteenth) = 125 ms.
        assert!((step_duration_ms(120.0, Subdivision::Quarter) - 125.0).abs() < 0.01);
    }

    #[test]
    fn step_duration_60bpm_one_is_1000ms() {
        assert!((step_duration_ms(60.0, Subdivision::One) - 1000.0).abs() < 0.01);
    }

    #[test]
    fn step_duration_clamps_to_safe_bpm() {
        // 0 BPM would otherwise divide by zero.
        let d = step_duration_ms(0.0, Subdivision::One);
        assert!(d.is_finite());
        assert!(d >= 60_000.0);
    }

    #[test]
    fn default_chaser_round_trips_serde() {
        let c = AmbientChaser::default_with_id("c1".into());
        let json = serde_json::to_string(&c).unwrap();
        let back: AmbientChaser = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "c1");
        assert_eq!(back.pattern, Pattern::AllTogether);
        assert_eq!(back.color_mode, ColorMode::Disabled);
    }
}
