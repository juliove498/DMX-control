//! GitHub Gist API client used as the storage backend for cross-
//! machine sync. We talk to a single private gist that holds one
//! file (`dmx-control-show.json`) with the portable share of the
//! show. The PAT must have the `gist` scope; nothing else.
//!
//! Conflict detection: every gist response carries `updated_at`. We
//! stash that in [crate::sync::settings::SyncSettings] on each
//! successful push/pull. Before the next push we re-fetch and refuse
//! to overwrite if the remote has moved on without us — the UI
//! offers "force push" once the user has eyeballed it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "dmx-control-show.json";
const USER_AGENT: &str = "dmx-control-sync/0.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GistFile {
    pub filename: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GistView {
    pub id: String,
    pub html_url: String,
    pub updated_at: DateTime<Utc>,
    pub description: Option<String>,
    pub files: std::collections::HashMap<String, GistFileView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GistFileView {
    pub filename: String,
    /// GitHub returns truncated content for files over 1 MB and
    /// gives a `raw_url` instead. We assume well under that limit
    /// for show JSON; if it ever truncates we'll switch to raw_url.
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub raw_url: Option<String>,
    pub size: u64,
}

/// What a push payload looks like — the show JSON wrapped with the
/// machine label so the receiving end can show "pushed from <label>".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPayload {
    pub schema_version: u32,
    pub pushed_at: DateTime<Utc>,
    pub pushed_by: String,
    /// JSON-encoded show. Stored as a string so the gist diff stays
    /// readable on github.com without their UI trying to expand a
    /// huge nested object.
    pub show_json: String,
    /// Whether the pushing machine included the outputs section.
    pub includes_outputs: bool,
}

pub const SCHEMA_VERSION: u32 = 1;

/// Build the reqwest client with the GitHub auth + UA headers wired in.
fn client(token: &str) -> Result<reqwest::Client, String> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|e| e.to_string())?,
    );
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "X-GitHub-Api-Version",
        reqwest::header::HeaderValue::from_static("2022-11-28"),
    );
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .build()
        .map_err(|e| e.to_string())
}

pub async fn whoami(token: &str) -> Result<String, String> {
    let c = client(token)?;
    let resp = c
        .get("https://api.github.com/user")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("github /user: {}", resp.status()));
    }
    #[derive(Deserialize)]
    struct U {
        login: String,
    }
    let u: U = resp.json().await.map_err(|e| e.to_string())?;
    Ok(u.login)
}

pub async fn create_gist(
    token: &str,
    description: &str,
    payload: &SyncPayload,
) -> Result<GistView, String> {
    let c = client(token)?;
    let body = serde_json::json!({
        "description": description,
        "public": false,
        "files": {
            FILE_NAME: { "content": serde_json::to_string_pretty(payload)
                .map_err(|e| e.to_string())? },
        },
    });
    let resp = c
        .post("https://api.github.com/gists")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "create gist failed ({}): {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        ));
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn fetch_gist(token: &str, gist_id: &str) -> Result<GistView, String> {
    let c = client(token)?;
    let url = format!("https://api.github.com/gists/{gist_id}");
    let resp = c.get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "fetch gist failed ({}): {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        ));
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn patch_gist(
    token: &str,
    gist_id: &str,
    payload: &SyncPayload,
) -> Result<GistView, String> {
    let c = client(token)?;
    let url = format!("https://api.github.com/gists/{gist_id}");
    let body = serde_json::json!({
        "files": {
            FILE_NAME: { "content": serde_json::to_string_pretty(payload)
                .map_err(|e| e.to_string())? },
        },
    });
    let resp = c.patch(&url).json(&body).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "patch gist failed ({}): {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        ));
    }
    resp.json().await.map_err(|e| e.to_string())
}

/// Pull the show payload from a gist. Returns `(payload, gist_view)` so
/// the caller can stash the new `updated_at` after applying.
pub async fn pull_payload(
    token: &str,
    gist_id: &str,
) -> Result<(SyncPayload, GistView), String> {
    let view = fetch_gist(token, gist_id).await?;
    let file = view.files.get(FILE_NAME).ok_or_else(|| {
        format!("gist has no {FILE_NAME}; was it created by this app?")
    })?;
    let content = match (&file.content, &file.raw_url) {
        (Some(c), _) => c.clone(),
        (None, Some(raw)) => {
            // Truncated content — fetch raw. Authenticated raw url
            // is fine to call without re-adding our headers.
            let c = client(token)?;
            c.get(raw)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .text()
                .await
                .map_err(|e| e.to_string())?
        }
        (None, None) => return Err("gist file has no content/raw_url".into()),
    };
    let payload: SyncPayload =
        serde_json::from_str(&content).map_err(|e| format!("payload parse: {e}"))?;
    Ok((payload, view))
}
