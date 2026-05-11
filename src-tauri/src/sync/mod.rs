//! Cross-machine sync via a private GitHub Gist. Per-user, per-machine
//! pairing: each operator stores their own PAT in the OS keychain and
//! uses their own gist. Two users → zero shared state, no "last writer
//! wins on a shared repo".
//!
//! Outputs (universes + drivers) default to **excluded** from the
//! pushed payload because Mac and Windows hardware bindings rarely
//! agree. The "Include outputs" toggle in the Sync tab lets you
//! override per-machine.

pub mod gist;
pub mod secret;
pub mod settings;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::{persist_show, CommandError, SHOW_EVENT};
use crate::engine::output_thread::{SharedChasers, SharedGlobals, SharedMovement};
use crate::show::{ShowFileV1, ShowState};

use gist::{
    create_gist, fetch_gist, patch_gist, pull_payload, whoami, SyncPayload, SCHEMA_VERSION,
};
use settings::{load as load_settings, save as save_settings, SyncSettings};

/// What the UI needs to render the Sync tab — everything from
/// [SyncSettings] plus a derived "is the PAT set?" flag (we never
/// expose the token itself to the frontend).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct SyncStatus {
    pub settings: SyncSettings,
    pub has_token: bool,
    pub github_user: Option<String>,
}

/// Probe response: tells the UI whether a push would conflict before
/// the user clicks. Cheap (one GET) and saves a "go back, you have
/// remote changes" round-trip.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct SyncProbe {
    pub remote_updated_at: Option<DateTime<Utc>>,
    pub remote_machine: Option<String>,
    pub local_last_remote_updated: Option<DateTime<Utc>>,
    /// True when remote has changed since this machine's last
    /// successful sync — the caller should warn the user.
    pub remote_ahead: bool,
}

/// Result of a successful push/pull, surfaced in the UI as a toast.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct SyncResult {
    pub gist_id: String,
    pub gist_html_url: Option<String>,
    pub remote_updated_at: DateTime<Utc>,
    pub remote_machine: Option<String>,
}

#[tauri::command]
pub fn sync_status() -> SyncStatus {
    SyncStatus {
        settings: load_settings(),
        has_token: secret::has_token(),
        github_user: None,
    }
}

#[tauri::command]
pub fn sync_save_settings(settings: SyncSettings) -> Result<SyncStatus, String> {
    save_settings(&settings).map_err(|e| e.to_string())?;
    Ok(SyncStatus {
        settings,
        has_token: secret::has_token(),
        github_user: None,
    })
}

#[tauri::command]
pub fn sync_set_token(token: String) -> Result<(), String> {
    if token.trim().is_empty() {
        return Err("token vacío".into());
    }
    secret::save_token(token.trim())
}

#[tauri::command]
pub fn sync_clear_token() -> Result<(), String> {
    secret::clear_token()
}

/// Validate the current PAT against the GitHub API. Returns the
/// `login` so the UI can show "logged in as <user>". Doesn't persist
/// — the caller decides whether to keep the token if the check fails.
#[tauri::command]
pub async fn sync_whoami() -> Result<String, String> {
    let token = secret::load_token()?.ok_or_else(|| "PAT no configurado".to_string())?;
    whoami(&token).await
}

/// Returns the timestamps from the remote without modifying anything.
/// Used by the UI to decide whether to grey-out "Push" or warn first.
#[tauri::command]
pub async fn sync_probe() -> Result<SyncProbe, String> {
    let cfg = load_settings();
    if cfg.gist_id.is_empty() {
        return Ok(SyncProbe {
            remote_updated_at: None,
            remote_machine: None,
            local_last_remote_updated: None,
            remote_ahead: false,
        });
    }
    let token = secret::load_token()?.ok_or_else(|| "PAT no configurado".to_string())?;
    let view = fetch_gist(&token, &cfg.gist_id).await?;
    // Try to surface the remote machine label without parsing the full
    // payload — the file content carries it. Best-effort: if parse
    // fails we just don't show the label.
    let remote_machine = view
        .files
        .get("dmx-control-show.json")
        .and_then(|f| f.content.as_ref())
        .and_then(|c| serde_json::from_str::<SyncPayload>(c).ok())
        .map(|p| p.pushed_by);
    let local = cfg.last_remote_updated;
    let remote_ahead = match (local, view.updated_at) {
        (Some(l), r) => r > l,
        (None, _) => true,
    };
    Ok(SyncProbe {
        remote_updated_at: Some(view.updated_at),
        remote_machine,
        local_last_remote_updated: local,
        remote_ahead,
    })
}

/// Build the JSON we'd push: the current show, with `outputs` zeroed
/// out unless the user has opted in. Pulled out for testability.
fn build_push_payload(
    show: &ShowState,
    cfg: &SyncSettings,
) -> Result<(String, SyncPayload), String> {
    let s = show.read();
    let mut snapshot = s.show.clone();
    drop(s);
    if !cfg.include_outputs {
        // Reset to default so the receiving machine doesn't pick up
        // an outputs config that points to USB devices it doesn't
        // have. Default is "no universes bound" — safer than the
        // pre-canned starter, which assumes a specific device.
        snapshot.outputs = Default::default();
    }
    let show_json = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
    let label = if cfg.machine_label.trim().is_empty() {
        "unknown".to_string()
    } else {
        cfg.machine_label.trim().to_string()
    };
    let payload = SyncPayload {
        schema_version: SCHEMA_VERSION,
        pushed_at: Utc::now(),
        pushed_by: label,
        show_json,
        includes_outputs: cfg.include_outputs,
    };
    Ok((snapshot.name.clone(), payload))
}

/// Push the local show to the gist. If `force` is false and the gist
/// has been touched since our last successful sync, returns an error
/// — the UI is supposed to call `sync_probe` first and ask the user.
#[tauri::command]
pub async fn sync_push(
    show: tauri::State<'_, ShowState>,
    force: bool,
) -> Result<SyncResult, String> {
    let mut cfg = load_settings();
    let token = secret::load_token()?.ok_or_else(|| "PAT no configurado".to_string())?;

    let (name, payload) = build_push_payload(&show, &cfg)?;
    let description = format!("dmx-control sync — {name}");

    // If we already have a gist, check the remote hasn't drifted.
    if !cfg.gist_id.is_empty() && !force {
        let view = fetch_gist(&token, &cfg.gist_id).await?;
        if let Some(local_seen) = cfg.last_remote_updated {
            if view.updated_at > local_seen {
                return Err(format!(
                    "remote_ahead: el gist se actualizó el {} y no hiciste pull. Pasá `force=true` o hacé pull primero.",
                    view.updated_at
                ));
            }
        }
    }

    let result = if cfg.gist_id.is_empty() {
        create_gist(&token, &description, &payload).await?
    } else {
        patch_gist(&token, &cfg.gist_id, &payload).await?
    };

    cfg.gist_id = result.id.clone();
    cfg.last_remote_updated = Some(result.updated_at);
    cfg.last_pushed_at = Some(Utc::now());
    cfg.last_remote_machine = Some(payload.pushed_by.clone());
    save_settings(&cfg).map_err(|e| e.to_string())?;

    Ok(SyncResult {
        gist_id: result.id,
        gist_html_url: Some(result.html_url),
        remote_updated_at: result.updated_at,
        remote_machine: Some(payload.pushed_by),
    })
}

/// Apply a remote payload to the local show. Outputs are preserved
/// from the local show unless the *remote* says it included them
/// (and the user has the toggle on locally too).
#[tauri::command]
pub async fn sync_pull(
    app: tauri::AppHandle,
    show: tauri::State<'_, ShowState>,
    chasers: tauri::State<'_, SharedChasers>,
    movement: tauri::State<'_, SharedMovement>,
    globals: tauri::State<'_, SharedGlobals>,
) -> Result<SyncResult, String> {
    let mut cfg = load_settings();
    if cfg.gist_id.is_empty() {
        return Err("no hay gist configurado todavía".into());
    }
    let token = secret::load_token()?.ok_or_else(|| "PAT no configurado".to_string())?;

    let (payload, view) = pull_payload(&token, &cfg.gist_id).await?;
    if payload.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "payload version {} (esperaba {})",
            payload.schema_version, SCHEMA_VERSION
        ));
    }

    let mut incoming: ShowFileV1 =
        serde_json::from_str(&payload.show_json).map_err(|e| format!("show parse: {e}"))?;

    // Auto-snapshot the current show before overwriting, so a bad
    // pull is recoverable. Best-effort — if writing fails we still
    // proceed with the pull but log loudly.
    if let Err(e) = backup_current_show(&show) {
        tracing::warn!(?e, "sync pull: pre-pull backup failed");
    }

    // Decide what to do with outputs. If the remote pushed without
    // them (the common case), keep ours. If the remote did include
    // them AND we've opted in locally, accept the remote ones.
    let keep_local_outputs = !(payload.includes_outputs && cfg.include_outputs);
    if keep_local_outputs {
        let local_outputs = show.read().show.outputs.clone();
        incoming.outputs = local_outputs;
    }

    // Persist + apply to runtime. Mirrors what `open_show` does
    // internally so chasers/movement/globals all pick up the new
    // values without a relaunch.
    {
        let mut s = show.write();
        s.show = incoming.clone();
        s.dirty = true;
    }
    persist_show(&show, &app).map_err(|e: CommandError| e.to_string())?;

    // Push the new show into the runtime engines so anything that
    // depends on chasers/movement/globals sees it without waiting
    // for the user to flip a toggle.
    sync_runtime_from_show(&show, &chasers, &movement, &globals);

    cfg.last_remote_updated = Some(view.updated_at);
    cfg.last_pulled_at = Some(Utc::now());
    cfg.last_remote_machine = Some(payload.pushed_by.clone());
    save_settings(&cfg).map_err(|e| e.to_string())?;

    let _ = tauri::Emitter::emit(&app, SHOW_EVENT, ());

    Ok(SyncResult {
        gist_id: cfg.gist_id.clone(),
        gist_html_url: Some(view.html_url),
        remote_updated_at: view.updated_at,
        remote_machine: Some(payload.pushed_by),
    })
}

fn backup_current_show(show: &ShowState) -> std::io::Result<()> {
    let Some(base) = dirs::config_dir() else {
        return Ok(());
    };
    let dir = base.join("dmx-control").join("sync-backups");
    std::fs::create_dir_all(&dir)?;
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("pre-pull-{stamp}.json"));
    let snap = show.read().show.clone();
    let bytes = serde_json::to_vec_pretty(&snap).map_err(std::io::Error::other)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

fn sync_runtime_from_show(
    show: &ShowState,
    chasers: &SharedChasers,
    movement: &SharedMovement,
    globals: &SharedGlobals,
) {
    let (fixtures, library, chasers_list, movement_list, globals_cfg) = {
        let s = show.read();
        (
            s.show.fixtures.clone(),
            s.library.clone(),
            s.show.chasers.clone(),
            s.show.movements.clone(),
            s.show.globals.clone(),
        )
    };
    {
        let mut e = chasers.lock();
        e.update_show_context(fixtures.clone(), library.clone());
        e.replace_chasers(chasers_list);
    }
    {
        let mut m = movement.lock();
        m.update_show_context(fixtures.clone(), library.clone());
        m.replace_generators(movement_list);
    }
    {
        let mut g = globals.lock();
        g.update_show_context(fixtures, library);
        g.replace_config(globals_cfg);
    }
}
