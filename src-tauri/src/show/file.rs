use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::fixture::FixtureInstance;
use crate::chaser::AmbientChaser;
use crate::globals::GlobalsConfig;
use crate::movement::MovementGenerator;
use crate::output::config::OutputsConfig;

pub const SHOW_FILE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ShowFileV1 {
    /// Always 1 in this version. Bumping this requires a migration.
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub fixtures: Vec<FixtureInstance>,
    #[serde(default)]
    pub outputs: OutputsConfig,
    /// Ambient Chasers — generative low-priority chases. Optional in the
    /// schema (`#[serde(default)]`) so shows saved before the chaser module
    /// existed load with an empty list, no migration required.
    #[serde(default)]
    pub chasers: Vec<AmbientChaser>,
    /// Legacy single-movement field. Pre-multi-movement show files used
    /// `movement: { … }`; the loader lifts whatever is here into the first
    /// slot of `movements` and clears it. `skip_serializing_if` makes sure
    /// fresh saves never re-emit the legacy key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movement: Option<MovementGenerator>,
    /// Movement Generators. Same shape/role as `chasers`: a list of
    /// independently-editable presets. The runtime enforces "one enabled
    /// at a time" through `toggle_movement_impl`, mirroring chasers.
    #[serde(default)]
    pub movements: Vec<MovementGenerator>,
    /// Global buttons (Blackout + Blind). Persisted so the user's fade
    /// times and blinder fixture list survive a restart.
    #[serde(default)]
    pub globals: GlobalsConfig,
}

impl Default for ShowFileV1 {
    fn default() -> Self {
        Self {
            version: SHOW_FILE_VERSION,
            name: "Untitled".to_string(),
            fixtures: Vec::new(),
            outputs: OutputsConfig::default_starter(),
            chasers: Vec::new(),
            movement: None,
            movements: Vec::new(),
            globals: GlobalsConfig::default(),
        }
    }
}

impl ShowFileV1 {
    /// Lift the legacy single `movement` field into `movements[0]` and
    /// clear it. Idempotent: a no-op when `movement` is `None`. Called
    /// from every load path so old show files come through with the
    /// new schema regardless of whether they reach us via `load`,
    /// autosave, or test fixtures.
    pub fn migrate_legacy_movement(&mut self) {
        if let Some(legacy) = self.movement.take() {
            if self.movements.iter().all(|m| m.id != legacy.id) {
                self.movements.insert(0, legacy);
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShowError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported show file version {0} (this build expects {SHOW_FILE_VERSION})")]
    UnsupportedVersion(u32),
}

pub fn load(path: &Path) -> Result<ShowFileV1, ShowError> {
    let bytes = fs::read(path)?;
    let mut show: ShowFileV1 = serde_json::from_slice(&bytes)?;
    if show.version != SHOW_FILE_VERSION {
        return Err(ShowError::UnsupportedVersion(show.version));
    }
    show.migrate_legacy_movement();
    Ok(show)
}

/// Save with rotating backups. Layout:
/// - `<path>` is the live file
/// - `<path>.bak.1` is the most recent prior version
/// - `<path>.bak.2` and `<path>.bak.3` are older still
///
/// On each save we rotate `.bak.2 → .bak.3`, `.bak.1 → .bak.2`, current → `.bak.1`,
/// then write the new contents atomically (write to temp, rename).
pub fn save(path: &Path, show: &ShowFileV1) -> Result<(), ShowError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    if path.exists() {
        rotate_backups(path)?;
    }

    let json = serde_json::to_vec_pretty(show)?;
    let tmp = tmp_path(path);
    fs::write(&tmp, &json)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".tmp");
    PathBuf::from(s)
}

fn backup_path(path: &Path, slot: u8) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(format!(".bak.{slot}"));
    PathBuf::from(s)
}

fn rotate_backups(path: &Path) -> io::Result<()> {
    let bak1 = backup_path(path, 1);
    let bak2 = backup_path(path, 2);
    let bak3 = backup_path(path, 3);

    if bak2.exists() {
        match fs::rename(&bak2, &bak3) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    if bak1.exists() {
        match fs::rename(&bak1, &bak2) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    fs::rename(path, &bak1)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmpdir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("dmx-show-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn round_trip() {
        let dir = tmpdir();
        let path = dir.join("show.json");
        let show = ShowFileV1 {
            name: "round-trip".into(),
            ..ShowFileV1::default()
        };
        save(&path, &show).unwrap();
        let read = load(&path).unwrap();
        assert_eq!(read.name, "round-trip");
        assert_eq!(read.version, SHOW_FILE_VERSION);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rotation_keeps_three_backups() {
        let dir = tmpdir();
        let path = dir.join("show.json");

        for i in 0..5 {
            let show = ShowFileV1 {
                name: format!("v{i}"),
                ..ShowFileV1::default()
            };
            save(&path, &show).unwrap();
        }

        // Current
        let cur = load(&path).unwrap();
        assert_eq!(cur.name, "v4");
        // bak.1 = v3, bak.2 = v2, bak.3 = v1 (v0 fell off)
        let bak1: ShowFileV1 =
            serde_json::from_slice(&fs::read(backup_path(&path, 1)).unwrap()).unwrap();
        let bak2: ShowFileV1 =
            serde_json::from_slice(&fs::read(backup_path(&path, 2)).unwrap()).unwrap();
        let bak3: ShowFileV1 =
            serde_json::from_slice(&fs::read(backup_path(&path, 3)).unwrap()).unwrap();
        assert_eq!(bak1.name, "v3");
        assert_eq!(bak2.name, "v2");
        assert_eq!(bak3.name, "v1");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_future_version() {
        let dir = tmpdir();
        let path = dir.join("show.json");
        let bogus = serde_json::json!({
            "version": 99,
            "name": "future",
            "fixtures": [],
            "outputs": { "bindings": [] }
        });
        fs::write(&path, serde_json::to_vec(&bogus).unwrap()).unwrap();
        match load(&path) {
            Err(ShowError::UnsupportedVersion(99)) => {}
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
        fs::remove_dir_all(&dir).ok();
    }
}
