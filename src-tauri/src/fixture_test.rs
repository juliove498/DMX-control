//! Pre-show fixture check ("Test" on Stage). Modelled on grandMA's
//! Highlight / Next and Eos' flash keys: a known level on every patched
//! fixture — or on one fixture at a time, in patch order — laid on top of
//! whatever the show is doing, without touching scenes, chasers, the
//! programmer or Modo Mágico. It sits below blind / blackout / master, the
//! way a highlight sits under the grand master, and switches itself off
//! after a while so it can't survive into the show by mistake.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

use crate::autopilot::profile::{self, FixtureProfile};
use crate::engine::{empty_overlay, ChannelOverlay, DMX_CHANNELS};
use crate::show::state::ShowState;

pub const STATUS_EVENT: &str = "fixture_test:status";
/// "All" mode: every fixture flashes once a second, lit for the first part.
const FLASH_PERIOD_MS: u128 = 1000;
const FLASH_ON_MS: u128 = 350;
/// "Step" mode: how long each fixture stays lit before the next one.
const STEP_DWELL_MS: u128 = 800;
/// Safety: a check left on by mistake switches itself off.
const AUTO_OFF: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum FixtureTestMode {
    /// Everything flashes together (did every light get the signal?).
    #[default]
    All,
    /// One fixture at a time in patch order (which light is which?).
    Step,
}

#[derive(Debug, Clone, Serialize, TS, Default)]
#[ts(export, export_to = "../bindings/")]
pub struct FixtureTestStatus {
    pub enabled: bool,
    pub mode: FixtureTestMode,
    /// Test intensity 0..1.
    pub level: f32,
    /// Fixtures taking part (the selection, or the whole patch).
    pub count: u32,
    /// Step mode: the fixture lit right now.
    pub current_id: Option<String>,
    pub current_label: Option<String>,
    pub remaining_secs: u32,
}

struct Inner {
    enabled: bool,
    mode: FixtureTestMode,
    level: f32,
    /// Selection filter; empty = every patched fixture.
    ids: Vec<String>,
    profiles: Vec<FixtureProfile>,
    started: Option<Instant>,
    app: Option<AppHandle>,
    last_emit: Option<Instant>,
    last_current: Option<String>,
    /// Swop mask: while `Some`, every in-use fixture NOT in the set is
    /// held dark (the held button's content keeps the room).
    exclusive: Option<HashSet<String>>,
}

fn state() -> &'static Mutex<Inner> {
    static S: OnceLock<Mutex<Inner>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(Inner {
            enabled: false,
            mode: FixtureTestMode::All,
            level: 0.3,
            ids: Vec::new(),
            profiles: Vec::new(),
            started: None,
            app: None,
            last_emit: None,
            last_current: None,
            exclusive: None,
        })
    })
}

/// Swop: `Some(owned)` darkens everything but `owned`; `None` releases.
pub fn set_exclusive(owned: Option<HashSet<String>>) {
    state().lock().exclusive = owned;
}

pub fn install(app: AppHandle) {
    state().lock().app = Some(app);
}

/// Re-resolve the channel map of every patched fixture (patch edits,
/// show swaps, mirror replication).
pub fn sync_from_show(show: &ShowState) {
    let (fixtures, library) = {
        let s = show.read();
        (s.show.fixtures.clone(), s.library.clone())
    };
    let all: Vec<String> = fixtures.iter().map(|f| f.id.clone()).collect();
    let profiles = profile::resolve(&fixtures, &library, &all);
    state().lock().profiles = profiles;
}

fn active(s: &Inner) -> Vec<&FixtureProfile> {
    if s.ids.is_empty() {
        s.profiles.iter().collect()
    } else {
        s.profiles
            .iter()
            .filter(|p| s.ids.contains(&p.id))
            .collect()
    }
}

fn current_index(s: &Inner, now: Instant, n: usize) -> Option<usize> {
    if s.mode != FixtureTestMode::Step || n == 0 {
        return None;
    }
    let elapsed = now.saturating_duration_since(s.started.unwrap_or(now));
    Some(((elapsed.as_millis() / STEP_DWELL_MS) as usize) % n)
}

fn snapshot(s: &Inner, now: Instant) -> FixtureTestStatus {
    let act = active(s);
    let cur = current_index(s, now, act.len()).and_then(|i| act.get(i).copied());
    let remaining = s
        .started
        .map(|t| {
            AUTO_OFF
                .saturating_sub(now.saturating_duration_since(t))
                .as_secs() as u32
        })
        .unwrap_or(0);
    FixtureTestStatus {
        enabled: s.enabled,
        mode: s.mode,
        level: s.level,
        count: act.len() as u32,
        current_id: cur.map(|p| p.id.clone()),
        current_label: cur.map(|p| p.label.clone()),
        remaining_secs: if s.enabled { remaining } else { 0 },
    }
}

fn emit(s: &Inner, st: &FixtureTestStatus) {
    if let Some(app) = &s.app {
        let _ = app.emit(STATUS_EVENT, st.clone());
    }
}

/// Switch the check on/off, pick the mode and level, restrict to `ids`
/// (empty = whole patch).
pub fn set(
    enabled: bool,
    mode: FixtureTestMode,
    level: f32,
    ids: Vec<String>,
) -> FixtureTestStatus {
    let mut s = state().lock();
    let now = Instant::now();
    if enabled && !s.enabled {
        s.started = Some(now);
        tracing::info!(target: "dmx::test", ?mode, level, "fixture check on");
    }
    if !enabled && s.enabled {
        s.started = None;
        s.last_current = None;
        tracing::info!(target: "dmx::test", "fixture check off");
    }
    s.enabled = enabled;
    s.mode = mode;
    s.level = level.clamp(0.0, 1.0);
    s.ids = ids;
    let st = snapshot(&s, now);
    emit(&s, &st);
    st
}

pub fn status() -> FixtureTestStatus {
    snapshot(&state().lock(), Instant::now())
}

fn put(out: &mut HashMap<u16, ChannelOverlay>, universe: u16, ch: Option<usize>, v: u8) {
    if let Some(ch) = ch {
        if ch < DMX_CHANNELS {
            out.entry(universe).or_insert_with(empty_overlay)[ch] = Some(v);
        }
    }
}

/// Highlight values, grandMA style: open white at the test level, shutter
/// open, additive emitters quiet. Pan/tilt and strobe are left alone.
fn paint(out: &mut HashMap<u16, ChannelOverlay>, p: &FixtureProfile, lit: bool, value: u8) {
    let u = p.universe;
    if p.dimmer.is_some() {
        put(out, u, p.dimmer, if lit { value } else { 0 });
        let w = if lit { 255 } else { 0 };
        put(out, u, p.red, w);
        put(out, u, p.green, w);
        put(out, u, p.blue, w);
        // The white emitter is not always under the dimmer (the Mekka RGB
        // Led1000w strobes drive their white bank straight from this
        // channel), so it gets the test level itself, never full.
        put(out, u, p.white, if lit { value } else { 0 });
    } else {
        let v = if lit { value } else { 0 };
        put(out, u, p.red, v);
        put(out, u, p.green, v);
        put(out, u, p.blue, v);
        put(out, u, p.white, v);
    }
    put(out, u, p.amber, 0);
    put(out, u, p.uv, 0);
    if p.shutter.is_some() {
        put(out, u, p.shutter, 255);
    }
}

/// One output frame of the check; empty when off.
pub fn tick(now: Instant) -> HashMap<u16, ChannelOverlay> {
    let mut s = state().lock();
    if !s.enabled {
        // Swop mask (independent of the check).
        if let Some(owned) = &s.exclusive {
            let mut out: HashMap<u16, ChannelOverlay> = HashMap::new();
            for p in s.profiles.iter().filter(|p| !owned.contains(&p.id)) {
                paint(&mut out, p, false, 0);
            }
            return out;
        }
        return HashMap::new();
    }
    let started = s.started.unwrap_or(now);
    if now.saturating_duration_since(started) >= AUTO_OFF {
        s.enabled = false;
        s.started = None;
        s.last_current = None;
        tracing::info!(target: "dmx::test", "fixture check auto-off");
        let st = snapshot(&s, now);
        emit(&s, &st);
        return HashMap::new();
    }
    let value = (s.level * 255.0).round() as u8;
    let elapsed = now.saturating_duration_since(started);
    let flash_on = (elapsed.as_millis() % FLASH_PERIOD_MS) < FLASH_ON_MS;
    let mut out: HashMap<u16, ChannelOverlay> = HashMap::new();
    let cur_id = {
        let act = active(&s);
        let current = current_index(&s, now, act.len());
        for (i, p) in act.iter().enumerate() {
            let lit = match s.mode {
                FixtureTestMode::All => flash_on,
                FixtureTestMode::Step => current == Some(i),
            };
            paint(&mut out, p, lit, value);
        }
        current.and_then(|i| act.get(i).map(|p| p.id.clone()))
    };
    let changed = cur_id != s.last_current;
    let due = s
        .last_emit
        .map(|t| now.saturating_duration_since(t) >= Duration::from_secs(1))
        .unwrap_or(true);
    if changed || due {
        s.last_current = cur_id;
        s.last_emit = Some(now);
        let st = snapshot(&s, now);
        emit(&s, &st);
    }
    out
}

#[tauri::command]
pub fn fixture_test_set(
    enabled: bool,
    mode: FixtureTestMode,
    level: f32,
    ids: Vec<String>,
) -> FixtureTestStatus {
    set(enabled, mode, level, ids)
}

#[tauri::command]
pub fn fixture_test_status() -> FixtureTestStatus {
    status()
}
