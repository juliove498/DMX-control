//! Stream Deck MK.2 fixed layout (5 columns × 3 rows = 15 keys).
//!
//! Mirrors the Launchpad MK2 surface as closely as the smaller key count
//! allows. The MK.2 sacrifices keys 6–8 of each row (chasers, movements,
//! scenes) and the BPM ±1 round buttons compared to the Launchpad — those
//! become candidates for a future learn-mode that lets the operator pick
//! which N entities to expose.
//!
//! Key indices on the MK.2 are read row-major, top-left = 0:
//! ```text
//!   0  1  2  3  4    ← chasers 1..5
//!   5  6  7  8  9    ← movements 1..5
//!  10 11 12 13 14    ← scenes 1..3, blind, blackout
//! ```

/// Row 1: chaser slots 0..5 (first five chasers in the show).
pub const CHASER_KEYS: [u8; 5] = [0, 1, 2, 3, 4];

/// Row 2: movement generator slots 0..5.
pub const MOVEMENT_KEYS: [u8; 5] = [5, 6, 7, 8, 9];

/// Row 3 left: scene slots 0..3 — press recalls; press of the active
/// scene releases it (same toggle semantics as the Launchpad).
pub const SCENE_KEYS: [u8; 3] = [10, 11, 12];

/// Row 3, fourth key: momentary blind. Press = blind on, release = off.
pub const BLIND_KEY: u8 = 13;

/// Row 3, fifth key: toggle blackout (uses the configured fade time).
pub const BLACKOUT_KEY: u8 = 14;

/// `(off_dim_rgb, on_bright_rgb)` palette for chaser keys. Hand-picked
/// to match the Launchpad's `CHASER_PAD_PALETTE` family — operator
/// muscle memory carries over: red = chaser 1, orange = chaser 2, etc.
pub const CHASER_PALETTE: [((u8, u8, u8), (u8, u8, u8)); 5] = [
    ((50, 0, 0), (255, 30, 30)),    // red
    ((50, 25, 0), (255, 130, 30)),  // orange
    ((50, 50, 0), (255, 230, 30)),  // yellow
    ((0, 50, 0), (30, 200, 30)),    // green
    ((0, 50, 50), (30, 200, 220)),  // cyan
];

/// Movements get a distinct family (warm/pastel) so a glance at the
/// surface tells chasers vs. movements apart even when both are lit.
pub const MOVEMENT_PALETTE: [((u8, u8, u8), (u8, u8, u8)); 5] = [
    ((45, 10, 30), (255, 90, 170)),  // pink
    ((25, 0, 40), (160, 50, 230)),   // purple
    ((45, 30, 0), (255, 180, 50)),   // amber
    ((30, 45, 0), (180, 255, 50)),   // lime
    ((0, 30, 30), (50, 200, 200)),   // teal
];

/// Scene palette — cool side of the wheel, separate from chasers and
/// movements. Three slots only; we sacrifice the last 5 scene keys vs.
/// the Launchpad.
pub const SCENE_PALETTE: [((u8, u8, u8), (u8, u8, u8)); 3] = [
    ((50, 50, 50), (235, 235, 245)),  // white
    ((0, 30, 50), (50, 150, 255)),    // blue
    ((30, 0, 50), (180, 80, 255)),    // violet
];

/// Blackout: dim red / bright red.
pub const BLACKOUT_COLORS: ((u8, u8, u8), (u8, u8, u8)) = ((40, 0, 0), (255, 40, 40));

/// Blind: dim white / bright white.
pub const BLIND_COLORS: ((u8, u8, u8), (u8, u8, u8)) = ((40, 40, 40), (235, 235, 235));
