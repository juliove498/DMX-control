//! Per-track cue lists: the app's own POI system.
//!
//! Instead of authoring action POIs inside VirtualDJ (create point,
//! switch type, type the `os2l_button` command…), the operator marks
//! cues in the app's own cue editor — with its own audio transport, so
//! VirtualDJ doesn't even need to be running — and picks the action
//! from the same selector the OS2L mappings use. At show time the cue
//! follower (`vdj::cues`) tracks VDJ's playhead over the Network
//! Control HTTP endpoint and fires each cue when the transport crosses
//! it; VirtualDJ's own database is never touched (it would overwrite
//! external edits on exit anyway).
//!
//! Positions: **seconds are the source of truth** (`pos_secs`) — that
//! makes cues independent of VDJ's beat grid, and the follower fires
//! on the elapsed time VDJ reports. `beat` is kept for the optional
//! grid overlay / snapping and for the VDJ POI exporter; cues from
//! before `pos_secs` existed (pos_secs == 0) still fire by beat.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::button_bindings::ButtonAction;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct TrackCue {
    pub id: String,
    /// Position on the track's beat grid (fractional beats from the
    /// grid origin — same coordinate VDJ shows in its POI editor).
    /// Derived from `pos_secs` + the set's grid when one is known.
    pub beat: f64,
    /// Position in track seconds — the primary coordinate. 0 only for
    /// cues marked before this field existed (those fire by beat and
    /// the VDJ exporter skips them with a warning).
    #[serde(default)]
    pub pos_secs: f64,
    /// Optional label ("Entrada bailarinas").
    #[serde(default)]
    pub name: String,
    pub action: ButtonAction,
    /// Optional marker colour override (`#rrggbb`). Empty/None = the
    /// editor colours the marker by action kind.
    #[serde(default)]
    pub color: Option<String>,
}

/// The track's beat grid as the editor understands it: a tempo and the
/// audio-file time of the first beat. Seeded from VirtualDJ while it
/// plays (bpm + beatpos ↔ elapsed time), estimated from the audio, or
/// typed/tapped by the operator.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct TrackGrid {
    pub bpm: f64,
    /// Seconds into the file where beat 0 falls.
    pub first_beat_secs: f64,
}

impl TrackGrid {
    pub fn beat_at(&self, secs: f64) -> f64 {
        if self.bpm <= 0.0 {
            return 0.0;
        }
        (secs - self.first_beat_secs) * self.bpm / 60.0
    }
    pub fn secs_at(&self, beat: f64) -> f64 {
        if self.bpm <= 0.0 {
            return self.first_beat_secs;
        }
        self.first_beat_secs + beat * 60.0 / self.bpm
    }
}

/// Structural marker on the timeline ("Intro", "Estribillo", "Salida
/// bailarines"): display-only orientation for the operator and the
/// event organiser, never fires anything.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct TrackSection {
    pub id: String,
    pub pos_secs: f64,
    pub name: String,
}

/// A show entity (scene, snapshot, loop…) that was created for — and
/// belongs to — one track. The editor scopes its target pickers to the
/// track's own entities by default so an act in a theatre show never
/// fires another act's look by accident; reusing another track's
/// entity is an explicit choice and is flagged in the UI.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct OwnedTarget {
    /// "scene" | "snapshot" | "loop" | "chaser" | "movement"
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct TrackCueSet {
    /// Full path as VirtualDJ reports it (`get_filepath`) — the key
    /// that matches a cue list to the loaded track. Tracks added from
    /// the editor use their local path.
    pub file_path: String,
    /// Display name (file basename captured when the set was created;
    /// editable).
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cues: Vec<TrackCue>,
    #[serde(default)]
    pub grid: Option<TrackGrid>,
    #[serde(default)]
    pub sections: Vec<TrackSection>,
    /// Track length, captured from the decoded audio (0 = unknown).
    #[serde(default)]
    pub duration_secs: f64,
    /// Entities created for this track from the cue editor.
    #[serde(default)]
    pub owned: Vec<OwnedTarget>,
}

impl TrackCueSet {
    /// Canonical order: by time, legacy beat-only cues ordered by beat
    /// among themselves after the timed ones.
    pub fn sort_cues(&mut self) {
        self.cues.sort_by(|a, b| {
            let ka = if a.pos_secs > 0.0 {
                (0, a.pos_secs)
            } else {
                (1, a.beat)
            };
            let kb = if b.pos_secs > 0.0 {
                (0, b.pos_secs)
            } else {
                (1, b.beat)
            };
            ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
        });
        self.sections.sort_by(|a, b| {
            a.pos_secs
                .partial_cmp(&b.pos_secs)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_round_trips_between_beats_and_seconds() {
        let g = TrackGrid {
            bpm: 128.0,
            first_beat_secs: 0.35,
        };
        let secs = g.secs_at(64.0);
        assert!((secs - 30.35).abs() < 1e-9);
        assert!((g.beat_at(secs) - 64.0).abs() < 1e-9);
    }

    #[test]
    fn sort_puts_timed_cues_first_then_legacy_by_beat() {
        let cue = |id: &str, secs: f64, beat: f64| TrackCue {
            id: id.into(),
            beat,
            pos_secs: secs,
            name: String::new(),
            action: ButtonAction::Blackout,
            color: None,
        };
        let mut set = TrackCueSet {
            file_path: "x".into(),
            title: "x".into(),
            cues: vec![
                cue("legacy-late", 0.0, 40.0),
                cue("late", 20.0, 0.0),
                cue("legacy-early", 0.0, 8.0),
                cue("early", 5.0, 0.0),
            ],
            grid: None,
            sections: vec![],
            duration_secs: 0.0,
            owned: vec![],
        };
        set.sort_cues();
        let ids: Vec<&str> = set.cues.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["early", "late", "legacy-early", "legacy-late"]);
    }
}
