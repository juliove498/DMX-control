//! Export the show's per-track cues into VirtualDJ's `database.xml` as
//! real action POIs, so the operator sees them on VDJ's waveform while
//! mixing and VDJ itself fires them over OS2L.
//!
//! Written POI shape (copied verbatim from a real VDJ 2023 database):
//! `<Poi Name="…" Pos="4.369603" Num="-1" Color="4278255360"
//!  Type="action" Action="os2l_button &apos;cue:<id>&apos;" />`
//!
//! The OS2L name is `cue:<cue-id>` — resolved by our OS2L server back
//! to the cue's action, so renaming scenes or editing a cue's action in
//! the app never requires re-exporting.
//!
//! Constraints respected:
//! - VirtualDJ keeps the database in memory and rewrites the file on
//!   exit: exporting while it runs would be silently lost (or worse).
//!   We refuse when the VDJ process is alive.
//! - Only POIs we own are touched: entries whose `Action` contains the
//!   `os2l_button 'cue:` marker are replaced; the operator's manual
//!   POIs (hot cues, beatgrid, automix, their own action POIs) pass
//!   through untouched.
//! - A timestamped backup of the database is written next to it before
//!   any modification.

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::show::track_cues::TrackCueSet;

/// Marker distinguishing our POIs from hand-made ones. Kept in the
/// Action attribute (`os2l_button &apos;cue:<id>&apos;`).
const OURS_MARKER: &str = "os2l_button &apos;cue:";

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct PoiExportReport {
    pub database_path: String,
    pub backup_path: String,
    /// Tracks found in the database and updated.
    pub tracks_written: u32,
    pub cues_written: u32,
    /// Track titles that exist in the show but not in VDJ's database
    /// (VDJ has never scanned them) — nothing written for those.
    pub tracks_missing: Vec<String>,
    /// Cues skipped because they lack a seconds position (marked with
    /// an old build) — re-mark them.
    pub cues_without_secs: Vec<String>,
}

/// Candidate database locations, most likely first.
pub fn default_database_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = dirs::home_dir() {
        out.push(home.join("Documents/VirtualDJ/database.xml"));
        out.push(home.join("Library/Application Support/VirtualDJ/database.xml"));
    }
    out
}

pub fn find_database() -> Option<PathBuf> {
    default_database_paths().into_iter().find(|p| p.exists())
}

/// Best-effort "is VirtualDJ running" probe (macOS/Linux: pgrep).
pub fn vdj_running() -> bool {
    std::process::Command::new("pgrep")
        .args(["-if", "virtualdj"])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

/// VirtualDJ's beat grid for one song, straight from its database:
/// `(seconds per beat, time of beat 0)`. VDJ stores `Scan Bpm` as a
/// beat *length* in seconds and `Scan Phase` as the grid origin — so
/// `secs = phase + beat * bpm`, the same coordinate as `get_beatpos`
/// over HTTP and `pos` in the OS2L beat events.
pub fn scan_grid(db: &str, file_path: &str) -> Option<(f64, f64)> {
    let needle = format!("FilePath=\"{}\"", xml_escape(file_path));
    let at = db.find(&needle)?;
    let end = db[at..].find("</Song>").map(|e| at + e).unwrap_or(db.len());
    let song = &db[at..end];
    let scan_at = song.find("<Scan ")?;
    let scan_end = song[scan_at..].find('>').map(|e| scan_at + e)?;
    let scan = &song[scan_at..scan_end];
    let beat_secs: f64 = xml_attr(scan, "Bpm")?.parse().ok()?;
    let phase: f64 = xml_attr(scan, "Phase")?.parse().ok()?;
    ((0.05..5.0).contains(&beat_secs) && phase.is_finite()).then_some((beat_secs, phase))
}

fn xml_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let s = tag.find(&key)? + key.len();
    let e = tag[s..].find('"')? + s;
    Some(&tag[s..e])
}

/// Read the beat grid of `file_path` from the VirtualDJ database on
/// disk (tens of MB — never call this on a real-time thread).
pub fn load_scan_grid(file_path: &str) -> Option<(f64, f64)> {
    let db = find_database()?;
    let text = std::fs::read_to_string(&db).ok()?;
    scan_grid(&text, file_path)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Render the `<Poi …/>` tags for one cue set, using VDJ's own
/// attribute order, two-space indentation, and — crucially — the
/// database file's own line ending. VDJ writes CRLF (Windows-heritage
/// format, even on macOS) and its loader flags a database with mixed
/// endings as corrupted; the first version of this exporter learned
/// that the hard way.
fn render_pois(set: &TrackCueSet, eol: &str, skipped: &mut Vec<String>) -> (String, u32) {
    let mut out = String::new();
    let mut count = 0u32;
    for cue in &set.cues {
        if cue.pos_secs <= 0.0 {
            skipped.push(if cue.name.is_empty() {
                format!("{} @ beat {}", set.title, cue.beat)
            } else {
                cue.name.clone()
            });
            continue;
        }
        let name = if cue.name.trim().is_empty() {
            format!("Cue {}", count + 1)
        } else {
            cue.name.clone()
        };
        out.push_str(&format!(
            "  <Poi Name=\"{}\" Pos=\"{:.6}\" Num=\"-1\" Color=\"4278255360\" Type=\"action\" Action=\"os2l_button &apos;cue:{}&apos;\" />{eol}",
            xml_escape(&name),
            cue.pos_secs,
            xml_escape(&cue.id),
        ));
        count += 1;
    }
    (out, count)
}

/// Surgically cut every line containing `marker` (each including its
/// own line ending), leaving every other byte untouched.
fn remove_marked_lines(content: &str, marker: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(hit) = rest.find(marker) {
        let line_start = rest[..hit].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line_end = rest[hit..]
            .find('\n')
            .map(|i| hit + i + 1)
            .unwrap_or(rest.len());
        out.push_str(&rest[..line_start]);
        rest = &rest[line_end..];
    }
    out.push_str(rest);
    out
}

/// Patch one `<Song>` block byte-faithfully: splice the new POI lines
/// in right above the `</Song>` line (our stale lines were already
/// removed globally). Every other byte — indentation, line endings,
/// attribute quirks — passes through untouched, so VirtualDJ sees a
/// file indistinguishable from one it wrote itself.
fn patch_song_block(block: &str, pois: &str) -> String {
    let mut out = block.to_string();
    if let Some(idx) = out.rfind("</Song>") {
        // Start of the line that carries `</Song>` — inserting there
        // keeps that line's own indentation intact.
        let insert_at = out[..idx].rfind('\n').map(|i| i + 1).unwrap_or(idx);
        out.insert_str(insert_at, pois);
    }
    out
}

/// Export `sets` into the database at `db_path`. Pure with respect to
/// inputs other than the filesystem; the caller handles the
/// VDJ-running check so tests can exercise this directly.
pub fn export_to_database(db_path: &Path, sets: &[TrackCueSet]) -> Result<PoiExportReport, String> {
    let content = std::fs::read_to_string(db_path)
        .map_err(|e| format!("no se pudo leer {}: {e}", db_path.display()))?;

    // Backup before touching anything.
    let stamp = chrono_lite_timestamp();
    let backup = db_path.with_extension(format!("xml.dmx-backup-{stamp}"));
    std::fs::copy(db_path, &backup)
        .map_err(|e| format!("no se pudo crear el backup {}: {e}", backup.display()))?;

    // Match the file's own newline convention (VDJ writes CRLF).
    let eol = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };

    // Global cleanup first: every POI we ever wrote goes away, across
    // ALL songs — so deleting a track's cues in the app (or renaming a
    // file) never leaves orphan POIs behind in VDJ.
    let mut out = remove_marked_lines(&content, OURS_MARKER);
    let mut tracks_written = 0u32;
    let mut cues_written = 0u32;
    let mut tracks_missing = Vec::new();
    let mut cues_without_secs = Vec::new();

    for set in sets {
        if set.cues.is_empty() {
            continue;
        }
        let needle = format!("FilePath=\"{}\"", xml_escape(&set.file_path));
        let Some(song_attr_at) = out.find(&needle) else {
            tracks_missing.push(set.title.clone());
            continue;
        };
        // Walk back to the opening tag and forward to the closing one.
        let Some(open_at) = out[..song_attr_at].rfind("<Song ") else {
            tracks_missing.push(set.title.clone());
            continue;
        };
        let Some(close_rel) = out[open_at..].find("</Song>") else {
            tracks_missing.push(set.title.clone());
            continue;
        };
        let close_at = open_at + close_rel + "</Song>".len();

        let (pois, count) = render_pois(set, eol, &mut cues_without_secs);
        let patched = patch_song_block(&out[open_at..close_at], &pois);
        out.replace_range(open_at..close_at, &patched);
        tracks_written += 1;
        cues_written += count;
    }

    // Atomic write: temp + rename, like every other file we own.
    let tmp = db_path.with_extension("xml.dmx-tmp");
    std::fs::write(&tmp, out.as_bytes()).map_err(|e| format!("escritura temporal: {e}"))?;
    std::fs::rename(&tmp, db_path).map_err(|e| format!("rename final: {e}"))?;

    tracing::info!(
        target: "dmx::cues",
        db = %db_path.display(),
        tracks_written, cues_written,
        "POIs exported to VirtualDJ database"
    );

    Ok(PoiExportReport {
        database_path: db_path.display().to_string(),
        backup_path: backup.display().to_string(),
        tracks_written,
        cues_written,
        tracks_missing,
        cues_without_secs,
    })
}

/// yyyymmdd-hhmmss without pulling in chrono: good enough for backup
/// filenames, from the system clock.
fn chrono_lite_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Days since epoch → civil date (Howard Hinnant's algorithm).
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mth <= 2 { y + 1 } else { y };
    format!("{y:04}{mth:02}{d:02}-{h:02}{m:02}{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::show::button_bindings::ButtonAction;
    use crate::show::track_cues::TrackCue;

    fn set_for(path: &str) -> TrackCueSet {
        TrackCueSet {
            file_path: path.into(),
            title: "Tema".into(),
            cues: vec![
                TrackCue {
                    id: "abc-1".into(),
                    beat: 16.0,
                    pos_secs: 8.5,
                    name: "Entrada".into(),
                    action: ButtonAction::RecallSceneByIndex { index: 0 },
                    color: None,
                },
                TrackCue {
                    id: "abc-2".into(),
                    beat: 32.0,
                    pos_secs: 0.0, // old cue without seconds → skipped
                    name: "Vieja".into(),
                    action: ButtonAction::Blackout,
                    color: None,
                },
            ],
            grid: None,
            sections: vec![],
            duration_secs: 0.0,
            owned: vec![],
        }
    }

    #[test]
    fn export_inserts_replaces_and_reports() {
        let dir = std::env::temp_dir().join(format!("dmx-poi-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("database.xml");
        std::fs::write(
            &db,
            concat!(
                "<VirtualDJ_Database Version=\"2023\">\n",
                " <Song FilePath=\"/Users/x/Music/tema.mp3\" FileSize=\"1\">\n",
                "  <Tags Author=\"A\" />\n",
                "  <Poi Pos=\"0.5\" Type=\"beatgrid\" />\n",
                "  <Poi Name=\"mio\" Pos=\"9.0\" Num=\"-1\" Color=\"1\" Type=\"action\" Action=\"os2l_button &apos;cue:viejo&apos;\" />\n",
                "  <Poi Name=\"manual\" Pos=\"3.0\" Num=\"-1\" Color=\"1\" Type=\"action\" Action=\"os2l_button &apos;drop-1&apos;\" />\n",
                " </Song>\n",
                " <Song FilePath=\"/Users/x/Music/otro.mp3\">\n",
                " </Song>\n",
                "</VirtualDJ_Database>\n"
            ),
        )
        .unwrap();

        let report = export_to_database(&db, &[set_for("/Users/x/Music/tema.mp3")]).unwrap();
        let out = std::fs::read_to_string(&db).unwrap();

        // New POI present, with the cue-id convention and escaped quotes.
        assert!(out.contains("Action=\"os2l_button &apos;cue:abc-1&apos;\""));
        assert!(out.contains("Name=\"Entrada\""));
        assert!(out.contains("Pos=\"8.500000\""));
        // Our stale POI was replaced; the manual one survived.
        assert!(!out.contains("cue:viejo"));
        assert!(out.contains("os2l_button &apos;drop-1&apos;"));
        // Untouched structures intact.
        assert!(out.contains("Type=\"beatgrid\""));
        assert!(out.contains("otro.mp3"));

        assert_eq!(report.tracks_written, 1);
        assert_eq!(report.cues_written, 1);
        assert_eq!(report.cues_without_secs, vec!["Vieja".to_string()]);
        assert!(report.tracks_missing.is_empty());
        assert!(std::path::Path::new(&report.backup_path).exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_track_is_reported_not_written() {
        let dir = std::env::temp_dir().join(format!("dmx-poi-test2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("database.xml");
        std::fs::write(&db, "<VirtualDJ_Database>\n</VirtualDJ_Database>\n").unwrap();
        let report = export_to_database(&db, &[set_for("/no/existe.mp3")]).unwrap();
        assert_eq!(report.tracks_written, 0);
        assert_eq!(report.tracks_missing, vec!["Tema".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn crlf_database_stays_byte_faithful() {
        // VDJ writes CRLF even on macOS. The exporter must produce a
        // file whose untouched bytes are identical and whose inserted
        // lines use the same ending — mixed endings made VDJ declare
        // the database corrupted (real incident, Sep 2026).
        let dir = std::env::temp_dir().join(format!("dmx-poi-crlf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("database.xml");
        let original = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n\
            <VirtualDJ_Database Version=\"2026\">\r\n \
            <Song FilePath=\"/x/a.mp3\" FileSize=\"1\">\r\n  \
            <Tags Author=\"A\" />\r\n  \
            <Poi Pos=\"0.5\" Type=\"beatgrid\" />\r\n \
            </Song>\r\n \
            <Song FilePath=\"/x/b.mp3\">\r\n \
            </Song>\r\n\
            </VirtualDJ_Database>\r\n";
        std::fs::write(&db, original).unwrap();

        let mut set = set_for("/x/a.mp3");
        set.cues.truncate(1); // keep only the valid cue
        export_to_database(&db, &[set.clone()]).unwrap();
        let out = std::fs::read_to_string(&db).unwrap();

        // No lone LF anywhere: every \n is preceded by \r.
        let bytes = out.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' {
                assert_eq!(bytes[i - 1], b'\r', "LF suelto en offset {i}");
            }
        }
        // Removing our POI line restores the original byte-for-byte.
        let ours: String = out
            .lines()
            .filter(|l| l.contains(OURS_MARKER))
            .map(|l| format!("{l}\r\n"))
            .collect();
        assert!(!ours.is_empty());
        let restored = out.replace(&ours, "");
        assert_eq!(restored, original, "los bytes ajenos deben quedar intactos");

        // Re-export is idempotent: same cue replaces itself, still CRLF.
        export_to_database(&db, &[set]).unwrap();
        let out2 = std::fs::read_to_string(&db).unwrap();
        assert_eq!(out2.matches(OURS_MARKER).count(), 1);
        assert_eq!(out2, out);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ampersand_paths_match_vdj_escaping() {
        let dir = std::env::temp_dir().join(format!("dmx-poi-test3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("database.xml");
        std::fs::write(
            &db,
            concat!(
                "<VirtualDJ_Database>\n",
                " <Song FilePath=\"/x/Rock &amp; Pop.mp3\">\n",
                " </Song>\n",
                "</VirtualDJ_Database>\n"
            ),
        )
        .unwrap();
        let report = export_to_database(&db, &[set_for("/x/Rock & Pop.mp3")]).unwrap();
        assert_eq!(report.tracks_written, 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
