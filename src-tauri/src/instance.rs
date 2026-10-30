//! Machine-local settings: things that must NOT travel with the show or
//! the mirror replication because they describe *this* laptop — its
//! live/mirror role and which optional modules are switched on here.
//!
//! Stored as `instance.json` next to the autosave. Every field has a
//! serde default so older files (role only) keep loading.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

use crate::mirror::Role;

pub const FEATURES_EVENT: &str = "features:changed";

/// Optional modules. Off = the UI never mounts them and nothing they
/// need (audio decoding, waveform caches, listeners) gets allocated.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct Features {
    /// The cue editor (planning mode): own audio transport, big
    /// zoomable waveform, cue authoring. Heavy while open — a decoded
    /// track lives in the webview's audio element — so it's opt-in.
    #[serde(default)]
    pub cue_editor: bool,
    /// The 3D preview tab. On by default; switch it off on a weak machine
    /// so the WebGL scene (bloom, volumetric beams) never even mounts.
    #[serde(default = "default_true")]
    pub preview3d: bool,
    /// "low" | "medium" | "high" — render budget for the 3D preview.
    #[serde(default = "default_quality")]
    pub preview3d_quality: String,
}

fn default_true() -> bool {
    true
}
fn default_quality() -> String {
    "medium".to_string()
}

impl Default for Features {
    fn default() -> Self {
        Self {
            cue_editor: false,
            preview3d: true,
            preview3d_quality: default_quality(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstanceFile {
    #[serde(default)]
    pub role: Role,
    #[serde(default)]
    pub features: Features,
}

fn instance_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("dmx-control").join("instance.json"))
}

pub fn load() -> InstanceFile {
    let Some(p) = instance_path() else {
        return InstanceFile::default();
    };
    match std::fs::read(&p) {
        Ok(bytes) => serde_json::from_slice::<InstanceFile>(&bytes).unwrap_or_default(),
        Err(_) => InstanceFile::default(),
    }
}

pub fn save(inst: &InstanceFile) {
    let Some(p) = instance_path() else { return };
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_vec_pretty(inst) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(&p, bytes) {
                tracing::warn!(target: "dmx::instance", error = %e, "could not persist instance settings");
            }
        }
        Err(e) => {
            tracing::warn!(target: "dmx::instance", error = %e, "could not serialize instance settings")
        }
    }
}

#[tauri::command]
pub fn instance_features() -> Features {
    load().features
}

#[tauri::command]
pub fn set_instance_features(app: AppHandle, features: Features) -> Features {
    let mut inst = load();
    let turning_editor_off = inst.features.cue_editor && !features.cue_editor;
    inst.features = features.clone();
    save(&inst);
    if turning_editor_off {
        // Free what the editor may have left behind.
        crate::audio_waveform::clear_cache();
    }
    tracing::info!(target: "dmx::instance", ?features, "instance features updated");
    let _ = app.emit(FEATURES_EVENT, features.clone());
    features
}
