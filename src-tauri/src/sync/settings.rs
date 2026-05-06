//! Persisted, non-secret sync settings — Gist id, the toggle for
//! pushing the Outputs section, last-known remote `updated_at` (used
//! for conflict detection), and the local timestamps the UI shows
//! ("last pushed 3 min ago"). The PAT lives in the OS keychain
//! ([crate::sync::secret]); this file is plain JSON.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct SyncSettings {
    /// Gist id (the 32-char hex string from the Gist URL). Empty
    /// before the user creates / picks a gist.
    #[serde(default)]
    pub gist_id: String,
    /// User-friendly label shown in the UI; just the machine name
    /// the user types in. Travels with each push as part of the
    /// payload so on pull you can see "this came from julio's mac".
    #[serde(default)]
    pub machine_label: String,
    /// Default false: outputs are local-machine state. Toggling on
    /// includes them in push/pull (use only when both machines run
    /// the same OS / same hardware). Off means the outputs config
    /// is preserved across pulls.
    #[serde(default)]
    pub include_outputs: bool,
    /// Last `updated_at` we saw on the remote gist. Used to detect
    /// "remote changed since we last synced" before push.
    #[serde(default)]
    pub last_remote_updated: Option<DateTime<Utc>>,
    /// When this machine pushed last.
    #[serde(default)]
    pub last_pushed_at: Option<DateTime<Utc>>,
    /// When this machine pulled last.
    #[serde(default)]
    pub last_pulled_at: Option<DateTime<Utc>>,
    /// Label of the machine that pushed the version we saw on the
    /// remote last time. UI shows this so you know "the latest
    /// remote came from my mac" without guessing.
    #[serde(default)]
    pub last_remote_machine: Option<String>,
}

pub fn settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("dmx-control").join("sync.json"))
}

pub fn load() -> SyncSettings {
    let Some(path) = settings_path() else {
        return SyncSettings::default();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return SyncSettings::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        tracing::warn!(?e, "sync.json corrupt; starting empty");
        SyncSettings::default()
    })
}

pub fn save(settings: &SyncSettings) -> std::io::Result<()> {
    let Some(path) = settings_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(std::io::Error::other)?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp_path = PathBuf::from(tmp);
    std::fs::write(&tmp_path, &bytes)?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}
