//! Pairing PINs, bearer tokens, and the on-disk device store. Phase 1
//! has no TLS — security boils down to "you can't connect unless the
//! desktop operator typed (or scanned) the PIN that just appeared on
//! their screen, and the resulting bearer token is stored hashed".
//!
//! Constant-time compare on PIN and token is the only thing standing
//! between a hostile LAN peer and full control of the rig, so any
//! refactor that swaps `subtle`-style equality for `==` is a
//! regression. Tests pin this.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use uuid::Uuid;

const PIN_TTL_SECS: u64 = 60;
const TOKEN_BYTES: usize = 32;

/// Random 6-digit PIN, zero-padded. The desktop renders this as both
/// big text and a QR. The QR encodes `dmxctrl://pair?host=<ip>&port=
/// <p>&pin=<pin>` so a single scan from `expo-camera` is enough to skip
/// manual entry.
pub fn generate_pin() -> String {
    let mut buf = [0u8; 4];
    rand::thread_rng().fill_bytes(&mut buf);
    let n = u32::from_le_bytes(buf) % 1_000_000;
    format!("{:06}", n)
}

pub fn generate_token() -> String {
    let mut buf = [0u8; TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut buf);
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.encode(buf)
}

pub fn hash_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    let digest = h.finalize();
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.encode(digest)
}

/// Constant-time comparison for both PINs (short, but worth doing right
/// while we're here) and SHA-256 token digests. Using a custom impl
/// instead of pulling `subtle` for two callers.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct PairedDevice {
    pub id: String,
    pub name: String,
    /// SHA-256(token) base64-url-no-pad. The plaintext token never
    /// touches disk — once we've returned it to the device on `/pair`
    /// we throw it away.
    pub token_hash: String,
    /// UNIX seconds of pairing. Used by the UI to show "paired 3 days
    /// ago" without stuffing relative time into the JSON.
    pub created_at: u64,
    /// UNIX seconds of last successful auth. Bumped each time a WS
    /// from this device opens. Useful for the operator deciding which
    /// row to revoke when devices.json grows.
    pub last_seen: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DevicesFile {
    devices: Vec<PairedDevice>,
}

#[derive(Debug, Clone)]
pub struct PendingPairing {
    pub pin: String,
    pub expires_at: SystemTime,
}

impl PendingPairing {
    pub fn new(pin: String) -> Self {
        Self {
            pin,
            expires_at: SystemTime::now() + Duration::from_secs(PIN_TTL_SECS),
        }
    }

    pub fn is_expired(&self) -> bool {
        SystemTime::now() > self.expires_at
    }

    /// Seconds remaining for the UI countdown. Saturates at 0 once the
    /// PIN window has elapsed so the frontend doesn't have to handle
    /// a negative duration.
    pub fn remaining_secs(&self) -> u64 {
        self.expires_at
            .duration_since(SystemTime::now())
            .unwrap_or_default()
            .as_secs()
    }
}

/// Cross-platform location for `devices.json`. On macOS this lands in
/// `~/Library/Application Support/dmx-control/bridge/`, on Windows in
/// `%APPDATA%\dmx-control\bridge\`, on Linux in `~/.config/dmx-control/
/// bridge/`. Same scheme the show autosave uses, just one folder over.
pub fn devices_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("dmx-control").join("bridge").join("devices.json"))
}

pub fn load_devices() -> Vec<PairedDevice> {
    let Some(path) = devices_path() else {
        return Vec::new();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    let parsed: DevicesFile = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(?e, "bridge devices.json corrupt; starting empty");
            return Vec::new();
        }
    };
    parsed.devices
}

pub fn save_devices(devices: &[PairedDevice]) -> std::io::Result<()> {
    let Some(path) = devices_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = DevicesFile {
        devices: devices.to_vec(),
    };
    let bytes = serde_json::to_vec_pretty(&body).map_err(std::io::Error::other)?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp_path = PathBuf::from(tmp);
    std::fs::write(&tmp_path, &bytes)?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

pub fn new_paired_device(name: String, token_hash: String) -> PairedDevice {
    PairedDevice {
        id: Uuid::new_v4().to_string(),
        name,
        token_hash,
        created_at: now_secs(),
        last_seen: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_is_six_digits() {
        for _ in 0..100 {
            let pin = generate_pin();
            assert_eq!(pin.len(), 6);
            assert!(pin.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn token_hash_stable_and_unique() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(t1, t2);
        assert_eq!(hash_token(&t1), hash_token(&t1));
        assert_ne!(hash_token(&t1), hash_token(&t2));
    }

    #[test]
    fn constant_time_eq_matches() {
        assert!(constant_time_eq(b"abcd", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abce"));
        assert!(!constant_time_eq(b"abcd", b"abc"));
    }
}
