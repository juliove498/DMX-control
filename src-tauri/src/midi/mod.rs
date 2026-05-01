//! MIDI integration. POC layer: list devices, connect input + output to a
//! single named device (Launchpads expose both ports under the same name),
//! receive events into a Tauri event stream, send raw bytes back out.
//!
//! Subsequent phases (not yet wired):
//!   - Learn mode + persisted note↔action mappings.
//!   - Launchpad MK2 RGB feedback (palette + SysEx true-RGB).
//!   - Auto-reconnect on app launch from the show file.

pub mod hub;
pub mod launchpad;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct MidiDeviceInfo {
    pub name: String,
    /// `true` if this device shows up on the input side (most surfaces
    /// expose both, but a few output-only devices like dumb DMX MIDI
    /// boxes wouldn't).
    pub has_input: bool,
    pub has_output: bool,
}

/// Single message captured off the input port. Keeps the raw bytes plus a
/// human-readable label so the UI can render either depending on user
/// preference (and so a future "learn" mode has the bytes intact).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct MidiMessage {
    pub status: u8,
    pub data1: Option<u8>,
    pub data2: Option<u8>,
    pub label: String,
    /// Wall-clock millis since unix epoch — handy for the UI log.
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct MidiStatus {
    pub connected: Option<String>,
    pub has_output: bool,
    pub last_event: Option<MidiMessage>,
}
