//! Elgato Stream Deck control surface integration.
//!
//! Mirror of [`crate::midi::launchpad`] but for the Stream Deck MK.2 (15
//! LCD keys, 5×3 layout). Talks USB-HID directly through the
//! `elgato-streamdeck` crate — no Stream Deck app needs to be running.
//!
//! Why a parallel module instead of folding into MIDI:
//! - The Stream Deck does not speak MIDI. Reusing [`crate::midi::hub`]
//!   would mean shoehorning HID into a MIDI-shaped abstraction.
//! - Both surfaces are independent and can run simultaneously: each
//!   reads the same `ShowState` and dispatches to the same `*_impl`
//!   helpers in [`crate::commands`].
//!
//! Architecture (single worker thread per device):
//! 1. `read_input(Some(50 ms))` polls button state with a short timeout.
//! 2. Between polls, the same thread computes the desired LCD image
//!    targets, diffs against the previous frame, and re-renders only
//!    the keys that changed. ~20 fps effective.
//! 3. On shutdown, an `AtomicBool` flips and the loop exits, blanking
//!    every key on its way out.
//!
//! Layout: see [`layout`].

pub mod controller;
pub mod layout;
pub mod render;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct StreamDeckDeviceInfo {
    pub serial: String,
    /// Human-readable model label (e.g. "Mk2", "XL", "Plus"). The frontend
    /// uses this to tell the user *which* Stream Deck got connected.
    pub kind: String,
    /// Number of keys on this kind. The MK.2 has 15; XL has 32.
    pub key_count: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct StreamDeckStatus {
    /// Serial of the currently-connected device, or `None` if disconnected.
    pub connected: Option<String>,
    pub kind: Option<String>,
    pub key_count: Option<u8>,
}
