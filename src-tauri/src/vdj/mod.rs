//! VirtualDJ tempo bridge: polls the Network Control Plugin's HTTP
//! endpoint for `get_bpm` and pipes the result into the same path the
//! header TAP / inbound `/vdj/bpm` webhook use.
//!
//! Why polling, not push: VirtualDJ has no native outbound webhook /
//! `internet_url` verb. The Network Control Plugin (VDJ 2023+ Pro)
//! exposes `GET /query?script=<vdjscript>` which the operator must call.
//! So we sit on this side and ask, on an interval.
//!
//! Behaviour notes:
//! - Only writes through to the show when the BPM crosses a small
//!   epsilon (0.01 BPM). VDJ ships fractional BPMs (e.g. 120.55) and
//!   we preserve them through the pipeline — the chaser engine and
//!   movement engine will drift over a song if we round to integers.
//! - Backs off geometrically on HTTP / parse errors up to 5 s, then
//!   stays there. The poller never gives up unless the operator stops
//!   it — that's the "Retry silent" answer from the design pass.
//! - Autostart on app boot: if `show.vdj.enabled` was true when the
//!   app closed, the setup hook restarts the poller.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::oneshot;
use ts_rs::TS;

use crate::engine::beatgrid::BeatAnchor;
use crate::engine::output_thread::{SharedChasers, SharedGlobals, SharedMovement};
use crate::show::ShowState;

/// Event the frontend listens to so the VDJ tab can refresh status
/// without polling. Fired whenever `running`, `last_bpm`, or
/// `last_error` change in a way the user would care about.
pub const VDJ_STATUS_EVENT: &str = "vdj:status";

/// Default values intentionally match the Network Control Plugin's
/// most common port. The empty bearer means "no auth", which mirrors
/// the plugin's own "Authentication string" being optional.
fn default_host() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    8080
}
fn default_interval_ms() -> u32 {
    250
}
fn default_halve_above_threshold() -> f32 {
    95.0
}
fn default_halve_above_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct VdjConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub bearer: Option<String>,
    #[serde(default = "default_interval_ms")]
    pub interval_ms: u32,
    /// Persisted across restarts. When true, the setup hook auto-
    /// restarts the poller at boot.
    #[serde(default)]
    pub enabled: bool,
    /// "Halve above" guard: when the VDJ BPM exceeds the threshold,
    /// apply BPM/2 to the rig instead. Classic DJ-lighting trick —
    /// electronic music at 130 BPM looks like a strobe attack if you
    /// drive chasers at 130, but at 65 (half-rate) the rig stays
    /// musical and breathes with the bar. Default ON because that's
    /// the behaviour the operator wanted out of the box.
    #[serde(default = "default_halve_above_enabled")]
    pub halve_above_enabled: bool,
    #[serde(default = "default_halve_above_threshold")]
    pub halve_above_threshold: f32,
}

impl Default for VdjConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            bearer: None,
            interval_ms: default_interval_ms(),
            enabled: false,
            halve_above_enabled: default_halve_above_enabled(),
            halve_above_threshold: default_halve_above_threshold(),
        }
    }
}

/// Apply the "halve above threshold" rule to a raw (bpm, beat_pos)
/// pair from VDJ. Both get scaled together so the chaser's notion of
/// "where we are in the beat grid" and "how fast time flows" stay
/// consistent: doubling time-scale would otherwise rip the anchor's
/// extrapolation off the music.
///
/// Pure for unit testing — the poller calls this with whatever the
/// active config says and uses the returned values without further
/// transformation.
fn apply_halve_rule(cfg: &VdjConfig, raw_bpm: f32, raw_beat: Option<f64>) -> (f32, Option<f64>) {
    if cfg.halve_above_enabled && raw_bpm > cfg.halve_above_threshold {
        (raw_bpm * 0.5, raw_beat.map(|b| b * 0.5))
    } else {
        (raw_bpm, raw_beat)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../bindings/")]
pub struct VdjStatus {
    pub running: bool,
    /// Last BPM we successfully read from VDJ. Fractional —
    /// `Some(120.55)` not `Some(121)`. The UI displays this with 2
    /// decimal places so the operator can see *exactly* what's flowing
    /// through the pipeline.
    pub last_bpm: Option<f32>,
    /// Last error string from the polling loop. Cleared on the next
    /// successful poll.
    pub last_error: Option<String>,
    /// Unix seconds of the last successful poll. The UI uses this to
    /// render "Last update Xs ago".
    pub last_success_at_secs: Option<u64>,
}

#[derive(Default)]
pub struct VdjStateInner {
    /// We spawn the poller through Tauri's bundled async runtime so
    /// `start_poller` can be called from the `setup()` closure — which
    /// runs *outside* any tokio context, where a plain `tokio::spawn`
    /// would panic with "no reactor running". The bundled handle
    /// dispatches onto the same runtime Tauri commands use, so all
    /// the `tokio::time::sleep` / `tokio::select!` calls inside the
    /// task still resolve normally.
    pub task: Option<tauri::async_runtime::JoinHandle<()>>,
    pub shutdown_tx: Option<oneshot::Sender<()>>,
    pub last_bpm: Option<f32>,
    pub last_error: Option<String>,
    pub last_success_at_secs: Option<u64>,
}

#[derive(Clone, Default)]
pub struct VdjState(pub Arc<Mutex<VdjStateInner>>);

impl VdjState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> VdjStatus {
        let inner = self.0.lock();
        VdjStatus {
            running: inner.task.is_some(),
            last_bpm: inner.last_bpm,
            last_error: inner.last_error.clone(),
            last_success_at_secs: inner.last_success_at_secs,
        }
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Build a `/query?script=<vdjscript>` URL. The script source is URL-
/// encoded so spaces (`deck 1 play`) and other VDJ punctuation pass
/// through cleanly.
fn build_query_url(cfg: &VdjConfig, script: &str) -> String {
    let encoded = urlencoding_lite(script.as_bytes());
    format!("http://{}:{}/query?script={}", cfg.host, cfg.port, encoded)
}

/// Number of decks we probe for "is playing" state. VDJ supports up to
/// 4 in Pro Infinity; most home/club setups are 2, but checking the
/// extra two costs at most two extra `/query` round-trips when both
/// upper decks are silent (the early-exit on first playing deck means
/// the common case is still one play + one BPM call).
const MAX_DECKS_TO_PROBE: u8 = 4;

/// Minimal percent-encoder: we depend on `reqwest` already, but pulling
/// in `url`/`urlencoding` just for one string is overkill. Encodes
/// anything outside the unreserved set per RFC 3986. We only ever feed
/// it ASCII script source strings.
fn urlencoding_lite(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input {
        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~');
        if unreserved {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// Generic VDJscript runner. Returns the trimmed plain-text body on
/// HTTP 2xx; bubbles network / non-2xx failures as `Err`. Empty body
/// is `Ok(String::new())` — the caller decides whether that's
/// meaningful.
async fn fetch_script(
    client: &reqwest::Client,
    cfg: &VdjConfig,
    script: &str,
) -> Result<String, String> {
    let url = build_query_url(cfg, script);
    let mut req = client.get(&url);
    if let Some(token) = cfg.bearer.as_deref().filter(|s| !s.is_empty()) {
        req = req.header("Authorization", format!("Bearer {token}"));
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("body read failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", body.trim()));
    }
    Ok(body.trim().to_string())
}

/// Parse VDJ's loose "boolean" reply. Different plugin versions /
/// verbs return "1"/"0", "true"/"false", "on"/"off". We accept any of
/// them and default to false on anything unrecognisable — safer than
/// erroring out and pausing the whole poll.
fn parse_vdj_bool(raw: &str) -> bool {
    let t = raw.trim().to_ascii_lowercase();
    matches!(t.as_str(), "1" | "true" | "on" | "yes")
}

/// Parse a BPM reply (e.g. "128.55\n") into a positive finite float.
/// Returns `None` for empty, zero, or NaN — VDJ uses zero to mean
/// "no track / unknown". Preserves the original decimal precision —
/// we don't round to integers, since a 0.5 BPM error compounds into a
/// 1-beat drift in ~120 beats (about a minute at 120 BPM).
fn parse_vdj_bpm(raw: &str) -> Option<f32> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let n: f32 = t.parse().ok()?;
    if !n.is_finite() || n <= 0.0 {
        return None;
    }
    Some(n)
}

/// Per-cycle reading from VDJ. Holds both the BPM and the beat-grid
/// position of the active deck, plus the wall-clock at which the BPM
/// fetch *completed* (used as the anchor reference).
#[derive(Debug, Clone, Copy)]
struct DeckReading {
    bpm: f32,
    /// VDJ's `get_beatpos` — fractional beat number from the start of
    /// the track (e.g. 27.5 = halfway through beat 27). `None` when
    /// the verb wasn't understood or the deck has no analysed grid.
    beat_pos: Option<f64>,
    /// `Instant::now()` captured *after* the HTTP request completed.
    /// Used as the anchor timestamp. Not perfectly synchronised with
    /// VDJ's clock (there's an unmeasured ~RTT/2 of latency), but the
    /// anchor refreshes every poll so drift doesn't accumulate.
    sampled_at: Instant,
}

/// Parse `get_beatpos` reply (e.g. "27.5\n"). Returns `None` for
/// empty/NaN/negative — VDJ uses 0 for "no analysed grid" which we
/// treat as "skip phase sync this cycle".
fn parse_vdj_beatpos(raw: &str) -> Option<f64> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let n: f64 = t.parse().ok()?;
    if !n.is_finite() || n < 0.0 {
        return None;
    }
    Some(n)
}

/// Walk the decks in order, find the first one that's *playing*, and
/// pull both its BPM and its beat-grid position. Solves the "operator
/// loaded the next track on deck B; deck A is still playing" gotcha
/// and bundles the beat anchor data in one round-trip pattern.
///
/// HTTP cost per cycle when deck 1 is playing: 3 calls (play probe,
/// bpm, beatpos). Cheap enough on localhost that we don't gate the
/// beat fetch behind a "sync enabled" flag — the engines simply
/// ignore the anchor when no phase sync is wanted.
async fn fetch_active_reading(
    client: &reqwest::Client,
    cfg: &VdjConfig,
) -> Result<Option<DeckReading>, String> {
    for deck in 1..=MAX_DECKS_TO_PROBE {
        let play_reply = fetch_script(client, cfg, &format!("deck {deck} play")).await?;
        if !parse_vdj_bool(&play_reply) {
            continue;
        }
        let bpm_reply = fetch_script(client, cfg, &format!("deck {deck} get_bpm")).await?;
        let Some(bpm) = parse_vdj_bpm(&bpm_reply) else {
            // Deck plays but BPM unknown (unanalysed track). Don't
            // fall through — return None so the caller knows "active
            // deck exists but its tempo can't be trusted yet".
            return Ok(None);
        };
        // Best-effort beatpos: if the verb fails or parses badly,
        // we still apply the BPM but skip the anchor update. Phase
        // sync remains degraded but the rig isn't broken.
        let beat_pos = fetch_script(client, cfg, &format!("deck {deck} get_beatpos"))
            .await
            .ok()
            .as_deref()
            .and_then(parse_vdj_beatpos);
        let sampled_at = Instant::now();
        return Ok(Some(DeckReading {
            bpm,
            beat_pos,
            sampled_at,
        }));
    }
    Ok(None)
}

/// Backoff schedule: starts at the configured interval, doubles on
/// every consecutive failure, caps at 5 s. Reset to base on first
/// success. Geometric is right here — a 250ms→5s ramp lands the
/// retry sequence at roughly 250, 500, 1000, 2000, 4000, 5000, 5000,
/// 5000…, which is forgiving of a transient blip without spinning
/// the CPU.
fn backoff_next(base_ms: u32, current_ms: u32) -> u32 {
    let doubled = current_ms.saturating_mul(2);
    doubled.clamp(base_ms.max(50), 5000)
}

/// Internal entrypoint: spawns the poller task. Caller must hold the
/// VdjState lock briefly to install the JoinHandle + shutdown sender.
#[allow(clippy::too_many_arguments)]
fn spawn_poller_task(
    app: AppHandle,
    show: ShowState,
    globals: SharedGlobals,
    chasers: SharedChasers,
    movement: SharedMovement,
    vdj_state: VdjState,
    cfg: VdjConfig,
    shutdown_rx: oneshot::Receiver<()>,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::builder()
            // Short timeout: a 5 s hang would block every other tick
            // behind it. If VDJ is unreachable we want to know in 2 s
            // and back off, not wait around.
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let base_ms = cfg.interval_ms.max(50);
        let mut sleep_ms = base_ms;
        let mut last_applied: Option<f32> = None;
        let mut shutdown_rx = shutdown_rx;

        tracing::info!(
            host = %cfg.host,
            port = cfg.port,
            interval_ms = base_ms,
            "vdj poller started"
        );

        loop {
            let sleep_dur = Duration::from_millis(sleep_ms as u64);
            tokio::select! {
                _ = &mut shutdown_rx => {
                    tracing::info!("vdj poller stopping (shutdown signal)");
                    break;
                }
                _ = tokio::time::sleep(sleep_dur) => {}
            }

            match fetch_active_reading(&client, &cfg).await {
                Ok(Some(reading)) => {
                    sleep_ms = base_ms;
                    // Apply the "halve above" rule before anything else
                    // downstream sees the BPM/beat. Both scale together
                    // so the anchor's interpolation stays internally
                    // consistent: at 140 BPM in VDJ with halving on,
                    // our rig sees 70 BPM AND beat positions read out
                    // of "ours" beat grid (which advances at half rate).
                    let (bpm, beat_pos) = apply_halve_rule(&cfg, reading.bpm, reading.beat_pos);
                    // Tight epsilon (0.01 BPM): we want to preserve the
                    // fractional BPM that VDJ reports. Larger epsilons
                    // would round 120.55 → 120.50 and reintroduce the
                    // drift the operator is trying to avoid.
                    let changed = match last_applied {
                        Some(prev) => (prev - bpm).abs() >= 0.01,
                        None => true,
                    };
                    {
                        let mut inner = vdj_state.0.lock();
                        inner.last_bpm = Some(bpm);
                        inner.last_error = None;
                        inner.last_success_at_secs = Some(now_unix_secs());
                    }
                    if changed {
                        last_applied = Some(bpm);
                        if let Err(e) = crate::commands::set_overall_bpm_enabled_impl(
                            &app, &show, &globals, true,
                        ) {
                            tracing::warn!(error = ?e, "vdj poller: failed to enable overall_bpm");
                        }
                        if let Err(e) =
                            crate::commands::set_overall_bpm_impl(&app, &show, &globals, bpm)
                        {
                            tracing::warn!(error = ?e, bpm, "vdj poller: failed to apply bpm");
                        }
                    }
                    // Phase-sync anchor: refresh on every successful
                    // poll. The next chaser/movement tick will snap to
                    // VDJ's beat grid. If beat_pos is None (verb
                    // failed or unanalysed track) we skip the anchor
                    // update — chasers keep their previous anchor or
                    // fall back to free-run.
                    if let Some(bp) = beat_pos {
                        let anchor = BeatAnchor {
                            set_at: reading.sampled_at,
                            beat_at_set: bp,
                            bpm,
                        };
                        chasers.lock().set_beat_anchor(Some(anchor));
                        movement.lock().set_beat_anchor(Some(anchor));
                    }
                    let _ = app.emit(VDJ_STATUS_EVENT, vdj_state.snapshot());
                }
                Ok(None) => {
                    // Empty / zero / NaN reply — VDJ is up but nothing
                    // is playing. Reset backoff + drop the beat anchor
                    // (no point keeping it locked to a stale beat
                    // position — when playback resumes we'll snap to
                    // wherever VDJ says we are then).
                    sleep_ms = base_ms;
                    chasers.lock().set_beat_anchor(None);
                    movement.lock().set_beat_anchor(None);
                    let mut inner = vdj_state.0.lock();
                    if inner.last_error.is_some() {
                        inner.last_error = None;
                        drop(inner);
                        let _ = app.emit(VDJ_STATUS_EVENT, vdj_state.snapshot());
                    }
                }
                Err(reason) => {
                    let prev_sleep = sleep_ms;
                    sleep_ms = backoff_next(base_ms, sleep_ms);
                    let mut inner = vdj_state.0.lock();
                    let changed = inner.last_error.as_deref() != Some(reason.as_str());
                    inner.last_error = Some(reason.clone());
                    drop(inner);
                    if changed {
                        tracing::warn!(
                            error = %reason,
                            next_sleep_ms = sleep_ms,
                            prev_sleep_ms = prev_sleep,
                            "vdj poller error"
                        );
                        let _ = app.emit(VDJ_STATUS_EVENT, vdj_state.snapshot());
                    }
                }
            }
        }

        // Clear running flag so a stop() race doesn't leave the UI
        // showing "running" forever. Also drop the beat anchor: when
        // the poller stops the engines should fall back to free-run
        // immediately, otherwise their last known anchor would keep
        // pulling them along with stale beats forever.
        chasers.lock().set_beat_anchor(None);
        movement.lock().set_beat_anchor(None);
        {
            let mut inner = vdj_state.0.lock();
            inner.task = None;
            inner.shutdown_tx = None;
        }
        let _ = app.emit(
            VDJ_STATUS_EVENT,
            VdjStatus {
                running: false,
                last_bpm: None,
                last_error: None,
                last_success_at_secs: None,
            },
        );
        tracing::info!("vdj poller exited");
    })
}

/// Public start. Idempotent: calling while already running is a no-op.
#[allow(clippy::too_many_arguments)]
pub fn start_poller(
    app: AppHandle,
    show: ShowState,
    globals: SharedGlobals,
    chasers: SharedChasers,
    movement: SharedMovement,
    vdj_state: VdjState,
    cfg: VdjConfig,
) {
    {
        let inner = vdj_state.0.lock();
        if inner.task.is_some() {
            return;
        }
    }
    let (tx, rx) = oneshot::channel();
    let handle = spawn_poller_task(
        app.clone(),
        show,
        globals,
        chasers,
        movement,
        vdj_state.clone(),
        cfg,
        rx,
    );
    {
        let mut inner = vdj_state.0.lock();
        inner.task = Some(handle);
        inner.shutdown_tx = Some(tx);
        inner.last_error = None;
    }
    let _ = app.emit(VDJ_STATUS_EVENT, vdj_state.snapshot());
}

/// Public stop. Sends the shutdown signal and forgets the task. We
/// don't `await` the join here — the task notices the oneshot fire
/// within one tick of `tokio::select!`, which is good enough; the
/// task itself clears `running` on its way out.
pub fn stop_poller(app: AppHandle, vdj_state: VdjState) {
    let (tx, _task) = {
        let mut inner = vdj_state.0.lock();
        (inner.shutdown_tx.take(), inner.task.take())
    };
    if let Some(tx) = tx {
        let _ = tx.send(());
    }
    // Clear last_error so the UI doesn't look stuck on the last
    // failure forever after the user pressed Stop.
    {
        let mut inner = vdj_state.0.lock();
        inner.last_error = None;
    }
    let _ = app.emit(VDJ_STATUS_EVENT, vdj_state.snapshot());
}

// ---- Tauri commands ------------------------------------------------------

#[tauri::command]
pub fn vdj_get_config(show: State<'_, ShowState>) -> VdjConfig {
    show.read().show.vdj.clone()
}

#[tauri::command]
pub fn vdj_get_status(vdj: State<'_, VdjState>) -> VdjStatus {
    vdj.snapshot()
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn vdj_set_config(
    app: AppHandle,
    show: State<'_, ShowState>,
    globals: State<'_, SharedGlobals>,
    chasers: State<'_, SharedChasers>,
    movement: State<'_, SharedMovement>,
    vdj: State<'_, VdjState>,
    config: VdjConfig,
) -> Result<VdjStatus, String> {
    // Persist first so a panic mid-restart can't leave a poller
    // running with a config that's not in the show file.
    {
        let mut s = show.write();
        s.show.vdj = config.clone();
        s.dirty = true;
    }
    if let Err(e) = crate::commands::persist_show(&show, &app) {
        return Err(e.to_string());
    }
    let _ = app.emit(crate::commands::SHOW_EVENT, ());

    // Restart pattern: stop whatever's running, start a new poller
    // *if* the new config says enabled. Always restart on save — the
    // operator may have just bumped the port or token and expects
    // the loop to pick it up without a separate "Restart" button.
    stop_poller(app.clone(), vdj.inner().clone());
    if config.enabled {
        start_poller(
            app.clone(),
            show.inner().clone(),
            globals.inner().clone(),
            chasers.inner().clone(),
            movement.inner().clone(),
            vdj.inner().clone(),
            config,
        );
    }
    Ok(vdj.snapshot())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_then_caps() {
        let base = 250;
        let mut s = base;
        for _ in 0..20 {
            s = backoff_next(base, s);
        }
        assert_eq!(s, 5000);
    }

    #[test]
    fn backoff_respects_minimum() {
        // Even with an absurdly low base, we never spin tighter than
        // 50 ms — otherwise a config typo could turn the poller into
        // a CPU hog.
        assert!(backoff_next(10, 10) >= 50);
    }

    #[test]
    fn url_encoder_passes_alphanumeric() {
        assert_eq!(urlencoding_lite(b"get_bpm"), "get_bpm");
    }

    #[test]
    fn url_encoder_escapes_space() {
        assert_eq!(urlencoding_lite(b"get bpm"), "get%20bpm");
    }

    #[test]
    fn default_config_disabled() {
        let c = VdjConfig::default();
        assert!(!c.enabled);
        assert_eq!(c.port, 8080);
        assert_eq!(c.interval_ms, 250);
    }

    #[test]
    fn parse_vdj_bool_accepts_common_truthy() {
        assert!(parse_vdj_bool("1"));
        assert!(parse_vdj_bool("true"));
        assert!(parse_vdj_bool("TRUE"));
        assert!(parse_vdj_bool("on"));
        assert!(parse_vdj_bool(" yes "));
        assert!(!parse_vdj_bool("0"));
        assert!(!parse_vdj_bool("false"));
        assert!(!parse_vdj_bool(""));
        assert!(!parse_vdj_bool("nonsense"));
    }

    #[test]
    fn parse_vdj_bpm_preserves_decimals() {
        // The whole point of fractional BPM: 120.55 stays 120.55,
        // doesn't get rounded to anything else.
        assert_eq!(parse_vdj_bpm("120.55"), Some(120.55));
        assert_eq!(parse_vdj_bpm("128"), Some(128.0));
        assert_eq!(parse_vdj_bpm("0"), None);
        assert_eq!(parse_vdj_bpm("nope"), None);
    }

    #[test]
    fn halve_rule_skips_below_threshold() {
        let cfg = VdjConfig {
            halve_above_enabled: true,
            halve_above_threshold: 95.0,
            ..VdjConfig::default()
        };
        let (b, beat) = apply_halve_rule(&cfg, 80.0, Some(12.0));
        assert_eq!(b, 80.0);
        assert_eq!(beat, Some(12.0));
    }

    #[test]
    fn halve_rule_applies_above_threshold() {
        let cfg = VdjConfig {
            halve_above_enabled: true,
            halve_above_threshold: 95.0,
            ..VdjConfig::default()
        };
        let (b, beat) = apply_halve_rule(&cfg, 140.0, Some(50.0));
        assert_eq!(b, 70.0, "BPM halved");
        assert_eq!(
            beat,
            Some(25.0),
            "beat position halved so anchor stays consistent"
        );
    }

    #[test]
    fn halve_rule_boundary_is_strict_greater_than() {
        // BPM == threshold should NOT halve. Otherwise toggling the
        // threshold to "match the song's BPM exactly" would surprise
        // the operator with a half-rate they didn't ask for.
        let cfg = VdjConfig {
            halve_above_enabled: true,
            halve_above_threshold: 100.0,
            ..VdjConfig::default()
        };
        let (b, _) = apply_halve_rule(&cfg, 100.0, None);
        assert_eq!(b, 100.0);
        let (b, _) = apply_halve_rule(&cfg, 100.01, None);
        assert!((b - 50.005).abs() < 1e-3);
    }

    #[test]
    fn halve_rule_off_passes_through() {
        let cfg = VdjConfig {
            halve_above_enabled: false,
            halve_above_threshold: 95.0,
            ..VdjConfig::default()
        };
        let (b, beat) = apply_halve_rule(&cfg, 140.0, Some(50.0));
        assert_eq!(b, 140.0);
        assert_eq!(beat, Some(50.0));
    }

    #[test]
    fn halve_rule_none_beat_handled() {
        let cfg = VdjConfig::default();
        // Default config has halving ON above 95.
        let (b, beat) = apply_halve_rule(&cfg, 130.0, None);
        assert_eq!(b, 65.0);
        assert_eq!(beat, None);
    }
}
