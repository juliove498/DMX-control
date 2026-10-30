//! Cue follower: fires the show's per-track cues (`show.track_cues`)
//! by tracking VirtualDJ's playhead.
//!
//! Two clocks cooperate:
//! - Every ~400 ms an HTTP poll (Network Control endpoint, same config
//!   as the BPM poller) reads the playing deck's file path, beat-grid
//!   position and BPM. This is the ground truth — it also detects
//!   track changes and seeks.
//! - Every 50 ms a local tick interpolates the position forward from
//!   the last poll and fires any cue whose time (`pos_secs`, the
//!   editor's coordinate; `beat` for legacy cues) was crossed since the
//!   previous tick. Interpolation makes firing precision a few ms even
//!   though HTTP only answers ~2.5×/s.
//!
//! Semantics chosen for rehearsals: seeking backwards re-arms the cues
//! behind the new position (play the passage again → lights repeat);
//! seeking forward does NOT machine-gun the skipped cues. A track
//! change re-arms everything.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{Emitter, Manager};
use ts_rs::TS;

use super::{fetch_script, parse_vdj_bool, parse_vdj_bpm, VdjConfig};
use crate::midi::generic::{dispatch_action, GenericHandles};
use crate::show::button_bindings::ButtonAction;
use crate::show::track_cues::TrackCue;

pub const CUES_STATUS_EVENT: &str = "cues:status";

#[derive(Debug, Clone, Serialize, TS, Default)]
#[ts(export, export_to = "../bindings/")]
pub struct CueFollowerStatus {
    pub running: bool,
    /// True while the position is known (VDJ answers over HTTP, or its
    /// beats arrive over OS2L on a song whose grid we know).
    pub playing: bool,
    /// File path of the tracked deck's song, as VDJ reports it (or the
    /// editor's open track while following over OS2L — see `assumed`).
    pub file_path: Option<String>,
    pub title: Option<String>,
    /// Interpolated beat-grid position (refreshed every fast tick).
    pub beat_pos: Option<f64>,
    /// Interpolated track position in seconds (VDJ's POI coordinate).
    pub pos_secs: Option<f64>,
    pub bpm: Option<f32>,
    /// Last HTTP failure, kept while VDJ's Network Control stays silent.
    pub last_error: Option<String>,
    /// Ids of cues already fired for the current track. The UI dims
    /// them; they re-arm on rewind or track reload.
    pub fired_cue_ids: Vec<String>,
    /// Where the position comes from right now: "http" (VDJ's Network
    /// Control answers), "os2l" (VDJ's beats over OS2L on the song's own
    /// grid while HTTP is down) or "none".
    pub signal: String,
    /// OS2L mode: "vdj" (grid from VDJ's database), "http" (learned from
    /// earlier readings), "estimate" (the editor's own grid) or "none".
    pub grid_source: String,
    /// OS2L mode: the song is the editor's open track, not confirmed by
    /// VDJ — the editor follows it, but no cue fires.
    pub assumed: bool,
    /// host:port of the HTTP endpoint the follower polls.
    pub http_endpoint: String,
}

/// Where the playhead comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Signal {
    #[default]
    None,
    Http,
    Os2l,
}

impl Signal {
    fn as_str(self) -> &'static str {
        match self {
            Signal::None => "none",
            Signal::Http => "http",
            Signal::Os2l => "os2l",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GridSource {
    /// `Scan Bpm`/`Phase` from VirtualDJ's database — VDJ's own grid.
    Vdj,
    /// Learned from HTTP readings (`secs` vs `beatpos`); exact at pitch 0.
    Http,
    /// The cue editor's own estimate for the file.
    Estimate,
}

impl GridSource {
    fn as_str(self) -> &'static str {
        match self {
            GridSource::Vdj => "vdj",
            GridSource::Http => "http",
            GridSource::Estimate => "estimate",
        }
    }
}

/// Beat index → track seconds for one song.
#[derive(Clone, Copy, Debug)]
struct FollowGrid {
    origin_secs: f64,
    beat_secs: f64,
    source: GridSource,
}

impl FollowGrid {
    fn secs_at(&self, beat: f64) -> f64 {
        self.origin_secs + beat * self.beat_secs
    }
}

#[derive(Default)]
struct CueFollowerInner {
    /// Bumped by every `start`; a retiring clock thread only clears
    /// `running` if no newer run replaced it.
    generation: u64,
    running: bool,
    playing: bool,
    file_path: Option<String>,
    title: Option<String>,
    beat_pos: Option<f64>,
    pos_secs: Option<f64>,
    bpm: Option<f32>,
    last_error: Option<String>,
    fired: Vec<String>,
    shutdown: Option<tokio::sync::watch::Sender<bool>>,
    signal: Signal,
    grid_source: Option<GridSource>,
    assumed: bool,
    http_endpoint: String,
    /// The track the cue editor has open: the song to follow over OS2L
    /// while VDJ's HTTP server is down.
    assumed_file: Option<String>,
    /// Beat grid per file (VDJ's own, or learned from HTTP).
    grids: HashMap<String, FollowGrid>,
}

/// Cross-path double-fire guard. A cue can reach the dispatcher two
/// ways at once: the follower crossing it AND VirtualDJ's own synced
/// action POI arriving over OS2L (`os2l_button 'cue:<id>'`). Both
/// paths call this before dispatching; the second call within the
/// window is refused. Global on purpose — the two callers live in
/// different subsystems with no shared handle.
pub fn claim_cue_fire(id: &str) -> bool {
    use std::collections::HashMap;
    use std::sync::{Mutex as StdMutex, OnceLock};
    const WINDOW: Duration = Duration::from_millis(1500);
    static RECENT: OnceLock<StdMutex<HashMap<String, Instant>>> = OnceLock::new();
    let m = RECENT.get_or_init(|| StdMutex::new(HashMap::new()));
    let mut g = m.lock().expect("cue fire guard");
    let now = Instant::now();
    g.retain(|_, t| now.duration_since(*t) < WINDOW);
    if g.contains_key(id) {
        return false;
    }
    g.insert(id.to_string(), now);
    true
}

/// A synced POI may sit this far *before* the cue's authored position
/// and still count as the same moment. Further out it is stale (the
/// cue was moved in the editor after the export) — the POI is ignored
/// and the follower fires the cue at the authored time.
const POI_STALE_SECS: f64 = 0.35;

/// Fire a cue from whichever clock saw it first: the follower crossing
/// it, or VirtualDJ's synced action POI arriving over OS2L. While the
/// follower runs its `fired` list is the dedup (re-armed on rewind and
/// track change, dims the cue in the UI); `claim_cue_fire` covers the
/// rest. Returns whether the cue dispatched.
/// `trust_fired`: whether the follower's `fired` list may veto this
/// call. The follower maintains the list, so it always trusts it; a POI
/// trusts it only while the follower agrees with VDJ about where the
/// song is (see `fire_cue_from_poi`) — otherwise the list may belong to
/// a previous pass (deck restarted, seek not seen yet) and the cue
/// falls back to the time-window dedup.
pub fn fire_cue(
    handles: &GenericHandles,
    id: &str,
    name: &str,
    action: &ButtonAction,
    source: &'static str,
    trust_fired: bool,
) -> bool {
    let follower = handles.app.try_state::<CueFollower>();
    if let Some(f) = follower.as_ref() {
        let mut g = f.0.lock();
        if g.running {
            let seen = g.fired.iter().any(|x| x == id);
            if seen && trust_fired {
                tracing::debug!(target: "dmx::cues", source, %name, "cue already fired; skipping");
                return false;
            }
            if !seen {
                g.fired.push(id.to_string());
            }
        }
    }
    if !claim_cue_fire(id) {
        tracing::debug!(target: "dmx::cues", source, %name, "cue just fired by the other path; skipping");
        return false;
    }
    tracing::info!(
        target: "dmx::cues",
        source, %name, what = %crate::os2l::describe_action(handles, action),
        "cue fired"
    );
    fire_cue_action(handles, action);
    if let Some(f) = follower.as_ref() {
        emit_status(handles, f.inner());
    }
    true
}

/// Entry for VirtualDJ's synced action POIs (`os2l_button 'cue:<id>'`).
/// The POI's timing is trusted only while it agrees with the authored
/// position of the cue it names; a POI that arrives early because the
/// cue was moved after the export is left for the follower.
pub fn fire_cue_from_poi(handles: &GenericHandles, id: &str, action: &ButtonAction) -> bool {
    let cue = {
        let s = handles.show.read();
        s.show.track_cues.iter().find_map(|t| {
            t.cues
                .iter()
                .find(|c| c.id == id)
                .map(|c| (t.file_path.clone(), c.name.clone(), c.pos_secs))
        })
    };
    let Some((file, name, pos_secs)) = cue else {
        tracing::info!(target: "dmx::cues", %id, "POI names a cue that no longer exists; ignored");
        return false;
    };
    // Where the follower thinks the song is, relative to this cue's
    // authored position — only meaningful while it is playing this song.
    let drift = (pos_secs > 0.0)
        .then(|| {
            handles.app.try_state::<CueFollower>().and_then(|f| {
                let g = f.0.lock();
                (g.running && g.playing && g.file_path.as_deref() == Some(file.as_str()))
                    .then_some(g.pos_secs)
                    .flatten()
                    .map(|now| now - pos_secs)
            })
        })
        .flatten();
    let trust_fired = match drift {
        // The POI sits well before the cue: it was moved in the editor
        // after the export. The follower fires it at the authored time.
        Some(d) if d < -POI_STALE_SECS => {
            tracing::info!(
                target: "dmx::cues",
                %name, early_secs = -d,
                "stale POI (cue moved after the export); the follower fires it on time"
            );
            return false;
        }
        // Follower and VDJ agree: its `fired` list is this pass's.
        Some(d) if d <= POI_STALE_SECS => true,
        // The follower is ahead of where VDJ just crossed: a seek it has
        // not seen yet (or a POI left late by an edit). Its list may be
        // last pass's — fire, letting the time window dedup.
        Some(d) => {
            tracing::info!(
                target: "dmx::cues",
                %name, follower_ahead_secs = d,
                "POI before the follower caught up; firing by time window"
            );
            false
        }
        None => false,
    };
    fire_cue(handles, id, &name, action, "poi", trust_fired)
}

/// Timeline semantics: a cue *asserts* a state, it never toggles. A
/// pad pressed twice should turn a look off; a cue crossed twice (two
/// cues, a rewind) must leave the rig in the same look.
///   blind    → one-beat hit (like a blinder bump)
///   blackout → on (the next look cue is how it ends)
///   scene / snapshot / loop group → that look, lifting any blackout
///   anything else → the usual button press
fn fire_cue_action(handles: &GenericHandles, action: &ButtonAction) {
    let Some(action) = crate::midi::generic::resolve_indexed(handles, action) else {
        return;
    };
    match &action {
        ButtonAction::Blind => {
            crate::midi::generic::blind_pulse(handles, cue_beat_len(handles));
        }
        ButtonAction::Blackout => {
            crate::autopilot::note_manual_action(&action);
            cue_blackout(handles, true);
        }
        ButtonAction::ToggleSnapshot { id } => {
            crate::autopilot::note_manual_action(&action);
            // Re-applying the active snapshot is harmless and makes the
            // cue idempotent; the snapshot carries its own blackout flag.
            if let Err(err) = crate::commands::activate_snapshot_impl(
                &handles.app,
                &handles.engine,
                &handles.show,
                &handles.chasers,
                &handles.movement,
                &handles.globals,
                &handles.scenes,
                &handles.loops,
                &handles.snapshots,
                id,
            ) {
                tracing::warn!(?err, "cue: snapshot failed");
            }
        }
        ButtonAction::RecallScene { id } => {
            crate::autopilot::note_manual_action(&action);
            let active = handles
                .scenes
                .lock()
                .active_scene_id()
                .map(|s| s.to_string());
            if active.as_deref() != Some(id.as_str()) {
                if let Err(err) = crate::commands::recall_scene_impl(
                    &handles.app,
                    &handles.engine,
                    &handles.show,
                    &handles.chasers,
                    &handles.movement,
                    &handles.scenes,
                    id,
                ) {
                    tracing::warn!(?err, "cue: scene recall failed");
                }
            }
            cue_blackout(handles, false);
        }
        ButtonAction::StartLoopGroup { id } => {
            crate::autopilot::note_manual_action(&action);
            let active = handles
                .loops
                .lock()
                .active_group_id()
                .map(|s| s.to_string());
            if active.as_deref() != Some(id.as_str()) {
                if let Err(err) = crate::commands::start_loop_group_impl(
                    &handles.app,
                    &handles.engine,
                    &handles.show,
                    &handles.chasers,
                    &handles.movement,
                    &handles.globals,
                    &handles.scenes,
                    &handles.loops,
                    id,
                ) {
                    tracing::warn!(?err, "cue: loop group failed");
                }
            }
            cue_blackout(handles, false);
        }
        _ => dispatch_action(action.clone(), true, true, handles),
    }
}

fn cue_blackout(handles: &GenericHandles, on: bool) {
    if handles.show.read().show.globals.blackout.active == on {
        return;
    }
    if let Err(err) =
        crate::commands::set_blackout_impl(&handles.app, &handles.show, &handles.globals, on)
    {
        tracing::warn!(?err, on, "cue: blackout failed");
    }
}

/// One beat for the blind hit: the follower's BPM (VirtualDJ's), else
/// the OS2L beat anchor, else the overall BPM (120 when nothing is known).
fn cue_beat_len(handles: &GenericHandles) -> Duration {
    let from_follower = handles
        .app
        .try_state::<CueFollower>()
        .and_then(|f| f.0.lock().bpm)
        .filter(|b| *b >= 20.0);
    if let Some(bpm) = from_follower {
        return Duration::from_secs_f32(60.0 / bpm);
    }
    if let Some(d) = crate::os2l::anchor_beat_len(&handles.app) {
        return d;
    }
    crate::midi::generic::beat_len(handles)
}

#[derive(Clone, Default)]
pub struct CueFollower(Arc<Mutex<CueFollowerInner>>);

impl CueFollower {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> CueFollowerStatus {
        let g = self.0.lock();
        CueFollowerStatus {
            running: g.running,
            playing: g.playing,
            file_path: g.file_path.clone(),
            title: g.title.clone(),
            beat_pos: g.beat_pos,
            pos_secs: g.pos_secs,
            bpm: g.bpm,
            last_error: g.last_error.clone(),
            fired_cue_ids: g.fired.clone(),
            signal: g.signal.as_str().to_string(),
            grid_source: g
                .grid_source
                .map(|s| s.as_str())
                .unwrap_or("none")
                .to_string(),
            assumed: g.assumed,
            http_endpoint: g.http_endpoint.clone(),
        }
    }
}

fn emit_status(handles: &GenericHandles, state: &CueFollower) {
    let _ = handles.app.emit(CUES_STATUS_EVENT, state.snapshot());
}

pub fn stop(state: &CueFollower) {
    let mut g = state.0.lock();
    if let Some(tx) = g.shutdown.take() {
        let _ = tx.send(true);
    }
    g.running = false;
    g.playing = false;
}

pub fn start(handles: GenericHandles, state: CueFollower, cfg: VdjConfig) {
    stop(&state);
    let (tx, rx) = tokio::sync::watch::channel(false);
    let generation = {
        let mut g = state.0.lock();
        g.shutdown = Some(tx);
        g.running = true;
        g.last_error = None;
        g.http_endpoint = format!("{}:{}", cfg.host, cfg.port);
        g.generation += 1;
        g.generation
    };
    // The playhead reader lives on the async runtime (it talks HTTP);
    // the clock that fires the cues is its own real-time thread, fed
    // through a channel, so a slow or hung VDJ answer, a save, a log
    // flush or a busy UI never delays a cue.
    let (reading_tx, reading_rx) = std::sync::mpsc::channel::<PollOutcome>();
    tauri::async_runtime::spawn(poller_task(
        cfg.clone(),
        state.clone(),
        reading_tx,
        rx.clone(),
    ));
    let state_for_error = state.clone();
    let spawned = std::thread::Builder::new()
        .name("dmx-cues".into())
        .spawn(move || clock_thread(handles, state, cfg, generation, reading_rx, rx));
    if let Err(err) = spawned {
        tracing::error!(target: "dmx::cues", ?err, "could not spawn the cue clock thread");
        stop(&state_for_error);
    }
}

/// The cue editor's open track. While VDJ's HTTP server is silent the
/// follower rides OS2L beats on this song (the editor follows; cues
/// don't fire on an assumption).
pub fn set_assumed_file(state: &CueFollower, path: Option<String>) {
    let mut g = state.0.lock();
    if g.assumed_file != path {
        tracing::info!(target: "dmx::cues", file = ?path, "editor track for OS2L following");
        g.assumed_file = path;
    }
}

/// Fetch VDJ's own beat grid for `file` from its database and keep it.
/// Blocking (reads the whole database): call from a worker, never from
/// the clock thread.
pub fn load_vdj_grid_into(state: &CueFollower, file: &str) {
    if state
        .0
        .lock()
        .grids
        .get(file)
        .is_some_and(|g| g.source == GridSource::Vdj)
    {
        return;
    }
    match super::poi_export::load_scan_grid(file) {
        Some((beat_secs, origin_secs)) => {
            tracing::info!(
                target: "dmx::cues",
                file = %basename(file), beat_secs, origin_secs,
                "beat grid from VirtualDJ's database"
            );
            state.0.lock().grids.insert(
                file.to_string(),
                FollowGrid {
                    origin_secs,
                    beat_secs,
                    source: GridSource::Vdj,
                },
            );
        }
        None => {
            tracing::info!(
                target: "dmx::cues",
                file = %basename(file),
                "no beat grid for this song in VirtualDJ's database"
            );
        }
    }
}

/// Beat grid for `file`: VDJ's own or HTTP-learned if we have one, else
/// the editor's estimate from the show.
fn resolve_grid(handles: &GenericHandles, state: &CueFollower, file: &str) -> Option<FollowGrid> {
    if let Some(g) = state.0.lock().grids.get(file) {
        return Some(*g);
    }
    let s = handles.show.read();
    let grid = s
        .show
        .track_cues
        .iter()
        .find(|t| t.file_path == file)
        .and_then(|t| t.grid.as_ref())?;
    (grid.bpm > 0.0).then_some(FollowGrid {
        origin_secs: grid.first_beat_secs,
        beat_secs: 60.0 / grid.bpm,
        source: GridSource::Estimate,
    })
}

type PollOutcome = Result<Option<Reading>, String>;

/// One HTTP reading of the active deck.
struct Reading {
    file: String,
    beat: f64,
    /// Elapsed track time in seconds (`get_time 'elapsed'`).
    secs: Option<f64>,
    bpm: f32,
    at: Instant,
}

/// Parse `get_beatpos` accepting the FULL grid range, negatives
/// included. VDJ reports negative beats before the grid origin (a
/// crescendo intro sits at e.g. -17.4), and the follower must keep
/// tracking there so the operator can mark cues from second zero. The
/// stricter shared parser (`parse_vdj_beatpos`) stays as-is for the
/// tempo poller, where a negative anchor would glitch chaser phase.
fn parse_beatpos_any(raw: &str) -> Option<f64> {
    let n: f64 = raw.trim().parse().ok()?;
    n.is_finite().then_some(n)
}

/// Parse VDJ's `get_time` reply. Observed formats across versions:
/// plain integer milliseconds ("83250"), float seconds ("83.25"), or
/// clock text ("1:23", "1:23.250"). Negative values (remain-style
/// replies) are rejected.
pub(crate) fn parse_vdj_time_secs(raw: &str) -> Option<f64> {
    let t = raw.trim();
    if t.is_empty() || t.starts_with('-') {
        return None;
    }
    if let Some((m, s)) = t.split_once(':') {
        let mins: f64 = m.trim().parse().ok()?;
        let secs: f64 = s.trim().parse().ok()?;
        let total = mins * 60.0 + secs;
        return total.is_finite().then_some(total);
    }
    let n: f64 = t.parse().ok()?;
    if !n.is_finite() || n < 0.0 {
        return None;
    }
    // Integer-looking values ≥1000 are almost certainly milliseconds
    // (a 16-minute track is <1000 s, and VDJ's ms replies have no dot).
    if !t.contains('.') && n >= 1000.0 {
        return Some(n / 1000.0);
    }
    Some(n)
}

async fn poll_deck(client: &reqwest::Client, cfg: &VdjConfig) -> Result<Option<Reading>, String> {
    // `deck master` first (see `decks_to_read`): the deck the room hears.
    let Some(decks) = super::decks_to_read(client, cfg).await? else {
        return Ok(None);
    };
    for deck in decks {
        if deck != "master" {
            let playing = fetch_script(client, cfg, &format!("deck {deck} play")).await?;
            if !parse_vdj_bool(&playing) {
                continue;
            }
        }
        let Some(bpm) =
            parse_vdj_bpm(&fetch_script(client, cfg, &format!("deck {deck} get_bpm")).await?)
        else {
            return Ok(None);
        };
        let Some(beat) = parse_beatpos_any(
            &fetch_script(client, cfg, &format!("deck {deck} get_beatpos")).await?,
        ) else {
            return Ok(None);
        };
        let at = Instant::now();
        // Track seconds: the primary firing coordinate for cues authored
        // in the editor (`pos_secs > 0`); legacy beat-only cues still fire
        // on the beat grid, so a missing reading degrades, not breaks.
        let secs = fetch_script(client, cfg, &format!("deck {deck} get_time 'elapsed'"))
            .await
            .ok()
            .as_deref()
            .and_then(parse_vdj_time_secs);
        let file = fetch_script(client, cfg, &format!("deck {deck} get_filepath")).await?;
        if file.is_empty() {
            return Ok(None);
        }
        return Ok(Some(Reading {
            file,
            beat,
            secs,
            bpm,
            at,
        }));
    }
    Ok(None)
}

fn basename(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// Fast-tick interval (interpolation + firing) and how many fast ticks
/// between HTTP polls (8 × 50 ms = 400 ms).
/// Clock tick of the cue thread: firing precision is one tick.
const FAST_TICK: Duration = Duration::from_millis(20);
/// Spacing between HTTP reads of the playing deck.
const POLL_EVERY: Duration = Duration::from_millis(400);
/// An HTTP anchor older than this no longer carries the position (the
/// poller answers every 400 ms: two misses and OS2L takes over).
const HTTP_STALE: Duration = Duration::from_millis(1500);
/// An OS2L beat older than this means VDJ stopped, or the link died.
const OS2L_STALE: Duration = Duration::from_millis(3000);
/// Without an HTTP answer for this long, which song is playing is no
/// longer VDJ's word: following goes on over OS2L on the editor's
/// track, firing waits for VDJ to answer again.
const CONFIRM_TTL: Duration = Duration::from_secs(20);
/// Status cadence while following over OS2L (HTTP mode: per reading).
const OS2L_STATUS_EVERY: Duration = Duration::from_millis(400);
/// Seek detector: if the HTTP reading disagrees with the interpolated
/// position by more than this many beats, treat it as a jump (scratch,
/// needle drop, pitch bend hell) and re-sync without firing anything
/// in between.
/// A jump of this many beats between the interpolated and the read
/// position is a seek (HTTP jitter is ~0.1 beat).
const SEEK_THRESHOLD_BEATS: f64 = 1.0;

/// Reads VirtualDJ's playing deck over HTTP and hands every reading (or
/// failure) to the clock thread. Requests never pile up: the next one
/// starts `POLL_EVERY` after the previous one *started*. A new song
/// also triggers a (blocking, off-thread) read of VDJ's beat grid.
async fn poller_task(
    cfg: VdjConfig,
    state: CueFollower,
    tx: std::sync::mpsc::Sender<PollOutcome>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let mut last_file: Option<String> = None;
    loop {
        let started = Instant::now();
        let reading = tokio::select! {
            _ = shutdown.changed() => break,
            r = poll_deck(&client, &cfg) => r,
        };
        if let Ok(Some(r)) = &reading {
            if last_file.as_deref() != Some(r.file.as_str()) {
                last_file = Some(r.file.clone());
                let st = state.clone();
                let file = r.file.clone();
                tauri::async_runtime::spawn_blocking(move || load_vdj_grid_into(&st, &file));
            }
        }
        if *shutdown.borrow() || tx.send(reading).is_err() {
            break;
        }
        let wait = POLL_EVERY.saturating_sub(started.elapsed());
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = tokio::time::sleep(wait) => {}
        }
    }
}

/// The cue clock: interpolates the playhead between readings and fires
/// whatever was crossed since the previous tick. Runs on its own
/// real-time thread — nothing here waits on the network, the disk or
/// the UI.
///
/// Two position sources: VDJ's HTTP readings (file + position, the
/// word of VDJ itself) and, while HTTP is silent, VDJ's beats over
/// OS2L on the song's own grid. Over OS2L the song is either the one
/// HTTP last confirmed (cues fire) or the editor's open track (the
/// editor follows, nothing fires).
fn clock_thread(
    handles: GenericHandles,
    state: CueFollower,
    cfg: VdjConfig,
    generation: u64,
    readings: std::sync::mpsc::Receiver<PollOutcome>,
    shutdown: tokio::sync::watch::Receiver<bool>,
) {
    use std::sync::mpsc::RecvTimeoutError;

    // Same band as the output thread: a cue landing late because the UI
    // or a save was hogging the CPU is a visible mistake on stage.
    if let Err(e) =
        thread_priority::set_current_thread_priority(thread_priority::ThreadPriority::Max)
    {
        tracing::warn!(target: "dmx::cues", error = ?e, "could not raise cue thread priority");
    }
    let endpoint = format!("{}:{}", cfg.host, cfg.port);
    tracing::info!(target: "dmx::cues", host = %cfg.host, port = cfg.port, "cue follower started");
    emit_status(&handles, &state);

    // Raw anchor from the last HTTP reading: (beat, bpm, secs, instant).
    let mut anchor: Option<(f64, f32, Option<f64>, Instant)> = None;
    // Interpolated position at the previous tick — the left edge of the
    // "fired if crossed" window.
    let mut prev_beat: Option<f64> = None;
    let mut prev_secs: Option<f64> = None;
    // The song VDJ confirmed over HTTP (`None` = not confirmed).
    let mut current_file: Option<String> = None;
    let mut last_http_ok: Option<Instant> = None;
    // Last HTTP outcome class, for transition logging: None = readings
    // flow, Some(text) = the error (or "idle" when VDJ reports nothing).
    let mut http_state: Option<String> = None;
    let mut signal = Signal::None;
    let mut last_raw_at: Option<Instant> = None;
    let mut last_status = Instant::now();
    let mut next_tick = Instant::now() + FAST_TICK;

    // `has_changed` is Ok(true) once `stop` fires, Err once the sender is gone.
    while let Ok(false) = shutdown.has_changed() {
        // Sleep until the tick, waking early for a fresh reading.
        let now = Instant::now();
        let reading = if next_tick > now {
            match readings.recv_timeout(next_tick - now) {
                Ok(r) => Some(r),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            readings.try_recv().ok()
        };
        if Instant::now() >= next_tick {
            next_tick += FAST_TICK;
            if next_tick < Instant::now() {
                // Fell behind (a long dispatch): resume from now, no burst.
                next_tick = Instant::now() + FAST_TICK;
            }
        }

        let mut emit = false;
        if let Some(reading) = reading {
            emit = true;
            match reading {
                Ok(Some(r)) => {
                    if http_state.is_some() {
                        tracing::info!(target: "dmx::cues", endpoint = %endpoint, "VDJ HTTP readings flowing");
                        http_state = None;
                    }
                    last_http_ok = Some(Instant::now());
                    let mut resync = false;
                    if current_file.as_deref() != Some(r.file.as_str()) {
                        tracing::info!(target: "dmx::cues", file = %r.file, "tracking new song");
                        current_file = Some(r.file.clone());
                        let mut g = state.0.lock();
                        g.file_path = Some(r.file.clone());
                        g.title = Some(basename(&r.file));
                        g.fired.clear();
                        resync = true;
                    }
                    // Learn secs↔beat for this song (exact at pitch 0);
                    // VDJ's own grid from the database wins when present.
                    if let Some(secs) = r.secs {
                        let learned = FollowGrid {
                            origin_secs: secs - r.beat * 60.0 / r.bpm.max(1.0) as f64,
                            beat_secs: 60.0 / r.bpm.max(1.0) as f64,
                            source: GridSource::Http,
                        };
                        let mut g = state.0.lock();
                        let slot = g.grids.entry(r.file.clone()).or_insert(learned);
                        if slot.source != GridSource::Vdj {
                            *slot = learned;
                        }
                    }
                    // No anchor = we just started, or the deck was stopped
                    // and is playing again — quite possibly from the top of
                    // the same song; or OS2L was carrying the position.
                    // Either way the crossing window restarts here.
                    if anchor.is_none() && !resync {
                        tracing::info!(
                            target: "dmx::cues",
                            at_secs = ?r.secs,
                            "playback (re)started; re-arming the cues ahead"
                        );
                        resync = true;
                    }
                    if let Some((b0, bpm0, _, t0)) = anchor {
                        let est = b0 + t0.elapsed().as_secs_f64() * bpm0.max(1.0) as f64 / 60.0;
                        if (est - r.beat).abs() > SEEK_THRESHOLD_BEATS {
                            tracing::info!(
                                target: "dmx::cues",
                                from = est, to = r.beat,
                                "seek detected; re-syncing"
                            );
                            resync = true;
                        }
                    }
                    if resync {
                        prev_beat = Some(r.beat);
                        prev_secs = r.secs;
                        rearm_ahead(&handles, &state, current_file.as_deref(), r.beat, r.secs);
                    }
                    {
                        let mut g = state.0.lock();
                        g.playing = true;
                        g.bpm = Some(r.bpm);
                        g.last_error = None;
                    }
                    anchor = Some((r.beat, r.bpm, r.secs, r.at));
                }
                Ok(None) => {
                    if http_state.as_deref() != Some("idle") {
                        tracing::info!(target: "dmx::cues", "VDJ answers but reports nothing playing");
                        http_state = Some("idle".into());
                    }
                    last_http_ok = Some(Instant::now());
                    state.0.lock().last_error = None;
                    anchor = None;
                }
                Err(e) => {
                    if http_state.as_deref() != Some(e.as_str()) {
                        tracing::warn!(
                            target: "dmx::cues",
                            endpoint = %endpoint, error = %e,
                            "VDJ HTTP (Network Control plugin) not answering; following OS2L beats meanwhile"
                        );
                        http_state = Some(e.clone());
                    }
                    state.0.lock().last_error = Some(e);
                    anchor = None;
                }
            }
        }

        // ---- Which song, on whose word ----------------------------------
        let now = Instant::now();
        let http_live =
            anchor.is_some() && last_http_ok.is_some_and(|t| now.duration_since(t) < HTTP_STALE);
        if !http_live
            && current_file.is_some()
            && last_http_ok.is_none_or(|t| now.duration_since(t) > CONFIRM_TTL)
        {
            tracing::info!(
                target: "dmx::cues",
                "no HTTP answer for 20 s: the playing song is no longer confirmed; cues wait for VDJ"
            );
            current_file = None;
            anchor = None;
        }
        let assumed_file = state.0.lock().assumed_file.clone();
        let (file, assumed) = match (&current_file, assumed_file) {
            (Some(f), _) => (Some(f.clone()), false),
            (None, Some(a)) => (Some(a), true),
            (None, None) => (None, false),
        };

        // ---- Position: HTTP while it answers, else OS2L on VDJ's grid ----
        let raw = crate::os2l::raw_beat(&handles.app)
            .filter(|r| now.saturating_duration_since(r.at) < OS2L_STALE);
        // (beat, secs, source, grid)
        let mut pos: Option<(f64, Option<f64>, Signal, Option<FollowGrid>)> = None;
        if http_live {
            if let Some((b0, bpm0, s0, t0)) = anchor {
                let elapsed = t0.elapsed().as_secs_f64();
                let beat = b0 + elapsed * bpm0.max(1.0) as f64 / 60.0;
                pos = Some((beat, s0.map(|s| s + elapsed), Signal::Http, None));
            }
        } else if let (Some(raw), Some(f)) = (raw, file.as_deref()) {
            if let Some(grid) = resolve_grid(&handles, &state, f) {
                let beat = raw.beat_at(now);
                pos = Some((beat, Some(grid.secs_at(beat)), Signal::Os2l, Some(grid)));
            }
        }
        let new_signal = pos.map(|p| p.2).unwrap_or(Signal::None);
        if new_signal != signal {
            tracing::info!(
                target: "dmx::cues",
                from = signal.as_str(), to = new_signal.as_str(),
                file = ?file.as_deref().map(basename), assumed,
                grid = ?pos.and_then(|p| p.3).map(|g| g.source.as_str()),
                "position source changed"
            );
            // Entering a source: the crossing window restarts at the new
            // position, nothing in between fires.
            prev_beat = pos.map(|p| p.0);
            prev_secs = pos.and_then(|p| p.1);
            if new_signal == Signal::Os2l {
                if let Some((beat, secs, _, _)) = pos {
                    rearm_ahead(&handles, &state, file.as_deref(), beat, secs);
                }
            }
            signal = new_signal;
            emit = true;
        }
        // Seeks over OS2L: VDJ's own flag on a fresh beat, or a jump
        // against the previous tick.
        if signal == Signal::Os2l {
            if let (Some(raw), Some((beat, secs, _, _))) = (raw, pos) {
                let fresh = last_raw_at != Some(raw.at);
                last_raw_at = Some(raw.at);
                let jump = prev_beat.is_some_and(|p| beat < p - 0.5 || beat > p + 4.0);
                if (fresh && raw.change) || jump {
                    tracing::info!(
                        target: "dmx::cues",
                        from = ?prev_beat, to = beat, vdj_flag = fresh && raw.change,
                        "seek over OS2L; re-syncing"
                    );
                    // A big rewind without HTTP could just as well be the
                    // next song: VDJ's word is needed again before firing.
                    if current_file.is_some() && prev_beat.is_some_and(|p| beat < p - 16.0) {
                        tracing::info!(
                            target: "dmx::cues",
                            "large jump back over OS2L without HTTP: song no longer confirmed"
                        );
                        current_file = None;
                    }
                    prev_beat = Some(beat);
                    prev_secs = secs;
                    rearm_ahead(&handles, &state, file.as_deref(), beat, secs);
                }
            }
        }

        // ---- Fire what was crossed ---------------------------------------
        if let Some((now_beat, now_secs, src, _)) = pos {
            let can_fire = src == Signal::Http || (src == Signal::Os2l && current_file.is_some());
            let from = prev_beat.unwrap_or(now_beat);
            let from_secs = prev_secs.or(now_secs);
            let advanced =
                now_beat > from || matches!((from_secs, now_secs), (Some(f), Some(n)) if n > f);
            if advanced && can_fire {
                let cues = cues_for(&handles, file.as_deref());
                if !cues.is_empty() {
                    let bpm_now = anchor.map(|a| a.1).or(raw.map(|r| r.bpm)).unwrap_or(120.0);
                    // Seconds are the truth for editor-authored cues;
                    // beat-only legacy cues keep the grid window.
                    let secs_of = |c: &TrackCue| {
                        if c.pos_secs > 0.0 {
                            c.pos_secs
                        } else {
                            c.beat * 60.0 / bpm_now.max(1.0) as f64
                        }
                    };
                    // Everything crossed since the previous tick, in
                    // timeline order: one tick can cross two cues placed
                    // close together and the later one must land last,
                    // whatever order the list stores them in.
                    let mut crossed: Vec<&TrackCue> = cues
                        .iter()
                        .filter(|c| {
                            if c.pos_secs > 0.0 {
                                matches!(
                                    (from_secs, now_secs),
                                    (Some(f), Some(n)) if c.pos_secs > f && c.pos_secs <= n
                                )
                            } else {
                                c.beat > from && c.beat <= now_beat
                            }
                        })
                        .collect();
                    crossed.sort_by(|a, b| {
                        secs_of(a)
                            .partial_cmp(&secs_of(b))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    for c in crossed {
                        // Dedup against VDJ's synced POI for the same cue
                        // (and against jittery re-crossings) lives in
                        // `fire_cue`.
                        fire_cue(&handles, &c.id, &c.name, &c.action, "follower", true);
                    }
                }
            }
            prev_beat = Some(now_beat);
            prev_secs = now_secs;
        }

        // ---- Status --------------------------------------------------------
        {
            let mut g = state.0.lock();
            g.playing = pos.is_some();
            g.signal = signal;
            g.assumed = assumed && signal == Signal::Os2l;
            g.grid_source = pos.and_then(|p| p.3).map(|gr| gr.source);
            if let Some((beat, secs, src, _)) = pos {
                g.beat_pos = Some(beat);
                g.pos_secs = secs;
                if src == Signal::Os2l {
                    if let Some(r) = raw {
                        g.bpm = Some(r.bpm);
                    }
                    if g.file_path.as_deref() != file.as_deref() {
                        g.file_path = file.clone();
                        g.title = file.as_deref().map(basename);
                    }
                }
            }
        }
        if signal == Signal::Os2l && last_status.elapsed() >= OS2L_STATUS_EVERY {
            emit = true;
        }
        if emit {
            last_status = Instant::now();
            emit_status(&handles, &state);
        }
    }

    {
        let mut g = state.0.lock();
        if g.generation == generation {
            g.running = false;
            g.playing = false;
            g.signal = Signal::None;
        }
    }
    emit_status(&handles, &state);
    tracing::info!(target: "dmx::cues", "cue follower stopped");
}

/// Re-arm every cue of `file` that lies ahead of (beat, secs); the ones
/// behind stay fired (rewind → the passage's lights replay, a skip
/// forward does not machine-gun what was jumped over).
fn rearm_ahead(
    handles: &GenericHandles,
    state: &CueFollower,
    file: Option<&str>,
    beat: f64,
    secs: Option<f64>,
) {
    let cues = cues_for(handles, file);
    let mut g = state.0.lock();
    g.fired.retain(|id| {
        cues.iter()
            .find(|c| &c.id == id)
            .map(|c| {
                if c.pos_secs > 0.0 {
                    secs.map(|s| c.pos_secs <= s).unwrap_or(true)
                } else {
                    c.beat <= beat
                }
            })
            .unwrap_or(false)
    });
}

fn cues_for(handles: &GenericHandles, file: Option<&str>) -> Vec<TrackCue> {
    let Some(file) = file else {
        return Vec::new();
    };
    handles
        .show
        .read()
        .show
        .track_cues
        .iter()
        .find(|s| s.file_path == file)
        .map(|s| s.cues.clone())
        .unwrap_or_default()
}
