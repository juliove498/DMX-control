//! User-customisable Launchpad and Stream Deck button bindings.
//!
//! The hardware layout for each surface is fixed, but what each button
//! *does* and how it *looks* is now driven by these structs instead of
//! by hardcoded tables in the controller modules.
//!
//! Activation rule: when [`ButtonBindings::custom_enabled`] is `false`
//! (the default for fresh shows), the surface controllers fall back to
//! the built-in factory layout. When the user flips it to `true`, the
//! per-surface lists below become the source of truth — empty list =
//! every button is dark / inactive. The UI offers a "Load defaults"
//! action that fills the lists with the factory layout so the operator
//! can start from there and tweak rather than building from scratch.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What pressing a configured button should do.
///
/// The variants mirror the entire set of actions the hardcoded
/// controllers already dispatch (chaser toggle, scene recall,
/// blackout, blind, tap, BPM override toggle) plus the new
/// loop-group play/stop pair that the Sequence Groups feature adds.
/// Indexed variants (`*ByIndex`) refer to the n-th item of the
/// corresponding show list (chaser/movement/scene). They survive a
/// rename of the underlying entity, which makes the "factory layout"
/// stable across shows — useful for muscle memory across rigs.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[ts(export, export_to = "../bindings/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ButtonAction {
    /// Button is configured but disabled. LED stays dark, presses
    /// are ignored. Distinguishes "intentionally empty slot" from
    /// "unconfigured" (which is the absence of any binding for this
    /// physical key).
    #[default]
    None,
    /// Toggle the chaser with the given id. No-op if the id no
    /// longer exists in the show.
    ToggleChaser {
        id: String,
    },
    /// Toggle the n-th chaser (0-based). Used by the factory defaults
    /// so swapping a chaser preserves the binding.
    ToggleChaserByIndex {
        index: u8,
    },
    ToggleMovement {
        id: String,
    },
    ToggleMovementByIndex {
        index: u8,
    },
    /// Recall a scene; pressing the active scene's button releases it.
    RecallScene {
        id: String,
    },
    RecallSceneByIndex {
        index: u8,
    },
    /// Toggle blackout, blind (momentary), tap-tempo, overall BPM
    /// override. These are global and have no id.
    Blackout,
    Blind,
    Tap,
    ToggleOverallBpm,
    /// Nudge the currently-active chaser's BPM by `delta` (negative
    /// values decrease). Mirrors the Launchpad CC up/down arrows.
    BumpActiveChaserBpm {
        delta: f32,
    },
    /// Start a sequence loop group (a playlist of scenes that cycles).
    /// Stop the playing group if a group is active; pressing the
    /// active group's button stops it.
    StartLoopGroup {
        id: String,
    },
    StartLoopGroupByIndex {
        index: u8,
    },
    StopLoopGroup,
    /// Activate a whole-rig snapshot; pressing the active snapshot's
    /// button deactivates it and restores the pre-activation state.
    ToggleSnapshot {
        id: String,
    },
    ToggleSnapshotByIndex {
        index: u8,
    },
}

/// How the LED / tile should reflect the action's "is it running" state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum ButtonActiveMode {
    /// Derive automatically from the action: chaser/movement/scene
    /// toggles light up when their target is enabled, blackout when
    /// blackout is on, blind while held, etc. Default and right for
    /// almost every binding.
    #[default]
    Auto,
    /// Force the button to always show its idle (off) colour. Useful
    /// for momentary triggers like tap-tempo where "active" has no
    /// stable meaning.
    AlwaysIdle,
    /// Force the button to always show its active (on/flash) colour.
    /// Niche; useful for label-only bindings.
    AlwaysActive,
}

/// Per-button binding for the Launchpad MK2 grid.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct LaunchpadBinding {
    /// The MK2 addresses every pad by a unique number. We accept both
    /// grid NoteOn pads (notes 11..=88) and the top-row CCs (104..=111)
    /// behind one byte; `is_cc` disambiguates.
    pub note: u8,
    #[serde(default)]
    pub is_cc: bool,
    pub action: ButtonAction,
    /// Free-text human label. Not drawn on hardware (the MK2 is just
    /// LEDs) but shown in the binding list UI so the operator can
    /// document what's wired up.
    #[serde(default)]
    pub label: String,
    /// MK2 colour palette index for the "off / available" state.
    /// 0 = dark. Values map directly to the MK2 palette §11.
    pub color_dim: u8,
    /// MK2 colour palette index for the "active / flash" state. When
    /// the button is showing as active the hardware blinks between
    /// `color_dim` and `color_bright` at ~1 Hz.
    pub color_bright: u8,
    #[serde(default)]
    pub active_mode: ButtonActiveMode,
}

/// What glyph to draw on the Stream Deck tile.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Hash, Default)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum ButtonIcon {
    /// No glyph — label-only tile.
    None,
    /// "Play" triangle. Default for scene / chaser / loop group.
    #[default]
    Play,
    /// Theatre-curtain glyph. Used for scenes.
    Stage,
    /// Orbiting dot — animates when active. Used for movements.
    Orbit,
    /// Lightning bolt — blackout.
    Bolt,
    /// Eye — blind.
    Eye,
    /// Tap ripples.
    Tap,
    /// Metronome with swinging arm — BPM toggle.
    Metronome,
    /// Looping arrows — sequence loop group.
    Loop,
}

/// Per-button binding for the Stream Deck MK2 / OriginalV2 5×3 grid.
/// `key` is the 0..=14 index used by elgato-streamdeck.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct StreamDeckBinding {
    pub key: u8,
    pub action: ButtonAction,
    /// Short label drawn near the top of the tile. Empty = no label.
    #[serde(default)]
    pub label: String,
    /// (R, G, B) for the idle (off) shade. The tile renderer dims this
    /// further so a saturated input still reads as "not running".
    pub color_off: (u8, u8, u8),
    /// (R, G, B) for the active (on / pulsing) shade.
    pub color_on: (u8, u8, u8),
    #[serde(default)]
    pub icon: ButtonIcon,
    #[serde(default)]
    pub active_mode: ButtonActiveMode,
}

/// What a learned generic MIDI control drives.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MidiControlTarget {
    /// Fire a [`ButtonAction`] on press (NoteOn vel>0, or a CC crossing
    /// the ≥64 threshold). Blind stays momentary: release fires too.
    Action { action: ButtonAction },
    /// Continuous fader: the control's 0–127 value drives the grand
    /// master (0–255). Not persisted per move — the engine autosave
    /// already snapshots the master.
    Master,
}

/// One "MIDI learn" mapping: any note/CC from any controller connected
/// through the MIDI hub, bound to an action or a continuous target.
/// Independent of the Launchpad/Stream Deck factory layouts and always
/// active (no `custom_enabled` gate).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct GenericMidiBinding {
    pub id: String,
    #[serde(default)]
    pub label: String,
    /// `true` = ControlChange, `false` = NoteOn/NoteOff.
    pub is_cc: bool,
    /// 0-based MIDI channel.
    pub channel: u8,
    /// Note number or CC number.
    pub data1: u8,
    pub target: MidiControlTarget,
}

/// Top-level config that lives off the show file.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct ButtonBindings {
    /// When `false` (default), the surface controllers ignore the
    /// per-binding lists and use the built-in factory layout. When
    /// `true`, the lists below are authoritative — a missing entry
    /// means "dark / no action" for that physical button.
    #[serde(default)]
    pub custom_enabled: bool,
    #[serde(default)]
    pub launchpad: Vec<LaunchpadBinding>,
    #[serde(default)]
    pub streamdeck: Vec<StreamDeckBinding>,
    /// MIDI-learn mappings for arbitrary controllers. Always active.
    #[serde(default)]
    pub generic: Vec<GenericMidiBinding>,
}

impl ButtonBindings {
    /// Look up the binding for a Launchpad note, if any. Linear scan
    /// is fine — the list is bounded by 80-ish pads.
    pub fn launchpad_for(&self, note: u8, is_cc: bool) -> Option<&LaunchpadBinding> {
        self.launchpad
            .iter()
            .find(|b| b.note == note && b.is_cc == is_cc)
    }

    /// Look up the binding for a Stream Deck key, if any.
    pub fn streamdeck_for(&self, key: u8) -> Option<&StreamDeckBinding> {
        self.streamdeck.iter().find(|b| b.key == key)
    }
}

// ---- Factory defaults --------------------------------------------------
//
// These reproduce the hardcoded layout that the surface controllers
// used before this feature existed. Loaded into the user's bindings
// list on demand via the "Load defaults" button so the operator can
// tweak from a known starting point.

/// MK2 row-1 pad notes, left-to-right (chasers).
pub const DEFAULT_LP_CHASER_NOTES: [u8; 8] = [11, 12, 13, 14, 15, 16, 17, 18];
/// MK2 row-2 pad notes (movements).
pub const DEFAULT_LP_MOVEMENT_NOTES: [u8; 8] = [21, 22, 23, 24, 25, 26, 27, 28];
/// MK2 row-3 pad notes (scenes).
pub const DEFAULT_LP_SCENE_NOTES: [u8; 8] = [31, 32, 33, 34, 35, 36, 37, 38];
/// MK2 row-4 pad notes (whole-rig snapshots). Row was unused before the
/// snapshot feature, so claiming it keeps every older binding intact.
pub const DEFAULT_LP_SNAPSHOT_NOTES: [u8; 8] = [41, 42, 43, 44, 45, 46, 47, 48];
/// MK2 right-side column (blackout, blind, tap, BPM toggle).
pub const DEFAULT_LP_BLACKOUT_NOTE: u8 = 19;
pub const DEFAULT_LP_BLIND_NOTE: u8 = 29;
pub const DEFAULT_LP_TAP_NOTE: u8 = 39;
pub const DEFAULT_LP_BPM_TOGGLE_NOTE: u8 = 49;

const LP_CHASER_PALETTE: [(u8, u8); 8] = [
    (7, 5),
    (11, 9),
    (15, 13),
    (19, 17),
    (39, 37),
    (43, 41),
    (47, 45),
    (55, 53),
];
const LP_MOVEMENT_PALETTE: [(u8, u8); 8] = [
    (96, 95),
    (82, 81),
    (85, 84),
    (74, 73),
    (34, 33),
    (50, 49),
    (57, 56),
    (100, 99),
];
const LP_SCENE_PALETTE: [(u8, u8); 8] = [
    (1, 3),
    (43, 41),
    (47, 45),
    (78, 79),
    (44, 117),
    (115, 116),
    (113, 114),
    (60, 61),
];
/// One uniform warm-yellow pair for the whole snapshot row — the row
/// reads as a block ("these are my saved looks"), and the flash state
/// matches the dorado halo the desktop UI paints on the active snapshot.
const LP_SNAPSHOT_PALETTE: (u8, u8) = (15, 13);
const LP_BLACKOUT_PALETTE: (u8, u8) = (7, 5);
const LP_BLIND_PALETTE: (u8, u8) = (1, 3);
const LP_TAP_PALETTE: (u8, u8) = (37, 33);
const LP_BPM_TOGGLE_PALETTE: (u8, u8) = (54, 53);

/// Stream Deck factory key layout — copies the constants from
/// `streamdeck::layout` so the defaults match the historical UX.
pub const DEFAULT_SD_CHASER_KEYS: [u8; 4] = [0, 1, 2, 3];
pub const DEFAULT_SD_TAP_KEY: u8 = 4;
pub const DEFAULT_SD_MOVEMENT_KEYS: [u8; 4] = [5, 6, 7, 8];
pub const DEFAULT_SD_BPM_TOGGLE_KEY: u8 = 9;
pub const DEFAULT_SD_SCENE_KEYS: [u8; 3] = [10, 11, 12];
pub const DEFAULT_SD_BLIND_KEY: u8 = 13;
pub const DEFAULT_SD_BLACKOUT_KEY: u8 = 14;

/// `(idle_rgb, active_rgb)` pair used across every Stream Deck palette
/// constant. Aliased so clippy's `type_complexity` lint stops flagging
/// the nested tuple literal at every palette site.
type SdColorPair = ((u8, u8, u8), (u8, u8, u8));

const SD_CHASER_PALETTE: [SdColorPair; 4] = [
    ((50, 0, 0), (255, 30, 30)),
    ((50, 25, 0), (255, 130, 30)),
    ((50, 50, 0), (255, 230, 30)),
    ((0, 50, 0), (30, 200, 30)),
];
const SD_MOVEMENT_PALETTE: [SdColorPair; 4] = [
    ((45, 10, 30), (255, 90, 170)),
    ((25, 0, 40), (160, 50, 230)),
    ((45, 30, 0), (255, 180, 50)),
    ((30, 45, 0), (180, 255, 50)),
];
const SD_SCENE_PALETTE: [SdColorPair; 3] = [
    ((50, 50, 50), (235, 235, 245)),
    ((0, 30, 50), (50, 150, 255)),
    ((30, 0, 50), (180, 80, 255)),
];
const SD_BLACKOUT_COLORS: SdColorPair = ((40, 0, 0), (255, 40, 40));
const SD_BLIND_COLORS: SdColorPair = ((40, 40, 40), (235, 235, 235));
const SD_TAP_COLORS: SdColorPair = ((10, 40, 50), (40, 220, 255));
const SD_BPM_TOGGLE_COLORS: SdColorPair = ((40, 10, 30), (255, 50, 180));

/// Build the factory Launchpad layout. Used by the "Load defaults"
/// UI action; never read at runtime when `custom_enabled = false`
/// (the controllers have their own hardcoded fallback).
pub fn default_launchpad_bindings() -> Vec<LaunchpadBinding> {
    let mut out = Vec::with_capacity(32);
    for i in 0..8 {
        let (dim, bright) = LP_CHASER_PALETTE[i];
        out.push(LaunchpadBinding {
            note: DEFAULT_LP_CHASER_NOTES[i],
            is_cc: false,
            action: ButtonAction::ToggleChaserByIndex { index: i as u8 },
            label: format!("Chaser {}", i + 1),
            color_dim: dim,
            color_bright: bright,
            active_mode: ButtonActiveMode::Auto,
        });
    }
    for i in 0..8 {
        let (dim, bright) = LP_MOVEMENT_PALETTE[i];
        out.push(LaunchpadBinding {
            note: DEFAULT_LP_MOVEMENT_NOTES[i],
            is_cc: false,
            action: ButtonAction::ToggleMovementByIndex { index: i as u8 },
            label: format!("Movement {}", i + 1),
            color_dim: dim,
            color_bright: bright,
            active_mode: ButtonActiveMode::Auto,
        });
    }
    for i in 0..8 {
        let (dim, bright) = LP_SCENE_PALETTE[i];
        out.push(LaunchpadBinding {
            note: DEFAULT_LP_SCENE_NOTES[i],
            is_cc: false,
            action: ButtonAction::RecallSceneByIndex { index: i as u8 },
            label: format!("Scene {}", i + 1),
            color_dim: dim,
            color_bright: bright,
            active_mode: ButtonActiveMode::Auto,
        });
    }
    for (i, &note) in DEFAULT_LP_SNAPSHOT_NOTES.iter().enumerate() {
        out.push(LaunchpadBinding {
            note,
            is_cc: false,
            action: ButtonAction::ToggleSnapshotByIndex { index: i as u8 },
            label: format!("Snapshot {}", i + 1),
            color_dim: LP_SNAPSHOT_PALETTE.0,
            color_bright: LP_SNAPSHOT_PALETTE.1,
            active_mode: ButtonActiveMode::Auto,
        });
    }
    out.push(LaunchpadBinding {
        note: DEFAULT_LP_BLACKOUT_NOTE,
        is_cc: false,
        action: ButtonAction::Blackout,
        label: "Blackout".to_string(),
        color_dim: LP_BLACKOUT_PALETTE.0,
        color_bright: LP_BLACKOUT_PALETTE.1,
        active_mode: ButtonActiveMode::Auto,
    });
    out.push(LaunchpadBinding {
        note: DEFAULT_LP_BLIND_NOTE,
        is_cc: false,
        action: ButtonAction::Blind,
        label: "Blind".to_string(),
        color_dim: LP_BLIND_PALETTE.0,
        color_bright: LP_BLIND_PALETTE.1,
        active_mode: ButtonActiveMode::Auto,
    });
    out.push(LaunchpadBinding {
        note: DEFAULT_LP_TAP_NOTE,
        is_cc: false,
        action: ButtonAction::Tap,
        label: "TAP".to_string(),
        color_dim: LP_TAP_PALETTE.0,
        color_bright: LP_TAP_PALETTE.1,
        active_mode: ButtonActiveMode::AlwaysIdle,
    });
    out.push(LaunchpadBinding {
        note: DEFAULT_LP_BPM_TOGGLE_NOTE,
        is_cc: false,
        action: ButtonAction::ToggleOverallBpm,
        label: "BPM Toggle".to_string(),
        color_dim: LP_BPM_TOGGLE_PALETTE.0,
        color_bright: LP_BPM_TOGGLE_PALETTE.1,
        active_mode: ButtonActiveMode::Auto,
    });
    // Top-row CCs: BPM up/down. Other top-row CCs stay dark by default
    // because their built-in role is to MIRROR the live RGB of the
    // active chaser's slots — a passive visual that doesn't fit the
    // "press = action" model bindings encode. The user can reassign
    // them via the UI if they want a different role.
    out.push(LaunchpadBinding {
        note: 104,
        is_cc: true,
        action: ButtonAction::BumpActiveChaserBpm { delta: 1.0 },
        label: "BPM +1".to_string(),
        color_dim: 0,
        color_bright: 0,
        active_mode: ButtonActiveMode::AlwaysIdle,
    });
    out.push(LaunchpadBinding {
        note: 105,
        is_cc: true,
        action: ButtonAction::BumpActiveChaserBpm { delta: -1.0 },
        label: "BPM -1".to_string(),
        color_dim: 0,
        color_bright: 0,
        active_mode: ButtonActiveMode::AlwaysIdle,
    });
    out
}

/// Build the factory Stream Deck layout.
pub fn default_streamdeck_bindings() -> Vec<StreamDeckBinding> {
    let mut out = Vec::with_capacity(15);
    for i in 0..4 {
        let (off, on) = SD_CHASER_PALETTE[i];
        out.push(StreamDeckBinding {
            key: DEFAULT_SD_CHASER_KEYS[i],
            action: ButtonAction::ToggleChaserByIndex { index: i as u8 },
            label: format!("C{}", i + 1),
            color_off: off,
            color_on: on,
            icon: ButtonIcon::Play,
            active_mode: ButtonActiveMode::Auto,
        });
    }
    for i in 0..4 {
        let (off, on) = SD_MOVEMENT_PALETTE[i];
        out.push(StreamDeckBinding {
            key: DEFAULT_SD_MOVEMENT_KEYS[i],
            action: ButtonAction::ToggleMovementByIndex { index: i as u8 },
            label: format!("M{}", i + 1),
            color_off: off,
            color_on: on,
            icon: ButtonIcon::Orbit,
            active_mode: ButtonActiveMode::Auto,
        });
    }
    for i in 0..3 {
        let (off, on) = SD_SCENE_PALETTE[i];
        out.push(StreamDeckBinding {
            key: DEFAULT_SD_SCENE_KEYS[i],
            action: ButtonAction::RecallSceneByIndex { index: i as u8 },
            label: format!("S{}", i + 1),
            color_off: off,
            color_on: on,
            icon: ButtonIcon::Stage,
            active_mode: ButtonActiveMode::Auto,
        });
    }
    out.push(StreamDeckBinding {
        key: DEFAULT_SD_BLACKOUT_KEY,
        action: ButtonAction::Blackout,
        label: String::new(),
        color_off: SD_BLACKOUT_COLORS.0,
        color_on: SD_BLACKOUT_COLORS.1,
        icon: ButtonIcon::Bolt,
        active_mode: ButtonActiveMode::Auto,
    });
    out.push(StreamDeckBinding {
        key: DEFAULT_SD_BLIND_KEY,
        action: ButtonAction::Blind,
        label: String::new(),
        color_off: SD_BLIND_COLORS.0,
        color_on: SD_BLIND_COLORS.1,
        icon: ButtonIcon::Eye,
        active_mode: ButtonActiveMode::Auto,
    });
    out.push(StreamDeckBinding {
        key: DEFAULT_SD_TAP_KEY,
        action: ButtonAction::Tap,
        label: "TAP".to_string(),
        color_off: SD_TAP_COLORS.0,
        color_on: SD_TAP_COLORS.1,
        icon: ButtonIcon::Tap,
        active_mode: ButtonActiveMode::AlwaysActive,
    });
    out.push(StreamDeckBinding {
        key: DEFAULT_SD_BPM_TOGGLE_KEY,
        action: ButtonAction::ToggleOverallBpm,
        label: String::new(),
        color_off: SD_BPM_TOGGLE_COLORS.0,
        color_on: SD_BPM_TOGGLE_COLORS.1,
        icon: ButtonIcon::Metronome,
        active_mode: ButtonActiveMode::Auto,
    });
    out
}
