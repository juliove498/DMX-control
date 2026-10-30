//! Live/mirror replication between consoles on the LAN.
//!
//! One instance is **live** (drives outputs, talks to VirtualDJ); any number
//! of **mirrors** hold an always-current copy of the show + fixture library so
//! the operator can jump machines mid-gig with one click. Design choices, per
//! the operator's spec:
//!
//! - Mirrors are passive: no DMX output, no OS2L server, no VDJ poller, no
//!   cue follower until they take control. Editing happens on the live one;
//!   mirrors receive.
//! - Replication is whole-show, last-write-wins from the live side: a 1 MB
//!   JSON over the LAN beats any merge cleverness for predictability.
//! - Takeover is MANUAL. Automatic failover on a flaky WiFi means two
//!   consoles fighting over the same Art-Net nodes (split-brain) — worse
//!   than the failure it tries to fix. When the live console vanishes, the
//!   mirror raises a "take control?" banner and the human decides.
//!
//! Transport: NDJSON over one TCP connection per mirror, on the same port the
//! presence beacon advertises. Mirrors discover the live console through the
//! mDNS peer list (role=live in the TXT record) and reconnect forever.
//! Trust model matches the bridge/OS2L servers: anyone on the rig's private
//! LAN is assumed friendly.

use std::io::ErrorKind;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use ts_rs::TS;

pub const MIRROR_STATUS_EVENT: &str = "mirror:status";
/// One port for everything: the presence beacon advertises it and the live
/// console listens on it.
pub const MIRROR_PORT: u16 = 9996;

const PING_EVERY: Duration = Duration::from_secs(2);
/// Cadence of the live→mirror engine-picture stream (fase 2). Only actually
/// sent when the merged frame changed since the last send.
const ENGINE_EVERY: Duration = Duration::from_millis(750);
/// A mirror that hears nothing for this long declares the live console lost.
const LIVE_TIMEOUT: Duration = Duration::from_secs(6);
const DEBOUNCE: Duration = Duration::from_millis(250);
const RECONNECT_EVERY: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    #[default]
    Live,
    Mirror,
}

impl Role {
    pub fn is_live(self) -> bool {
        matches!(self, Role::Live)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Live => "live",
            Role::Mirror => "mirror",
        }
    }
}

/// Persisted per machine (NOT synced) via `instance.rs`.
pub fn load_role() -> Role {
    crate::instance::load().role
}

fn save_role(role: Role) {
    let mut inst = crate::instance::load();
    inst.role = role;
    crate::instance::save(&inst);
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct MirrorStatus {
    /// "live" | "mirror"
    pub role: String,
    /// Live side: connected mirror count.
    pub mirrors: u32,
    /// Mirror side: name of the live console we're following, if connected.
    pub live_name: Option<String>,
    /// Mirror side: we had (or expected) a live console and lost it.
    pub live_lost: bool,
    /// Replication revision (live: last broadcast; mirror: last applied).
    pub rev: u32,
}

struct MirrorInner {
    role: Role,
    rev: u64,
    lib_dirty: bool,
    mirrors: u32,
    live_name: Option<String>,
    live_lost: bool,
    /// Bumped on every role change; tasks capture their generation and exit
    /// as soon as it moves on, so stale servers/clients can't linger.
    generation: u64,
    /// Mirror side: outbound line channel to the live console (for "yield").
    to_live: Option<tokio::sync::mpsc::UnboundedSender<String>>,
}

pub struct MirrorShared {
    inner: Mutex<MirrorInner>,
    /// Wakes the live-side broadcaster; the value is the show revision.
    show_changed: tokio::sync::watch::Sender<u64>,
}

#[derive(Clone)]
pub struct MirrorState(Arc<MirrorShared>);

impl MirrorState {
    pub fn new(role: Role) -> Self {
        let (tx, _rx) = tokio::sync::watch::channel(0u64);
        Self(Arc::new(MirrorShared {
            inner: Mutex::new(MirrorInner {
                role,
                rev: 0,
                lib_dirty: false,
                mirrors: 0,
                live_name: None,
                live_lost: false,
                generation: 0,
                to_live: None,
            }),
            show_changed: tx,
        }))
    }

    pub fn role(&self) -> Role {
        self.0.inner.lock().unwrap().role
    }

    fn generation(&self) -> u64 {
        self.0.inner.lock().unwrap().generation
    }

    pub fn status(&self) -> MirrorStatus {
        let g = self.0.inner.lock().unwrap();
        MirrorStatus {
            role: g.role.as_str().to_string(),
            mirrors: g.mirrors,
            live_name: g.live_name.clone(),
            live_lost: g.live_lost,
            rev: g.rev as u32,
        }
    }
}

fn emit_status(app: &AppHandle, state: &MirrorState) {
    let _ = app.emit(MIRROR_STATUS_EVENT, state.status());
}

/// Called from `persist_show` after every successful show mutation. On the
/// live console it bumps the revision and wakes the broadcaster; on mirrors
/// it does nothing (their mutations come FROM the live side).
pub fn notify_show_changed(app: &AppHandle) {
    let Some(state) = app.try_state::<MirrorState>() else {
        return;
    };
    let state = state.inner().clone();
    let mut g = state.0.inner.lock().unwrap();
    if g.role.is_live() {
        g.rev += 1;
        let rev = g.rev;
        drop(g);
        let _ = state.0.show_changed.send(rev);
    }
}

/// Called when a fixture definition changes on disk so mirrors get the
/// library too (the connect handshake always sends the full library, which
/// also covers imports done while a mirror was offline).
pub fn notify_library_changed(app: &AppHandle) {
    let Some(state) = app.try_state::<MirrorState>() else {
        return;
    };
    let state = state.inner().clone();
    {
        let mut g = state.0.inner.lock().unwrap();
        if !g.role.is_live() {
            return;
        }
        g.lib_dirty = true;
        g.rev += 1;
    }
    let rev = state.0.inner.lock().unwrap().rev;
    let _ = state.0.show_changed.send(rev);
}

// ---- Commands ---------------------------------------------------------------

#[tauri::command]
pub fn mirror_status(state: tauri::State<'_, MirrorState>) -> MirrorStatus {
    state.status()
}

#[tauri::command]
pub async fn take_control(app: AppHandle) -> Result<(), String> {
    let state = app.state::<MirrorState>().inner().clone();
    if state.role().is_live() {
        return Ok(());
    }
    // Best-effort: ask the current live console to stand down first so we
    // never have two sets of outputs open at once. If it's dead or gone the
    // send just fails and we proceed.
    let tx = state.0.inner.lock().unwrap().to_live.clone();
    if let Some(tx) = tx {
        let _ = tx.send("{\"t\":\"yield\"}".to_string());
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    promote(&app, &state);
    Ok(())
}

#[tauri::command]
pub async fn become_mirror(app: AppHandle) -> Result<(), String> {
    let state = app.state::<MirrorState>().inner().clone();
    if !state.role().is_live() {
        return Ok(());
    }
    demote(&app, &state);
    Ok(())
}

// ---- Role transitions -------------------------------------------------------

fn promote(app: &AppHandle, state: &MirrorState) {
    {
        let mut g = state.0.inner.lock().unwrap();
        g.role = Role::Live;
        g.generation += 1;
        g.live_name = None;
        g.live_lost = false;
        g.mirrors = 0;
        g.to_live = None;
    }
    save_role(Role::Live);
    crate::presence::advertise_role(app, Role::Live.as_str());
    tracing::info!(target: "dmx::mirror", "TAKING CONTROL: promoting to live");

    // Outputs first (the audience-visible part), then the VDJ services.
    if let Err(e) = crate::commands::resume_output(
        app.clone(),
        app.state(),
        app.state(),
        app.state(),
        app.state(),
        app.state(),
        app.state(),
        app.state(),
    ) {
        tracing::error!(target: "dmx::mirror", error = ?e, "takeover: output start failed");
    }
    crate::start_music_services(app);
    start_server_tasks(app.clone(), state.clone());
    emit_status(app, state);
}

fn demote(app: &AppHandle, state: &MirrorState) {
    {
        let mut g = state.0.inner.lock().unwrap();
        g.role = Role::Mirror;
        g.generation += 1;
        g.mirrors = 0;
        g.live_lost = false;
    }
    save_role(Role::Mirror);
    crate::presence::advertise_role(app, Role::Mirror.as_str());
    tracing::info!(target: "dmx::mirror", "standing down: demoting to mirror");

    // Passive mode per spec: no outputs, no VDJ listeners of any kind.
    let _ = crate::commands::suspend_output(app.state());
    crate::os2l::stop_server(app.state::<crate::os2l::Os2lState>().inner());
    crate::vdj::cues::stop(app.state::<crate::vdj::cues::CueFollower>().inner());
    crate::vdj::stop_poller(
        app.clone(),
        app.state::<crate::vdj::VdjState>().inner().clone(),
    );
    start_client_loop(app.clone(), state.clone());
    emit_status(app, state);
}

/// Boot entry point: spawns whatever the persisted role needs. Outputs and
/// music services for the live role are handled by `lib.rs` setup itself.
pub fn start(app: AppHandle) {
    let state = app.state::<MirrorState>().inner().clone();
    match state.role() {
        Role::Live => start_server_tasks(app.clone(), state),
        Role::Mirror => start_client_loop(app.clone(), state),
    }
}

// ---- Live side: replication server -----------------------------------------

fn start_server_tasks(app: AppHandle, state: MirrorState) {
    let my_gen = state.generation();
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(("0.0.0.0", MIRROR_PORT)).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(target: "dmx::mirror", error = %e, port = MIRROR_PORT, "sync server could not bind");
                return;
            }
        };
        tracing::info!(target: "dmx::mirror", port = MIRROR_PORT, "sync server listening");
        loop {
            if state.generation() != my_gen {
                return; // role changed; a new task owns the socket lifecycle
            }
            let accepted =
                tokio::time::timeout(Duration::from_millis(500), listener.accept()).await;
            match accepted {
                Err(_) => continue, // timeout tick: just re-check generation
                Ok(Err(e)) => {
                    tracing::warn!(target: "dmx::mirror", error = %e, "sync accept failed");
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                Ok(Ok((stream, addr))) => {
                    tracing::info!(target: "dmx::mirror", %addr, "mirror connected");
                    let app = app.clone();
                    let state = state.clone();
                    tauri::async_runtime::spawn(async move {
                        {
                            let mut g = state.0.inner.lock().unwrap();
                            g.mirrors += 1;
                        }
                        emit_status(&app, &state);
                        let res = serve_mirror(&app, &state, stream, my_gen).await;
                        if let Err(e) = res {
                            tracing::info!(target: "dmx::mirror", %addr, error = %e, "mirror connection ended");
                        }
                        {
                            let mut g = state.0.inner.lock().unwrap();
                            g.mirrors = g.mirrors.saturating_sub(1);
                        }
                        emit_status(&app, &state);
                    });
                }
            }
        }
    });
}

async fn serve_mirror(
    app: &AppHandle,
    state: &MirrorState,
    stream: tokio::net::TcpStream,
    my_gen: u64,
) -> Result<(), std::io::Error> {
    let (rx_half, mut tx) = stream.into_split();
    let mut lines = BufReader::new(rx_half).lines();
    let mut rev_rx = state.0.show_changed.subscribe();
    let mut ping = tokio::time::interval(PING_EVERY);
    let mut engine_tick = tokio::time::interval(ENGINE_EVERY);
    let mut last_engine_sig: u64 = 0;

    // Handshake: the full library first (definitions the show references),
    // then the current show.
    send_line(&mut tx, &library_message()).await?;
    send_line(&mut tx, &show_message(app, state)).await?;

    loop {
        if state.generation() != my_gen {
            return Ok(());
        }
        tokio::select! {
            changed = rev_rx.changed() => {
                if changed.is_err() {
                    return Ok(());
                }
                // Debounce: a slider drag persists dozens of times per
                // second; ship one snapshot after the burst settles.
                tokio::time::sleep(DEBOUNCE).await;
                rev_rx.borrow_and_update();
                let lib_dirty = {
                    let mut g = state.0.inner.lock().unwrap();
                    std::mem::take(&mut g.lib_dirty)
                };
                if lib_dirty {
                    send_line(&mut tx, &library_message()).await?;
                }
                send_line(&mut tx, &show_message(app, state)).await?;
            }
            _ = ping.tick() => {
                send_line(&mut tx, "{\"t\":\"ping\"}").await?;
            }
            _ = engine_tick.tick() => {
                if let Some((sig, msg)) = engine_message(app, last_engine_sig) {
                    last_engine_sig = sig;
                    send_line(&mut tx, &msg).await?;
                }
            }
            line = lines.next_line() => {
                match line? {
                    None => return Ok(()), // mirror hung up
                    Some(l) => {
                        let msg: serde_json::Value = match serde_json::from_str(&l) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        if msg.get("t").and_then(|t| t.as_str()) == Some("yield") {
                            // A mirror is taking over; stand down. The
                            // generation bump inside demote() ends this
                            // task and the accept loop.
                            tracing::warn!(target: "dmx::mirror", "yield received: a mirror is taking control");
                            let app2 = app.clone();
                            let state2 = state.clone();
                            tauri::async_runtime::spawn(async move {
                                demote(&app2, &state2);
                            });
                        }
                    }
                }
            }
        }
    }
}

async fn send_line(
    tx: &mut tokio::net::tcp::OwnedWriteHalf,
    line: &str,
) -> Result<(), std::io::Error> {
    tx.write_all(line.as_bytes()).await?;
    tx.write_all(b"\n").await?;
    Ok(())
}

fn show_message(app: &AppHandle, state: &MirrorState) -> String {
    let show_st = app.state::<crate::show::state::ShowState>();
    let snapshot = show_st.read().show.clone();
    let rev = state.0.inner.lock().unwrap().rev;
    serde_json::json!({ "t": "show", "rev": rev, "blob": snapshot }).to_string()
}

fn library_message() -> String {
    let mut files: Vec<serde_json::Value> = Vec::new();
    if let Some(dir) = crate::show::library::library_dir() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                match std::fs::read_to_string(&path) {
                    Ok(text) => files.push(serde_json::json!({ "name": name, "text": text })),
                    Err(e) => {
                        tracing::warn!(target: "dmx::mirror", file = %name, error = %e, "library file unreadable; skipping")
                    }
                }
            }
        }
    }
    serde_json::json!({ "t": "lib", "files": files }).to_string()
}

/// FNV-1a over the merged output frames; used to skip idle engine sends.
fn engine_signature(frames: &[(u16, [u8; crate::engine::DMX_CHANNELS])]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (id, data) in frames {
        for b in id.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        for &b in data.iter() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// Live side, fase 2: the merged output picture (base + effects + master +
/// blackout — exactly what the audience sees) for the mirror to display and
/// for a takeover to resume from. `None` when nothing changed since
/// `last_sig` or there are no universes yet.
fn engine_message(app: &AppHandle, last_sig: u64) -> Option<(u64, String)> {
    let engine = app.state::<crate::engine::EngineState>();
    let frames: Vec<(u16, [u8; crate::engine::DMX_CHANNELS])> = {
        let g = engine.read();
        let ids: Vec<u16> = g.universes.iter().map(|u| u.id).collect();
        ids.into_iter()
            .filter_map(|id| g.snapshot_universe(id).map(|d| (id, d)))
            .collect()
    };
    if frames.is_empty() {
        return None;
    }
    let sig = engine_signature(&frames);
    if sig == last_sig {
        return None;
    }
    let universes: Vec<serde_json::Value> = frames
        .iter()
        .map(|(id, data)| serde_json::json!({ "id": id, "data": data.to_vec() }))
        .collect();
    Some((
        sig,
        serde_json::json!({ "t": "engine", "universes": universes }).to_string(),
    ))
}

// ---- Mirror side: replication client ---------------------------------------

fn start_client_loop(app: AppHandle, state: MirrorState) {
    let my_gen = state.generation();
    tauri::async_runtime::spawn(async move {
        let mut ever_saw_live = false;
        loop {
            if state.generation() != my_gen {
                return;
            }
            let live = crate::presence::live_peer(&app);
            let Some((live_name, addr)) = live else {
                if ever_saw_live {
                    let mut g = state.0.inner.lock().unwrap();
                    if !g.live_lost || g.live_name.is_some() {
                        g.live_lost = true;
                        g.live_name = None;
                        drop(g);
                        emit_status(&app, &state);
                    }
                }
                tokio::time::sleep(RECONNECT_EVERY).await;
                continue;
            };
            let connected =
                tokio::time::timeout(Duration::from_secs(3), tokio::net::TcpStream::connect(addr))
                    .await;
            let stream = match connected {
                Ok(Ok(s)) => s,
                _ => {
                    if ever_saw_live {
                        let mut g = state.0.inner.lock().unwrap();
                        g.live_lost = true;
                        g.live_name = None;
                        drop(g);
                        emit_status(&app, &state);
                    }
                    tokio::time::sleep(RECONNECT_EVERY).await;
                    continue;
                }
            };
            ever_saw_live = true;
            tracing::info!(target: "dmx::mirror", live = %live_name, %addr, "mirroring live console");
            {
                let mut g = state.0.inner.lock().unwrap();
                g.live_name = Some(live_name.clone());
                g.live_lost = false;
            }
            emit_status(&app, &state);

            let res = follow_live(&app, &state, stream, my_gen).await;
            if state.generation() != my_gen {
                return;
            }
            tracing::info!(target: "dmx::mirror", error = ?res.err(), "lost the live console");
            {
                let mut g = state.0.inner.lock().unwrap();
                g.live_name = None;
                g.live_lost = true;
                g.to_live = None;
            }
            emit_status(&app, &state);
            tokio::time::sleep(RECONNECT_EVERY).await;
        }
    });
}

async fn follow_live(
    app: &AppHandle,
    state: &MirrorState,
    stream: tokio::net::TcpStream,
    my_gen: u64,
) -> Result<(), std::io::Error> {
    let (rx_half, mut tx) = stream.into_split();
    let mut lines = BufReader::new(rx_half).lines();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    state.0.inner.lock().unwrap().to_live = Some(out_tx);

    send_line(&mut tx, "{\"t\":\"hello\"}").await?;

    loop {
        if state.generation() != my_gen {
            return Ok(());
        }
        tokio::select! {
            outbound = out_rx.recv() => {
                if let Some(line) = outbound {
                    send_line(&mut tx, &line).await?;
                }
            }
            line = tokio::time::timeout(LIVE_TIMEOUT, lines.next_line()) => {
                let line = line.map_err(|_| {
                    std::io::Error::new(ErrorKind::TimedOut, "no data from live console")
                })??;
                let Some(l) = line else {
                    return Ok(()); // live hung up
                };
                let msg: serde_json::Value = match serde_json::from_str(&l) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                match msg.get("t").and_then(|t| t.as_str()) {
                    Some("show") => {
                        let rev = msg.get("rev").and_then(|r| r.as_u64()).unwrap_or(0);
                        if let Some(blob) = msg.get("blob") {
                            apply_synced_show(app, state, blob.clone(), rev);
                        }
                    }
                    Some("lib") => {
                        if let Some(files) = msg.get("files").and_then(|f| f.as_array()) {
                            apply_synced_library(app, files);
                        }
                    }
                    Some("engine") => {
                        if let Some(unis) = msg.get("universes").and_then(|u| u.as_array()) {
                            apply_synced_engine(app, unis);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn apply_synced_show(app: &AppHandle, state: &MirrorState, blob: serde_json::Value, rev: u64) {
    let show: crate::show::file::ShowFileV1 = match serde_json::from_value(blob) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(target: "dmx::mirror", error = %e, "synced show did not parse; ignoring");
            return;
        }
    };
    let show_st = app.state::<crate::show::state::ShowState>();
    let (snapshot, path) = {
        let mut s = show_st.write();
        s.show = show;
        s.dirty = false;
        (s.show.clone(), s.path.clone())
    };
    // Autosave only: never touch a named show file the mirror may have open —
    // taking over must not silently rewrite the operator's own files.
    if let Err(e) = crate::show::session::write_autosave(&snapshot, path.as_deref()) {
        tracing::warn!(target: "dmx::mirror", error = %e, "mirror autosave failed");
    }
    // Push the replicated config into the runtime engines so a takeover
    // starts from current data (outputs stay down until promotion).
    crate::commands::sync_chasers(
        show_st.inner(),
        app.state::<crate::engine::output_thread::SharedChasers>()
            .inner(),
    );
    crate::commands::sync_movements(
        show_st.inner(),
        app.state::<crate::engine::output_thread::SharedMovement>()
            .inner(),
    );
    crate::commands::sync_globals(
        show_st.inner(),
        app.state::<crate::engine::output_thread::SharedGlobals>()
            .inner(),
    );
    {
        let mut g = state.0.inner.lock().unwrap();
        g.rev = rev;
    }
    let _ = app.emit(crate::commands::SHOW_EVENT, ());
    emit_status(app, state);
}

/// Mirror side, fase 2: load the live console's merged frames into the local
/// engine, with master pinned to 255 so the local merge displays them
/// verbatim (they already carry the live master/blackout). The Stage/3D views
/// poll the engine every 200 ms, so the mirror's UI animates with the live
/// show for free — and a takeover's first output frame equals the last thing
/// the audience saw.
fn apply_synced_engine(app: &AppHandle, universes: &[serde_json::Value]) {
    let engine = app.state::<crate::engine::EngineState>();
    let mut g = engine.write();
    g.master = 255;
    for u in universes {
        let Some(id) = u.get("id").and_then(|v| v.as_u64()) else {
            continue;
        };
        let id = id as u16;
        let Some(arr) = u.get("data").and_then(|v| v.as_array()) else {
            continue;
        };
        if arr.len() != crate::engine::DMX_CHANNELS {
            continue;
        }
        let mut data = [0u8; crate::engine::DMX_CHANNELS];
        for (i, v) in arr.iter().enumerate() {
            data[i] = v.as_u64().unwrap_or(0).min(255) as u8;
        }
        match g.universes.iter_mut().find(|un| un.id == id) {
            Some(un) => un.data = data,
            None => {
                let mut un = crate::engine::Universe::new(id);
                un.data = data;
                g.universes.push(un);
            }
        }
    }
}

fn apply_synced_library(app: &AppHandle, files: &[serde_json::Value]) {
    let Some(dir) = crate::show::library::library_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let show_st = app.state::<crate::show::state::ShowState>();
    let mut applied = 0usize;
    for f in files {
        let (Some(name), Some(text)) = (
            f.get("name").and_then(|n| n.as_str()),
            f.get("text").and_then(|t| t.as_str()),
        ) else {
            continue;
        };
        // The name is used as a filename: refuse anything path-shaped.
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            tracing::warn!(target: "dmx::mirror", %name, "suspicious library filename; skipping");
            continue;
        }
        let def: crate::show::fixture::FixtureDefinition = match serde_json::from_str(text) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(target: "dmx::mirror", %name, error = %e, "synced definition did not parse; skipping");
                continue;
            }
        };
        if let Err(e) = std::fs::write(dir.join(name), text.as_bytes()) {
            tracing::warn!(target: "dmx::mirror", %name, error = %e, "could not write synced definition");
        }
        show_st.write().library.insert(def.id.clone(), def);
        applied += 1;
    }
    if applied > 0 {
        tracing::info!(target: "dmx::mirror", count = applied, "library synced from live console");
        let _ = app.emit(crate::commands::SHOW_EVENT, ());
    }
}
