//! Session autosave: a single file under the user's config dir that mirrors
//! the current show *and* its associated user path. Restored at startup so a
//! crash or normal close doesn't lose work-in-progress, even on a brand new
//! "Untitled" show that was never explicitly saved.
//!
//! There's a *second* autosave file in the same directory — `engine.json` —
//! holding the live channel values + master. The show file persists on every
//! mutation; the engine snapshot ticks on a background thread so the user's
//! actual on-stage state (slider positions, picked colours…) survives a
//! crash too.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::file::{ShowError, ShowFileV1, SHOW_FILE_VERSION};
use crate::engine::DMX_CHANNELS;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutosaveBlob {
    pub show: ShowFileV1,
    pub path: Option<PathBuf>,
}

pub fn autosave_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("dmx-control").join("autosave.json"))
}

pub fn write_autosave(show: &ShowFileV1, path: Option<&Path>) -> Result<(), ShowError> {
    let Some(target) = autosave_path() else {
        return Ok(());
    };
    write_to(&target, show, path)
}

/// Returns `(show, optional_user_path)` from the autosave file, or `None` if no
/// usable autosave exists. Silently ignores parse / version errors so a broken
/// file never blocks the app from booting.
pub fn read_autosave() -> Option<(ShowFileV1, Option<PathBuf>)> {
    let target = autosave_path()?;
    read_from(&target)
}

fn write_to(target: &Path, show: &ShowFileV1, path: Option<&Path>) -> Result<(), ShowError> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let blob = AutosaveBlob {
        show: show.clone(),
        path: path.map(|p| p.to_path_buf()),
    };
    let bytes = serde_json::to_vec_pretty(&blob)?;
    let mut tmp = target.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, &bytes)?;
    fs::rename(&tmp, target)?;
    Ok(())
}

fn read_from(target: &Path) -> Option<(ShowFileV1, Option<PathBuf>)> {
    let bytes = fs::read(target).ok()?;
    let blob: AutosaveBlob = match serde_json::from_slice(&bytes) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(?e, "autosave file is corrupt; starting fresh");
            return None;
        }
    };
    if blob.show.version != SHOW_FILE_VERSION {
        tracing::warn!(
            version = blob.show.version,
            expected = SHOW_FILE_VERSION,
            "autosave from a different show file version; ignoring"
        );
        return None;
    }
    let mut show = blob.show;
    show.migrate_legacy_movement();
    Some((show, blob.path))
}

// ---- Engine state autosave ----------------------------------------------

/// Snapshot of the live engine state (channel values + master). Persisted
/// independently from the show config so a periodic background thread can
/// dump it without taking show-mutation locks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineAutosave {
    pub master: u8,
    pub universes: Vec<UniverseAutosave>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniverseAutosave {
    pub id: u16,
    /// Always 512 bytes when written. Restore validates length before use.
    pub data: Vec<u8>,
}

pub fn engine_autosave_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("dmx-control").join("engine.json"))
}

pub fn write_engine_autosave(snap: &EngineAutosave) -> Result<(), ShowError> {
    let Some(target) = engine_autosave_path() else {
        return Ok(());
    };
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec(snap)?;
    let mut tmp = target.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, &bytes)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

pub fn read_engine_autosave() -> Option<EngineAutosave> {
    let target = engine_autosave_path()?;
    let bytes = fs::read(&target).ok()?;
    match serde_json::from_slice::<EngineAutosave>(&bytes) {
        Ok(snap) => {
            // Validate channel data length so corrupted files don't crash
            // the engine restore.
            if snap.universes.iter().all(|u| u.data.len() == DMX_CHANNELS) {
                Some(snap)
            } else {
                tracing::warn!("engine autosave has malformed universe data; ignoring");
                None
            }
        }
        Err(e) => {
            tracing::warn!(?e, "engine autosave is corrupt; ignoring");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_target() -> PathBuf {
        std::env::temp_dir().join(format!("dmx-autosave-test-{}.json", uuid::Uuid::new_v4()))
    }

    #[test]
    fn round_trip_preserves_show_and_path() {
        let target = tmp_target();
        let show = ShowFileV1 {
            name: "session-test".into(),
            ..ShowFileV1::default()
        };
        let user_path = PathBuf::from("/tmp/some-show.json");
        write_to(&target, &show, Some(&user_path)).unwrap();

        let (read_show, read_path) = read_from(&target).expect("autosave exists");
        assert_eq!(read_show.name, "session-test");
        assert_eq!(read_path, Some(user_path));
        fs::remove_file(&target).ok();
    }

    #[test]
    fn round_trip_with_no_user_path() {
        let target = tmp_target();
        let show = ShowFileV1::default();
        write_to(&target, &show, None).unwrap();

        let (_, read_path) = read_from(&target).unwrap();
        assert_eq!(read_path, None);
        fs::remove_file(&target).ok();
    }

    #[test]
    fn missing_file_returns_none() {
        let target = tmp_target();
        assert!(read_from(&target).is_none());
    }

    #[test]
    fn corrupt_file_returns_none() {
        let target = tmp_target();
        fs::write(&target, b"not json").unwrap();
        assert!(read_from(&target).is_none());
        fs::remove_file(&target).ok();
    }

    #[test]
    fn future_version_returns_none() {
        let target = tmp_target();
        let bogus = serde_json::json!({
            "show": {
                "version": 99,
                "name": "future",
                "fixtures": [],
                "outputs": { "bindings": [] }
            },
            "path": null
        });
        fs::write(&target, serde_json::to_vec(&bogus).unwrap()).unwrap();
        assert!(read_from(&target).is_none());
        fs::remove_file(&target).ok();
    }
}
