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

pub mod cues;
pub mod poi_export;

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
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
    /// Cue follower (per-track cues fired by tracking VDJ's playhead
    /// over this same HTTP endpoint). Autostarts at boot when true.
    #[serde(default)]
    pub cue_follower_enabled: bool,
    /// Networked setups: VDJ's file paths are remote ("C:\\Musica\\x.mp3").
    /// When the music is reachable through a mounted share, this prefix
    /// pair rewrites them for LOCAL file access (waveform decoding).
    /// Empty = no mapping (VDJ runs on this machine).
    #[serde(default)]
    pub music_path_map_from: String,
    #[serde(default)]
    pub music_path_map_to: String,
    /// Explicit path to VDJ's database.xml for the POI export. Empty =
    /// auto-detect in the local default locations; set it to a mounted
    /// share path when VDJ lives on another machine.
    #[serde(default)]
    pub vdj_database_path: String,
    /// Known VirtualDJ instances on the network (two DJs in the booth):
    /// label and HTTP port by canonical IP. The selected one is
    /// `host`/`port`; the OS2L server only listens to that one.
    #[serde(default)]
    pub instances: Vec<VdjInstance>,
    /// Hand over by itself: when the selected instance stops sending
    /// beats and another connected one is beating, switch to it.
    #[serde(default)]
    pub auto_switch: bool,
}

/// A VirtualDJ instance on the network, as the operator labelled it
/// ("Julio", "Rafa"). `ip` is canonical: this machine is always
/// `127.0.0.1`, any other its LAN address.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct VdjInstance {
    pub ip: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_port")]
    pub http_port: u16,
    /// Bonjour name of the machine (`_virtualdj._tcp` instance). DHCP
    /// hands out new addresses between venues; the scan follows the
    /// name and moves the label (and the selection) to the new IP.
    #[serde(default)]
    pub hostname: Option<String>,
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
            cue_follower_enabled: false,
            music_path_map_from: String::new(),
            music_path_map_to: String::new(),
            vdj_database_path: String::new(),
            instances: Vec::new(),
            auto_switch: false,
        }
    }
}

// ---- Instances: who is who on the network ---------------------------------

/// IPv4 addresses of this machine's interfaces (10 s cache).
pub(crate) fn local_ipv4s() -> Vec<std::net::Ipv4Addr> {
    use std::sync::{Mutex as StdMutex, OnceLock};
    type Cache = StdMutex<(Option<Instant>, Vec<std::net::Ipv4Addr>)>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let c = CACHE.get_or_init(|| StdMutex::new((None, Vec::new())));
    let mut g = c.lock().unwrap_or_else(|e| e.into_inner());
    if g.0.is_none_or(|t| t.elapsed() > Duration::from_secs(10)) {
        let mut v = Vec::new();
        if let Ok(ifaces) = if_addrs::get_if_addrs() {
            for i in ifaces {
                if let if_addrs::IfAddr::V4(a) = i.addr {
                    v.push(a.ip);
                }
            }
        }
        *g = (Some(Instant::now()), v);
    }
    g.1.clone()
}

/// Canonical form of a peer address: this machine → "127.0.0.1" (a VDJ
/// on the same Mac shows up under the LAN address over OS2L and under
/// the loopback over HTTP), anything else → its IP.
pub(crate) fn canonical_ip(ip: std::net::IpAddr) -> String {
    match ip {
        std::net::IpAddr::V4(v4) if v4.is_loopback() || local_ipv4s().contains(&v4) => {
            "127.0.0.1".into()
        }
        std::net::IpAddr::V4(v4) => v4.to_string(),
        std::net::IpAddr::V6(v6) if v6.is_loopback() => "127.0.0.1".into(),
        std::net::IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(m) => canonical_ip(std::net::IpAddr::V4(m)),
            None => v6.to_string(),
        },
    }
}

/// The instance key the config's `host` selects.
pub(crate) fn selection_key(host: &str) -> String {
    let h = host.trim();
    if h.eq_ignore_ascii_case("localhost") {
        return "127.0.0.1".into();
    }
    match h.parse::<std::net::IpAddr>() {
        Ok(ip) => canonical_ip(ip),
        Err(_) => h.to_string(),
    }
}

/// Stop and start the poller and the cue follower to match `cfg`.
pub(crate) fn restart_services(app: &AppHandle, cfg: &VdjConfig) {
    let vdj_state = app.state::<VdjState>().inner().clone();
    stop_poller(app.clone(), vdj_state.clone());
    if cfg.enabled {
        start_poller(
            app.clone(),
            app.state::<ShowState>().inner().clone(),
            app.state::<SharedGlobals>().inner().clone(),
            app.state::<SharedChasers>().inner().clone(),
            app.state::<SharedMovement>().inner().clone(),
            vdj_state,
            cfg.clone(),
        );
    }
    let follower = app.state::<cues::CueFollower>().inner().clone();
    cues::stop(&follower);
    if cfg.cue_follower_enabled {
        cues::start(crate::standalone_handles(app), follower, cfg.clone());
    }
}

/// Make `ip` the instance the rig follows: HTTP host/port, OS2L filter,
/// poller and follower restarted on it. Unknown IPs get an unlabelled
/// entry so the operator can name them.
pub fn select_instance(
    app: &AppHandle,
    ip: &str,
    hostname: Option<String>,
) -> Result<VdjConfig, String> {
    let ip = selection_key(ip);
    let show = app.state::<ShowState>().inner().clone();
    let cfg = {
        let mut s = show.write();
        let v = &mut s.show.vdj;
        match v.instances.iter_mut().find(|i| i.ip == ip) {
            Some(i) => {
                v.port = i.http_port;
                if hostname.is_some() {
                    i.hostname = hostname.clone();
                }
            }
            None => v.instances.push(VdjInstance {
                ip: ip.clone(),
                label: String::new(),
                http_port: v.port,
                hostname: hostname.clone(),
            }),
        }
        v.host = ip.clone();
        let cfg = v.clone();
        s.dirty = true;
        cfg
    };
    let label = cfg
        .instances
        .iter()
        .find(|i| i.ip == ip)
        .map(|i| i.label.clone())
        .unwrap_or_default();
    tracing::info!(target: "dmx::vdj", %ip, %label, port = cfg.port, "VirtualDJ instance selected");
    crate::commands::persist_show(&show, app).map_err(|e| e.to_string())?;
    let _ = app.emit(crate::commands::SHOW_EVENT, ());
    restart_services(app, &cfg);
    Ok(cfg)
}

#[tauri::command]
pub fn vdj_select_instance(
    app: AppHandle,
    ip: String,
    hostname: Option<String>,
) -> Result<VdjConfig, String> {
    select_instance(&app, &ip, hostname)
}

/// Labels / ports / auto hand-over. No restart: nothing live changes.
#[tauri::command]
pub fn vdj_set_instances(
    app: AppHandle,
    show: State<'_, ShowState>,
    instances: Vec<VdjInstance>,
    auto_switch: bool,
) -> Result<VdjConfig, String> {
    let cfg = {
        let mut s = show.write();
        s.show.vdj.instances = instances
            .into_iter()
            .map(|mut i| {
                i.ip = selection_key(&i.ip);
                i.label = i.label.trim().to_string();
                i
            })
            .collect();
        s.show.vdj.auto_switch = auto_switch;
        s.dirty = true;
        s.show.vdj.clone()
    };
    crate::commands::persist_show(&show, &app).map_err(|e| e.to_string())?;
    let _ = app.emit(crate::commands::SHOW_EVENT, ());
    Ok(cfg)
}

/// Ask one instance over HTTP whether it is playing. `None` = no answer
/// (plugin off, firewall, wrong port). Used by the instances card to
/// show the HTTP state without a full scan.
#[tauri::command]
pub async fn vdj_probe_http(
    app: AppHandle,
    ip: String,
    port: Option<u16>,
) -> Result<Option<bool>, String> {
    let cfg = app.state::<ShowState>().read().show.vdj.clone();
    let ip = selection_key(&ip);
    let port = port.unwrap_or_else(|| {
        cfg.instances
            .iter()
            .find(|i| i.ip == ip)
            .map(|i| i.http_port)
            .unwrap_or(cfg.port)
    });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
        .map_err(|e| e.to_string())?;
    let probe_cfg = VdjConfig {
        host: ip,
        port,
        ..cfg
    };
    Ok(fetch_script(&client, &probe_cfg, "deck master play")
        .await
        .ok()
        .as_deref()
        .map(parse_vdj_bool))
}

/// One VirtualDJ found (or remembered) on the network.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct VdjScanEntry {
    pub ip: String,
    pub hostname: Option<String>,
    pub label: String,
    pub http_port: u16,
    /// The Network Control plugin answered on `http_port`.
    pub http_ok: bool,
    /// `deck master play` as answered over HTTP.
    pub playing: Option<bool>,
    pub os2l_connected: bool,
    pub os2l_beating: bool,
    pub bpm: Option<f32>,
    pub local: bool,
    pub selected: bool,
}

fn scan_slot<'a>(
    found: &'a mut std::collections::BTreeMap<String, VdjScanEntry>,
    ip: &str,
    cfg: &VdjConfig,
    selected: &str,
) -> &'a mut VdjScanEntry {
    found.entry(ip.to_string()).or_insert_with(|| {
        let known = cfg.instances.iter().find(|i| i.ip == ip);
        VdjScanEntry {
            ip: ip.to_string(),
            hostname: None,
            label: known.map(|i| i.label.clone()).unwrap_or_default(),
            http_port: known.map(|i| i.http_port).unwrap_or(cfg.port),
            http_ok: false,
            playing: None,
            os2l_connected: false,
            os2l_beating: false,
            bpm: None,
            local: ip == "127.0.0.1",
            selected: ip == selected,
        }
    })
}

/// Look for VirtualDJ instances: Bonjour (`_virtualdj._tcp`, VDJ's
/// remote service), the OS2L connections we hold, the remembered
/// instances, this machine, and a sweep of the local /24 on the HTTP
/// port — then ask each one over HTTP whether it is playing.
#[tauri::command]
pub async fn vdj_scan(app: AppHandle) -> Result<Vec<VdjScanEntry>, String> {
    let port = app.state::<ShowState>().read().show.vdj.port;
    let (mdns, sweep) = tokio::join!(
        async {
            tauri::async_runtime::spawn_blocking(|| browse_virtualdj(Duration::from_millis(1800)))
                .await
                .unwrap_or_default()
        },
        sweep_port(port, Duration::from_millis(300)),
    );
    // A known machine answering from a new address (DHCP at another
    // venue): move its label — and the selection, if it had it.
    let mdns: Vec<(String, String)> = mdns
        .into_iter()
        .flat_map(|(name, ips)| {
            ips.into_iter()
                .map(move |ip| (name.clone(), canonical_ip(std::net::IpAddr::V4(ip))))
        })
        .collect();
    migrate_moved_instances(&app, &mdns)?;

    let cfg = app.state::<ShowState>().read().show.vdj.clone();
    let selected = selection_key(&cfg.host);
    let mut found = std::collections::BTreeMap::new();
    for i in &cfg.instances {
        let e = scan_slot(&mut found, &i.ip, &cfg, &selected);
        e.hostname = i.hostname.clone();
    }
    scan_slot(&mut found, "127.0.0.1", &cfg, &selected);
    for p in crate::os2l::peer_summaries(&app) {
        let e = scan_slot(&mut found, &p.ip, &cfg, &selected);
        e.os2l_connected = p.connected;
        e.os2l_beating = p.beating;
        e.bpm = p.bpm;
    }
    for (name, key) in &mdns {
        let e = scan_slot(&mut found, key, &cfg, &selected);
        if e.hostname.is_none() {
            e.hostname = Some(name.clone());
        }
    }
    for ip in sweep {
        scan_slot(
            &mut found,
            &canonical_ip(std::net::IpAddr::V4(ip)),
            &cfg,
            &selected,
        );
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
        .map_err(|e| e.to_string())?;
    let probes: Vec<(String, u16)> = found
        .values()
        .map(|e| (e.ip.clone(), e.http_port))
        .collect();
    let mut answers = Vec::new();
    for (ip, port) in probes {
        let probe_cfg = VdjConfig {
            host: ip.clone(),
            port,
            ..cfg.clone()
        };
        let r = fetch_script(&client, &probe_cfg, "deck master play").await;
        answers.push((ip, r.ok()));
    }
    for (ip, reply) in answers {
        if let Some(e) = found.get_mut(&ip) {
            e.http_ok = reply.is_some();
            e.playing = reply.as_deref().map(parse_vdj_bool);
        }
    }
    let mut out: Vec<VdjScanEntry> = found.into_values().collect();
    out.sort_by_key(|e| (!e.selected, !e.local, e.ip.clone()));
    tracing::info!(
        target: "dmx::vdj",
        found = out.len(),
        answering = out.iter().filter(|e| e.http_ok).count(),
        "VirtualDJ scan"
    );
    Ok(out)
}

/// Known instances whose Bonjour name now answers from another IP get
/// that IP (label kept); if one of them was selected, follow it.
fn migrate_moved_instances(app: &AppHandle, mdns: &[(String, String)]) -> Result<(), String> {
    let show = app.state::<ShowState>().inner().clone();
    let mut moved: Vec<(String, String, String)> = Vec::new(); // (old ip, new ip, name)
    {
        let s = show.read();
        for i in &s.show.vdj.instances {
            let Some(name) = i.hostname.as_deref() else {
                continue;
            };
            if let Some((_, key)) = mdns.iter().find(|(n, _)| n == name) {
                if key != &i.ip && !s.show.vdj.instances.iter().any(|o| &o.ip == key) {
                    moved.push((i.ip.clone(), key.clone(), name.to_string()));
                }
            }
        }
    }
    if moved.is_empty() {
        return Ok(());
    }
    let mut reselect: Option<(String, String)> = None;
    {
        let mut s = show.write();
        let host_key = selection_key(&s.show.vdj.host);
        for (old, new, name) in &moved {
            if let Some(i) = s.show.vdj.instances.iter_mut().find(|i| &i.ip == old) {
                tracing::info!(
                    target: "dmx::vdj",
                    label = %i.label, hostname = %name, from = %old, to = %new,
                    "VirtualDJ instance moved to a new address"
                );
                i.ip = new.clone();
            }
            if &host_key == old {
                reselect = Some((new.clone(), name.clone()));
            }
        }
        s.dirty = true;
    }
    crate::commands::persist_show(&show, app).map_err(|e| e.to_string())?;
    let _ = app.emit(crate::commands::SHOW_EVENT, ());
    if let Some((ip, name)) = reselect {
        select_instance(app, &ip, Some(name))?;
    }
    Ok(())
}

/// Browse Bonjour for VirtualDJ's own service for `window`.
fn browse_virtualdj(window: Duration) -> Vec<(String, Vec<std::net::Ipv4Addr>)> {
    const SERVICE: &str = "_virtualdj._tcp.local.";
    let Ok(daemon) = mdns_sd::ServiceDaemon::new() else {
        return Vec::new();
    };
    let Ok(rx) = daemon.browse(SERVICE) else {
        return Vec::new();
    };
    let deadline = Instant::now() + window;
    let mut out: Vec<(String, Vec<std::net::Ipv4Addr>)> = Vec::new();
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                let full = info.get_fullname().to_string();
                let name = full
                    .strip_suffix(&format!(".{SERVICE}"))
                    .unwrap_or(&full)
                    .to_string();
                let ips: Vec<std::net::Ipv4Addr> =
                    info.get_addresses_v4().into_iter().copied().collect();
                out.push((name, ips));
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = daemon.shutdown();
    out
}

/// TCP-connect sweep of the /24 around each non-loopback interface on
/// `port`: catches a VDJ whose Bonjour is blocked by the router.
async fn sweep_port(port: u16, per_host: Duration) -> Vec<std::net::Ipv4Addr> {
    let mut targets: Vec<std::net::Ipv4Addr> = Vec::new();
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for i in ifaces {
            if i.is_loopback() {
                continue;
            }
            if let if_addrs::IfAddr::V4(a) = i.addr {
                let ip = u32::from(a.ip);
                // Never wider than a /24: a /16 would take too long.
                let mask = u32::from(a.netmask) | 0xFFFF_FF00;
                let net = ip & mask;
                let bcast = net | !mask;
                for h in (net + 1)..bcast {
                    let c = std::net::Ipv4Addr::from(h);
                    if c != a.ip && !targets.contains(&c) {
                        targets.push(c);
                    }
                }
            }
        }
    }
    let sem = Arc::new(tokio::sync::Semaphore::new(64));
    let mut tasks = Vec::new();
    for ip in targets {
        let sem = sem.clone();
        tasks.push(tauri::async_runtime::spawn(async move {
            let _permit = sem.acquire().await.ok()?;
            let ok = tokio::time::timeout(per_host, tokio::net::TcpStream::connect((ip, port)))
                .await
                .map(|r| r.is_ok())
                .unwrap_or(false);
            ok.then_some(ip)
        }));
    }
    let mut hits = Vec::new();
    for t in tasks {
        if let Ok(Some(ip)) = t.await {
            hits.push(ip);
        }
    }
    hits
}

/// Rewrite a VDJ-reported music path for local access per the config's
/// prefix mapping. Backslashes in the remainder become slashes so a
/// Windows-origin path maps cleanly onto a mounted share.
pub fn map_music_path(cfg: &VdjConfig, path: &str) -> String {
    let from = cfg.music_path_map_from.trim();
    if from.is_empty() {
        return path.to_string();
    }
    let Some(rest) = path.strip_prefix(from) else {
        return path.to_string();
    };
    let rest = rest.replace('\\', "/");
    let to = cfg.music_path_map_to.trim().trim_end_matches('/');
    format!("{}/{}", to, rest.trim_start_matches('/'))
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
pub(crate) fn apply_halve_rule(
    cfg: &VdjConfig,
    raw_bpm: f32,
    raw_beat: Option<f64>,
) -> (f32, Option<f64>) {
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
    /// VDJ's VU meter for the playing deck (0..1), when the verb answers.
    #[serde(default)]
    pub last_level: Option<f32>,
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
    /// Bumped by every start. A poller that exits only clears the shared
    /// state if it still owns it — a stop+start on "Guardar" used to let
    /// the old task wipe the new one's handle ("detenido" while running,
    /// then a second poller on the next save).
    pub generation: u64,
    pub last_bpm: Option<f32>,
    pub last_level: Option<f32>,
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
            last_level: inner.last_level,
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
pub(crate) const MAX_DECKS_TO_PROBE: u8 = 4;

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
pub(crate) async fn fetch_script(
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
        .map_err(|e| describe_request_error(&e, cfg))?;
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

/// "host:port no responde (Connection refused (os error 61))" instead of
/// reqwest's nested "error sending request for url (...)".
fn describe_request_error(e: &reqwest::Error, cfg: &VdjConfig) -> String {
    let cause = if e.is_timeout() {
        "timeout".to_string()
    } else {
        let mut root: &dyn std::error::Error = e;
        while let Some(s) = root.source() {
            root = s;
        }
        root.to_string()
    };
    format!("{}:{} no responde ({cause})", cfg.host, cfg.port)
}

/// Parse VDJ's loose "boolean" reply. Different plugin versions /
/// verbs return "1"/"0", "true"/"false", "on"/"off". We accept any of
/// them and default to false on anything unrecognisable — safer than
/// erroring out and pausing the whole poll.
pub(crate) fn parse_vdj_bool(raw: &str) -> bool {
    let t = raw.trim().to_ascii_lowercase();
    matches!(t.as_str(), "1" | "true" | "on" | "yes")
}

/// Parse a BPM reply (e.g. "128.55\n") into a positive finite float.
/// Returns `None` for empty, zero, or NaN — VDJ uses zero to mean
/// "no track / unknown". Preserves the original decimal precision —
/// we don't round to integers, since a 0.5 BPM error compounds into a
/// 1-beat drift in ~120 beats (about a minute at 120 BPM).
pub(crate) fn parse_vdj_bpm(raw: &str) -> Option<f32> {
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
    /// VDJ's `get_vu_meter` (0..1) for the autopilot's energy tracker.
    /// Best-effort: older VDJ builds may not answer the verb.
    level: Option<f32>,
}

/// Parse `get_beatpos` reply (e.g. "27.5\n"). Returns `None` for
/// empty/NaN/negative — VDJ uses 0 for "no analysed grid" which we
/// treat as "skip phase sync this cycle".
pub(crate) fn parse_vdj_beatpos(raw: &str) -> Option<f64> {
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
/// What one poll cycle learned.
enum Probe {
    /// A deck plays and we read its tempo (and maybe beat position / level).
    Playing(DeckReading),
    /// A deck plays but the tempo verb failed or the track is unanalysed:
    /// the music is on, the grid can't be trusted this cycle.
    PlayingNoBpm,
    /// VDJ answered and no deck is playing.
    Nothing,
}

/// Which decks to read. VDJ's `master` deck is the one the room hears;
/// asking it directly avoids probing decks 1-4 (on this rig the hidden
/// decks 3 and 4 answer "play: yes" while mirroring deck 1) and saves
/// three requests a cycle. Older builds without the verb fall back to the
/// probe loop.
pub(crate) async fn decks_to_read(
    client: &reqwest::Client,
    cfg: &VdjConfig,
) -> Result<Option<Vec<String>>, String> {
    let master = fetch_script(client, cfg, "deck master play").await?;
    let t = master.trim();
    if t.starts_with("error") || t.is_empty() {
        return Ok(Some(
            (1..=MAX_DECKS_TO_PROBE).map(|d| d.to_string()).collect(),
        ));
    }
    if parse_vdj_bool(t) {
        Ok(Some(vec!["master".to_string()]))
    } else {
        Ok(None)
    }
}

async fn fetch_active_reading(client: &reqwest::Client, cfg: &VdjConfig) -> Result<Probe, String> {
    let Some(decks) = decks_to_read(client, cfg).await? else {
        return Ok(Probe::Nothing);
    };
    for deck in decks {
        if deck != "master" {
            let play_reply = fetch_script(client, cfg, &format!("deck {deck} play")).await?;
            if !parse_vdj_bool(&play_reply) {
                continue;
            }
        }
        // From here on the deck is known to play: a hung follow-up verb
        // must not turn into "nothing is playing" (VDJ's web server
        // answers one request and stalls on the next on some machines).
        let Ok(bpm_reply) = fetch_script(client, cfg, &format!("deck {deck} get_bpm")).await else {
            return Ok(Probe::PlayingNoBpm);
        };
        let Some(bpm) = parse_vdj_bpm(&bpm_reply) else {
            return Ok(Probe::PlayingNoBpm);
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
        let level = fetch_script(client, cfg, &format!("deck {deck} get_vu_meter"))
            .await
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(0.0, 1.0));
        return Ok(Probe::Playing(DeckReading {
            bpm,
            beat_pos,
            sampled_at,
            level,
        }));
    }
    Ok(Probe::Nothing)
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
    generation: u64,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::builder()
            // Short timeout: a hung request must cost less than a bar.
            // VDJ's web server stalls on individual requests now and then;
            // we abandon them fast and ask again on the next cycle.
            .timeout(Duration::from_millis(900))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let base_ms = cfg.interval_ms.max(50);
        let mut sleep_ms = base_ms;
        let mut last_applied: Option<f32> = None;
        let mut shutdown_rx = shutdown_rx;
        // Last time VDJ answered anything: while that is recent, errors
        // are stalls, not absence, and we keep polling at full rate.
        let mut last_answer: Option<Instant> = None;

        tracing::info!(
            host = %cfg.host,
            port = cfg.port,
            interval_ms = base_ms,
            "vdj poller started"
        );
        crate::autopilot::set_vdj_poller_up(true);

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
                Ok(Probe::PlayingNoBpm) => {
                    sleep_ms = base_ms;
                    last_answer = Some(Instant::now());
                    crate::autopilot::note_vdj_playing();
                }
                Ok(Probe::Playing(reading)) => {
                    sleep_ms = base_ms;
                    last_answer = Some(Instant::now());
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
                        inner.last_level = reading.level;
                        inner.last_error = None;
                        inner.last_success_at_secs = Some(now_unix_secs());
                    }
                    crate::autopilot::note_vdj_playing();
                    if let Some(level) = reading.level {
                        crate::autopilot::set_energy_sample(
                            crate::autopilot::EnergySource::VdjVu,
                            level,
                        );
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
                    // Modo Mágico follows the musical grid, not the chasers'
                    // halved one: a pulse per beat must be a pulse per beat,
                    // and bars/phrases must be the song's.
                    if let Some(raw) = reading.beat_pos {
                        crate::autopilot::set_beat_anchor(
                            Some(BeatAnchor {
                                set_at: reading.sampled_at,
                                beat_at_set: raw,
                                bpm: reading.bpm,
                            }),
                            crate::autopilot::ClockSource::Vdj,
                        );
                    }
                    let _ = app.emit(VDJ_STATUS_EVENT, vdj_state.snapshot());
                }
                Ok(Probe::Nothing) => {
                    // VDJ is up but nothing is playing. Reset backoff +
                    // drop the beat anchor (no point keeping it locked to
                    // a stale beat position — when playback resumes we'll
                    // snap to wherever VDJ says we are then).
                    sleep_ms = base_ms;
                    last_answer = Some(Instant::now());
                    chasers.lock().set_beat_anchor(None);
                    movement.lock().set_beat_anchor(None);
                    crate::autopilot::set_beat_anchor(None, crate::autopilot::ClockSource::Vdj);
                    crate::autopilot::note_vdj_stopped();
                    let mut inner = vdj_state.0.lock();
                    if inner.last_error.is_some() {
                        inner.last_error = None;
                        drop(inner);
                        let _ = app.emit(VDJ_STATUS_EVENT, vdj_state.snapshot());
                    }
                }
                Err(reason) => {
                    let prev_sleep = sleep_ms;
                    let recently_answered = last_answer
                        .map(|t| t.elapsed() < Duration::from_secs(10))
                        .unwrap_or(false);
                    // A stall right after a good answer is not absence:
                    // keep the full poll rate so the next request lands
                    // within a beat. Only escalate when VDJ is truly gone.
                    sleep_ms = if recently_answered {
                        base_ms
                    } else {
                        backoff_next(base_ms, sleep_ms)
                    };
                    crate::autopilot::note_vdj_unreachable();
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

        // Only the poller that still owns the state tears it down: after
        // a restart a newer task is already polling and must keep its
        // handle, its beat anchor and its "up" flag.
        let mine = {
            let mut inner = vdj_state.0.lock();
            let mine = inner.generation == generation;
            if mine {
                inner.task = None;
                inner.shutdown_tx = None;
                inner.last_bpm = None;
                inner.last_level = None;
                inner.last_error = None;
            }
            mine
        };
        if mine {
            // Drop the beat anchor: the engines fall back to free-run
            // instead of riding a stale anchor forever.
            chasers.lock().set_beat_anchor(None);
            movement.lock().set_beat_anchor(None);
            crate::autopilot::set_beat_anchor(None, crate::autopilot::ClockSource::Vdj);
            crate::autopilot::set_vdj_poller_up(false);
        }
        let _ = app.emit(VDJ_STATUS_EVENT, vdj_state.snapshot());
        tracing::info!(generation, superseded = !mine, "vdj poller exited");
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
    let generation = {
        let mut inner = vdj_state.0.lock();
        inner.generation += 1;
        inner.generation
    };
    let handle = spawn_poller_task(
        app.clone(),
        show,
        globals,
        chasers,
        movement,
        vdj_state.clone(),
        cfg,
        rx,
        generation,
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
    // The cue follower polls the same endpoint, so it restarts too.
    stop_poller(app.clone(), vdj.inner().clone());
    if config.enabled {
        start_poller(
            app.clone(),
            show.inner().clone(),
            globals.inner().clone(),
            chasers.inner().clone(),
            movement.inner().clone(),
            vdj.inner().clone(),
            config.clone(),
        );
    }
    let follower = app.state::<cues::CueFollower>().inner().clone();
    cues::stop(&follower);
    if config.cue_follower_enabled {
        cues::start(crate::standalone_handles(&app), follower, config);
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
