//! Procedural looks for the autopilot: a palette, a colour distribution
//! rule, an intensity program, a moving-head program and an optional strobe
//! accent — all evaluated per frame from the musical beat position, so
//! everything the rig does lands on the grid VirtualDJ is playing.
//!
//! Pure functions over resolved `FixtureProfile`s; no engine state, no I/O.

use std::collections::HashMap;

use rand::rngs::StdRng;
use rand::Rng;
use serde::Serialize;
use ts_rs::TS;

use crate::engine::{empty_overlay, ChannelOverlay, DMX_CHANNELS};
use crate::show::fixture::PanTiltRange;

use super::profile::{FixtureKind, FixtureProfile};
use super::{HeadZone, PaletteStyle};

pub type Rgb = [u8; 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum AutoColorMode {
    /// Everyone the same colour.
    Solid,
    /// Palette colours alternate fixture by fixture.
    Alternate,
    /// Colour bands drift left→right over the bars.
    Sweep,
    /// The palette steps one fixture per beat.
    Chase,
    /// Each fixture picks a palette colour per beat.
    Sparkle,
    /// Hue interpolates left→right across the rig between two palette colours.
    Gradient,
    /// Stage left one colour, stage right the other; sides swap every two bars.
    Complement,
    /// Base colour with an accent flash on the downbeat.
    Downbeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum AutoDimProgram {
    Static,
    /// Hit on every beat, decaying.
    PulseBeat,
    /// Hit every two beats.
    PulseHalf,
    /// Sine over a bar.
    Swell,
    /// A lit window runs left→right one fixture per beat.
    ChaseLr,
    /// Random brightness per fixture per beat.
    Sparkle,
    /// Slow sine over two bars.
    Breathe,
    /// Sawtooth ramp up over a bar, snap down on the downbeat.
    Ramp,
    /// Lit window ping-pongs across the rig one fixture per beat.
    Bounce,
    /// Bloom from the centre outwards on every beat.
    Center,
    /// Position-based wave rolling across the rig every two beats.
    Ripple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum AutoMoverProgram {
    Static,
    Circle,
    Figure8,
    SweepH,
    SweepV,
    /// Sine with a phase offset per head — a rolling wave across the rig.
    Wave,
    /// Heads fan out across the zone and breathe slowly.
    Fan,
    /// Alternate heads run opposite diagonals — a crossing X.
    Cross,
    /// Raster scan: slow horizontal sweep, one vertical step per bar.
    Scan,
    /// Snap to a new hashed point every cycle and hold it.
    Stab,
    /// 3:2 Lissajous knot.
    Lissajous,
    /// Slow wandering — summed sines with a phase of its own per head.
    Drift,
    /// Tilt-only bob on each head's fan slot.
    Nod,
    /// Four jumps per cycle — frantic, party energy only.
    Ballyhoo,
    /// One head at a time takes the centre; the rest park high.
    Relay,
}

/// How the moving heads relate to each other inside a program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum AutoMoverSpread {
    /// All heads identical.
    Unison,
    /// Odd heads mirrored — symmetric pairs.
    Mirror,
    /// Each head runs the same figure phase-shifted — a canon.
    Canon,
    /// Each head gets its own phase and its own tempo multiplier.
    Scatter,
}

/// Rhythmic feel applied to the intensity program's clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoRhythm {
    Straight,
    /// Program runs at half speed.
    HalfTime,
    /// Program runs at double speed.
    DoubleTime,
    /// Hits land on the "and" of every beat.
    Offbeat,
    /// 3-3-2: hits on 1, the "and" of 2, and 4 — the Latin / EDM tresillo.
    Tresillo,
    /// Every second beat lands late — a shuffle.
    Swing,
}

/// How the rig splits into two groups for counterpoint and two-tone colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoGrouping {
    /// One group: everybody together.
    All,
    OddEven,
    /// Stage left vs stage right.
    Halves,
    /// Centre fixtures vs the edges.
    InnerOuter,
    /// Neighbouring pairs alternate.
    Pairs,
}

/// What the second group does against the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoCounterpoint {
    /// Same program, same time.
    Together,
    /// Same program, half a bar later.
    Offset,
    /// Group B holds a steady level while A plays.
    Hold,
    /// Call and response: A plays bars 1–2 of each block, B plays bars 3–4.
    Answer,
}

/// Structure-aware accents over the 32-beat phrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoPhraseFx {
    Plain,
    /// The last 8 beats of the phrase climb in level and speed.
    Build,
    /// A bright hit on the first beat of the phrase, decaying over a beat.
    Hit,
    /// Every fourth bar runs double time — a drum fill.
    Fill,
    /// Build into a hit: the full cliché, party energy only.
    BuildHit,
}

const PHRASE: f64 = 32.0;

#[derive(Debug, Clone)]
pub struct Look {
    pub id: u64,
    pub palette: Vec<Rgb>,
    pub style: PaletteStyle,
    pub color: AutoColorMode,
    pub dim: AutoDimProgram,
    /// Intensity floor..ceiling as fractions of full.
    pub base: f32,
    pub depth: f32,
    pub mover: AutoMoverProgram,
    /// Beats per movement cycle.
    pub mover_period: f32,
    pub spread: AutoMoverSpread,
    pub rhythm: AutoRhythm,
    pub grouping: AutoGrouping,
    pub counterpoint: AutoCounterpoint,
    /// Group B takes the next palette colour.
    pub two_tone: bool,
    pub phrase_fx: AutoPhraseFx,
    /// Hue sway amplitude in degrees over the phrase (0 = palette stays put).
    pub hue_drift: f32,
    /// Size of the movement figure inside the zone (0.2..1.0).
    pub mover_amp: f32,
    /// Beats to crossfade from the previous look (0 = hard cut on the downbeat).
    pub transition_beats: f32,
    pub strobe: bool,
    pub seed: u64,
}

impl Look {
    /// A small in-place change so a held look keeps evolving: rotate the
    /// palette, swap the spread, retime the heads, or shift the dim program
    /// inside its family. Never touches the strobe permission.
    pub fn mutate(&mut self, rng: &mut StdRng) {
        match rng.gen_range(0..8) {
            0 => {
                if self.palette.len() > 1 {
                    self.palette.rotate_left(1);
                }
            }
            1 => {
                self.spread = pick(
                    rng,
                    &[
                        AutoMoverSpread::Mirror,
                        AutoMoverSpread::Canon,
                        AutoMoverSpread::Scatter,
                        AutoMoverSpread::Unison,
                    ],
                );
            }
            2 => {
                self.mover_period = if rng.gen::<bool>() {
                    (self.mover_period * 2.0).min(32.0)
                } else {
                    (self.mover_period / 2.0).max(1.0)
                };
            }
            3 => {
                self.dim = match self.dim {
                    AutoDimProgram::PulseBeat => AutoDimProgram::PulseHalf,
                    AutoDimProgram::PulseHalf => AutoDimProgram::PulseBeat,
                    AutoDimProgram::ChaseLr => AutoDimProgram::Bounce,
                    AutoDimProgram::Bounce => AutoDimProgram::Center,
                    AutoDimProgram::Center => AutoDimProgram::ChaseLr,
                    AutoDimProgram::Swell => AutoDimProgram::Ramp,
                    AutoDimProgram::Ramp => AutoDimProgram::Swell,
                    AutoDimProgram::Breathe => AutoDimProgram::Static,
                    AutoDimProgram::Static => AutoDimProgram::Breathe,
                    AutoDimProgram::Sparkle => AutoDimProgram::Ripple,
                    AutoDimProgram::Ripple => AutoDimProgram::Sparkle,
                };
            }
            5 => {
                // A mutation never introduces syncopation mid-look; it only
                // settles a syncopated look back to straight time.
                if self.rhythm != AutoRhythm::Straight {
                    self.rhythm = AutoRhythm::Straight;
                } else if self.palette.len() > 1 {
                    self.palette.rotate_left(1);
                } else {
                    self.mover_amp = if self.mover_amp > 0.7 { 0.5 } else { 1.0 };
                }
            }
            6 => {
                self.grouping = match self.grouping {
                    AutoGrouping::All => AutoGrouping::Halves,
                    AutoGrouping::Halves => AutoGrouping::OddEven,
                    AutoGrouping::OddEven => AutoGrouping::InnerOuter,
                    AutoGrouping::InnerOuter => AutoGrouping::Pairs,
                    AutoGrouping::Pairs => AutoGrouping::All,
                };
                if self.grouping == AutoGrouping::All {
                    self.counterpoint = AutoCounterpoint::Together;
                } else if self.counterpoint == AutoCounterpoint::Together {
                    self.counterpoint = pick(
                        rng,
                        &[
                            AutoCounterpoint::Offset,
                            AutoCounterpoint::Hold,
                            AutoCounterpoint::Answer,
                        ],
                    );
                }
            }
            7 => {
                if rng.gen::<bool>() {
                    self.hue_drift = if self.hue_drift > 0.0 { 0.0 } else { 20.0 };
                } else {
                    self.mover_amp = if self.mover_amp > 0.7 { 0.5 } else { 1.0 };
                }
            }
            _ => {
                self.color = match self.color {
                    AutoColorMode::Solid => AutoColorMode::Downbeat,
                    AutoColorMode::Alternate => AutoColorMode::Complement,
                    AutoColorMode::Complement => AutoColorMode::Alternate,
                    AutoColorMode::Sweep => AutoColorMode::Gradient,
                    AutoColorMode::Gradient => AutoColorMode::Sweep,
                    AutoColorMode::Chase => AutoColorMode::Sparkle,
                    AutoColorMode::Sparkle => AutoColorMode::Chase,
                    AutoColorMode::Downbeat => AutoColorMode::Solid,
                };
            }
        }
        self.seed = rng.gen();
    }

    /// How many of the big traits differ — used to reject near-repeats.
    pub fn distance(&self, other: &Look) -> u32 {
        (self.style != other.style) as u32
            + (self.color != other.color) as u32
            + (self.dim != other.dim) as u32
            + (self.mover != other.mover) as u32
            + (self.rhythm != other.rhythm) as u32
            + (self.grouping != other.grouping) as u32
    }
}

impl Look {
    pub fn palette_hex(&self) -> Vec<String> {
        self.palette
            .iter()
            .map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]))
            .collect()
    }
}

pub fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0).floor() as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    ]
}

fn pick<T: Copy>(rng: &mut StdRng, items: &[T]) -> T {
    items[rng.gen_range(0..items.len())]
}

pub fn palette_for(rng: &mut StdRng, style: PaletteStyle, energy: f32) -> (PaletteStyle, Vec<Rgb>) {
    let resolved = match style {
        PaletteStyle::Auto => {
            if energy < 0.35 {
                pick(
                    rng,
                    &[PaletteStyle::Cold, PaletteStyle::Mono, PaletteStyle::Warm],
                )
            } else if energy < 0.7 {
                pick(
                    rng,
                    &[
                        PaletteStyle::Warm,
                        PaletteStyle::Cold,
                        PaletteStyle::Rainbow,
                    ],
                )
            } else {
                pick(
                    rng,
                    &[
                        PaletteStyle::Rainbow,
                        PaletteStyle::Warm,
                        PaletteStyle::Cold,
                    ],
                )
            }
        }
        other => other,
    };
    let sat = if energy < 0.35 { 0.7 } else { 0.95 };
    let palette = match resolved {
        PaletteStyle::Warm => {
            let base = pick(rng, &[0.0, 12.0, 25.0, 40.0, 330.0, 345.0]);
            vec![
                hsv(base, sat, 1.0),
                hsv(base + 30.0, sat, 1.0),
                hsv(base - 25.0, sat * 0.9, 1.0),
            ]
        }
        PaletteStyle::Cold => {
            let base = pick(rng, &[190.0, 205.0, 220.0, 240.0, 260.0]);
            vec![
                hsv(base, sat, 1.0),
                hsv(base + 35.0, sat, 1.0),
                hsv(base - 30.0, sat * 0.9, 1.0),
            ]
        }
        PaletteStyle::Rainbow => {
            let start = rng.gen_range(0.0..360.0);
            vec![
                hsv(start, 1.0, 1.0),
                hsv(start + 120.0, 1.0, 1.0),
                hsv(start + 240.0, 1.0, 1.0),
            ]
        }
        PaletteStyle::Mono | PaletteStyle::Auto => {
            let h = rng.gen_range(0.0..360.0);
            vec![hsv(h, 1.0, 1.0), hsv(h, 0.45, 1.0), hsv(h, 0.15, 1.0)]
        }
    };
    (resolved, palette)
}

/// A fresh look for the current energy. `phrase_start` biases towards
/// bigger changes (new palette family, bolder program).
pub fn generate(
    rng: &mut StdRng,
    style: PaletteStyle,
    energy: f32,
    allow_strobe: bool,
    allow_movement: bool,
    floor: f32,
) -> Look {
    let (resolved, palette) = palette_for(rng, style, energy);
    let (dims, colors, movers, periods): (
        &[AutoDimProgram],
        &[AutoColorMode],
        &[AutoMoverProgram],
        &[f32],
    ) = if energy < 0.35 {
        (
            &[
                AutoDimProgram::Static,
                AutoDimProgram::Breathe,
                AutoDimProgram::Swell,
                AutoDimProgram::Ripple,
            ],
            &[
                AutoColorMode::Solid,
                AutoColorMode::Alternate,
                AutoColorMode::Sweep,
                AutoColorMode::Gradient,
            ],
            &[
                AutoMoverProgram::Static,
                AutoMoverProgram::SweepH,
                AutoMoverProgram::Wave,
                AutoMoverProgram::Fan,
                AutoMoverProgram::Scan,
                AutoMoverProgram::Drift,
                AutoMoverProgram::Nod,
            ],
            &[16.0, 8.0],
        )
    } else if energy < 0.7 {
        (
            &[
                AutoDimProgram::Swell,
                AutoDimProgram::PulseHalf,
                AutoDimProgram::ChaseLr,
                AutoDimProgram::Bounce,
                AutoDimProgram::Ramp,
                AutoDimProgram::Ripple,
                AutoDimProgram::Static,
            ],
            &[
                AutoColorMode::Alternate,
                AutoColorMode::Sweep,
                AutoColorMode::Chase,
                AutoColorMode::Gradient,
                AutoColorMode::Complement,
                AutoColorMode::Downbeat,
            ],
            &[
                AutoMoverProgram::Circle,
                AutoMoverProgram::SweepH,
                AutoMoverProgram::Wave,
                AutoMoverProgram::Figure8,
                AutoMoverProgram::Fan,
                AutoMoverProgram::Cross,
                AutoMoverProgram::Scan,
                AutoMoverProgram::Lissajous,
                AutoMoverProgram::Stab,
                AutoMoverProgram::Relay,
                AutoMoverProgram::Drift,
            ],
            &[8.0, 4.0],
        )
    } else {
        (
            &[
                AutoDimProgram::PulseBeat,
                AutoDimProgram::ChaseLr,
                AutoDimProgram::Bounce,
                AutoDimProgram::Center,
                AutoDimProgram::Sparkle,
                AutoDimProgram::PulseHalf,
                AutoDimProgram::Ramp,
            ],
            &[
                AutoColorMode::Chase,
                AutoColorMode::Sparkle,
                AutoColorMode::Sweep,
                AutoColorMode::Downbeat,
                AutoColorMode::Complement,
                AutoColorMode::Alternate,
            ],
            &[
                AutoMoverProgram::Circle,
                AutoMoverProgram::Figure8,
                AutoMoverProgram::SweepV,
                AutoMoverProgram::Wave,
                AutoMoverProgram::Cross,
                AutoMoverProgram::Fan,
                AutoMoverProgram::Stab,
                AutoMoverProgram::Ballyhoo,
                AutoMoverProgram::Lissajous,
                AutoMoverProgram::Relay,
            ],
            &[4.0, 2.0],
        )
    };
    // Head relationship: unison is the exception, not the rule — the point
    // is coverage, so most looks spread the heads out.
    let spread = if energy < 0.35 {
        pick(
            rng,
            &[
                AutoMoverSpread::Canon,
                AutoMoverSpread::Mirror,
                AutoMoverSpread::Unison,
            ],
        )
    } else {
        pick(
            rng,
            &[
                AutoMoverSpread::Canon,
                AutoMoverSpread::Scatter,
                AutoMoverSpread::Mirror,
                AutoMoverSpread::Canon,
                AutoMoverSpread::Scatter,
                AutoMoverSpread::Unison,
            ],
        )
    };
    let depth = if energy < 0.35 {
        rng.gen_range(0.3..0.5)
    } else if energy < 0.7 {
        rng.gen_range(0.5..0.8)
    } else {
        rng.gen_range(0.7..1.0)
    };
    let base = (floor.max(0.0)).min(1.0 - depth).max(0.0);
    // The orthogonal "feel" traits: each multiplies the space of looks.
    let band = if energy < 0.35 {
        0
    } else if energy < 0.7 {
        1
    } else {
        2
    };
    let rhythm = pick(
        rng,
        match band {
            // Straight rules: syncopation reads as "late" when it's the norm,
            // so it stays an occasional spice. Offbeat and swing are never
            // generated — they fight a four-on-the-floor kick.
            0 => &[
                AutoRhythm::Straight,
                AutoRhythm::Straight,
                AutoRhythm::Straight,
                AutoRhythm::HalfTime,
            ][..],
            1 => &[
                AutoRhythm::Straight,
                AutoRhythm::Straight,
                AutoRhythm::Straight,
                AutoRhythm::Straight,
                AutoRhythm::Straight,
                AutoRhythm::Tresillo,
            ][..],
            _ => &[
                AutoRhythm::Straight,
                AutoRhythm::Straight,
                AutoRhythm::Straight,
                AutoRhythm::Straight,
                AutoRhythm::Straight,
                AutoRhythm::Tresillo,
                AutoRhythm::DoubleTime,
            ][..],
        },
    );
    let grouping = pick(
        rng,
        match band {
            0 => &[
                AutoGrouping::All,
                AutoGrouping::All,
                AutoGrouping::Halves,
                AutoGrouping::InnerOuter,
            ][..],
            _ => &[
                AutoGrouping::All,
                AutoGrouping::OddEven,
                AutoGrouping::Halves,
                AutoGrouping::InnerOuter,
                AutoGrouping::Pairs,
            ][..],
        },
    );
    let counterpoint = if grouping == AutoGrouping::All {
        AutoCounterpoint::Together
    } else {
        pick(
            rng,
            match band {
                0 => &[
                    AutoCounterpoint::Together,
                    AutoCounterpoint::Together,
                    AutoCounterpoint::Hold,
                ][..],
                1 => &[
                    AutoCounterpoint::Together,
                    AutoCounterpoint::Together,
                    AutoCounterpoint::Answer,
                    AutoCounterpoint::Hold,
                    AutoCounterpoint::Offset,
                ][..],
                _ => &[
                    AutoCounterpoint::Together,
                    AutoCounterpoint::Together,
                    AutoCounterpoint::Answer,
                    AutoCounterpoint::Hold,
                    AutoCounterpoint::Offset,
                ][..],
            },
        )
    };
    let two_tone = grouping != AutoGrouping::All && rng.gen::<f32>() < 0.5;
    let phrase_fx = pick(
        rng,
        match band {
            0 => &[AutoPhraseFx::Plain][..],
            1 => &[
                AutoPhraseFx::Plain,
                AutoPhraseFx::Plain,
                AutoPhraseFx::Build,
                AutoPhraseFx::Fill,
            ][..],
            _ => &[
                AutoPhraseFx::Plain,
                AutoPhraseFx::Build,
                AutoPhraseFx::Hit,
                AutoPhraseFx::Fill,
                AutoPhraseFx::BuildHit,
            ][..],
        },
    );
    let hue_drift = if rng.gen::<f32>() < 0.5 {
        0.0
    } else {
        match band {
            0 => rng.gen_range(8.0..14.0),
            1 => rng.gen_range(15.0..30.0),
            _ => rng.gen_range(20.0..45.0),
        }
    };
    let mover_amp = match band {
        0 => rng.gen_range(0.35..0.7),
        1 => rng.gen_range(0.5..1.0),
        _ => rng.gen_range(0.6..1.0),
    };
    let transition_beats = pick(
        rng,
        match band {
            0 => &[2.0, 2.0, 4.0][..],
            1 => &[0.0, 0.0, 1.0][..],
            _ => &[0.0][..],
        },
    );
    Look {
        id: rng.gen(),
        palette,
        style: resolved,
        color: pick(rng, colors),
        dim: pick(rng, dims),
        base,
        depth,
        mover: if allow_movement {
            pick(rng, movers)
        } else {
            AutoMoverProgram::Static
        },
        mover_period: pick(rng, periods),
        spread,
        rhythm,
        grouping,
        counterpoint,
        two_tone,
        phrase_fx,
        hue_drift,
        mover_amp,
        transition_beats,
        strobe: allow_strobe && energy >= 0.7 && rng.gen::<f32>() < 0.35,
        seed: rng.gen(),
    }
}

/// A clean white pulse per beat with the heads parked: the look used while
/// the operator calibrates the latency offset.
pub fn calibration_look() -> Look {
    Look {
        id: 0,
        palette: vec![[255, 255, 255]],
        style: PaletteStyle::Mono,
        color: AutoColorMode::Solid,
        dim: AutoDimProgram::PulseBeat,
        base: 0.0,
        depth: 1.0,
        mover: AutoMoverProgram::Static,
        mover_period: 4.0,
        spread: AutoMoverSpread::Unison,
        rhythm: AutoRhythm::Straight,
        grouping: AutoGrouping::All,
        counterpoint: AutoCounterpoint::Together,
        two_tone: false,
        phrase_fx: AutoPhraseFx::Plain,
        hue_drift: 0.0,
        mover_amp: 1.0,
        transition_beats: 0.0,
        strobe: false,
        seed: 1,
    }
}

/// Like `generate`, but keeps drawing until the look differs from `avoid`
/// in at least two of its big traits — the cure for "it got monotonous".
pub fn generate_unlike(
    rng: &mut StdRng,
    style: PaletteStyle,
    energy: f32,
    allow_strobe: bool,
    allow_movement: bool,
    floor: f32,
    avoid: &[&Look],
) -> Look {
    let mut best: Option<(u32, Look)> = None;
    for _ in 0..8 {
        let cand = generate(rng, style, energy, allow_strobe, allow_movement, floor);
        let dist = avoid.iter().map(|l| cand.distance(l)).min().unwrap_or(4);
        if dist >= 2 {
            return cand;
        }
        if best.as_ref().map(|(d, _)| dist > *d).unwrap_or(true) {
            best = Some((dist, cand));
        }
    }
    best.map(|(_, l)| l).expect("at least one candidate")
}

fn hash3(a: u64, b: u64, c: u64) -> u64 {
    let mut h = a ^ 0x9e37_79b9_7f4a_7c15;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9) ^ b;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb) ^ c;
    h ^= h >> 31;
    h.wrapping_mul(0xbf58_476d_1ce4_e5b9)
}

fn unit_hash(a: u64, b: u64, c: u64) -> f32 {
    (hash3(a, b, c) % 10_000) as f32 / 10_000.0
}

fn mix(a: Rgb, b: Rgb, f: f32) -> Rgb {
    let f = f.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round() as u8;
    [ch(a[0], b[0]), ch(a[1], b[1]), ch(a[2], b[2])]
}

fn rgb_to_hsv(c: Rgb) -> (f32, f32, f32) {
    let r = c[0] as f32 / 255.0;
    let g = c[1] as f32 / 255.0;
    let b = c[2] as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= 1e-6 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max <= 1e-6 { 0.0 } else { d / max };
    (h, s, max)
}

/// Rotate a colour's hue, leaving whites alone.
fn shift_hue(c: Rgb, dh: f32) -> Rgb {
    let (h, s, v) = rgb_to_hsv(c);
    if s < 0.05 {
        return c;
    }
    hsv(h + dh, s, v)
}

/// Which of the two groups a fixture belongs to (0 = the lead group).
fn group_of(g: AutoGrouping, p: &FixtureProfile, n: usize) -> usize {
    if n < 3 {
        return 0;
    }
    match g {
        AutoGrouping::All => 0,
        AutoGrouping::OddEven => p.index % 2,
        AutoGrouping::Halves => usize::from(p.index * 2 >= n),
        AutoGrouping::InnerOuter => {
            let centre = (n as f32 - 1.0) / 2.0;
            usize::from((p.index as f32 - centre).abs() > n as f32 / 4.0)
        }
        AutoGrouping::Pairs => (p.index / 2) % 2,
    }
}

/// The intensity program's clock after the rhythmic feel is applied.
fn rhythm_time(r: AutoRhythm, beat: f64) -> f64 {
    match r {
        AutoRhythm::Straight => beat,
        AutoRhythm::HalfTime => beat / 2.0,
        AutoRhythm::DoubleTime => beat * 2.0,
        AutoRhythm::Offbeat => beat + 0.5,
        AutoRhythm::Tresillo => {
            // Three "beats" per bar landing at 0, 1.5 and 3.0.
            let bar = (beat / 4.0).floor();
            let b = beat.rem_euclid(4.0);
            let m = if b < 1.5 {
                b / 1.5
            } else if b < 3.0 {
                1.0 + (b - 1.5) / 1.5
            } else {
                2.0 + (b - 3.0)
            };
            bar * 3.0 + m
        }
        AutoRhythm::Swing => {
            let pair = (beat / 2.0).floor();
            let x = beat.rem_euclid(2.0);
            let m = if x < 1.34 {
                x / 1.34
            } else {
                1.0 + (x - 1.34) / 0.66
            };
            pair * 2.0 + m
        }
    }
}

fn color_for(look: &Look, p: &FixtureProfile, n: usize, beat: f64, energy: f32) -> Rgb {
    let len = look.palette.len().max(1);
    let shift = usize::from(look.two_tone && group_of(look.grouping, p, n) == 1);
    let mut rgb = match look.color {
        AutoColorMode::Gradient => {
            let a = look.palette[shift % len];
            let b = look.palette[(1 + shift) % len];
            let f = if n > 1 {
                p.index as f32 / (n - 1) as f32
            } else {
                0.5
            };
            mix(a, b, f)
        }
        mode => {
            let idx = match mode {
                AutoColorMode::Solid | AutoColorMode::Gradient => 0,
                AutoColorMode::Alternate => p.index % len,
                AutoColorMode::Sweep => {
                    let pos = p.index as f64 / n.max(1) as f64;
                    ((pos * len as f64 + beat / 4.0).floor() as usize) % len
                }
                AutoColorMode::Chase => (beat.floor().max(0.0) as usize + p.index) % len,
                AutoColorMode::Sparkle => {
                    (hash3(look.seed, beat.floor().max(0.0) as u64, p.index as u64) % len as u64)
                        as usize
                }
                AutoColorMode::Complement => {
                    let left = p.index * 2 < n;
                    let swap = ((beat / 8.0).floor() as i64).rem_euclid(2) == 1;
                    if left != swap {
                        0
                    } else {
                        1 % len
                    }
                }
                AutoColorMode::Downbeat => {
                    let in_bar = beat.rem_euclid(4.0);
                    if in_bar < 0.5 {
                        1 % len
                    } else {
                        0
                    }
                }
            };
            look.palette[(idx + shift) % len]
        }
    };
    // Hue life over the phrase: a sway for the families, a slow full turn
    // for rainbow — the palette breathes instead of sitting still.
    if look.hue_drift > 0.0 {
        let dh = if look.style == PaletteStyle::Rainbow {
            360.0 * (beat / (PHRASE * 4.0)).rem_euclid(1.0) as f32
        } else {
            let x = (beat / PHRASE).rem_euclid(1.0) as f32;
            let tri = if x < 0.5 {
                4.0 * x - 1.0
            } else {
                3.0 - 4.0 * x
            };
            look.hue_drift * tri
        };
        rgb = shift_hue(rgb, dh);
    }
    // Phrase accents whiten: the hit flashes, the build bleaches its last bars.
    if energy >= 0.3 {
        let bip = beat.rem_euclid(PHRASE) as f32;
        match look.phrase_fx {
            AutoPhraseFx::Hit | AutoPhraseFx::BuildHit if bip < 1.0 => {
                rgb = mix(rgb, [255, 255, 255], 0.7 * (1.0 - bip).powi(2));
            }
            AutoPhraseFx::Build | AutoPhraseFx::BuildHit if bip >= 24.0 => {
                rgb = mix(rgb, [255, 255, 255], 0.3 * (bip - 24.0) / 8.0);
            }
            _ => {}
        }
    }
    rgb
}

/// The raw 0..1 shape of an intensity program at a (possibly warped) clock.
fn dim_shape(look: &Look, p: &FixtureProfile, n: usize, beat: f64) -> f32 {
    let phase = beat.rem_euclid(1.0) as f32;
    match look.dim {
        AutoDimProgram::Static => 0.6,
        AutoDimProgram::PulseBeat => (1.0 - phase).powi(2),
        AutoDimProgram::PulseHalf => (1.0 - (beat / 2.0).rem_euclid(1.0) as f32).powf(1.5),
        AutoDimProgram::Swell => 0.5 * (1.0 + ((beat / 4.0) as f32 * std::f32::consts::TAU).sin()),
        AutoDimProgram::ChaseLr => {
            let pos = beat.floor().rem_euclid(n.max(1) as f64) as usize;
            if p.index == pos {
                1.0
            } else if (p.index as i64 - pos as i64).abs() == 1 {
                0.45
            } else {
                0.12
            }
        }
        AutoDimProgram::Sparkle => {
            unit_hash(look.seed, beat.floor().max(0.0) as u64, p.index as u64)
        }
        AutoDimProgram::Breathe => {
            0.5 * (1.0 + ((beat / 8.0) as f32 * std::f32::consts::TAU).sin())
        }
        AutoDimProgram::Ramp => beat.rem_euclid(4.0) as f32 / 4.0,
        AutoDimProgram::Bounce => {
            let span = (n.max(2) - 1) as i64;
            let step = beat.floor().rem_euclid((span * 2) as f64) as i64;
            let pos = if step <= span { step } else { 2 * span - step };
            let d = (p.index as i64 - pos).abs();
            if d == 0 {
                1.0
            } else if d == 1 {
                0.45
            } else {
                0.12
            }
        }
        AutoDimProgram::Center => {
            let centre = (n.max(1) as f32 - 1.0) / 2.0;
            let dist = (p.index as f32 - centre).abs() / centre.max(1.0);
            // The bloom front travels outwards during the beat.
            let front = phase;
            (1.0 - (dist - front).abs() * 3.0).clamp(0.1, 1.0)
        }
        AutoDimProgram::Ripple => {
            let pos = if n > 1 {
                p.index as f32 / (n - 1) as f32
            } else {
                0.0
            };
            0.5 * (1.0 + (((beat / 2.0) as f32 - pos) * std::f32::consts::TAU).sin())
        }
    }
}

fn intensity_for(look: &Look, p: &FixtureProfile, n: usize, beat: f64, energy: f32) -> f32 {
    let g = group_of(look.grouping, p, n);
    let bip = beat.rem_euclid(PHRASE);
    let bar4 = (beat / 4.0).floor().rem_euclid(4.0) as usize;
    // Structure accents only make sense once the music is actually going.
    let fx_on = energy >= 0.3;
    let fill = fx_on && look.phrase_fx == AutoPhraseFx::Fill && bar4 == 3;
    let build = fx_on
        && matches!(look.phrase_fx, AutoPhraseFx::Build | AutoPhraseFx::BuildHit)
        && bip >= 24.0;
    let hit =
        fx_on && matches!(look.phrase_fx, AutoPhraseFx::Hit | AutoPhraseFx::BuildHit) && bip < 1.0;

    // The program's clock: rhythmic feel first, then fills and the tail of
    // a build run it at double speed.
    let mut clock = rhythm_time(look.rhythm, beat);
    if fill || (build && bip >= 28.0) {
        clock *= 2.0;
    }
    // Counterpoint: what the second group does against the lead group.
    let mut hold: Option<f32> = None;
    match look.counterpoint {
        AutoCounterpoint::Together => {}
        AutoCounterpoint::Offset => {
            if g == 1 {
                clock += 2.0;
            }
        }
        AutoCounterpoint::Hold => {
            if g == 1 {
                hold = Some(0.55);
            }
        }
        AutoCounterpoint::Answer => {
            let a_plays = bar4 < 2;
            if (g == 0) != a_plays {
                hold = Some(0.35);
            }
        }
    }
    let shape = hold.unwrap_or_else(|| dim_shape(look, p, n, clock));
    // Quiet passages flatten the program: near-zero energy settles into a
    // steady glow instead of pulsing or chasing over a soft intro.
    let calm = 1.0 - smoothstep(0.04, 0.16, energy);
    let shape = shape + (0.7 - shape) * calm;
    // The ceiling follows the music continuously: an intro at 10 % energy
    // caps the modulated part around 12 %, a drop opens it fully.
    let ceiling = energy_ceiling(energy);
    let top = look.base + look.depth * ceiling;
    let mut level = look.base + look.depth * shape * ceiling;
    if build {
        let t = ((bip - 24.0) / 8.0) as f32;
        level += (top - level) * t * t * 0.7;
    }
    if hit {
        let env = (1.0 - bip as f32).powi(2);
        level = level.max(top * (0.4 + 0.6 * env));
    }
    level.clamp(0.0, 1.0)
}

/// Hermite step: 0 below `e0`, 1 above `e1`, smooth in between.
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much of a look's modulated intensity the current energy allows.
/// S-shaped so quiet parts stay dim (0.1 → 0.17, 0.3 → 0.39) while a
/// normal party setting (0.6 → 0.82) is close to full and 0.85+ is full.
pub fn energy_ceiling(energy: f32) -> f32 {
    0.15 + 0.85 * smoothstep(0.0, 0.85, energy.clamp(0.0, 1.0))
}

/// Normalised (u, v) inside the head's allowed zone for this beat. The
/// spread decides how the heads relate: unison, mirrored pairs, a canon
/// (phase-shifted copies) or a scatter (own phase and own tempo).
/// `rank` is the head's rank AMONG THE MOVERS (0 = leftmost head), not the
/// fixture's index among all participants: with four PARs before the heads
/// the global indexes clamp to the same value and every head moves in
/// unison — the "no hay canon en ningún momento" bug.
fn mover_uv(look: &Look, rank: usize, movers: usize, beat: f64) -> (f32, f32) {
    let tau = std::f32::consts::TAU;
    let n = movers.max(1);
    let rank = rank.min(n - 1);
    let slot = if n > 1 {
        rank as f32 / (n - 1) as f32
    } else {
        0.5
    };
    let (phase_off, tempo, mirror) = match look.spread {
        AutoMoverSpread::Unison => (0.0, 1.0, false),
        AutoMoverSpread::Mirror => (0.0, 1.0, rank % 2 == 1),
        AutoMoverSpread::Canon => (rank as f32 / n as f32, 1.0, false),
        AutoMoverSpread::Scatter => {
            let h = hash3(look.seed, rank as u64, 7);
            let tempo = [0.5, 1.0, 1.0, 2.0][(h % 4) as usize];
            (((h >> 8) % 1000) as f32 / 1000.0, tempo, (h >> 20) % 2 == 1)
        }
    };
    let t = (beat / look.mover_period as f64) as f32 * tempo + phase_off;
    // Cycle counter for the programs that jump instead of glide.
    let cell = |scale: f32| -> u64 { ((t * scale).floor() as i64).rem_euclid(1 << 20) as u64 };
    let jump = |c: u64| -> (f32, f32) {
        (
            unit_hash(look.seed, c, rank as u64 * 2),
            unit_hash(look.seed, c, rank as u64 * 2 + 1),
        )
    };
    let (u, v) = match look.mover {
        AutoMoverProgram::Static => (0.5, 0.5),
        AutoMoverProgram::Circle => (0.5 + 0.5 * (tau * t).cos(), 0.5 + 0.5 * (tau * t).sin()),
        AutoMoverProgram::Figure8 => (
            0.5 + 0.5 * (tau * t).sin(),
            0.5 + 0.5 * (2.0 * tau * t).sin(),
        ),
        AutoMoverProgram::SweepH => (0.5 + 0.5 * (tau * t).sin(), 0.5),
        AutoMoverProgram::SweepV => (0.5, 0.5 + 0.5 * (tau * t).sin()),
        AutoMoverProgram::Wave => (
            0.5 + 0.5 * (tau * (t + rank as f32 / n as f32)).sin(),
            0.5 + 0.3 * (tau * (t + rank as f32 / n as f32)).sin(),
        ),
        AutoMoverProgram::Fan => {
            // Each head parks on its own slice of the zone and breathes.
            (
                (slot + 0.08 * (tau * t).sin()).clamp(0.0, 1.0),
                0.5 + 0.35 * (tau * t * 0.5).sin(),
            )
        }
        AutoMoverProgram::Cross => {
            // Odd heads run the opposite diagonal — beams cross mid-air.
            let s = 0.5 + 0.5 * (tau * t).sin();
            if rank.is_multiple_of(2) {
                (s, s)
            } else {
                (1.0 - s, s)
            }
        }
        AutoMoverProgram::Scan => {
            // Horizontal sweep; the row steps down one notch per cycle.
            let rows = 4.0;
            let cycle = t.floor();
            let row = (cycle.rem_euclid(rows)) / (rows - 1.0);
            let x = t.rem_euclid(1.0);
            let x = if cycle.rem_euclid(2.0) < 1.0 {
                x
            } else {
                1.0 - x
            };
            (x, 1.0 - row)
        }
        AutoMoverProgram::Stab => jump(cell(1.0)),
        AutoMoverProgram::Ballyhoo => jump(cell(4.0)),
        AutoMoverProgram::Lissajous => (
            0.5 + 0.5 * (tau * t * 1.5).sin(),
            0.5 + 0.5 * (tau * t + std::f32::consts::FRAC_PI_2).sin(),
        ),
        AutoMoverProgram::Drift => {
            // Summed slow sines with a phase of its own per head: a wander
            // that never repeats within a song.
            let ph = |k: u64| unit_hash(look.seed, rank as u64, 40 + k) * tau;
            let u =
                0.5 + 0.32 * (tau * t * 0.37 + ph(0)).sin() + 0.18 * (tau * t * 0.91 + ph(1)).sin();
            let v =
                0.5 + 0.28 * (tau * t * 0.53 + ph(2)).sin() + 0.15 * (tau * t * 1.13 + ph(3)).sin();
            (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0))
        }
        AutoMoverProgram::Nod => (slot, 0.5 + 0.5 * (tau * t).sin()),
        AutoMoverProgram::Relay => {
            // One head takes the centre for a cycle; the others park high.
            let active = (t.floor() as i64).rem_euclid(n as i64) as usize == rank;
            if active {
                (0.5, 0.5 + 0.15 * (tau * t).sin())
            } else {
                (slot, 0.92)
            }
        }
    };
    // Figure size inside the zone.
    let amp = look.mover_amp.clamp(0.2, 1.0);
    let (u, v) = (0.5 + (u - 0.5) * amp, 0.5 + (v - 0.5) * amp);
    if mirror {
        (1.0 - u, v)
    } else {
        (u, v)
    }
}

fn map_to_dmx(norm: f32, range: &PanTiltRange) -> (u8, u8) {
    let (lo, hi) = if range.min <= range.max {
        (range.min as f32, range.max as f32)
    } else {
        (range.max as f32, range.min as f32)
    };
    let v = (lo + norm.clamp(0.0, 1.0) * (hi - lo)).round() as u16;
    ((v >> 8) as u8, (v & 0xff) as u8)
}

fn set(overlay: &mut HashMap<u16, ChannelOverlay>, universe: u16, ch: Option<usize>, value: u8) {
    if let Some(ch) = ch {
        if ch < DMX_CHANNELS {
            overlay.entry(universe).or_insert_with(empty_overlay)[ch] = Some(value);
        }
    }
}

fn zone_for(zones: &HashMap<String, HeadZone>, id: &str) -> HeadZone {
    zones.get(id).cloned().unwrap_or(HeadZone {
        fixture_id: id.to_string(),
        pan_min: 0.3,
        pan_max: 0.7,
        tilt_min: 0.35,
        tilt_max: 0.7,
    })
}

pub struct EvalInput<'a> {
    pub look: &'a Look,
    pub profiles: &'a [FixtureProfile],
    pub beat: f64,
    pub energy: f32,
    pub zones: &'a HashMap<String, HeadZone>,
    pub allow_movement: bool,
    /// Operator's ceiling for the generated intensities ("techo de luz").
    pub max_level: f32,
    /// Manual strobe burst: beat-locked sixteenths on every fixture.
    pub force_burst: bool,
    /// Fixtures a vocabulary scene currently owns — the generator leaves
    /// their colour/intensity alone (movement is still ours).
    pub skip_colour: &'a dyn Fn(&FixtureProfile) -> bool,
}

/// Evaluate a look into an effects overlay.
pub fn evaluate(input: EvalInput<'_>) -> HashMap<u16, ChannelOverlay> {
    let mut out: HashMap<u16, ChannelOverlay> = HashMap::new();
    let look = input.look;
    let n = input.profiles.len();
    // Heads get their own left-to-right rank so spreads (canon, mirror,
    // scatter) and fan slots are computed among movers only.
    let mut mover_order: Vec<usize> = input
        .profiles
        .iter()
        .enumerate()
        .filter(|(_, p)| p.is_mover())
        .map(|(i, _)| i)
        .collect();
    mover_order.sort_by_key(|&i| input.profiles[i].index);
    let movers = mover_order.len();
    let burst = input.force_burst
        || (look.strobe && input.energy >= 0.6 && input.beat.rem_euclid(8.0) < 1.0);

    for (i, p) in input.profiles.iter().enumerate() {
        if !(input.skip_colour)(p) {
            let color = color_for(look, p, n, input.beat, input.energy);
            let mut level = intensity_for(look, p, n, input.beat, input.energy);
            if burst {
                if input.force_burst {
                    // Manual burst: sixteenths locked to the grid, on the
                    // dimmer so every fixture flashes together.
                    level = if (input.beat * 4.0).rem_euclid(1.0) < 0.5 {
                        1.0
                    } else {
                        0.0
                    };
                    set(&mut out, p.universe, p.strobe, 0);
                } else if p.strobe.is_some() {
                    set(&mut out, p.universe, p.strobe, 220);
                    level = 1.0;
                } else {
                    // No strobe channel: emulate with an 8 Hz shutter on the dimmer.
                    level = if (input.beat * 8.0).rem_euclid(1.0) < 0.5 {
                        1.0
                    } else {
                        0.0
                    };
                }
            } else if p.strobe.is_some() {
                set(&mut out, p.universe, p.strobe, 0);
            }
            // Operator's ceiling: map [floor, 1] onto [floor, max] so the
            // floor holds and nothing (hits, bursts included) goes above
            // what the room wants.
            let base = look.base.clamp(0.0, 1.0);
            let top = input.max_level.clamp(0.0, 1.0);
            level = if top <= base {
                top.min(level)
            } else {
                base + (level - base).max(0.0) * (top - base) / (1.0 - base).max(1e-6)
            };
            match p.kind {
                FixtureKind::Strobe => {
                    // Strobe-only devices: only speak during bursts.
                    if !burst {
                        set(&mut out, p.universe, p.strobe, 0);
                    }
                }
                _ => {
                    if p.dimmer.is_some() {
                        set(
                            &mut out,
                            p.universe,
                            p.dimmer,
                            (level * 255.0).round() as u8,
                        );
                        set(&mut out, p.universe, p.red, color[0]);
                        set(&mut out, p.universe, p.green, color[1]);
                        set(&mut out, p.universe, p.blue, color[2]);
                    } else {
                        set(
                            &mut out,
                            p.universe,
                            p.red,
                            (color[0] as f32 * level).round() as u8,
                        );
                        set(
                            &mut out,
                            p.universe,
                            p.green,
                            (color[1] as f32 * level).round() as u8,
                        );
                        set(
                            &mut out,
                            p.universe,
                            p.blue,
                            (color[2] as f32 * level).round() as u8,
                        );
                    }
                    // Keep the additive emitters quiet so palettes stay pure.
                    set(&mut out, p.universe, p.white, 0);
                    set(&mut out, p.universe, p.amber, 0);
                    set(&mut out, p.universe, p.uv, 0);
                    if p.shutter.is_some() && !burst {
                        // Open shutter on heads that have one (most map 255 = open).
                        set(&mut out, p.universe, p.shutter, 255);
                    }
                }
            }
        }
        if input.allow_movement && p.is_mover() {
            let rank = mover_order.iter().position(|&j| j == i).unwrap_or(0);
            let (u, v) = mover_uv(look, rank, movers, input.beat);
            let z = zone_for(input.zones, &p.id);
            let pan_n = z.pan_min + u * (z.pan_max - z.pan_min);
            let tilt_n = z.tilt_min + v * (z.tilt_max - z.tilt_min);
            let (ph, pl) = map_to_dmx(pan_n, &p.pan_range);
            let (th, tl) = map_to_dmx(tilt_n, &p.tilt_range);
            set(&mut out, p.universe, p.pan, ph);
            set(&mut out, p.universe, p.pan_fine, pl);
            set(&mut out, p.universe, p.tilt, th);
            set(&mut out, p.universe, p.tilt_fine, tl);
        }
    }
    out
}

/// Human name for the UI ("Cálida · Pulso a contratiempo · Círculo en canon · mitades en diálogo · build").
pub fn describe(look: &Look) -> String {
    let style = match look.style {
        PaletteStyle::Warm => "Cálida",
        PaletteStyle::Cold => "Fría",
        PaletteStyle::Rainbow => "Arcoíris",
        PaletteStyle::Mono => "Mono",
        PaletteStyle::Auto => "Auto",
    };
    let drift = if look.hue_drift > 0.0 { " deriva" } else { "" };
    let dim = match look.dim {
        AutoDimProgram::Static => "Fija",
        AutoDimProgram::PulseBeat => "Pulso",
        AutoDimProgram::PulseHalf => "Pulso ½",
        AutoDimProgram::Swell => "Oleaje",
        AutoDimProgram::ChaseLr => "Chase",
        AutoDimProgram::Sparkle => "Chispas",
        AutoDimProgram::Breathe => "Respira",
        AutoDimProgram::Ramp => "Rampa",
        AutoDimProgram::Bounce => "Rebote",
        AutoDimProgram::Center => "Centro",
        AutoDimProgram::Ripple => "Ondas",
    };
    let rhythm = match look.rhythm {
        AutoRhythm::Straight => "",
        AutoRhythm::HalfTime => " ½ tiempo",
        AutoRhythm::DoubleTime => " doble",
        AutoRhythm::Offbeat => " a contratiempo",
        AutoRhythm::Tresillo => " tresillo",
        AutoRhythm::Swing => " swing",
    };
    let mov = match look.mover {
        AutoMoverProgram::Static => "Quieto",
        AutoMoverProgram::Circle => "Círculo",
        AutoMoverProgram::Figure8 => "Ocho",
        AutoMoverProgram::SweepH => "Barrido",
        AutoMoverProgram::SweepV => "Cabeceo",
        AutoMoverProgram::Wave => "Ola",
        AutoMoverProgram::Fan => "Abanico",
        AutoMoverProgram::Cross => "Cruce",
        AutoMoverProgram::Scan => "Rastreo",
        AutoMoverProgram::Stab => "Estocada",
        AutoMoverProgram::Lissajous => "Lissajous",
        AutoMoverProgram::Drift => "Deriva",
        AutoMoverProgram::Nod => "Asentir",
        AutoMoverProgram::Ballyhoo => "Frenesí",
        AutoMoverProgram::Relay => "Posta",
    };
    let spread = match look.spread {
        AutoMoverSpread::Unison => "",
        AutoMoverSpread::Mirror => " espejo",
        AutoMoverSpread::Canon => " en canon",
        AutoMoverSpread::Scatter => " disperso",
    };
    let group = match look.grouping {
        AutoGrouping::All => "",
        AutoGrouping::OddEven => "pares/impares",
        AutoGrouping::Halves => "mitades",
        AutoGrouping::InnerOuter => "centro/bordes",
        AutoGrouping::Pairs => "de a pares",
    };
    let cp = match look.counterpoint {
        AutoCounterpoint::Together => "",
        AutoCounterpoint::Offset => " desfasados",
        AutoCounterpoint::Hold => " con sostén",
        AutoCounterpoint::Answer => " en diálogo",
    };
    let fx = match look.phrase_fx {
        AutoPhraseFx::Plain => "",
        AutoPhraseFx::Build => "build",
        AutoPhraseFx::Hit => "golpe",
        AutoPhraseFx::Fill => "relleno",
        AutoPhraseFx::BuildHit => "build + golpe",
    };
    let mut s = format!("{style}{drift} · {dim}{rhythm} · {mov}{spread}");
    if !group.is_empty() {
        let tone = if look.two_tone { " bicolor" } else { "" };
        s.push_str(&format!(" · {group}{cp}{tone}"));
    }
    if !fx.is_empty() {
        s.push_str(&format!(" · {fx}"));
    }
    if look.strobe {
        s.push_str(" · ⚡");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    /// Strip the "feel" traits so a test can look at one program in isolation.
    fn plain(look: &mut Look) {
        look.rhythm = AutoRhythm::Straight;
        look.grouping = AutoGrouping::All;
        look.counterpoint = AutoCounterpoint::Together;
        look.two_tone = false;
        look.phrase_fx = AutoPhraseFx::Plain;
        look.hue_drift = 0.0;
        look.mover_amp = 1.0;
    }

    #[test]
    fn rhythms_warp_the_clock_predictably() {
        assert!((rhythm_time(AutoRhythm::Offbeat, 4.0) - 4.5).abs() < 1e-9);
        assert!((rhythm_time(AutoRhythm::HalfTime, 8.0) - 4.0).abs() < 1e-9);
        assert!((rhythm_time(AutoRhythm::DoubleTime, 2.0) - 4.0).abs() < 1e-9);
        // Tresillo hits: bar positions 0, 1.5 and 3.0 land on whole program beats.
        for b in [0.0, 1.5, 3.0, 4.0, 5.5, 7.0] {
            let t = rhythm_time(AutoRhythm::Tresillo, b);
            assert!((t - t.round()).abs() < 1e-9, "beat {b} → {t}");
        }
        assert!(
            (rhythm_time(AutoRhythm::Tresillo, 4.0) - 3.0).abs() < 1e-9,
            "three per bar"
        );
        // Swing: the second beat of a pair is late, the pair boundary is exact.
        assert!(rhythm_time(AutoRhythm::Swing, 1.0) < 1.0);
        assert!((rhythm_time(AutoRhythm::Swing, 2.0) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn groupings_split_the_rig_in_two() {
        let rig: Vec<FixtureProfile> = (0..8).map(|i| par(&format!("p{i}"), i, i * 4)).collect();
        let groups =
            |g: AutoGrouping| -> Vec<usize> { rig.iter().map(|p| group_of(g, p, 8)).collect() };
        assert_eq!(groups(AutoGrouping::Halves), vec![0, 0, 0, 0, 1, 1, 1, 1]);
        assert_eq!(groups(AutoGrouping::OddEven), vec![0, 1, 0, 1, 0, 1, 0, 1]);
        assert_eq!(
            groups(AutoGrouping::InnerOuter),
            vec![1, 1, 0, 0, 0, 0, 1, 1]
        );
        assert_eq!(groups(AutoGrouping::Pairs), vec![0, 0, 1, 1, 0, 0, 1, 1]);
        assert_eq!(groups(AutoGrouping::All), vec![0; 8]);
        assert_eq!(
            group_of(AutoGrouping::Halves, &rig[1], 2),
            0,
            "tiny rigs never split"
        );
    }

    #[test]
    fn counterpoint_and_two_tone_make_the_groups_differ() {
        let mut rng = StdRng::seed_from_u64(31);
        let mut look = generate(&mut rng, PaletteStyle::Cold, 0.8, false, false, 0.05);
        plain(&mut look);
        look.dim = AutoDimProgram::PulseBeat;
        look.grouping = AutoGrouping::Halves;
        look.counterpoint = AutoCounterpoint::Hold;
        let a = par("a", 0, 0);
        let b = par("b", 3, 12);
        let held: Vec<f32> = (0..8)
            .map(|i| intensity_for(&look, &b, 4, i as f64 * 0.25, 0.8))
            .collect();
        assert!(
            held.iter().all(|v| (v - held[0]).abs() < 1e-6),
            "group B holds steady: {held:?}"
        );
        let lead: Vec<f32> = (0..8)
            .map(|i| intensity_for(&look, &a, 4, i as f64 * 0.25, 0.8))
            .collect();
        assert!(
            lead.iter().any(|v| (v - lead[0]).abs() > 0.05),
            "group A keeps pulsing"
        );
        // Answer: the resting group sits low while the other plays.
        look.counterpoint = AutoCounterpoint::Answer;
        let a_bar1 = intensity_for(&look, &a, 4, 0.0, 0.8);
        let b_bar1 = intensity_for(&look, &b, 4, 0.0, 0.8);
        let a_bar3 = intensity_for(&look, &a, 4, 8.0, 0.8);
        let b_bar3 = intensity_for(&look, &b, 4, 8.0, 0.8);
        assert!(
            a_bar1 > b_bar1 && b_bar3 > a_bar3,
            "call and response: {a_bar1} {b_bar1} / {a_bar3} {b_bar3}"
        );
        // Two-tone: group B shows another palette colour.
        look.counterpoint = AutoCounterpoint::Together;
        look.color = AutoColorMode::Solid;
        look.two_tone = true;
        assert_ne!(
            color_for(&look, &a, 4, 0.0, 0.8),
            color_for(&look, &b, 4, 0.0, 0.8)
        );
    }

    #[test]
    fn phrase_hit_and_build_land_where_the_phrase_says() {
        let mut rng = StdRng::seed_from_u64(8);
        let mut look = generate(&mut rng, PaletteStyle::Warm, 0.9, false, false, 0.05);
        plain(&mut look);
        look.dim = AutoDimProgram::Static;
        look.color = AutoColorMode::Solid;
        look.base = 0.05;
        look.depth = 0.9;
        let p = par("a", 0, 0);
        let steady = intensity_for(&look, &p, 1, 10.0, 0.9);
        look.phrase_fx = AutoPhraseFx::Hit;
        let on_one = intensity_for(&look, &p, 1, 32.02, 0.9);
        let later = intensity_for(&look, &p, 1, 34.0, 0.9);
        assert!(
            on_one > steady + 0.2,
            "hit on the one: {on_one} vs {steady}"
        );
        assert!(
            (later - steady).abs() < 1e-6,
            "back to the program after the hit"
        );
        assert_eq!(
            color_for(&look, &p, 1, 32.0, 0.9),
            mix(look.palette[0], [255, 255, 255], 0.7)
        );
        look.phrase_fx = AutoPhraseFx::Build;
        let early = intensity_for(&look, &p, 1, 25.0, 0.9);
        let late = intensity_for(&look, &p, 1, 31.5, 0.9);
        assert!(
            late > early && late > steady,
            "build climbs: {early} → {late}"
        );
        // Below the calm threshold the accents stay off.
        assert!(
            (intensity_for(&look, &p, 1, 31.5, 0.1) - intensity_for(&look, &p, 1, 10.0, 0.1)).abs()
                < 1e-6
        );
    }

    #[test]
    fn every_mover_program_stays_in_the_unit_square() {
        let mut rng = StdRng::seed_from_u64(12);
        let mut look = generate(&mut rng, PaletteStyle::Cold, 0.8, false, true, 0.0);
        look.mover_period = 4.0;
        let heads: Vec<FixtureProfile> =
            (0..5).map(|i| head(&format!("h{i}"), i, i * 10)).collect();
        let programs = [
            AutoMoverProgram::Static,
            AutoMoverProgram::Circle,
            AutoMoverProgram::Figure8,
            AutoMoverProgram::SweepH,
            AutoMoverProgram::SweepV,
            AutoMoverProgram::Wave,
            AutoMoverProgram::Fan,
            AutoMoverProgram::Cross,
            AutoMoverProgram::Scan,
            AutoMoverProgram::Stab,
            AutoMoverProgram::Lissajous,
            AutoMoverProgram::Drift,
            AutoMoverProgram::Nod,
            AutoMoverProgram::Ballyhoo,
            AutoMoverProgram::Relay,
        ];
        for prog in programs {
            look.mover = prog;
            for spread in [
                AutoMoverSpread::Unison,
                AutoMoverSpread::Mirror,
                AutoMoverSpread::Canon,
                AutoMoverSpread::Scatter,
            ] {
                look.spread = spread;
                for amp in [0.4, 1.0] {
                    look.mover_amp = amp;
                    for step in 0..96 {
                        let beat = step as f64 / 6.0 - 3.0;
                        for i in 0..heads.len() {
                            let (u, v) = mover_uv(&look, i, heads.len(), beat);
                            assert!(
                                (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v),
                                "{prog:?} {spread:?} {u} {v}"
                            );
                        }
                    }
                }
            }
        }
        // Stab holds still inside a cycle and moves between cycles.
        look.mover = AutoMoverProgram::Stab;
        look.spread = AutoMoverSpread::Unison;
        look.mover_amp = 1.0;
        let a = mover_uv(&look, 0, 5, 0.5);
        let b = mover_uv(&look, 0, 5, 3.5);
        let c = mover_uv(&look, 0, 5, 4.5);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn hue_drift_moves_the_palette_but_keeps_white_white() {
        let mut rng = StdRng::seed_from_u64(2);
        let mut look = generate(&mut rng, PaletteStyle::Warm, 0.6, false, false, 0.0);
        plain(&mut look);
        look.color = AutoColorMode::Solid;
        look.hue_drift = 30.0;
        let p = par("a", 0, 0);
        let at_start = color_for(&look, &p, 1, 0.0, 0.6);
        let mid_phrase = color_for(&look, &p, 1, 16.0, 0.6);
        assert_ne!(at_start, mid_phrase, "the hue sways across the phrase");
        assert_eq!(shift_hue([255, 255, 255], 90.0), [255, 255, 255]);
        let (h, s, v) = rgb_to_hsv(hsv(200.0, 0.9, 1.0));
        assert!((h - 200.0).abs() < 1.0 && (s - 0.9).abs() < 0.02 && (v - 1.0).abs() < 0.01);
    }

    #[test]
    fn the_look_space_is_wide() {
        let mut rng = StdRng::seed_from_u64(99);
        let mut names = std::collections::HashSet::new();
        for _ in 0..300 {
            let look = generate(&mut rng, PaletteStyle::Auto, 0.8, true, true, 0.05);
            names.insert(describe(&look));
        }
        assert!(
            names.len() >= 250,
            "only {} distinct looks in 300 draws",
            names.len()
        );
    }

    fn par(id: &str, index: usize, base: usize) -> FixtureProfile {
        FixtureProfile {
            id: id.into(),
            label: id.into(),
            kind: FixtureKind::Par,
            universe: 0,
            dimmer: Some(base),
            red: Some(base + 1),
            green: Some(base + 2),
            blue: Some(base + 3),
            white: None,
            amber: None,
            uv: None,
            strobe: None,
            shutter: None,
            pan: None,
            pan_fine: None,
            tilt: None,
            tilt_fine: None,
            pan_range: PanTiltRange::default_pan(),
            tilt_range: PanTiltRange::default_tilt(),
            position: [index as f32, 0.0],
            index,
        }
    }

    fn head(id: &str, index: usize, base: usize) -> FixtureProfile {
        let mut p = par(id, index, base);
        p.kind = FixtureKind::Mover;
        p.pan = Some(base + 4);
        p.pan_fine = Some(base + 5);
        p.tilt = Some(base + 6);
        p.tilt_fine = Some(base + 7);
        p
    }

    #[test]
    fn generated_look_respects_energy_bands() {
        let mut rng = StdRng::seed_from_u64(7);
        let low = generate(&mut rng, PaletteStyle::Auto, 0.1, true, true, 0.05);
        assert!(!low.strobe, "no strobes at low energy");
        assert!(low.depth <= 0.5);
        assert_eq!(low.palette.len(), 3);
        let high = generate(&mut rng, PaletteStyle::Rainbow, 0.95, false, false, 0.0);
        assert!(!high.strobe, "strobe disallowed by config");
        assert_eq!(high.mover, AutoMoverProgram::Static, "movement disallowed");
        assert!(high.depth >= 0.7);
    }

    #[test]
    fn ceiling_tracks_energy_and_quiet_passages_stay_calm() {
        let mut rng = StdRng::seed_from_u64(21);
        let mut look = generate(&mut rng, PaletteStyle::Warm, 0.9, false, false, 0.05);
        look.dim = AutoDimProgram::PulseBeat;
        look.base = 0.05;
        look.depth = 0.95;
        plain(&mut look);
        let p = par("a", 0, 0);
        let quiet_on = intensity_for(&look, &p, 1, 4.0, 0.05);
        let quiet_late = intensity_for(&look, &p, 1, 4.9, 0.05);
        assert!(quiet_on <= 0.16, "an intro stays dim: {quiet_on}");
        assert!(
            (quiet_on - quiet_late).abs() < 0.02,
            "no pulsing over a quiet intro: {quiet_on} vs {quiet_late}"
        );
        let loud_on = intensity_for(&look, &p, 1, 4.0, 0.95);
        assert!(loud_on >= 0.9, "the drop opens everything up: {loud_on}");
        assert!(energy_ceiling(0.0) < energy_ceiling(0.5));
        assert!(energy_ceiling(0.5) < energy_ceiling(1.0));
    }

    #[test]
    fn pulse_decays_within_the_beat_and_floor_holds() {
        let mut rng = StdRng::seed_from_u64(1);
        let mut look = generate(&mut rng, PaletteStyle::Warm, 0.9, false, false, 0.2);
        look.dim = AutoDimProgram::PulseBeat;
        look.base = 0.2;
        look.depth = 0.8;
        plain(&mut look);
        let p = par("a", 0, 0);
        let on_beat = intensity_for(&look, &p, 1, 4.0, 1.0);
        let late = intensity_for(&look, &p, 1, 4.9, 1.0);
        assert!(on_beat > late, "{on_beat} vs {late}");
        assert!(late >= 0.2 - 1e-6, "floor holds: {late}");
    }

    #[test]
    fn movers_stay_inside_their_zone() {
        let mut rng = StdRng::seed_from_u64(3);
        let mut look = generate(&mut rng, PaletteStyle::Cold, 0.8, false, true, 0.0);
        look.mover = AutoMoverProgram::Circle;
        look.mover_period = 4.0;
        let h = head("h", 0, 10);
        let mut zones = HashMap::new();
        zones.insert(
            "h".to_string(),
            HeadZone {
                fixture_id: "h".into(),
                pan_min: 0.4,
                pan_max: 0.6,
                tilt_min: 0.5,
                tilt_max: 0.55,
            },
        );
        let range = PanTiltRange::default_pan();
        let (lo_hi, _) = map_to_dmx(0.4, &range);
        let (hi_hi, _) = map_to_dmx(0.6, &range);
        for step in 0..64 {
            let beat = step as f64 / 8.0;
            let out = evaluate(EvalInput {
                look: &look,
                profiles: std::slice::from_ref(&h),
                beat,
                energy: 0.8,
                zones: &zones,
                allow_movement: true,
                max_level: 1.0,
                force_burst: false,
                skip_colour: &|_| false,
            });
            let ov = &out[&0];
            let pan = ov[14].expect("pan written");
            assert!(
                pan >= lo_hi.saturating_sub(1) && pan <= hi_hi + 1,
                "pan {pan} outside zone"
            );
            assert!(ov[10].is_some(), "dimmer written");
        }
    }

    #[test]
    fn canon_and_scatter_give_heads_independent_positions() {
        let mut rng = StdRng::seed_from_u64(9);
        let mut look = generate(&mut rng, PaletteStyle::Cold, 0.8, false, true, 0.0);
        look.mover = AutoMoverProgram::Circle;
        look.mover_period = 4.0;
        let heads: Vec<FixtureProfile> =
            (0..4).map(|i| head(&format!("h{i}"), i, i * 10)).collect();
        for spread in [AutoMoverSpread::Canon, AutoMoverSpread::Scatter] {
            look.spread = spread;
            let uvs: Vec<(f32, f32)> = (0..heads.len())
                .map(|i| mover_uv(&look, i, 4, 1.0))
                .collect();
            let distinct = uvs.iter().enumerate().all(|(i, a)| {
                uvs.iter()
                    .enumerate()
                    .all(|(j, b)| i == j || (a.0 - b.0).abs() > 1e-3 || (a.1 - b.1).abs() > 1e-3)
            });
            assert!(distinct, "{spread:?}: heads should not coincide: {uvs:?}");
        }
        look.spread = AutoMoverSpread::Unison;
        let uvs: Vec<(f32, f32)> = (0..heads.len())
            .map(|i| mover_uv(&look, i, 4, 1.0))
            .collect();
        assert!(
            uvs.iter().all(|uv| (uv.0 - uvs[0].0).abs() < 1e-6),
            "unison moves together"
        );
    }

    #[test]
    fn mutate_changes_something_and_unlike_avoids_repeats() {
        let mut rng = StdRng::seed_from_u64(11);
        let base = generate(&mut rng, PaletteStyle::Warm, 0.6, false, true, 0.0);
        let mut mutated = base.clone();
        mutated.mutate(&mut rng);
        let changed = mutated.palette != base.palette
            || mutated.spread != base.spread
            || mutated.mover_period != base.mover_period
            || mutated.dim != base.dim
            || mutated.color != base.color
            || mutated.rhythm != base.rhythm
            || mutated.grouping != base.grouping
            || mutated.counterpoint != base.counterpoint
            || mutated.hue_drift != base.hue_drift
            || mutated.mover_amp != base.mover_amp;
        assert!(changed, "mutate must alter the look");
        let next = generate_unlike(
            &mut rng,
            PaletteStyle::Warm,
            0.6,
            false,
            true,
            0.0,
            &[&base],
        );
        assert!(
            next.distance(&base) >= 2,
            "generate_unlike should differ in ≥2 traits"
        );
    }

    #[test]
    fn heads_behind_a_row_of_pars_still_get_their_own_rank() {
        // Four PARs (indexes 0-3) then two heads (indexes 4-5): the heads
        // must not collapse onto one rank and move in unison.
        let mut rng = StdRng::seed_from_u64(4);
        let mut look = generate(&mut rng, PaletteStyle::Cold, 0.8, false, true, 0.0);
        plain(&mut look);
        look.mover = AutoMoverProgram::Circle;
        look.mover_period = 4.0;
        look.spread = AutoMoverSpread::Canon;
        let mut rig: Vec<FixtureProfile> =
            (0..4).map(|i| par(&format!("p{i}"), i, i * 4)).collect();
        rig.push(head("h1", 4, 40));
        rig.push(head("h2", 5, 60));
        let zones = HashMap::new();
        let mut differ = false;
        for step in 0..16 {
            let out = evaluate(EvalInput {
                look: &look,
                profiles: &rig,
                beat: step as f64 / 4.0,
                energy: 0.8,
                zones: &zones,
                allow_movement: true,
                max_level: 1.0,
                force_burst: false,
                skip_colour: &|_| false,
            });
            let ov = &out[&0];
            if ov[44] != ov[64] || ov[46] != ov[66] {
                differ = true;
            }
        }
        assert!(differ, "canon: the two heads must not share every position");
        look.spread = AutoMoverSpread::Unison;
        let out = evaluate(EvalInput {
            look: &look,
            profiles: &rig,
            beat: 1.0,
            energy: 0.8,
            zones: &zones,
            allow_movement: true,
            max_level: 1.0,
            force_burst: false,
            skip_colour: &|_| false,
        });
        assert_eq!(out[&0][44], out[&0][64], "unison still moves together");
    }

    #[test]
    fn skip_colour_leaves_scene_owned_fixtures_alone_but_still_moves_heads() {
        let mut rng = StdRng::seed_from_u64(5);
        let look = generate(&mut rng, PaletteStyle::Mono, 0.5, false, true, 0.0);
        let h = head("h", 0, 0);
        let zones = HashMap::new();
        let out = evaluate(EvalInput {
            look: &look,
            profiles: std::slice::from_ref(&h),
            beat: 2.0,
            energy: 0.5,
            zones: &zones,
            allow_movement: true,
            max_level: 1.0,
            force_burst: false,
            skip_colour: &|p| p.id == "h",
        });
        let ov = &out[&0];
        assert!(ov[0].is_none(), "dimmer untouched");
        assert!(ov[1].is_none(), "red untouched");
        assert!(ov[4].is_some(), "pan still driven");
    }
}
