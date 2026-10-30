//! OS2L server: VirtualDJ's native "Open Sound 2 Light" protocol.
//!
//! VDJ (2018+) can connect to a lighting controller over TCP and push
//! JSON events. We advertise `_os2l._tcp` via mDNS so VDJ's "auto"
//! detection finds us without any manual setup, then consume:
//!
//! - `{"evt":"beat","change":…,"pos":…,"bpm":…,"strength":…}` — one per
//!   beat while a deck plays. Fed into the same Overall-BPM + beat
//!   anchor path the VDJ HTTP poller uses, but push-based and with the
//!   deck's true beat phase (better than polling).
//! - `{"evt":"btn","name":"…","state":"on"|"off"}` — sent by the
//!   `os2l_button "name"` VDJScript verb. The operator drops action
//!   POIs (cues) in their tracks with that verb, and each one triggers
//!   a mapped [`ButtonAction`] here: scenes, loop groups, chasers,
//!   movements, snapshots, blackout…
//! - `{"evt":"cmd",…}` — value faders; ignored for now.
//!
//! Button names resolve in two steps:
//! 1. Explicit mappings from the show config (exact name match,
//!    case-insensitive).
//! 2. Naming convention, zero-config: `escena:Intro`, `scene:2`,
//!    `loop:Drop`, `loop:stop`, `chaser:Ambar`, `mov:Circulo`,
//!    `snap:Golpe`, plus bare `blackout` / `blind` / `tap`. Values
//!    match the item's name first (case-insensitive), then fall back
//!    to a 1-based index.
//!
//! Dispatch reuses the MIDI-learn dispatcher so semantics stay
//! identical across surfaces (toggles flip, active scene re-press
//! releases, blind is momentary via the on/off pair).

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use ts_rs::TS;

use crate::engine::beatgrid::BeatAnchor;
use crate::midi::generic::{button_event, GenericHandles};
use crate::show::button_bindings::ButtonAction;

/// Event the frontend listens to for live status (connection, beats,
/// button log) without polling.
pub const OS2L_STATUS_EVENT: &str = "os2l:status";

fn default_port() -> u16 {
    9995
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct Os2lMapping {
    /// OS2L button name as typed in the VDJ POI (`os2l_button "name"`).
    pub name: String,
    pub action: ButtonAction,
    /// Toggle / Flash / Swop; VDJ sends on/off for pads so momentary works.
    #[serde(default)]
    pub mode: crate::show::button_bindings::ButtonMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct Os2lConfig {
    /// Persisted across restarts; when true the setup hook starts the
    /// server at boot, mirroring the VDJ poller's autostart.
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Snap button dispatches to the beat grid: instead of firing the
    /// moment the event arrives, fire on the next beat (per the live
    /// anchor VDJ's beat events feed us). The transport latency between
    /// VDJ and us stops mattering — the operator drops POIs slightly
    /// *before* the musical moment and the change lands on the grid.
    /// Events that arrive just after a beat (within a small grace
    /// window) fire immediately: they were aimed at *that* beat.
    #[serde(default)]
    pub quantize_to_beat: bool,
    /// Extra delay added to quantized dispatches, in ms. Calibration
    /// knob for the audio-output latency: if the sound reaches the
    /// room later than VDJ's internal clock (big audio buffers), the
    /// lights would land early by that amount — dial it out here using
    /// the per-button beat offsets shown in the log.
    #[serde(default)]
    pub latency_offset_ms: u32,
    #[serde(default)]
    pub mappings: Vec<Os2lMapping>,
}

impl Default for Os2lConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_port(),
            quantize_to_beat: false,
            latency_offset_ms: 0,
            mappings: Vec::new(),
        }
    }
}

/// One received button event, kept in a small ring for the UI — the
/// operator triggers the POI in VDJ and immediately sees the name here
/// to map it (a "learn" flow without arming anything).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct Os2lRecentButton {
    pub name: String,
    pub state_on: bool,
    pub at_secs: u64,
    /// What the name resolved to, for display ("Escena · Intro"), or
    /// `None` when nothing matched.
    pub resolved: Option<String>,
    /// Arrival offset relative to the previous beat, in ms — how late
    /// after the grid the event reached us. `None` when no beat anchor
    /// was live. This is the calibration readout for POI placement.
    pub beat_offset_ms: Option<i32>,
    /// True when the dispatch was deferred to the next beat.
    pub quantized: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct Os2lStatus {
    pub running: bool,
    pub port: u16,
    /// The selected VirtualDJ's address while it is connected.
    pub peer: Option<String>,
    /// Every VirtualDJ seen over OS2L (two DJs in the booth), by
    /// canonical IP — only `selected` drives the rig.
    pub peers: Vec<Os2lPeerStatus>,
    /// Canonical IP of the instance the config selects (`vdj.host`).
    pub selected: Option<String>,
    pub last_error: Option<String>,
    pub last_beat_bpm: Option<f32>,
    pub last_beat_at_secs: Option<u64>,
    pub recent_buttons: Vec<Os2lRecentButton>,
}

/// One `beat` event from VirtualDJ, untouched: `beat` is the track's
/// beat index on VDJ's grid, `bpm` the live tempo, `change` VDJ's own
/// "this beat was not the expected next one" flag (seek, needle drop).
#[derive(Clone, Copy, Debug)]
pub struct RawBeat {
    pub beat: f64,
    pub bpm: f32,
    pub at: Instant,
    pub change: bool,
}

impl RawBeat {
    /// Beat position at `now`, extrapolated from the last beat.
    pub fn beat_at(&self, now: Instant) -> f64 {
        self.beat
            + now.saturating_duration_since(self.at).as_secs_f64() * self.bpm.max(1.0) as f64 / 60.0
    }
}

/// VirtualDJ's latest raw beat, if the OS2L link is up.
pub fn raw_beat(app: &tauri::AppHandle) -> Option<RawBeat> {
    use tauri::Manager;
    app.try_state::<Os2lState>()?.0.lock().last_raw
}

/// One VirtualDJ instance as seen over OS2L.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct Os2lPeerStatus {
    pub ip: String,
    pub connected: bool,
    /// A beat arrived in the last 3 s.
    pub beating: bool,
    pub bpm: Option<f32>,
    pub selected: bool,
}

#[derive(Default)]
struct PeerInfo {
    conns: u32,
    last_beat: Option<Instant>,
    beat_times: std::collections::VecDeque<Instant>,
    bpm: Option<f32>,
}

impl PeerInfo {
    fn beating(&self, now: Instant) -> bool {
        self.last_beat
            .is_some_and(|t| now.duration_since(t) < Duration::from_secs(3))
    }
    fn recent_beats(&self, now: Instant) -> usize {
        self.beat_times
            .iter()
            .filter(|t| now.duration_since(**t) < Duration::from_secs(3))
            .count()
    }
    fn note_beat(&mut self, now: Instant, bpm: Option<f32>) {
        self.last_beat = Some(now);
        self.beat_times.push_back(now);
        while self.beat_times.len() > 8 {
            self.beat_times.pop_front();
        }
        if bpm.is_some() {
            self.bpm = bpm;
        }
    }
}

/// Events from a connection, tagged with the peer.
enum Msg {
    Event(std::net::IpAddr, serde_json::Value),
    Gone(std::net::IpAddr),
}

/// The selected instance must be silent this long, with another one
/// beating, before an automatic hand-over.
const AUTO_QUIET: Duration = Duration::from_secs(5);
/// Minimum gap between automatic hand-overs.
const AUTO_MIN_GAP: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Os2lInner {
    running: bool,
    port: u16,
    peer: Option<String>,
    last_error: Option<String>,
    last_beat_bpm: Option<f32>,
    last_beat_at_secs: Option<u64>,
    /// Live beat anchor (post-halve grid), refreshed on every beat
    /// event. Basis for quantized dispatch and the per-button offset
    /// readout.
    last_anchor: Option<BeatAnchor>,
    /// VirtualDJ's last beat as sent (its own grid, no halving): what
    /// the cue follower rides on while VDJ's HTTP server is down.
    last_raw: Option<RawBeat>,
    recent_buttons: Vec<Os2lRecentButton>,
    shutdown: Option<tokio::sync::watch::Sender<bool>>,
    /// Quantized presses still waiting for their beat, by button key. A
    /// release for the same button is held back until after its press —
    /// otherwise the "off" lands first and the "on" sticks forever.
    pending_press: std::collections::HashMap<String, Instant>,
    /// The listener task, so a restart can wait for the old socket to close.
    task: Option<tauri::async_runtime::JoinHandle<()>>,
    /// Bumped on every start; a finishing task only clears state it still owns.
    generation: u64,
    /// Every VirtualDJ that connected, by canonical IP.
    peers: std::collections::HashMap<String, PeerInfo>,
    /// Canonical IP the config selects (`vdj.host`).
    selected: Option<String>,
    last_auto_switch: Option<Instant>,
}

#[derive(Clone)]
pub struct Os2lState(Arc<Mutex<Os2lInner>>);

impl Os2lState {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Os2lInner::default())))
    }

    /// Empty the recent-buttons log (UI "clear" button).
    pub fn clear_recent(&self) {
        self.0.lock().recent_buttons.clear();
    }

    pub fn snapshot(&self) -> Os2lStatus {
        let g = self.0.lock();
        let now = Instant::now();
        let mut peers: Vec<Os2lPeerStatus> = g
            .peers
            .iter()
            .map(|(ip, p)| Os2lPeerStatus {
                ip: ip.clone(),
                connected: p.conns > 0,
                beating: p.beating(now),
                bpm: p.bpm,
                selected: g.selected.as_deref() == Some(ip.as_str()),
            })
            .collect();
        peers.sort_by(|a, b| a.ip.cmp(&b.ip));
        Os2lStatus {
            running: g.running,
            port: g.port,
            peer: g.peer.clone(),
            peers,
            selected: g.selected.clone(),
            last_error: g.last_error.clone(),
            last_beat_bpm: g.last_beat_bpm,
            last_beat_at_secs: g.last_beat_at_secs,
            recent_buttons: g.recent_buttons.clone(),
        }
    }
}

impl Default for Os2lState {
    fn default() -> Self {
        Self::new()
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The VirtualDJ instances seen over OS2L, for the network scan.
pub fn peer_summaries(app: &tauri::AppHandle) -> Vec<Os2lPeerStatus> {
    use tauri::Manager;
    app.try_state::<Os2lState>()
        .map(|s| s.snapshot().peers)
        .unwrap_or_default()
}

/// Read one connection's JSON stream and forward its events; ends on
/// EOF, error, or server shutdown (closing the socket makes VDJ
/// reconnect to the new listener instead of talking to a ghost).
async fn reader_task(
    mut stream: tokio::net::TcpStream,
    ip: std::net::IpAddr,
    tx: tokio::sync::mpsc::UnboundedSender<Msg>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut splitter = JsonSplitter::new();
    let mut buf = [0u8; 4096];
    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            read = stream.read(&mut buf) => match read {
                Ok(0) => break,
                Ok(n) => {
                    for evt in splitter.push(&buf[..n]) {
                        if tx.send(Msg::Event(ip, evt)).is_err() {
                            return;
                        }
                    }
                }
                Err(e) => {
                    tracing::info!(target: "dmx::os2l", peer = %ip, error = %e, "connection dropped");
                    break;
                }
            },
        }
    }
    let _ = tx.send(Msg::Gone(ip));
}

/// Re-read which instance the config selects; keep `peer`, the Magic
/// mode's OS2L flag and the bpm latch in step with it.
fn refresh_selection(
    handles: &GenericHandles,
    state: &Os2lState,
    selected_key: &mut Option<String>,
    last_applied_bpm: &mut Option<f32>,
    last_up: &mut Option<bool>,
) {
    let host = handles.show.read().show.vdj.host.clone();
    let key = crate::vdj::selection_key(&host);
    if selected_key.as_ref() != Some(&key) {
        tracing::info!(target: "dmx::os2l", instance = %key, "OS2L follows this VirtualDJ");
        *last_applied_bpm = None;
        *selected_key = Some(key.clone());
    }
    let connected = {
        let mut g = state.0.lock();
        let c = g.peers.get(&key).is_some_and(|p| p.conns > 0);
        g.selected = Some(key.clone());
        g.peer = if c { Some(key) } else { None };
        c
    };
    if *last_up != Some(connected) {
        *last_up = Some(connected);
        crate::autopilot::set_os2l_up(connected);
    }
}

/// A beat from a non-selected instance: hand over to it when the
/// selected one has gone quiet and the operator allowed it.
fn maybe_auto_switch(handles: &GenericHandles, state: &Os2lState, candidate: &str) {
    let (auto, host) = {
        let s = handles.show.read();
        (s.show.vdj.auto_switch, s.show.vdj.host.clone())
    };
    if !auto {
        return;
    }
    let selected = crate::vdj::selection_key(&host);
    if selected == candidate {
        return;
    }
    let now = Instant::now();
    let go = {
        let mut g = state.0.lock();
        let quiet = g
            .peers
            .get(&selected)
            .and_then(|p| p.last_beat)
            .is_none_or(|t| now.duration_since(t) > AUTO_QUIET);
        let ready = g
            .peers
            .get(candidate)
            .is_some_and(|p| p.conns > 0 && p.recent_beats(now) >= 2);
        let spaced = g
            .last_auto_switch
            .is_none_or(|t| now.duration_since(t) > AUTO_MIN_GAP);
        let go = quiet && ready && spaced;
        if go {
            g.last_auto_switch = Some(now);
        }
        go
    };
    if go {
        match crate::vdj::select_instance(&handles.app, candidate, None) {
            Ok(cfg) => {
                let label = cfg
                    .instances
                    .iter()
                    .find(|i| i.ip == candidate)
                    .map(|i| i.label.clone())
                    .unwrap_or_default();
                tracing::info!(
                    target: "dmx::os2l",
                    from = %selected, to = %candidate, %label,
                    "automatic hand-over: the selected VirtualDJ went quiet, this one is playing"
                );
            }
            Err(e) => {
                tracing::warn!(target: "dmx::os2l", error = %e, "automatic hand-over failed");
            }
        }
    }
}

fn emit_status(handles: &GenericHandles, state: &Os2lState) {
    let _ = handles.app.emit(OS2L_STATUS_EVENT, state.snapshot());
}

// ---- JSON stream framing ---------------------------------------------------

/// Incremental splitter for concatenated JSON objects on a TCP stream.
/// OS2L has no length prefix or delimiter guarantee — objects may
/// arrive glued together or split across reads — so we track brace
/// depth with string/escape awareness and cut on depth returning to 0.
struct JsonSplitter {
    buf: Vec<u8>,
    depth: u32,
    in_str: bool,
    escaped: bool,
    /// Byte offset where the current object started.
    start: Option<usize>,
    /// Scan cursor (everything before it is already classified).
    pos: usize,
}

impl JsonSplitter {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            depth: 0,
            in_str: false,
            escaped: false,
            start: None,
            pos: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) -> Vec<serde_json::Value> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        while self.pos < self.buf.len() {
            let b = self.buf[self.pos];
            if self.in_str {
                if self.escaped {
                    self.escaped = false;
                } else if b == b'\\' {
                    self.escaped = true;
                } else if b == b'"' {
                    self.in_str = false;
                }
            } else {
                match b {
                    b'"' if self.start.is_some() => self.in_str = true,
                    b'{' => {
                        if self.start.is_none() {
                            self.start = Some(self.pos);
                        }
                        self.depth += 1;
                    }
                    b'}' if self.start.is_some() => {
                        self.depth = self.depth.saturating_sub(1);
                        if self.depth == 0 {
                            let s = self.start.take().unwrap();
                            if let Ok(v) =
                                serde_json::from_slice::<serde_json::Value>(&self.buf[s..=self.pos])
                            {
                                out.push(v);
                            }
                            // Drop consumed bytes to keep the buffer flat.
                            self.buf.drain(..=self.pos);
                            self.pos = 0;
                            continue;
                        }
                    }
                    _ => {}
                }
            }
            self.pos += 1;
        }
        // Garbage guard: nothing object-like in a large buffer → junk
        // peer, drop it so memory stays bounded.
        if self.start.is_none() && self.buf.len() > 64 * 1024 {
            self.buf.clear();
            self.pos = 0;
        }
        out
    }
}

// ---- Name resolution -------------------------------------------------------

/// Resolve an OS2L button name to an action + a human description.
/// Explicit mappings first, then the naming convention.
pub fn resolve_button(
    handles: &GenericHandles,
    mappings: &[Os2lMapping],
    raw: &str,
) -> Option<(ButtonAction, String)> {
    let name = raw.trim();
    if let Some(m) = mappings
        .iter()
        .find(|m| m.name.trim().eq_ignore_ascii_case(name))
    {
        return Some((m.action.clone(), describe_action(handles, &m.action)));
    }
    let action = resolve_convention(handles, name)?;
    let desc = describe_action(handles, &action);
    Some((action, desc))
}

/// Case-insensitive lookup of an item id by display name.
fn find_id_by_name<'a>(
    pairs: impl Iterator<Item = (&'a str, &'a str)>,
    wanted: &str,
) -> Option<String> {
    for (id, name) in pairs {
        if name.trim().eq_ignore_ascii_case(wanted) {
            return Some(id.to_string());
        }
    }
    None
}

/// Display name of an item id, `"?"` when it no longer exists.
fn name_by_id<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>, id: &str) -> String {
    for (i, name) in pairs {
        if i == id {
            return name.to_string();
        }
    }
    "?".into()
}

fn resolve_convention(handles: &GenericHandles, name: &str) -> Option<ButtonAction> {
    match name.to_lowercase().as_str() {
        "blackout" => return Some(ButtonAction::Blackout),
        "blind" => return Some(ButtonAction::Blind),
        "tap" => return Some(ButtonAction::Tap),
        "auto" | "magico" | "mágico" | "magic" | "autopilot" => {
            return Some(ButtonAction::ToggleAutopilot)
        }
        // Energy pads for Modo Mágico, named after how the night feels.
        "tranqui" | "chill" | "energia baja" | "energía baja" => {
            return Some(ButtonAction::AutopilotEnergy { level: 0.25 })
        }
        "groove" | "energia media" | "energía media" => {
            return Some(ButtonAction::AutopilotEnergy { level: 0.55 })
        }
        "fiesta" | "party" | "energia alta" | "energía alta" => {
            return Some(ButtonAction::AutopilotEnergy { level: 0.85 })
        }
        "siguiente" | "next" => {
            return Some(ButtonAction::AutopilotNudge {
                kind: crate::show::button_bindings::AutopilotNudgeKind::Next,
            })
        }
        "anterior" | "prev" | "back" => {
            return Some(ButtonAction::AutopilotNudge {
                kind: crate::show::button_bindings::AutopilotNudgeKind::Back,
            })
        }
        "mantener" | "hold" => return Some(ButtonAction::AutopilotHold),
        "strobo" | "strobe" | "burst" | "rafaga" | "ráfaga" => {
            return Some(ButtonAction::AutopilotBurst)
        }
        "drop" => return Some(ButtonAction::AutopilotDrop),
        "paleta auto" | "palette auto" => {
            return Some(ButtonAction::AutopilotPalette {
                style: crate::autopilot::PaletteStyle::Auto,
            })
        }
        "calida" | "cálida" | "warm" => {
            return Some(ButtonAction::AutopilotPalette {
                style: crate::autopilot::PaletteStyle::Warm,
            })
        }
        "fria" | "fría" | "cold" => {
            return Some(ButtonAction::AutopilotPalette {
                style: crate::autopilot::PaletteStyle::Cold,
            })
        }
        "arcoiris" | "arcoíris" | "rainbow" => {
            return Some(ButtonAction::AutopilotPalette {
                style: crate::autopilot::PaletteStyle::Rainbow,
            })
        }
        "mono" => {
            return Some(ButtonAction::AutopilotPalette {
                style: crate::autopilot::PaletteStyle::Mono,
            })
        }
        "cabezales" | "movimiento" | "movement" => return Some(ButtonAction::AutopilotMovement),
        _ => {}
    }
    let (kind, rest) = name.split_once(':')?;
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    let g = handles.show.read();
    // Synced VDJ POIs carry `cue:<id>` — resolve to the cue's action so
    // editing a cue in the app never requires re-exporting the POIs.
    if kind.trim().eq_ignore_ascii_case("cue") {
        return g
            .show
            .track_cues
            .iter()
            .flat_map(|s| s.cues.iter())
            .find(|c| c.id == rest)
            .map(|c| c.action.clone());
    }
    // 1-based index fallback: "chaser:2" or "chaser:#2".
    let index = rest
        .strip_prefix('#')
        .unwrap_or(rest)
        .parse::<usize>()
        .ok()
        .and_then(|n| n.checked_sub(1))
        .map(|n| n.min(u8::MAX as usize) as u8);
    match kind.trim().to_lowercase().as_str() {
        // "energia:70" → Modo Mágico at 70 %.
        "energia" | "energía" | "energy" => rest
            .trim_end_matches('%')
            .trim()
            .parse::<f32>()
            .ok()
            .map(|p| ButtonAction::AutopilotEnergy {
                level: (p / 100.0).clamp(0.0, 1.0),
            }),
        "scene" | "escena" => {
            let it = g
                .show
                .scenes
                .iter()
                .map(|s| (s.id.as_str(), s.name.as_str()));
            if let Some(id) = find_id_by_name(it, rest) {
                return Some(ButtonAction::RecallScene { id });
            }
            index.map(|index| ButtonAction::RecallSceneByIndex { index })
        }
        "loop" | "secuencia" => {
            if rest.eq_ignore_ascii_case("stop") {
                return Some(ButtonAction::StopLoopGroup);
            }
            let it = g
                .show
                .scene_loop_groups
                .iter()
                .map(|l| (l.id.as_str(), l.name.as_str()));
            if let Some(id) = find_id_by_name(it, rest) {
                return Some(ButtonAction::StartLoopGroup { id });
            }
            index.map(|index| ButtonAction::StartLoopGroupByIndex { index })
        }
        "chaser" => {
            let it = g
                .show
                .chasers
                .iter()
                .map(|c| (c.id.as_str(), c.name.as_str()));
            if let Some(id) = find_id_by_name(it, rest) {
                return Some(ButtonAction::ToggleChaser { id });
            }
            index.map(|index| ButtonAction::ToggleChaserByIndex { index })
        }
        "movement" | "movimiento" | "mov" => {
            let it = g
                .show
                .movements
                .iter()
                .map(|m| (m.id.as_str(), m.name.as_str()));
            if let Some(id) = find_id_by_name(it, rest) {
                return Some(ButtonAction::ToggleMovement { id });
            }
            index.map(|index| ButtonAction::ToggleMovementByIndex { index })
        }
        "snapshot" | "snap" => {
            let it = g
                .show
                .snapshots
                .iter()
                .map(|s| (s.id.as_str(), s.name.as_str()));
            if let Some(id) = find_id_by_name(it, rest) {
                return Some(ButtonAction::ToggleSnapshot { id });
            }
            index.map(|index| ButtonAction::ToggleSnapshotByIndex { index })
        }
        _ => None,
    }
}

/// Short human label for the recent-buttons log ("Escena · Intro").
pub(crate) fn describe_action(handles: &GenericHandles, action: &ButtonAction) -> String {
    let g = handles.show.read();
    match action {
        ButtonAction::None => "—".into(),
        ButtonAction::RecallScene { id } => {
            let it = g
                .show
                .scenes
                .iter()
                .map(|s| (s.id.as_str(), s.name.as_str()));
            format!("Escena · {}", name_by_id(it, id))
        }
        ButtonAction::RecallSceneByIndex { index } => format!("Escena · #{}", index + 1),
        ButtonAction::StartLoopGroup { id } => {
            let it = g
                .show
                .scene_loop_groups
                .iter()
                .map(|l| (l.id.as_str(), l.name.as_str()));
            format!("Loop · {}", name_by_id(it, id))
        }
        ButtonAction::StartLoopGroupByIndex { index } => format!("Loop · #{}", index + 1),
        ButtonAction::StopLoopGroup => "Loop · stop".into(),
        ButtonAction::ToggleChaser { id } => {
            let it = g
                .show
                .chasers
                .iter()
                .map(|c| (c.id.as_str(), c.name.as_str()));
            format!("Chaser · {}", name_by_id(it, id))
        }
        ButtonAction::ToggleChaserByIndex { index } => format!("Chaser · #{}", index + 1),
        ButtonAction::ToggleMovement { id } => {
            let it = g
                .show
                .movements
                .iter()
                .map(|m| (m.id.as_str(), m.name.as_str()));
            format!("Movimiento · {}", name_by_id(it, id))
        }
        ButtonAction::ToggleMovementByIndex { index } => format!("Movimiento · #{}", index + 1),
        ButtonAction::ToggleSnapshot { id } => {
            let it = g
                .show
                .snapshots
                .iter()
                .map(|s| (s.id.as_str(), s.name.as_str()));
            format!("Snapshot · {}", name_by_id(it, id))
        }
        ButtonAction::ToggleSnapshotByIndex { index } => format!("Snapshot · #{}", index + 1),
        ButtonAction::Blackout => "Blackout".into(),
        ButtonAction::Blind => "Blind".into(),
        ButtonAction::Tap => "Tap".into(),
        ButtonAction::ToggleOverallBpm => "Overall BPM".into(),
        ButtonAction::BumpActiveChaserBpm { delta } => format!("BPM {delta:+}"),
        ButtonAction::ToggleAutopilot => "Modo Mágico".into(),
        ButtonAction::AutopilotEnergy { level } => format!("Energía {:.0}%", level * 100.0),
        ButtonAction::AutopilotNudge { kind } => match kind {
            crate::show::button_bindings::AutopilotNudgeKind::Next => "Siguiente look".into(),
            crate::show::button_bindings::AutopilotNudgeKind::Back => "Look anterior".into(),
        },
        ButtonAction::AutopilotHold => "Mantener look".into(),
        ButtonAction::AutopilotBurst => "Ráfaga strobo".into(),
        ButtonAction::AutopilotDrop => "Drop".into(),
        ButtonAction::AutopilotPalette { style } => format!("Paleta {style:?}"),
        ButtonAction::AutopilotMovement => "Cabezales on/off".into(),
    }
}

/// Events landing within this fraction of a beat *after* the grid line
/// are treated as aimed at that beat and fire immediately; anything
/// later defers to the next beat. 20% at 128 BPM ≈ 94 ms of grace —
/// generous enough for POIs placed "on" the beat plus transport jitter.
/// One beat at the tempo of the last OS2L beat, if VirtualDJ sent any.
pub fn anchor_beat_len(app: &tauri::AppHandle) -> Option<Duration> {
    use tauri::Manager;
    let st = app.try_state::<Os2lState>()?;
    let a = st.0.lock().last_anchor?;
    Some(Duration::from_secs_f32(60.0 / a.bpm.max(20.0)))
}

const ON_BEAT_GRACE: f64 = 0.20;

/// How long a quantized dispatch must wait so it lands on the next
/// beat. `None` = we're inside the grace window, fire now.
fn quantize_delay(beat_pos: f64, bpm: f32) -> Option<Duration> {
    let frac = beat_pos - beat_pos.floor();
    if frac <= ON_BEAT_GRACE {
        return None;
    }
    let beat_ms = 60_000.0 / bpm.max(1.0) as f64;
    Some(Duration::from_micros(
        ((1.0 - frac) * beat_ms * 1000.0).max(0.0) as u64,
    ))
}

/// Handle one button event: resolve, dispatch (immediately or snapped
/// to the beat grid), record, notify. Also the backend for the
/// "probar" button in the UI (no VDJ required).
pub fn handle_button(handles: &GenericHandles, state: &Os2lState, name: &str, state_on: bool) {
    let (mappings, quantize, extra_ms) = {
        let s = handles.show.read();
        (
            s.show.os2l.mappings.clone(),
            s.show.os2l.quantize_to_beat,
            s.show.os2l.latency_offset_ms,
        )
    };
    let anchor = state.0.lock().last_anchor;
    let now = Instant::now();
    let beat_offset_ms = anchor.map(|a| {
        let pos = a.beat_at(now);
        let frac = pos - pos.floor();
        ((frac * 60_000.0 / a.bpm.max(1.0) as f64).round()) as i32
    });

    let resolved = resolve_button(handles, &mappings, name);
    let desc = resolved.as_ref().map(|(_, d)| d.clone());
    let mode = mappings
        .iter()
        .find(|m| m.name.trim().eq_ignore_ascii_case(name.trim()))
        .map(|m| m.mode)
        .unwrap_or_default();
    let key = format!("os2l:{}", name.trim().to_lowercase());
    // Synced action POIs (`cue:<id>`, written by the exporter) carry a
    // cue the operator placed on the timeline: that position IS the
    // timing. They are never quantized or delayed (a beat of delay
    // collapsed and even reordered cues placed close together), VDJ's
    // "off" for them is noise, and the cue path's own dedup decides
    // whether the POI or the follower fires.
    let trimmed = name.trim();
    let cue_id = trimmed
        .get(..4)
        .filter(|p| p.eq_ignore_ascii_case("cue:"))
        .map(|_| trimmed[4..].trim().to_string());
    let mut quantized = false;
    if let Some(cue_id) = cue_id {
        if !state_on {
            tracing::debug!(target: "dmx::os2l", %name, "cue POI release ignored");
        } else if let Some((action, _)) = &resolved {
            crate::vdj::cues::fire_cue_from_poi(handles, &cue_id, action);
        } else {
            tracing::info!(target: "dmx::os2l", %name, "cue POI for a cue that no longer exists");
        }
    } else if let Some((action, _)) = resolved {
        // Only "press" events quantize: releases (blind off) must track
        // the operator's actual gesture, not the grid…
        let delay = if quantize && state_on {
            anchor.and_then(|a| quantize_delay(a.beat_at(now), a.bpm))
        } else {
            None
        };
        let delay = match (delay, quantize && state_on && extra_ms > 0) {
            (Some(d), true) => Some(d + Duration::from_millis(extra_ms as u64)),
            (Some(d), false) => Some(d),
            (None, true) => Some(Duration::from_millis(extra_ms as u64)),
            (None, false) => None,
        };
        // …but a release must never overtake its own quantized press.
        let delay = if state_on {
            if let Some(d) = delay {
                state.0.lock().pending_press.insert(key.clone(), now + d);
            }
            delay
        } else {
            let pending = state.0.lock().pending_press.get(&key).copied();
            match pending.filter(|t| *t > now) {
                Some(t) => Some(t - now + Duration::from_millis(10)),
                None => delay,
            }
        };
        match delay {
            Some(d) if !d.is_zero() => {
                quantized = true;
                tracing::info!(
                    target: "dmx::os2l",
                    %name, state_on, delay_ms = d.as_millis() as u64,
                    "os2l button dispatch quantized to the beat"
                );
                let handles2 = handles.clone();
                let state2 = state.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(d).await;
                    button_event(&handles2, &key, &action, mode, state_on);
                    if state_on {
                        state2.0.lock().pending_press.remove(&key);
                    }
                });
            }
            _ => {
                tracing::info!(target: "dmx::os2l", %name, state_on, "os2l button dispatch");
                button_event(handles, &key, &action, mode, state_on);
            }
        }
    } else {
        tracing::info!(target: "dmx::os2l", %name, "os2l button with no matching action");
    }
    {
        let mut g = state.0.lock();
        g.recent_buttons.insert(
            0,
            Os2lRecentButton {
                name: name.to_string(),
                state_on,
                at_secs: now_unix_secs(),
                resolved: desc,
                beat_offset_ms,
                quantized,
            },
        );
        g.recent_buttons.truncate(8);
    }
    emit_status(handles, state);
}

// ---- Server ----------------------------------------------------------------

/// Signal the running server to stop and hand back its task handle so the
/// caller can wait for the socket to actually close.
fn take_down(state: &Os2lState) -> Option<tauri::async_runtime::JoinHandle<()>> {
    let mut g = state.0.lock();
    if let Some(tx) = g.shutdown.take() {
        let _ = tx.send(true);
    }
    g.running = false;
    g.peer = None;
    drop(g);
    crate::autopilot::set_os2l_up(false);
    state.0.lock().task.take()
}

/// Stop the server if it's running. Idempotent.
pub fn stop_server(state: &Os2lState) {
    let _ = take_down(state);
}

/// Start listening. Errors (port busy…) are reported both in the
/// return value and in the status snapshot.
pub fn start_server(handles: GenericHandles, state: Os2lState, cfg: Os2lConfig) {
    let previous = take_down(&state);
    let (tx, rx) = tokio::sync::watch::channel(false);
    let generation = {
        let mut g = state.0.lock();
        g.shutdown = Some(tx);
        g.running = true;
        g.port = cfg.port;
        g.last_error = None;
        g.generation += 1;
        g.generation
    };
    let task = tauri::async_runtime::spawn(server_task(
        handles,
        state.clone(),
        cfg,
        rx,
        previous,
        generation,
    ));
    state.0.lock().task = Some(task);
}

/// Bind the port, retrying briefly: the previous listener may still be
/// in the middle of closing (a restart), and the kernel needs a moment.
async fn bind_with_retry(port: u16) -> std::io::Result<TcpListener> {
    let mut attempt = 0;
    loop {
        match TcpListener::bind(("0.0.0.0", port)).await {
            Ok(l) => return Ok(l),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse && attempt < 25 => {
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

async fn server_task(
    handles: GenericHandles,
    state: Os2lState,
    cfg: Os2lConfig,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
    previous: Option<tauri::async_runtime::JoinHandle<()>>,
    generation: u64,
) {
    // Wait for the previous listener to let go of the port. Without this
    // every other "Aplicar" raced the old socket and failed with
    // "Address already in use" — the "una sí, una no" bug.
    if let Some(prev) = previous {
        let _ = tokio::time::timeout(Duration::from_secs(3), prev).await;
    }
    if *shutdown.borrow() {
        return; // superseded while waiting
    }
    let listener = match bind_with_retry(cfg.port).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(target: "dmx::os2l", port = cfg.port, error = %e, "os2l bind failed");
            let mut g = state.0.lock();
            if g.generation == generation {
                g.running = false;
                g.last_error = Some(format!("no se pudo abrir el puerto {}: {e}", cfg.port));
            }
            drop(g);
            emit_status(&handles, &state);
            return;
        }
    };

    // Advertise _os2l._tcp so VirtualDJ's OS2L auto-detection finds us.
    // Best-effort: if mDNS fails the operator can still point VDJ at
    // ip:port manually. The instance and host names embed the machine's
    // hostname: with two mirrored Macs on the same LAN, identical mDNS
    // identities would make VDJ's pick nondeterministic (it could latch
    // onto the *other* machine's server and fire cues there).
    let host_label = crate::presence::machine_host_label();
    let mdns_instance = format!("DMX Control ({host_label})");
    let mdns_fullname = format!("{mdns_instance}._os2l._tcp.local.");
    let mdns = mdns_sd::ServiceDaemon::new().ok().and_then(|daemon| {
        let info = mdns_sd::ServiceInfo::new(
            "_os2l._tcp.local.",
            &mdns_instance,
            &format!("{host_label}.local."),
            "0.0.0.0",
            cfg.port,
            None,
        )
        .ok()?
        .enable_addr_auto();
        daemon.register(info).ok()?;
        tracing::info!(target: "dmx::os2l", port = cfg.port, instance = %mdns_instance, "advertising _os2l._tcp via mDNS");
        Some(daemon)
    });

    tracing::info!(target: "dmx::os2l", port = cfg.port, "os2l server listening");
    emit_status(&handles, &state);

    // Any number of VirtualDJ instances may be connected at once (two
    // DJs in the booth, each with OS2L on). Every connection gets a
    // reader task that frames its JSON stream and forwards the events
    // here tagged with the peer. Only the *selected* instance
    // (`vdj.host`) drives the rig; the others are tracked — connected,
    // beating, bpm — for the UI and the automatic hand-over.
    let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<Msg>();
    let mut last_applied_bpm: Option<f32> = None;
    let mut last_emit = Instant::now();
    let mut selected_key: Option<String> = None;
    let mut last_up: Option<bool> = None;
    let mut ignored_logged: std::collections::HashSet<String> = Default::default();
    refresh_selection(
        &handles,
        &state,
        &mut selected_key,
        &mut last_applied_bpm,
        &mut last_up,
    );

    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, addr)) => {
                    let key = crate::vdj::canonical_ip(addr.ip());
                    tracing::info!(target: "dmx::os2l", peer = %addr, instance = %key, "VirtualDJ connected");
                    {
                        let mut g = state.0.lock();
                        g.peers.entry(key.clone()).or_default().conns += 1;
                        g.last_error = None;
                    }
                    tauri::async_runtime::spawn(reader_task(
                        stream,
                        addr.ip(),
                        ev_tx.clone(),
                        shutdown.clone(),
                    ));
                    refresh_selection(&handles, &state, &mut selected_key, &mut last_applied_bpm, &mut last_up);
                    emit_status(&handles, &state);
                }
                Err(e) => {
                    tracing::warn!(target: "dmx::os2l", error = %e, "accept failed");
                }
            },
            msg = ev_rx.recv() => match msg {
                None => break,
                Some(Msg::Gone(ip)) => {
                    let key = crate::vdj::canonical_ip(ip);
                    let still = {
                        let mut g = state.0.lock();
                        let p = g.peers.entry(key.clone()).or_default();
                        p.conns = p.conns.saturating_sub(1);
                        p.conns > 0
                    };
                    if !still {
                        tracing::info!(target: "dmx::os2l", instance = %key, "VirtualDJ disconnected");
                    }
                    refresh_selection(&handles, &state, &mut selected_key, &mut last_applied_bpm, &mut last_up);
                    emit_status(&handles, &state);
                }
                Some(Msg::Event(ip, evt)) => {
                    let key = crate::vdj::canonical_ip(ip);
                    let is_beat = evt.get("evt").and_then(|v| v.as_str()) == Some("beat");
                    if is_beat {
                        let bpm = evt.get("bpm").and_then(|v| v.as_f64()).map(|b| b as f32);
                        state
                            .0
                            .lock()
                            .peers
                            .entry(key.clone())
                            .or_default()
                            .note_beat(Instant::now(), bpm);
                    }
                    refresh_selection(&handles, &state, &mut selected_key, &mut last_applied_bpm, &mut last_up);
                    if selected_key.as_deref() == Some(key.as_str()) {
                        handle_event(
                            &handles,
                            &state,
                            &evt,
                            &mut last_applied_bpm,
                            &mut last_emit,
                        );
                    } else {
                        if ignored_logged.insert(key.clone()) {
                            tracing::info!(
                                target: "dmx::os2l",
                                instance = %key,
                                "events from a non-selected VirtualDJ are ignored (choose it in Config → VirtualDJ)"
                            );
                        }
                        if is_beat {
                            maybe_auto_switch(&handles, &state, &key);
                        }
                    }
                }
            }
        }
    }

    if let Some(daemon) = mdns {
        let _ = daemon.unregister(&mdns_fullname);
        let _ = daemon.shutdown();
    }
    let mine = {
        let mut g = state.0.lock();
        // A newer server may already own the state; don't clobber it.
        let mine = g.generation == generation;
        if mine {
            g.running = false;
            g.peer = None;
        }
        mine
    };
    if mine {
        crate::autopilot::set_os2l_up(false);
    }
    emit_status(&handles, &state);
    tracing::info!(target: "dmx::os2l", "os2l server stopped");
}

/// Rate-limited diagnostic: how VirtualDJ's beat events actually arrive
/// (one DEBUG line every 2 s), the ground truth for tuning Modo Mágico.
fn note_beat_stats(bpm: f32, pos: Option<f64>, strength: Option<f32>, change: Option<bool>) {
    struct Stats {
        since: Instant,
        count: u32,
        changes: u32,
        min_s: f32,
        max_s: f32,
        first_pos: Option<f64>,
    }
    static STATS: std::sync::OnceLock<Mutex<Stats>> = std::sync::OnceLock::new();
    let m = STATS.get_or_init(|| {
        Mutex::new(Stats {
            since: Instant::now(),
            count: 0,
            changes: 0,
            min_s: f32::MAX,
            max_s: f32::MIN,
            first_pos: None,
        })
    });
    let mut st = m.lock();
    st.count += 1;
    if change == Some(true) {
        st.changes += 1;
    }
    if let Some(s) = strength {
        st.min_s = st.min_s.min(s);
        st.max_s = st.max_s.max(s);
    }
    if st.first_pos.is_none() {
        st.first_pos = pos;
    }
    let elapsed = st.since.elapsed();
    if elapsed >= Duration::from_secs(2) {
        tracing::debug!(
            target: "dmx::os2l",
            beats = st.count,
            per_sec = format!("{:.2}", st.count as f32 / elapsed.as_secs_f32()),
            changes = st.changes,
            bpm,
            pos_from = ?st.first_pos,
            pos_now = ?pos,
            strength = ?strength,
            strength_range = format!("{:.2}..{:.2}", st.min_s, st.max_s),
            "os2l beats"
        );
        st.since = Instant::now();
        st.count = 0;
        st.changes = 0;
        st.min_s = f32::MAX;
        st.max_s = f32::MIN;
        st.first_pos = None;
    }
}

fn handle_event(
    handles: &GenericHandles,
    state: &Os2lState,
    evt: &serde_json::Value,
    last_applied_bpm: &mut Option<f32>,
    last_emit: &mut Instant,
) {
    match evt.get("evt").and_then(|v| v.as_str()) {
        Some("beat") => {
            let Some(bpm_raw) = evt.get("bpm").and_then(|v| v.as_f64()) else {
                return;
            };
            if !(20.0..=999.0).contains(&bpm_raw) {
                return;
            }
            let pos = evt.get("pos").and_then(|v| v.as_f64());
            // Same "halve above" musical guard as the HTTP poller, read
            // from the shared VDJ config so both paths behave alike.
            let vdj_cfg = { handles.show.read().show.vdj.clone() };
            let (bpm, beat_pos) = crate::vdj::apply_halve_rule(&vdj_cfg, bpm_raw as f32, pos);

            let changed = match *last_applied_bpm {
                Some(prev) => (prev - bpm).abs() >= 0.01,
                None => true,
            };
            if changed {
                *last_applied_bpm = Some(bpm);
                if let Err(e) = crate::commands::set_overall_bpm_enabled_impl(
                    &handles.app,
                    &handles.show,
                    &handles.globals,
                    true,
                ) {
                    tracing::warn!(target: "dmx::os2l", error = ?e, "failed to enable overall bpm");
                }
                if let Err(e) = crate::commands::set_overall_bpm_impl(
                    &handles.app,
                    &handles.show,
                    &handles.globals,
                    bpm,
                ) {
                    tracing::warn!(target: "dmx::os2l", error = ?e, bpm, "failed to apply bpm");
                }
            }
            if let Some(bp) = beat_pos {
                let anchor = BeatAnchor {
                    set_at: Instant::now(),
                    beat_at_set: bp,
                    bpm,
                };
                handles.chasers.lock().set_beat_anchor(Some(anchor));
                handles.movement.lock().set_beat_anchor(Some(anchor));
                state.0.lock().last_anchor = Some(anchor);
            }
            // Modo Mágico and the cue follower ride the musical grid (raw
            // bpm + raw pos), not the chasers' halved one.
            if let Some(raw) = pos {
                let change = evt.get("change").and_then(|v| v.as_bool()).unwrap_or(false);
                state.0.lock().last_raw = Some(RawBeat {
                    beat: raw,
                    bpm: bpm_raw as f32,
                    at: Instant::now(),
                    change,
                });
                crate::autopilot::set_beat_anchor(
                    Some(BeatAnchor {
                        set_at: Instant::now(),
                        beat_at_set: raw,
                        bpm: bpm_raw as f32,
                    }),
                    crate::autopilot::ClockSource::Os2l,
                );
            }
            {
                let mut g = state.0.lock();
                g.last_beat_bpm = Some(bpm);
                g.last_beat_at_secs = Some(now_unix_secs());
            }
            // Modo Mágico: a beat means the music is on; its strength is the
            // level when the HTTP poller isn't around.
            let strength = evt
                .get("strength")
                .and_then(|v| v.as_f64())
                .map(|s| s as f32);
            crate::autopilot::note_os2l_beat(strength);
            note_beat_stats(
                bpm_raw as f32,
                pos,
                strength,
                evt.get("change").and_then(|v| v.as_bool()),
            );
            // Beats arrive ~2-4×/s; a full status emit each time is
            // wasteful. Refresh the UI at most twice a second.
            if changed || last_emit.elapsed() > Duration::from_millis(500) {
                *last_emit = Instant::now();
                emit_status(handles, state);
            }
        }
        Some("btn") => {
            let Some(name) = evt.get("name").and_then(|v| v.as_str()) else {
                return;
            };
            let state_on = evt
                .get("state")
                .and_then(|v| v.as_str())
                .map(|s| s.eq_ignore_ascii_case("on"))
                .unwrap_or(true);
            handle_button(handles, state, name, state_on);
        }
        other => {
            tracing::trace!(target: "dmx::os2l", evt = ?other, "ignored os2l event");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitter_handles_glued_and_split_objects() {
        let mut s = JsonSplitter::new();
        // Two glued objects plus the first half of a third.
        let out = s.push(br#"{"evt":"beat","bpm":128}{"evt":"btn","name":"a{b}"} {"evt":"#);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["evt"], "beat");
        assert_eq!(out[1]["name"], "a{b}");
        // Rest of the third arrives later.
        let out = s.push(br#""cmd","id":3}"#);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["evt"], "cmd");
    }

    #[test]
    fn splitter_survives_escaped_quotes_and_noise() {
        let mut s = JsonSplitter::new();
        let out = s.push(br#"junk {"name":"say \"hi\" {ok}","evt":"btn"} trailing"#);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["name"], r#"say "hi" {ok}"#);
    }

    #[test]
    fn quantize_grace_window_fires_immediately() {
        // 10% past the beat at 120 BPM (500 ms/beat) → inside grace.
        assert!(quantize_delay(16.10, 120.0).is_none());
        // Exactly on the beat too.
        assert!(quantize_delay(16.0, 120.0).is_none());
    }

    #[test]
    fn quantize_waits_for_the_next_beat() {
        // 60% into a 500 ms beat → 200 ms to the next grid line.
        let d = quantize_delay(16.60, 120.0).expect("must defer");
        let ms = d.as_millis() as i64;
        assert!((ms - 200).abs() <= 1, "got {ms} ms");
        // 90% in → 50 ms.
        let d = quantize_delay(7.90, 120.0).expect("must defer");
        let ms = d.as_millis() as i64;
        assert!((ms - 50).abs() <= 1, "got {ms} ms");
    }
}
