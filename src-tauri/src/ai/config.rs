//! Persistence of AI provider settings (provider choice, API keys, model).
//!
//! Keys live in the OS app-config directory (one file per app), NOT in
//! the show file. Rationale:
//! - Show files get shared / version-controlled / synced over Drive;
//!   keys absolutely must not travel with them.
//! - The same operator runs many shows with the same key, so per-show
//!   storage would mean re-entering it every time.
//!
//! POC caveat: this writes the key in plaintext JSON. Real-world
//! release should move to the OS keychain (`keyring` crate or
//! `tauri-plugin-stronghold`). The shape is designed so that swap is
//! a one-file change — `AiConfig` doesn't leak the storage backend.

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Which provider the operator wants to use right now. `None` = AI
/// disabled; the UI hides the Generate button entirely in that state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum AiProvider {
    #[default]
    None,
    Anthropic,
    Openai,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../bindings/")]
pub struct AiProviderConfig {
    /// Plaintext API key. Empty string = not configured. See module
    /// docs for the eventual keychain migration.
    #[serde(default)]
    pub api_key: String,
    /// Model identifier as accepted by the provider (e.g.
    /// `claude-sonnet-4-6` or `gpt-5`). Empty string falls back to the
    /// provider's recommended default at request time.
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../bindings/")]
pub struct AiConfig {
    #[serde(default)]
    pub provider: AiProvider,
    #[serde(default)]
    pub anthropic: AiProviderConfig,
    #[serde(default)]
    pub openai: AiProviderConfig,
}

/// Recommended model identifiers, surfaced to the UI as the dropdown
/// options. Both lists lead with the balanced default for that
/// provider; the rest are shortcuts to faster/cheaper or more
/// capable variants the operator can A/B against during the POC.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct AiModelOption {
    pub id: String,
    pub label: String,
    /// One-line UI hint — speed/cost/quality tradeoff in plain words.
    pub hint: String,
}

pub fn anthropic_models() -> Vec<AiModelOption> {
    vec![
        AiModelOption {
            id: "claude-sonnet-4-6".into(),
            label: "Claude Sonnet 4.6".into(),
            hint: "Equilibrio recomendado · velocidad y calidad".into(),
        },
        AiModelOption {
            id: "claude-opus-4-7".into(),
            label: "Claude Opus 4.7".into(),
            hint: "Máxima capacidad · más lento y más caro".into(),
        },
        AiModelOption {
            id: "claude-haiku-4-5-20251001".into(),
            label: "Claude Haiku 4.5".into(),
            hint: "Rápido y barato · ideal para iteración".into(),
        },
    ]
}

pub fn openai_models() -> Vec<AiModelOption> {
    vec![
        AiModelOption {
            id: "gpt-5".into(),
            label: "GPT-5".into(),
            hint: "Equilibrio recomendado".into(),
        },
        AiModelOption {
            id: "gpt-5-mini".into(),
            label: "GPT-5 mini".into(),
            hint: "Rápido y económico".into(),
        },
        AiModelOption {
            id: "gpt-4o".into(),
            label: "GPT-4o".into(),
            hint: "Generación previa, sigue siendo capaz".into(),
        },
    ]
}

/// Resolve the path to `ai-config.json` inside the OS app-config dir.
/// Created lazily on first write; missing → use defaults.
fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    Ok(dir.join("ai-config.json"))
}

pub fn load(app: &tauri::AppHandle) -> AiConfig {
    let Ok(path) = config_path(app) else {
        return AiConfig::default();
    };
    let Ok(text) = fs::read_to_string(&path) else {
        return AiConfig::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save(app: &tauri::AppHandle, cfg: &AiConfig) -> Result<(), String> {
    let path = config_path(app)?;
    let text = serde_json::to_string_pretty(cfg).map_err(|e| format!("serialize: {e}"))?;
    fs::write(&path, text).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}

impl AiConfig {
    /// Pull out the `(api_key, model)` pair the orchestrator needs to
    /// dispatch a call. Returns the provider variant for matching.
    /// `None` when the operator hasn't picked a provider, or has but
    /// left the key blank — the UI surfaces this as "configurá la
    /// API key primero".
    pub fn active(&self) -> Option<(AiProvider, &str, &str)> {
        let pick = match self.provider {
            AiProvider::None => return None,
            AiProvider::Anthropic => &self.anthropic,
            AiProvider::Openai => &self.openai,
        };
        if pick.api_key.is_empty() {
            return None;
        }
        Some((self.provider, &pick.api_key, &pick.model))
    }
}
