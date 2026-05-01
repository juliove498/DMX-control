use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::file::ShowError;
use super::fixture::FixtureDefinition;

/// Built-in starter library, written to disk on first launch if the user's
/// fixture directory is empty so they have something to patch immediately.
pub const SEED_FIXTURES: &[(&str, &str)] = &[
    (
        "generic-rgb-3ch.json",
        include_str!("../../fixtures/library/generic-rgb-3ch.json"),
    ),
    (
        "generic-rgbw-4ch.json",
        include_str!("../../fixtures/library/generic-rgbw-4ch.json"),
    ),
    (
        "generic-dimmer-1ch.json",
        include_str!("../../fixtures/library/generic-dimmer-1ch.json"),
    ),
    (
        "generic-mover-11ch.json",
        include_str!("../../fixtures/library/generic-mover-11ch.json"),
    ),
    (
        "proton-18-rgb.json",
        include_str!("../../fixtures/library/proton-18-rgb.json"),
    ),
];

pub fn library_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("dmx-control").join("fixtures"))
}

/// Subdirectory under `library_dir()` where fixture images are copied when the
/// user picks one through the Library UI. Kept alongside the JSON definitions
/// so backups and exports include the art.
pub fn image_dir() -> Option<PathBuf> {
    library_dir().map(|d| d.join("images"))
}

/// Make sure the fixture directory (and its `images/` subdirectory) exist and
/// the seed library is on disk. Existing user files are never overwritten.
pub fn ensure_seeded(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::create_dir_all(dir.join("images"))?;
    for (name, body) in SEED_FIXTURES {
        let path = dir.join(name);
        if !path.exists() {
            fs::write(&path, body)?;
        }
    }
    Ok(())
}

pub fn load_all(dir: &Path) -> Result<HashMap<String, FixtureDefinition>, ShowError> {
    let mut out = HashMap::new();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let bytes = fs::read(&path)?;
        match serde_json::from_slice::<FixtureDefinition>(&bytes) {
            Ok(def) => {
                if out.contains_key(&def.id) {
                    tracing::warn!(
                        path = %path.display(),
                        id = %def.id,
                        "duplicate fixture id, keeping the first one"
                    );
                    continue;
                }
                out.insert(def.id.clone(), def);
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping invalid fixture file");
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_files_parse() {
        for (name, body) in SEED_FIXTURES {
            serde_json::from_str::<FixtureDefinition>(body)
                .unwrap_or_else(|e| panic!("seed {name} is invalid: {e}"));
        }
    }
}
