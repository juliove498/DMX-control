//! Modo Mágico — the autopilot. Improvises the rig from the music so the
//! floor stays alive while the DJ can't be on the lighting console.
//!
//! Inputs it listens to (all already flowing through the app):
//! - the beat grid (OS2L beats, the VDJ poller, or the mic BPM counter),
//!   for bars and phrases;
//! - an energy level (VirtualDJ's VU meter, the mic level, or a manual
//!   knob), for how bold to be.
//!
//! On every cadence boundary (4/8/16/32 beats) a *director* decides, with
//! memory, whether to invent a new look, return to the previous one or
//! hold — bigger changes on phrase boundaries. Looks come from two
//! sources: a procedural generator (`generator.rs`) and the show's own
//! vocabulary (scenes and snapshots tagged by energy). Looks are written
//! into the effects overlay each frame and never persisted — only the
//! configuration lives in the show file.
//!
//! Etiquette: it yields (ducks) for a few seconds whenever the operator
//! fires something by hand, respects master/blackout/blind by living in
//! the effects layer, keeps moving heads inside per-venue zones, and only
//! strobes when allowed and the energy is high.

pub mod generator;
pub mod profile;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use crate::engine::beatgrid::BeatAnchor;
use crate::engine::ChannelOverlay;
use crate::midi::generic::{dispatch_action, GenericHandles};
use crate::show::button_bindings::ButtonAction;
use crate::show::fixture::{FixtureDefinition, FixtureInstance};
use crate::show::state::ShowState;

use generator::Look;
use profile::{FixtureProfile, RigFixture};

pub const STATUS_EVENT: &str = "autopilot:status";
const PHRASE_BEATS: f64 = 32.0;
const ANCHOR_STALE: Duration = Duration::from_secs(6);
const ENERGY_STALE: Duration = Duration::from_secs(4);
/// Without new level samples we hold the last smoothed energy this long
/// before falling back to the manual slider — a flaky link must not make
/// the intensity jump between "live" and "manual" every few seconds.
const ENERGY_HOLD: Duration = Duration::from_secs(30);
const HISTORY_CAP: usize = 4;
/// Seconds to fade what we own to black once the music stops, and to
/// come back when it returns.
const FADE_OUT_SECS: f32 = 1.5;
const FADE_IN_SECS: f32 = 0.8;
/// A deck that keeps reporting "play" with the meter flat (the track ran
/// out, a paused deck on some builds) counts as silence after this long.
const SILENCE_LEVEL: f32 = 0.02;
const SILENCE_SECS: f32 = 2.0;
/// Evidence windows for "is music playing". The HTTP link to VirtualDJ can
/// answer one request and hang on the next, so silence is never inferred
/// from a single gap: it takes a sustained explicit "nothing plays", or a
/// long stretch with no contact at all.
/// A "deck is playing" answer keeps us going for this long.
const POLL_GRACE: Duration = Duration::from_secs(6);
/// VDJ must say "nothing plays" continuously for this long.
const STOP_CONFIRM: Duration = Duration::from_secs(3);
/// No answer of any kind for this long: VDJ is gone, go dark.
const CONTACT_LOST: Duration = Duration::from_secs(12);
/// OS2L-only rigs: no beat event for this long means the music stopped.
const OS2L_STOP: Duration = Duration::from_secs(4);
/// A silence shorter than this keeps the current look when the music is back.
const KEEP_LOOK_SECS: f32 = 8.0;

// ---- Configuration (persisted in the show) ---------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum PaletteStyle {
    #[default]
    Auto,
    Warm,
    Cold,
    Rainbow,
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum EnergySource {
    /// VirtualDJ's VU meter over the HTTP poller. No longer offered in the
    /// UI: the link stalls on this rig and the energy went wrong with it.
    /// Shows that still carry it load as `Manual`.
    VdjVu,
    #[default]
    Manual,
    Mic,
    /// VirtualDJ's per-beat strength over OS2L: VDJ's own analysis of how
    /// hard each beat hits (0 in breaks, ~0.8 with the kick in). Pushed per
    /// beat, independent of the room and of the volume fader.
    Os2l,
}

/// Loading migration: `vdj_vu` becomes `manual`.
fn deser_energy_source<'de, D: serde::Deserializer<'de>>(d: D) -> Result<EnergySource, D::Error> {
    let s = EnergySource::deserialize(d)?;
    Ok(match s {
        EnergySource::VdjVu => EnergySource::Manual,
        other => other,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
#[serde(rename_all = "snake_case")]
pub enum EnergyBand {
    Low,
    Mid,
    High,
}

impl EnergyBand {
    pub fn of(energy: f32) -> Self {
        if energy < 0.35 {
            EnergyBand::Low
        } else if energy < 0.7 {
            EnergyBand::Mid
        } else {
            EnergyBand::High
        }
    }
    fn neighbour(self, other: Self) -> bool {
        matches!(
            (self, other),
            (EnergyBand::Low, EnergyBand::Mid)
                | (EnergyBand::Mid, EnergyBand::Low)
                | (EnergyBand::Mid, EnergyBand::High)
                | (EnergyBand::High, EnergyBand::Mid)
        )
    }
    fn label(self) -> &'static str {
        match self {
            EnergyBand::Low => "low",
            EnergyBand::Mid => "mid",
            EnergyBand::High => "high",
        }
    }
}

/// A look of the operator's own (scene or snapshot) the autopilot may use.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct VocabEntry {
    pub action: ButtonAction,
    pub energy: EnergyBand,
    #[serde(default = "default_weight")]
    pub weight: f32,
}

fn default_weight() -> f32 {
    1.0
}

/// Allowed pan/tilt window for one moving head, as fractions (0..1) of the
/// fixture's declared physical range. Per venue: the balcony is not the
/// bride's table.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct HeadZone {
    pub fixture_id: String,
    pub pan_min: f32,
    pub pan_max: f32,
    pub tilt_min: f32,
    pub tilt_max: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../bindings/")]
pub struct AutopilotConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub fixture_ids: Vec<String>,
    #[serde(default = "default_cadence")]
    pub cadence_beats: u32,
    /// 0 = sober (repeats and holds a lot) … 1 = surprise me.
    #[serde(default = "default_variety")]
    pub variety: f32,
    #[serde(default)]
    pub palette: PaletteStyle,
    #[serde(default, deserialize_with = "deser_energy_source")]
    pub energy_source: EnergySource,
    #[serde(default = "default_manual_energy")]
    pub manual_energy: f32,
    /// Multiplier on the live level (VU / mic) before it becomes energy —
    /// lets a setup whose meter sits low still reach "party".
    #[serde(default = "default_gain")]
    pub energy_gain: f32,
    #[serde(default)]
    pub allow_strobe: bool,
    #[serde(default = "default_true")]
    pub allow_movement: bool,
    /// Occasional one-beat blackout "drops" on phrase boundaries at high energy.
    #[serde(default)]
    pub allow_drops: bool,
    /// Fade to black when VirtualDJ reports no music. Off by default: it
    /// needs a reliable VDJ link, and a flaky one produced long blackouts
    /// with the track at full tilt.
    #[serde(default)]
    pub stop_on_silence: bool,
    /// Latency calibration in milliseconds: positive delays the lights
    /// (they ran ahead of the sound), negative anticipates (they lagged).
    #[serde(default)]
    pub beat_offset_ms: f32,
    /// Minimum intensity (fraction) the generator never goes below.
    #[serde(default = "default_floor")]
    pub floor: f32,
    /// Maximum intensity (fraction) the generator never exceeds — "100 %
    /// is a lot" in a small room. Everything between floor and max keeps
    /// its dynamics, rescaled.
    #[serde(default = "default_max_level")]
    pub max_level: f32,
    /// How long to stay out of the way after the operator fires something.
    #[serde(default = "default_ducking")]
    pub ducking_secs: f32,
    #[serde(default = "default_true")]
    pub use_vocabulary: bool,
    #[serde(default = "default_true")]
    pub use_generator: bool,
    #[serde(default)]
    pub vocabulary: Vec<VocabEntry>,
    #[serde(default)]
    pub head_zones: Vec<HeadZone>,
}

fn default_cadence() -> u32 {
    8
}
fn default_variety() -> f32 {
    0.5
}
fn default_manual_energy() -> f32 {
    0.6
}
fn default_gain() -> f32 {
    1.0
}
fn default_true() -> bool {
    true
}
fn default_floor() -> f32 {
    0.08
}
fn default_max_level() -> f32 {
    1.0
}
fn default_ducking() -> f32 {
    8.0
}

impl Default for AutopilotConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            fixture_ids: Vec::new(),
            cadence_beats: default_cadence(),
            variety: default_variety(),
            palette: PaletteStyle::Auto,
            energy_source: EnergySource::Manual,
            manual_energy: default_manual_energy(),
            energy_gain: default_gain(),
            allow_strobe: false,
            allow_movement: true,
            allow_drops: false,
            stop_on_silence: false,
            beat_offset_ms: 0.0,
            floor: default_floor(),
            max_level: default_max_level(),
            ducking_secs: default_ducking(),
            use_vocabulary: true,
            use_generator: true,
            vocabulary: Vec::new(),
            head_zones: Vec::new(),
        }
    }
}

// ---- Status (for the UI) ------------------------------------------------------

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct LookInfo {
    pub name: String,
    pub palette: Vec<String>,
    /// Resolved palette family of this look (Auto picks one).
    pub style: PaletteStyle,
    pub color_mode: generator::AutoColorMode,
    pub dim_program: generator::AutoDimProgram,
    pub mover_program: generator::AutoMoverProgram,
    pub strobe: bool,
    /// Name of the scene/snapshot in play, when this look came from the vocabulary.
    pub vocab_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct AutopilotStatus {
    pub enabled: bool,
    /// Enabled AND has participants AND not ducked.
    pub active: bool,
    /// "os2l" | "vdj" | "mic" | "free"
    pub clock_source: String,
    pub bpm: f32,
    pub beat: f64,
    /// 1-based beat inside the bar and bar inside the phrase.
    pub beat_in_bar: u32,
    pub bar_in_phrase: u32,
    pub phrase: u32,
    pub energy: f32,
    /// "low" | "mid" | "high"
    pub energy_band: String,
    /// "vdj_vu" | "manual" | "mic" | "none" — what the energy currently comes from.
    pub energy_from: String,
    /// "live" (following the music), "silent" (VDJ is up but nothing plays:
    /// faded to black) or "fallback" (VDJ is off in Config: manual energy).
    pub signal: String,
    pub silent: bool,
    /// Diagnostic line: what each VDJ link is reporting right now.
    pub signal_detail: String,
    /// Calibration mode: a white pulse on every beat, heads parked.
    pub calibrating: bool,
    /// The operator is holding the current look (no director changes).
    pub hold: bool,
    /// A manual strobe burst is running.
    pub burst: bool,
    /// A one-beat drop is running.
    pub drop: bool,
    pub look: Option<LookInfo>,
    pub history: Vec<LookInfo>,
    pub next_change_in: f32,
    pub ducked_for: f32,
    pub participants: u32,
    pub movers: u32,
}

// ---- Runtime ------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockSource {
    Os2l,
    Vdj,
    Mic,
}

impl ClockSource {
    fn label(self) -> &'static str {
        match self {
            ClockSource::Os2l => "os2l",
            ClockSource::Vdj => "vdj",
            ClockSource::Mic => "mic",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Nudge {
    Next,
    Back,
    Hold,
}

#[derive(Clone)]
struct VocabActive {
    action: ButtonAction,
    name: String,
    /// Fixture ids the scene owns (None = a snapshot: owns everything).
    covered: Option<HashSet<String>>,
}

/// What the VirtualDJ links say about the music.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Music {
    Playing,
    Stopped,
    /// No link is up, so nobody can tell.
    Unknown,
}

/// What the chosen energy source is reporting.
#[derive(Debug, Clone, PartialEq)]
enum Signal {
    /// Following a live source (or the manual slider by choice).
    Live(f32, EnergySource),
    /// The source can't report (VDJ off in Config, mic idle): manual energy.
    Fallback(f32),
    /// VDJ is up but nothing plays: fade to black and wait.
    Silent,
}

impl Signal {
    fn energy(&self) -> f32 {
        match self {
            Signal::Live(e, _) | Signal::Fallback(e) => *e,
            Signal::Silent => 0.0,
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Signal::Live(..) => "live",
            Signal::Fallback(_) => "fallback",
            Signal::Silent => "silent",
        }
    }
    fn source_label(&self) -> &'static str {
        match self {
            Signal::Live(_, EnergySource::VdjVu) => "vdj_vu",
            Signal::Live(_, EnergySource::Mic) => "mic",
            Signal::Live(_, EnergySource::Os2l) => "os2l",
            Signal::Live(_, EnergySource::Manual) | Signal::Fallback(_) => "manual",
            Signal::Silent => "none",
        }
    }
}

struct Runtime {
    cfg: AutopilotConfig,
    profiles: Vec<FixtureProfile>,
    zones: HashMap<String, HeadZone>,
    anchor: Option<(BeatAnchor, ClockSource)>,
    free_beat: f64,
    last_tick: Option<Instant>,
    vu: Option<(f32, Instant)>,
    /// Slow-decaying peak of the raw VU, to normalise it (VDJ's meter
    /// rarely reaches 1.0 even at full tilt).
    vu_peak: f32,
    /// Last time the VDJ poller saw a deck playing (with or without a VU verb).
    poller_last_playing: Option<Instant>,
    /// Since when VDJ has explicitly answered "no deck is playing".
    poller_stopped_since: Option<Instant>,
    /// Last answer of any kind from VDJ's HTTP interface.
    poller_last_contact: Option<Instant>,
    /// Since when the VU has sat under `SILENCE_LEVEL`.
    quiet_since: Option<Instant>,
    mic: Option<(f32, Instant)>,
    smooth: f32,
    /// VDJ HTTP poller running (deck play state + VU arrive from it).
    poller_up: bool,
    /// VirtualDJ connected over OS2L (beat events arrive from it).
    os2l_up: bool,
    os2l_beat_at: Option<Instant>,
    /// Slow-decaying peak of the OS2L beat strength, to normalise it.
    strength_peak: f32,
    /// Master fade on our overlay: 0 while the music is gone.
    gain: f32,
    silent_since: Option<Instant>,
    /// Beat frozen when the signal dropped, so the heads hold still in the dark.
    frozen_beat: Option<f64>,
    /// Latency calibration mode (not persisted).
    calibrating: bool,
    /// A Swop button is held: step aside without losing the clock.
    muted: bool,
    /// Operator holds the current look: the director neither changes nor
    /// mutates it until released.
    hold: bool,
    /// Manual strobe burst: held (Flash pad) or a beat window.
    burst_held: bool,
    burst_from: Option<f64>,
    burst_until: Option<f64>,
    /// Beat the current drop starts at (the director's own drops are
    /// immediate; a manual drop waits for the next beat).
    drop_from: Option<f64>,
    last_frame: Option<Instant>,
    rng: StdRng,
    look: Option<Look>,
    /// The look we are crossfading out of: (look, fade start beat, fade length in beats).
    prev_look: Option<(Look, f64, f64)>,
    /// Palette family that sticks for a few looks (with "Auto"), and how many are left.
    theme: Option<(PaletteStyle, u32)>,
    history: VecDeque<Look>,
    next_change: f64,
    last_phrase: i64,
    drop_until: Option<f64>,
    /// Last bar in which a micro-mutation was considered.
    last_bar: i64,
    ducked_until: Option<Instant>,
    vocab: Option<VocabActive>,
    usage: HashMap<String, u32>,
    pending_nudge: Option<Nudge>,
    handles: Option<GenericHandles>,
    tx: Option<crossbeam_channel::Sender<ButtonAction>>,
    last_status: Option<Instant>,
}

impl Runtime {
    fn new() -> Self {
        Self {
            cfg: AutopilotConfig::default(),
            profiles: Vec::new(),
            zones: HashMap::new(),
            anchor: None,
            free_beat: 0.0,
            last_tick: None,
            vu: None,
            vu_peak: 0.5,
            poller_last_playing: None,
            poller_stopped_since: None,
            poller_last_contact: None,
            quiet_since: None,
            mic: None,
            smooth: 0.0,
            poller_up: false,
            os2l_up: false,
            os2l_beat_at: None,
            strength_peak: 0.5,
            gain: 0.0,
            silent_since: None,
            frozen_beat: None,
            calibrating: false,
            muted: false,
            hold: false,
            burst_held: false,
            burst_from: None,
            burst_until: None,
            drop_from: None,
            last_frame: None,
            rng: StdRng::from_entropy(),
            look: None,
            prev_look: None,
            theme: None,
            history: VecDeque::new(),
            next_change: 0.0,
            last_phrase: i64::MIN,
            drop_until: None,
            last_bar: i64::MIN,
            ducked_until: None,
            vocab: None,
            usage: HashMap::new(),
            pending_nudge: None,
            handles: None,
            tx: None,
            last_status: None,
        }
    }

    /// Is music playing, according to whichever VirtualDJ link is up? The
    /// HTTP poller reports deck play state; OS2L sends a beat per beat.
    fn music_state(&self, now: Instant) -> Music {
        let since = |t: Option<Instant>| t.map(|t| now.saturating_duration_since(t));
        if !self.poller_up && !self.os2l_up {
            return Music::Unknown;
        }
        // Positive evidence wins: a recent "playing" answer or a recent beat.
        let poller_playing = self.poller_up
            && since(self.poller_last_playing)
                .map(|d| d < POLL_GRACE)
                .unwrap_or(false);
        let beats = self.os2l_up
            && since(self.os2l_beat_at)
                .map(|d| d < OS2L_STOP)
                .unwrap_or(false);
        if poller_playing || beats {
            return Music::Playing;
        }
        // Negative evidence has to be sustained; transient errors are not.
        let poller_stopped = self.poller_up
            && since(self.poller_stopped_since)
                .map(|d| d >= STOP_CONFIRM)
                .unwrap_or(false);
        let poller_lost = self.poller_up
            && since(self.poller_last_contact)
                .map(|d| d >= CONTACT_LOST)
                .unwrap_or(true);
        let os2l_silent = self.os2l_up && !self.poller_up;
        if poller_stopped || poller_lost || os2l_silent {
            Music::Stopped
        } else {
            Music::Playing
        }
    }

    /// What the energy source is telling us right now.
    fn signal(&self, now: Instant) -> Signal {
        let fresh = |s: &Option<(f32, Instant)>| {
            s.filter(|(_, at)| now.saturating_duration_since(*at) < ENERGY_STALE)
                .map(|(v, _)| v)
        };
        // Opt-in: VirtualDJ is observed and says nothing plays → go dark,
        // whatever the energy source. Off by default because it needs a
        // reliable link; a stalling one produced blackouts mid-track.
        if self.cfg.stop_on_silence && self.music_state(now) == Music::Stopped {
            return Signal::Silent;
        }
        let held = |s: &Option<(f32, Instant)>| {
            s.filter(|(_, at)| now.saturating_duration_since(*at) < ENERGY_HOLD)
                .map(|(v, _)| v)
        };
        match self.cfg.energy_source {
            EnergySource::Manual => Signal::Live(self.cfg.manual_energy, EnergySource::Manual),
            // No VDJ link at all: nothing will ever report, use the slider.
            EnergySource::VdjVu if !self.poller_up && !self.os2l_up => {
                Signal::Fallback(self.cfg.manual_energy)
            }
            EnergySource::VdjVu => {
                // The deck says "play" but the meter has been flat for a
                // while: the track ran out. Same thing as stopped.
                let flat = self.cfg.stop_on_silence
                    && self
                        .quiet_since
                        .map(|since| {
                            now.saturating_duration_since(since).as_secs_f32() >= SILENCE_SECS
                        })
                        .unwrap_or(false);
                match (fresh(&self.vu), held(&self.vu)) {
                    (Some(_), _) if flat => Signal::Silent,
                    // Fresh or held: keep following the last smoothed level.
                    (_, Some(_)) => Signal::Live(self.smooth, EnergySource::VdjVu),
                    // No level for a long while (old VDJ build without
                    // get_vu_meter, or the link is gone): the slider.
                    _ => Signal::Fallback(self.cfg.manual_energy),
                }
            }
            EnergySource::Mic => match fresh(&self.mic) {
                Some(_) => Signal::Live(self.smooth, EnergySource::Mic),
                None => Signal::Fallback(self.cfg.manual_energy),
            },
            EnergySource::Os2l => {
                if !self.os2l_up {
                    return Signal::Fallback(self.cfg.manual_energy);
                }
                match held(&self.vu) {
                    Some(_) => Signal::Live(self.smooth, EnergySource::Os2l),
                    None => Signal::Fallback(self.cfg.manual_energy),
                }
            }
        }
    }

    /// One line for the UI: what each VDJ link reports (diagnostics).
    fn signal_detail(&self, now: Instant) -> String {
        let mut parts: Vec<String> = Vec::new();
        let since = |t: Option<Instant>| t.map(|t| now.saturating_duration_since(t).as_secs_f32());
        if self.poller_up {
            match (
                since(self.poller_last_playing),
                since(self.poller_stopped_since),
                since(self.poller_last_contact),
            ) {
                (Some(p), _, _) if p < POLL_GRACE.as_secs_f32() => {
                    parts.push(format!("HTTP ▶ {p:.1}s"))
                }
                (_, Some(st), _) => parts.push(format!("HTTP ⏹ {st:.0}s")),
                (_, _, Some(c)) => parts.push(format!("HTTP ? {c:.0}s")),
                _ => parts.push("HTTP ✗".into()),
            }
        } else {
            parts.push("HTTP —".into());
        }
        if self.os2l_up {
            match self.os2l_beat_at {
                Some(at) => parts.push(format!(
                    "OS2L ♪ {:.1}s",
                    now.saturating_duration_since(at).as_secs_f32()
                )),
                None => parts.push("OS2L ♪ —".into()),
            }
        } else {
            parts.push("OS2L —".into());
        }
        if let Some((v, at)) = self.vu {
            if now.saturating_duration_since(at) < ENERGY_STALE {
                parts.push(format!("VU {v:.2}"));
            }
        }
        parts.join(" · ")
    }

    fn current_beat(&mut self, now: Instant, overall_bpm: Option<f32>) -> (f64, f32, &'static str) {
        let (beat, bpm, src) = self.raw_beat(now, overall_bpm);
        // Latency calibration: shift the clock so the programs land on the
        // beat the room hears, not the one VirtualDJ reports. +100 ms at
        // 128 BPM is 0.21 beats later.
        let shift = self.cfg.beat_offset_ms as f64 / 1000.0 * bpm as f64 / 60.0;
        (beat - shift, bpm, src)
    }

    fn raw_beat(&mut self, now: Instant, overall_bpm: Option<f32>) -> (f64, f32, &'static str) {
        if let Some((anchor, src)) = self.anchor {
            if now.duration_since(anchor.set_at) < ANCHOR_STALE {
                let b = anchor.beat_at(now);
                self.free_beat = b;
                self.last_tick = Some(now);
                return (b, anchor.bpm, src.label());
            }
        }
        // Free-run on the last musical tempo we saw (the overall BPM may be
        // the chasers' halved grid), else the global BPM.
        let bpm = self
            .anchor
            .map(|(a, _)| a.bpm)
            .or(overall_bpm)
            .unwrap_or(120.0)
            .max(20.0);
        if let Some(last) = self.last_tick {
            self.free_beat += now.duration_since(last).as_secs_f64() * bpm as f64 / 60.0;
        }
        self.last_tick = Some(now);
        (self.free_beat, bpm, "free")
    }

    fn enqueue(&self, action: ButtonAction) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(action);
        }
    }

    /// Release whatever vocabulary look we activated, if it's still the one in play.
    fn release_vocab(&mut self) {
        let Some(v) = self.vocab.take() else { return };
        let Some(h) = &self.handles else { return };
        let still_ours = match &v.action {
            ButtonAction::RecallScene { id } => {
                h.scenes.lock().active_scene_id() == Some(id.as_str())
            }
            ButtonAction::ToggleSnapshot { id } => {
                h.snapshots.lock().active_id() == Some(id.as_str())
            }
            _ => false,
        };
        if still_ours {
            // Both actions toggle: re-dispatching the active one releases it.
            self.enqueue(v.action.clone());
        }
    }

    fn vocab_candidates(&self, band: EnergyBand) -> Vec<(usize, f32)> {
        self.cfg
            .vocabulary
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let w = if e.energy == band {
                    1.0
                } else if e.energy.neighbour(band) {
                    0.35
                } else {
                    return None;
                };
                let learned =
                    1.0 + (1.0 + *self.usage.get(&usage_key(&e.action)).unwrap_or(&0) as f32).ln();
                Some((i, (e.weight.max(0.05) * w * learned)))
            })
            .collect()
    }

    fn resolve_vocab(&self, entry: &VocabEntry) -> Option<VocabActive> {
        let h = self.handles.as_ref()?;
        let s = h.show.read();
        match &entry.action {
            ButtonAction::RecallScene { id } => {
                let scene = s.show.scenes.iter().find(|sc| &sc.id == id)?;
                let covered: HashSet<String> = scene
                    .steps
                    .iter()
                    .flat_map(|st| st.fixtures.iter().map(|f| f.fixture_id.clone()))
                    .collect();
                Some(VocabActive {
                    action: entry.action.clone(),
                    name: scene.name.clone(),
                    covered: Some(covered),
                })
            }
            ButtonAction::ToggleSnapshot { id } => {
                let snap = s.show.snapshots.iter().find(|sn| &sn.id == id)?;
                Some(VocabActive {
                    action: entry.action.clone(),
                    name: snap.name.clone(),
                    covered: None,
                })
            }
            _ => None,
        }
    }

    fn new_look(&mut self, energy: f32) {
        let want_vocab = self.cfg.use_vocabulary && {
            let cands = self.vocab_candidates(EnergyBand::of(energy));
            !cands.is_empty() && (!self.cfg.use_generator || self.rng.gen::<f32>() < 0.5)
        };
        self.release_vocab();
        if want_vocab {
            let cands = self.vocab_candidates(EnergyBand::of(energy));
            let total: f32 = cands.iter().map(|(_, w)| w).sum();
            let mut r = self.rng.gen::<f32>() * total;
            let mut chosen = cands[0].0;
            for (i, w) in &cands {
                if r <= *w {
                    chosen = *i;
                    break;
                }
                r -= w;
            }
            let entry = self.cfg.vocabulary[chosen].clone();
            if let Some(v) = self.resolve_vocab(&entry) {
                self.enqueue(v.action.clone());
                tracing::info!(target: "dmx::autopilot", look = %v.name, "vocabulary look");
                self.vocab = Some(v);
            }
        }
        if let Some(prev) = self.look.take() {
            self.history.push_back(prev);
            while self.history.len() > HISTORY_CAP {
                self.history.pop_front();
            }
        }
        let style = self.theme_style();
        let avoid: Vec<&Look> = self.history.iter().rev().take(2).collect();
        let look = generator::generate_unlike(
            &mut self.rng,
            style,
            energy,
            self.cfg.allow_strobe,
            self.cfg.allow_movement,
            self.cfg.floor,
            &avoid,
        );
        tracing::debug!(target: "dmx::autopilot", look = %generator::describe(&look), energy, "new look");
        self.look = Some(look);
    }

    /// Palette family for the next look. With "Auto", a theme sticks for a
    /// few looks so variety reads as intentional instead of a colour lottery.
    fn theme_style(&mut self) -> PaletteStyle {
        if self.cfg.palette != PaletteStyle::Auto {
            return self.cfg.palette;
        }
        if let Some((style, left)) = self.theme {
            if left > 0 {
                self.theme = Some((style, left - 1));
                return style;
            }
        }
        let current = self.theme.map(|(s, _)| s);
        let pool = [
            PaletteStyle::Warm,
            PaletteStyle::Cold,
            PaletteStyle::Rainbow,
            PaletteStyle::Mono,
            PaletteStyle::Warm,
            PaletteStyle::Cold,
        ];
        let mut style = pool[self.rng.gen_range(0..pool.len())];
        if Some(style) == current {
            style = pool[self.rng.gen_range(0..pool.len())];
        }
        self.theme = Some((style, self.rng.gen_range(2..=5)));
        style
    }

    fn go_back(&mut self) {
        if let Some(prev) = self.history.pop_back() {
            self.release_vocab();
            if let Some(cur) = self.look.take() {
                self.history.push_back(cur);
            }
            self.look = Some(prev);
        }
    }

    fn decide(&mut self, beat: f64, energy: f32, phrase_boundary: bool) {
        let before = self.look.as_ref().map(|l| (l.id, l.clone()));
        self.decide_inner(beat, energy, phrase_boundary);
        // A look change may crossfade out of the old one instead of cutting.
        if let (Some((old_id, old)), Some(new)) = (before, self.look.as_ref()) {
            if new.id != old_id {
                self.prev_look = if new.transition_beats > 0.0 {
                    Some((old, beat, new.transition_beats as f64))
                } else {
                    None
                };
            }
        }
    }

    fn decide_inner(&mut self, beat: f64, energy: f32, phrase_boundary: bool) {
        if let Some(n) = self.pending_nudge.take() {
            match n {
                Nudge::Next => self.new_look(energy),
                Nudge::Back => self.go_back(),
                Nudge::Hold => {}
            }
            return;
        }
        if self.hold && self.look.is_some() {
            return;
        }
        let variety = self.cfg.variety.clamp(0.0, 1.0);
        let p_new = (0.3 + 0.55 * variety + if phrase_boundary { 0.25 } else { 0.0 }).min(0.97);
        let r = self.rng.gen::<f32>();
        if self.look.is_none() || r < p_new {
            self.new_look(energy);
        } else if r < p_new + (1.0 - p_new) * 0.5 && !self.history.is_empty() {
            self.go_back();
        }
        if self.cfg.allow_drops && phrase_boundary && energy >= 0.7 && self.rng.gen::<f32>() < 0.25
        {
            self.drop_until = Some(beat + 1.0);
        }
    }

    fn look_info(&self, look: &Look, vocab: Option<&VocabActive>) -> LookInfo {
        LookInfo {
            name: generator::describe(look),
            palette: look.palette_hex(),
            style: look.style,
            color_mode: look.color,
            dim_program: look.dim,
            mover_program: look.mover,
            strobe: look.strobe,
            vocab_name: vocab.map(|v| v.name.clone()),
        }
    }

    fn status(&mut self, now: Instant, overall_bpm: Option<f32>) -> AutopilotStatus {
        let (beat, bpm, src) = self.current_beat(now, overall_bpm);
        let signal = self.signal(now);
        let energy = signal.energy();
        let ducked = self
            .ducked_until
            .map(|u| u.saturating_duration_since(now).as_secs_f32())
            .unwrap_or(0.0);
        let phrase_pos = beat.rem_euclid(PHRASE_BEATS);
        AutopilotStatus {
            enabled: self.cfg.enabled,
            active: self.cfg.enabled && !self.profiles.is_empty() && ducked <= 0.0,
            clock_source: src.to_string(),
            bpm,
            beat,
            beat_in_bar: (beat.rem_euclid(4.0).floor() as u32) + 1,
            bar_in_phrase: ((phrase_pos / 4.0).floor() as u32) + 1,
            phrase: (beat / PHRASE_BEATS).floor().max(0.0) as u32,
            energy,
            energy_band: EnergyBand::of(energy).label().to_string(),
            energy_from: signal.source_label().to_string(),
            signal: signal.label().to_string(),
            silent: self.silent_since.is_some(),
            signal_detail: self.signal_detail(now),
            calibrating: self.calibrating,
            hold: self.hold,
            burst: self.burst_active(beat),
            drop: self.drop_active(beat),
            look: self
                .look
                .as_ref()
                .map(|l| self.look_info(l, self.vocab.as_ref())),
            history: self
                .history
                .iter()
                .rev()
                .map(|l| self.look_info(l, None))
                .collect(),
            next_change_in: if self.cfg.enabled {
                (self.next_change - beat).max(0.0) as f32
            } else {
                0.0
            },
            ducked_for: ducked,
            participants: self.profiles.len() as u32,
            movers: self.profiles.iter().filter(|p| p.is_mover()).count() as u32,
        }
    }
}

fn usage_key(a: &ButtonAction) -> String {
    serde_json::to_string(a).unwrap_or_default()
}

fn rt() -> &'static Mutex<Runtime> {
    static RT: OnceLock<Mutex<Runtime>> = OnceLock::new();
    RT.get_or_init(|| Mutex::new(Runtime::new()))
}

thread_local! {
    /// Set on the conductor thread so its own dispatches don't duck the autopilot.
    static SELF_DISPATCH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// ---- Public API ---------------------------------------------------------------

/// Wire the runtime to the app: store the dispatch handles and start the
/// conductor thread that fires vocabulary looks outside the DMX loop.
pub fn install(handles: GenericHandles) {
    let (tx, rx) = crossbeam_channel::unbounded::<ButtonAction>();
    let conductor_handles = handles.clone();
    let _ = std::thread::Builder::new()
        .name("dmx-autopilot-conductor".into())
        .spawn(move || {
            SELF_DISPATCH.with(|c| c.set(true));
            for action in rx.iter() {
                dispatch_action(action, true, true, &conductor_handles);
            }
        });
    let mut r = rt().lock();
    r.handles = Some(handles);
    r.tx = Some(tx);
}

/// Re-read config + fixtures + library from the show (boot, patch edits,
/// config saves, mirror replication).
pub fn sync_from_show(show: &ShowState) {
    let (cfg, fixtures, library) = {
        let s = show.read();
        (
            s.show.autopilot.clone(),
            s.show.fixtures.clone(),
            s.library.clone(),
        )
    };
    apply(cfg, &fixtures, &library);
}

pub fn apply(
    cfg: AutopilotConfig,
    fixtures: &[FixtureInstance],
    library: &HashMap<String, FixtureDefinition>,
) {
    let mut r = rt().lock();
    let was_enabled = r.cfg.enabled;
    r.profiles = profile::resolve(fixtures, library, &cfg.fixture_ids);
    r.zones = cfg
        .head_zones
        .iter()
        .map(|z| (z.fixture_id.clone(), z.clone()))
        .collect();
    r.cfg = cfg;
    if was_enabled && !r.cfg.enabled {
        r.release_vocab();
        r.look = None;
        r.history.clear();
        r.drop_until = None;
        r.ducked_until = None;
    }
    if !was_enabled && r.cfg.enabled {
        r.look = None;
        r.next_change = 0.0;
    }
}

pub fn is_enabled() -> bool {
    rt().lock().cfg.enabled
}

pub fn set_beat_anchor(anchor: Option<BeatAnchor>, source: ClockSource) {
    let mut r = rt().lock();
    match anchor {
        Some(a) => r.anchor = Some((a, source)),
        // Only the source that owns the anchor may clear it.
        None => {
            if matches!(r.anchor, Some((_, s)) if s == source) {
                r.anchor = None;
            }
        }
    }
}

/// Energy sample from VirtualDJ's VU meter or the mic (0..1).
pub fn set_energy_sample(source: EnergySource, value: f32) {
    let mut r = rt().lock();
    let raw = value.clamp(0.0, 1.0);
    let gain = r.cfg.energy_gain.max(0.0);
    let now = Instant::now();
    let v = match source {
        EnergySource::VdjVu => {
            // Normalise against a peak that decays over a couple of minutes:
            // VDJ's meter sits around 0.3–0.5 even with the track at full
            // tilt, which read as "chill" and left the rig barely moving.
            r.vu_peak = (r.vu_peak * 0.995).max(raw).max(0.25);
            let v = ((raw / r.vu_peak) * gain).clamp(0.0, 1.0);
            r.vu = Some((v, now));
            r.poller_last_playing = Some(now);
            r.poller_last_contact = Some(now);
            r.poller_stopped_since = None;
            if raw < SILENCE_LEVEL {
                r.quiet_since.get_or_insert(now);
            } else {
                r.quiet_since = None;
            }
            v
        }
        EnergySource::Mic => {
            let v = (raw * gain).clamp(0.0, 1.0);
            r.mic = Some((v, now));
            v
        }
        // Manual has no samples; OS2L strength arrives via `note_os2l_beat`.
        EnergySource::Manual | EnergySource::Os2l => return,
    };
    if r.cfg.energy_source == source {
        r.smooth = follow(r.smooth, v);
    }
}

/// The VDJ poller saw a deck playing (level may be unknown on old builds).
pub fn note_vdj_playing() {
    let mut r = rt().lock();
    let now = Instant::now();
    r.poller_last_playing = Some(now);
    r.poller_last_contact = Some(now);
    r.poller_stopped_since = None;
}

/// VDJ answered and no deck is playing (stop, pause, track ran out).
pub fn note_vdj_stopped() {
    let mut r = rt().lock();
    let now = Instant::now();
    r.poller_last_contact = Some(now);
    r.poller_stopped_since.get_or_insert(now);
}

/// VDJ's HTTP interface didn't answer (timeout, connection refused). Not
/// evidence of anything by itself: the link answers one request and hangs
/// on the next on some machines.
pub fn note_vdj_unreachable() {
    // Nothing to record: `music_state` reads the age of the last contact.
}

/// Asymmetric smoothing: a drop should light up within a beat, a
/// breakdown may take a couple of seconds to settle. Samples arrive at
/// the VDJ poll rate (~4/s).
fn follow(prev: f32, v: f32) -> f32 {
    let a = if v > prev { 0.8 } else { 0.3 };
    prev + (v - prev) * a
}

/// Whether the VDJ HTTP poller is running. Off → deck play state and VU
/// can't come from it (OS2L beats may still tell us the music is on).
pub fn set_vdj_poller_up(up: bool) {
    let mut r = rt().lock();
    if r.poller_up != up {
        r.poller_up = up;
        if !up {
            r.vu = None;
            r.poller_last_playing = None;
            r.poller_stopped_since = None;
            r.poller_last_contact = None;
            r.quiet_since = None;
        }
    }
}

/// VirtualDJ connected / disconnected over OS2L.
pub fn set_os2l_up(up: bool) {
    let mut r = rt().lock();
    r.os2l_up = up;
    if !up {
        r.os2l_beat_at = None;
    }
}

/// An OS2L beat event arrived. Without the HTTP poller, its `strength`
/// (how hard the beat hits) is the best level we have, so it feeds the
/// energy the same way the VU meter would.
pub fn note_os2l_beat(strength: Option<f32>) {
    let mut r = rt().lock();
    let now = Instant::now();
    r.os2l_beat_at = Some(now);
    let strength_is_the_source = r.cfg.energy_source == EnergySource::Os2l;
    if r.poller_up && !strength_is_the_source {
        return;
    }
    // VDJ's beat strength has a scale of its own; normalise against a peak
    // that decays over roughly a minute so a soft intro reads low and the
    // drop reads full, whatever the absolute numbers are.
    let level = match strength {
        Some(st) => {
            let st = st.clamp(0.0, 1.0);
            r.strength_peak = (r.strength_peak * 0.995).max(st).max(0.25);
            (st / r.strength_peak).clamp(0.0, 1.0)
        }
        None => 0.6,
    };
    let v = (level * r.cfg.energy_gain.max(0.0)).clamp(0.0, 1.0);
    r.vu = Some((v, now));
    if v < SILENCE_LEVEL {
        r.quiet_since.get_or_insert(now);
    } else {
        r.quiet_since = None;
    }
    if strength_is_the_source {
        // One sample per beat: a single weak beat inside a strong section
        // must not dip the lights, a break should settle within a bar.
        let a = if v > r.smooth { 0.5 } else { 0.15 };
        r.smooth += (v - r.smooth) * a;
    } else if r.cfg.energy_source == EnergySource::VdjVu {
        r.smooth = follow(r.smooth, v);
    }
}

/// The operator fired something by hand: step aside for a while, and
/// remember what they like for the vocabulary weights.
pub fn note_manual_action(action: &ButtonAction) {
    if SELF_DISPATCH.with(|c| c.get()) {
        return;
    }
    let looks_like_a_look = matches!(
        action,
        ButtonAction::RecallScene { .. }
            | ButtonAction::RecallSceneByIndex { .. }
            | ButtonAction::ToggleSnapshot { .. }
            | ButtonAction::ToggleSnapshotByIndex { .. }
            | ButtonAction::ToggleChaser { .. }
            | ButtonAction::ToggleChaserByIndex { .. }
            | ButtonAction::ToggleMovement { .. }
            | ButtonAction::ToggleMovementByIndex { .. }
            | ButtonAction::StartLoopGroup { .. }
            | ButtonAction::StartLoopGroupByIndex { .. }
    );
    if !looks_like_a_look {
        return;
    }
    let mut r = rt().lock();
    *r.usage.entry(usage_key(action)).or_insert(0) += 1;
    if r.cfg.enabled {
        let secs = r.cfg.ducking_secs.max(0.0);
        r.ducked_until = Some(Instant::now() + Duration::from_secs_f32(secs));
        r.vocab = None; // the operator replaced whatever we had up
        tracing::info!(target: "dmx::autopilot", secs, "manual action: ducking");
    }
}

pub fn nudge(n: Nudge) {
    let mut r = rt().lock();
    r.pending_nudge = Some(n);
    r.next_change = f64::MIN; // decide on the next frame
}

/// Hold the autopilot's output (a Swop button is held).
pub fn set_muted(on: bool) {
    rt().lock().muted = on;
}

/// Keep the current look (no director changes, no mutations) while on.
pub fn set_hold(on: bool) {
    rt().lock().hold = on;
}

pub fn toggle_hold() {
    let mut r = rt().lock();
    r.hold = !r.hold;
    tracing::info!(target: "dmx::autopilot", hold = r.hold, "hold toggled");
}

pub fn is_hold() -> bool {
    rt().lock().hold
}

/// Manual strobe burst while a pad is held (beat-locked sixteenths).
pub fn burst_set(on: bool) {
    let mut r = rt().lock();
    r.burst_held = on;
    if !on {
        r.burst_from = None;
        r.burst_until = None;
    }
}

/// Manual strobe burst for one bar, starting on the next beat.
pub fn burst_bar() {
    let mut r = rt().lock();
    let beat = r.free_beat;
    let from = beat.floor() + 1.0;
    r.burst_from = Some(from);
    r.burst_until = Some(from + 4.0);
}

pub fn is_bursting() -> bool {
    let r = rt().lock();
    let beat = r.free_beat;
    r.burst_active(beat)
}

/// One-beat drop on the next beat.
pub fn drop_next() {
    let mut r = rt().lock();
    let beat = r.free_beat;
    let from = beat.floor() + 1.0;
    r.drop_from = Some(from);
    r.drop_until = Some(from + 1.0);
}

/// Palette family from a pad (persists into the show, like the slider).
pub fn set_palette_from_action(app: &AppHandle, style: PaletteStyle) {
    let show = app.state::<ShowState>().inner().clone();
    show.write().show.autopilot.palette = style;
    if let Err(e) = crate::commands::persist_show(&show, app) {
        tracing::warn!(target: "dmx::autopilot", error = ?e, "palette persist failed");
    }
    let _ = app.emit(crate::commands::SHOW_EVENT, ());
    sync_from_show(&show);
    // Make the change audible now: a fresh theme at the next cadence.
    rt().lock().theme = None;
}

/// Head movement on/off from a pad.
pub fn toggle_movement_from_action(app: &AppHandle) {
    let show = app.state::<ShowState>().inner().clone();
    let on = {
        let mut s = show.write();
        s.show.autopilot.allow_movement = !s.show.autopilot.allow_movement;
        s.show.autopilot.allow_movement
    };
    if let Err(e) = crate::commands::persist_show(&show, app) {
        tracing::warn!(target: "dmx::autopilot", error = ?e, "movement persist failed");
    }
    let _ = app.emit(crate::commands::SHOW_EVENT, ());
    sync_from_show(&show);
    tracing::info!(target: "dmx::autopilot", on, "movement toggled from a button");
}

/// Flash/Swop pads need "on" and "off", not a toggle.
pub fn set_enabled_from_action(app: &AppHandle, on: bool) {
    let show = app.state::<ShowState>().inner().clone();
    if show.read().show.autopilot.enabled == on {
        return;
    }
    if let Err(e) = set_enabled_impl(app, &show, on) {
        tracing::warn!(target: "dmx::autopilot", error = ?e, "set enabled failed");
    }
}

/// Latency calibration mode on/off.
pub fn set_calibrating(on: bool) {
    let mut r = rt().lock();
    r.calibrating = on;
    if !on {
        // Come back with a fresh look on the next frame.
        r.look = None;
        r.next_change = 0.0;
    }
}

pub fn status(overall_bpm: Option<f32>) -> AutopilotStatus {
    rt().lock().status(Instant::now(), overall_bpm)
}

/// One frame of the autopilot. Called by the output thread; returns the
/// effects overlay to merge on top of chasers/movement.
pub fn tick(now: Instant, overall_bpm: Option<f32>) -> HashMap<u16, ChannelOverlay> {
    rt().lock().tick(now, overall_bpm)
}

impl Runtime {
    fn tick(&mut self, now: Instant, overall_bpm: Option<f32>) -> HashMap<u16, ChannelOverlay> {
        let enabled = self.cfg.enabled && !self.profiles.is_empty();
        let (beat, _bpm, _src) = self.current_beat(now, overall_bpm);
        let signal = self.signal(now);
        let dt = self
            .last_frame
            .map(|t| now.saturating_duration_since(t).as_secs_f32())
            .unwrap_or(0.0)
            .min(0.25);
        self.last_frame = Some(now);

        // Status heartbeat (slower when idle so the UI clock still ticks).
        let interval = if enabled { 250 } else { 1000 };
        let due = self
            .last_status
            .map(|t| now.duration_since(t) >= Duration::from_millis(interval))
            .unwrap_or(true);
        if due {
            self.last_status = Some(now);
            let st = self.status(now, overall_bpm);
            if let Some(h) = &self.handles {
                let _ = h.app.emit(STATUS_EVENT, st);
            }
        }

        if !enabled {
            self.gain = 0.0;
            self.silent_since = None;
            self.frozen_beat = None;
            return HashMap::new();
        }

        // A Swop button owns the room for the moment.
        if self.muted {
            return HashMap::new();
        }

        // Calibration: a clean white pulse on every beat with the heads
        // parked, whatever the energy or the signal, so the operator can set
        // the latency offset against the kick.
        if self.calibrating {
            let look = generator::calibration_look();
            return generator::evaluate(generator::EvalInput {
                look: &look,
                profiles: &self.profiles,
                beat,
                energy: 1.0,
                zones: &self.zones,
                allow_movement: false,
                max_level: self.cfg.max_level,
                force_burst: false,
                skip_colour: &|_| false,
            });
        }

        // Ducked: the operator has the floor.
        if let Some(until) = self.ducked_until {
            if now < until {
                return HashMap::new();
            }
            self.ducked_until = None;
            self.look = None; // come back with something fresh
            self.prev_look = None;
            self.next_change = 0.0;
        }

        // No music (deck stopped, track ended, VDJ gone): fade what we own
        // to black and hold it there. The beat freezes so the heads stop
        // wandering in the dark; when the music is back we fade in on a
        // fresh look.
        let energy = match signal {
            Signal::Silent => {
                if self.silent_since.is_none() {
                    self.silent_since = Some(now);
                    self.frozen_beat = Some(beat);
                    self.drop_until = None;
                    self.prev_look = None;
                    self.release_vocab();
                    tracing::info!(target: "dmx::autopilot", "no signal: fading to black");
                }
                self.gain = (self.gain - dt / FADE_OUT_SECS).max(0.0);
                0.0
            }
            live => {
                if let Some(since) = self.silent_since.take() {
                    self.frozen_beat = None;
                    let gap = now.saturating_duration_since(since).as_secs_f32();
                    if gap > KEEP_LOOK_SECS || self.look.is_none() {
                        self.look = None;
                        self.next_change = 0.0;
                    } else {
                        // A short break: carry the same look on to the next
                        // cadence boundary instead of reshuffling everything.
                        let cadence = self.cfg.cadence_beats.max(1) as f64;
                        self.next_change = (beat / cadence).floor() * cadence + cadence;
                    }
                    tracing::info!(target: "dmx::autopilot", gap_secs = gap, "signal back: resuming");
                }
                self.gain = (self.gain + dt / FADE_IN_SECS).min(1.0);
                live.energy()
            }
        };
        let silent = self.silent_since.is_some();
        let beat = self.frozen_beat.unwrap_or(beat);

        if !silent {
            let cadence = self.cfg.cadence_beats.max(1) as f64;
            let phrase = (beat / PHRASE_BEATS).floor() as i64;
            let phrase_boundary = phrase != self.last_phrase;
            if beat >= self.next_change || beat < self.next_change - cadence * 2.0 {
                self.decide(beat, energy, phrase_boundary);
                self.next_change = (beat / cadence).floor() * cadence + cadence;
                self.last_phrase = phrase;
            } else {
                // Between decisions, every new bar may micro-mutate the look
                // (palette rotation, head spread, tempo, program family) so a
                // held look keeps evolving instead of looping.
                // Only every other bar, and rarely: a look that shifts every
                // bar reads as random instead of evolving.
                let bar = (beat / 4.0).floor() as i64;
                if bar != self.last_bar && bar.rem_euclid(2) == 0 && !self.hold {
                    self.last_bar = bar;
                    let p = 0.08 + 0.3 * self.cfg.variety.clamp(0.0, 1.0);
                    if self.rng.gen::<f32>() < p {
                        // Take the look out for a moment: two mutable borrows
                        // (look + rng) can't coexist.
                        if let Some(mut look) = self.look.take() {
                            look.mutate(&mut self.rng);
                            self.look = Some(look);
                        }
                    }
                }
            }

            if let Some(until) = self.drop_until {
                if self.drop_active(beat) {
                    // One-beat drop: everything we own goes dark.
                    return self.dark_overlay();
                }
                if beat >= until {
                    self.drop_until = None;
                    self.drop_from = None;
                }
            }
        }
        let force_burst = self.burst_active(beat);
        if !force_burst && self.burst_until.map(|u| beat >= u).unwrap_or(false) {
            self.burst_from = None;
            self.burst_until = None;
        }

        let Some(look) = self.look.clone() else {
            return if silent {
                self.dark_overlay()
            } else {
                HashMap::new()
            };
        };
        let covered = self.vocab.as_ref().map(|v| v.covered.clone());
        let skip = move |p: &FixtureProfile| -> bool {
            match &covered {
                None => false,
                Some(None) => true, // snapshot: owns every fixture
                Some(Some(set)) => set.contains(&p.id),
            }
        };
        let mut out = generator::evaluate(generator::EvalInput {
            look: &look,
            profiles: &self.profiles,
            beat,
            energy,
            zones: &self.zones,
            allow_movement: self.cfg.allow_movement,
            max_level: self.cfg.max_level,
            force_burst,
            skip_colour: &skip,
        });
        // Crossfade from the previous look while its window is open.
        if let Some((prev, from, len)) = self.prev_look.take() {
            let t = ((beat - from) / len.max(1e-3)).clamp(0.0, 1.0);
            if t < 1.0 && !silent {
                let prev_out = generator::evaluate(generator::EvalInput {
                    look: &prev,
                    profiles: &self.profiles,
                    beat,
                    energy,
                    zones: &self.zones,
                    allow_movement: self.cfg.allow_movement,
                    max_level: self.cfg.max_level,
                    force_burst: false,
                    skip_colour: &skip,
                });
                out = blend_overlays(&prev_out, out, t as f32);
                self.prev_look = Some((prev, from, len));
            }
        }
        if self.gain < 1.0 {
            apply_gain(&mut out, &self.profiles, self.gain);
        }
        out
    }

    fn burst_active(&self, beat: f64) -> bool {
        self.burst_held
            || match (self.burst_from, self.burst_until) {
                (Some(from), Some(until)) => beat >= from && beat < until,
                _ => false,
            }
    }

    fn drop_active(&self, beat: f64) -> bool {
        match self.drop_until {
            Some(until) => {
                let from = self.drop_from.unwrap_or(until - 1.0);
                beat >= from && beat < until
            }
            None => false,
        }
    }

    /// Every intensity / colour / strobe channel we own, at 0.
    fn dark_overlay(&self) -> HashMap<u16, ChannelOverlay> {
        let mut dark: HashMap<u16, ChannelOverlay> = HashMap::new();
        for p in &self.profiles {
            for ch in [
                p.dimmer, p.red, p.green, p.blue, p.white, p.amber, p.uv, p.strobe,
            ]
            .into_iter()
            .flatten()
            {
                dark.entry(p.universe)
                    .or_insert_with(crate::engine::empty_overlay)[ch] = Some(0);
            }
        }
        dark
    }
}

/// Linear blend of two overlays (`t` = 0 → `from`, 1 → `to`). Channels only
/// the old look owned linger for the first half, then let go.
fn blend_overlays(
    from: &HashMap<u16, ChannelOverlay>,
    mut to: HashMap<u16, ChannelOverlay>,
    t: f32,
) -> HashMap<u16, ChannelOverlay> {
    let t = t.clamp(0.0, 1.0);
    for (u, prev) in from {
        let cur = to.entry(*u).or_insert_with(crate::engine::empty_overlay);
        for (ch, old) in prev.iter().enumerate() {
            cur[ch] = match (*old, cur[ch]) {
                (Some(a), Some(b)) => Some((a as f32 + (b as f32 - a as f32) * t).round() as u8),
                (Some(a), None) => {
                    if t < 0.5 {
                        Some(a)
                    } else {
                        None
                    }
                }
                (None, b) => b,
            };
        }
    }
    to
}

/// Master fade on our overlay: scales the dimmer (or the colour mix when
/// there is no dimmer) and mutes strobes while fading.
fn apply_gain(out: &mut HashMap<u16, ChannelOverlay>, profiles: &[FixtureProfile], gain: f32) {
    let g = gain.clamp(0.0, 1.0);
    for p in profiles {
        let Some(ov) = out.get_mut(&p.universe) else {
            continue;
        };
        let channels: [Option<usize>; 3] = if p.dimmer.is_some() {
            [p.dimmer, None, None]
        } else {
            [p.red, p.green, p.blue]
        };
        for ch in channels.iter().flatten() {
            if let Some(v) = ov[*ch] {
                ov[*ch] = Some((v as f32 * g).round() as u8);
            }
        }
        if let Some(st) = p.strobe {
            if ov[st].is_some() {
                ov[st] = Some(0);
            }
        }
    }
}

// ---- Commands ----------------------------------------------------------------

fn set_enabled_impl(
    app: &AppHandle,
    show: &ShowState,
    enabled: bool,
) -> Result<(), crate::commands::CommandError> {
    show.write().show.autopilot.enabled = enabled;
    crate::commands::persist_show(show, app)?;
    let _ = app.emit(crate::commands::SHOW_EVENT, ());
    sync_from_show(show);
    tracing::info!(target: "dmx::autopilot", enabled, "autopilot toggled");
    Ok(())
}

/// ButtonAction::AutopilotEnergy from any surface: the DJ sets the energy
/// by hand (chill / groove / party pads) — the reliable way on this rig,
/// where neither VDJ's meter nor a room mic can be trusted.
pub fn set_energy_from_action(app: &AppHandle, level: f32) {
    let show = app.state::<ShowState>().inner().clone();
    {
        let mut s = show.write();
        s.show.autopilot.manual_energy = level.clamp(0.0, 1.0);
        s.show.autopilot.energy_source = EnergySource::Manual;
    }
    if let Err(e) = crate::commands::persist_show(&show, app) {
        tracing::warn!(target: "dmx::autopilot", error = ?e, "energy persist failed");
    }
    let _ = app.emit(crate::commands::SHOW_EVENT, ());
    sync_from_show(&show);
    tracing::info!(target: "dmx::autopilot", level, "energy set from a button");
}

/// Is this preset the energy in force? (pad LEDs)
pub fn manual_energy_is(level: f32) -> bool {
    let r = rt().lock();
    r.cfg.energy_source == EnergySource::Manual && (r.cfg.manual_energy - level).abs() < 0.02
}

/// ButtonAction::ToggleAutopilot from any surface (Launchpad, OS2L, cues).
pub fn toggle_from_action(app: &AppHandle) {
    let show = app.state::<ShowState>().inner().clone();
    let current = show.read().show.autopilot.enabled;
    if let Err(e) = set_enabled_impl(app, &show, !current) {
        tracing::warn!(target: "dmx::autopilot", error = ?e, "toggle failed");
    }
}

#[tauri::command]
pub fn autopilot_get_config(show: tauri::State<'_, ShowState>) -> AutopilotConfig {
    show.read().show.autopilot.clone()
}

#[tauri::command]
pub fn autopilot_set_config(
    app: AppHandle,
    show: tauri::State<'_, ShowState>,
    config: AutopilotConfig,
) -> Result<AutopilotStatus, crate::commands::CommandError> {
    show.write().show.autopilot = config;
    crate::commands::persist_show(&show, &app)?;
    let _ = app.emit(crate::commands::SHOW_EVENT, ());
    sync_from_show(&show);
    Ok(status(None))
}

#[tauri::command]
pub fn autopilot_set_enabled(
    app: AppHandle,
    show: tauri::State<'_, ShowState>,
    enabled: bool,
) -> Result<AutopilotStatus, crate::commands::CommandError> {
    set_enabled_impl(&app, &show, enabled)?;
    Ok(status(None))
}

#[tauri::command]
pub fn autopilot_status() -> AutopilotStatus {
    status(None)
}

#[tauri::command]
pub fn autopilot_set_calibrate(on: bool) -> AutopilotStatus {
    set_calibrating(on);
    status(None)
}

#[tauri::command]
pub fn autopilot_nudge(kind: String) -> AutopilotStatus {
    nudge(match kind.as_str() {
        "back" => Nudge::Back,
        "hold" => Nudge::Hold,
        _ => Nudge::Next,
    });
    status(None)
}

#[tauri::command]
pub fn autopilot_rig(show: tauri::State<'_, ShowState>) -> Vec<RigFixture> {
    let s = show.read();
    profile::rig_summary(&s.show.fixtures, &s.library)
}

/// Park one head at a normalised pan/tilt (fractions of its physical range)
/// so the operator can see where a zone corner lands in the room.
#[tauri::command]
pub fn autopilot_point_head(
    engine: tauri::State<'_, crate::engine::EngineState>,
    show: tauri::State<'_, ShowState>,
    fixture_id: String,
    pan: f32,
    tilt: f32,
) -> Result<(), crate::commands::CommandError> {
    let profiles = {
        let s = show.read();
        profile::resolve(
            &s.show.fixtures,
            &s.library,
            std::slice::from_ref(&fixture_id),
        )
    };
    let Some(p) = profiles.first() else {
        return Err(crate::commands::CommandError::Other(format!(
            "fixture {fixture_id} no encontrado o sin pan/tilt"
        )));
    };
    let to_dmx = |norm: f32, range: &crate::show::fixture::PanTiltRange| -> (u8, u8) {
        let (lo, hi) = if range.min <= range.max {
            (range.min as f32, range.max as f32)
        } else {
            (range.max as f32, range.min as f32)
        };
        let v = (lo + norm.clamp(0.0, 1.0) * (hi - lo)).round() as u16;
        ((v >> 8) as u8, (v & 0xff) as u8)
    };
    let (ph, pl) = to_dmx(pan, &p.pan_range);
    let (th, tl) = to_dmx(tilt, &p.tilt_range);
    let mut e = engine.write();
    for (ch, v) in [
        (p.pan, ph),
        (p.pan_fine, pl),
        (p.tilt, th),
        (p.tilt_fine, tl),
    ] {
        if let Some(ch) = ch {
            let _ = e.set_channel(p.universe, ch as u16, v);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn energy_bands_and_neighbours() {
        assert_eq!(EnergyBand::of(0.1), EnergyBand::Low);
        assert_eq!(EnergyBand::of(0.5), EnergyBand::Mid);
        assert_eq!(EnergyBand::of(0.9), EnergyBand::High);
        assert!(EnergyBand::Low.neighbour(EnergyBand::Mid));
        assert!(!EnergyBand::Low.neighbour(EnergyBand::High));
    }

    #[test]
    fn config_round_trips_with_defaults() {
        let json = r#"{"enabled":true,"fixture_ids":["a"]}"#;
        let cfg: AutopilotConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.cadence_beats, 8);
        assert!(cfg.allow_movement);
        assert!(!cfg.allow_strobe);
        let back = serde_json::to_string(&cfg).unwrap();
        let again: AutopilotConfig = serde_json::from_str(&back).unwrap();
        assert_eq!(cfg, again);
    }

    fn par_profile(id: &str, index: usize, base: usize) -> FixtureProfile {
        FixtureProfile {
            id: id.into(),
            label: id.into(),
            kind: profile::FixtureKind::Par,
            universe: 0,
            dimmer: Some(base),
            red: Some(base + 1),
            green: Some(base + 2),
            blue: Some(base + 3),
            white: None,
            amber: None,
            uv: None,
            strobe: None,
            shutter: None,
            pan: None,
            pan_fine: None,
            tilt: None,
            tilt_fine: None,
            pan_range: crate::show::fixture::PanTiltRange::default_pan(),
            tilt_range: crate::show::fixture::PanTiltRange::default_tilt(),
            position: [index as f32, 0.0],
            index,
        }
    }

    #[test]
    fn energy_follow_attacks_fast_and_releases_slow() {
        let up = follow(0.1, 0.9);
        let down = follow(0.9, 0.1);
        assert!(
            up - 0.1 > 0.9 - down,
            "attack {up} moves further than release {down}"
        );
        assert!(up > 0.5 && down > 0.5);
    }

    #[test]
    fn vdj_source_goes_silent_only_when_the_poller_is_up() {
        let mut r = Runtime::new();
        r.cfg.stop_on_silence = true;
        r.cfg.energy_source = EnergySource::VdjVu;
        r.cfg.manual_energy = 0.4;
        let now = Instant::now();
        assert_eq!(
            r.signal(now),
            Signal::Fallback(0.4),
            "VDJ off in Config: manual energy"
        );
        r.poller_up = true;
        assert_eq!(
            r.signal(now),
            Signal::Silent,
            "poller up but VDJ never answered"
        );
        r.poller_last_playing = Some(now);
        r.poller_last_contact = Some(now);
        assert_eq!(
            r.signal(now),
            Signal::Fallback(0.4),
            "playing but no VU verb: manual energy, not black"
        );
        r.vu = Some((0.7, now));
        r.smooth = 0.7;
        assert_eq!(r.signal(now), Signal::Live(0.7, EnergySource::VdjVu));
        // A flaky link: no answer for 7 s is not silence; the last level holds.
        assert_eq!(
            r.signal(now + Duration::from_secs(7)),
            Signal::Live(0.7, EnergySource::VdjVu),
            "transient errors keep the lights going on the held level"
        );
        assert_eq!(
            r.signal(now + Duration::from_secs(13)),
            Signal::Silent,
            "no contact at all for 12 s: VDJ is gone"
        );
        // An explicit, sustained "nothing plays" is silence.
        r.poller_last_playing = Some(now - Duration::from_secs(7));
        r.poller_stopped_since = Some(now - Duration::from_millis(3500));
        r.poller_last_contact = Some(now);
        assert_eq!(r.signal(now), Signal::Silent, "VDJ said stop for 3 s");
        r.poller_stopped_since = Some(now - Duration::from_millis(1000));
        assert_eq!(
            r.signal(now),
            Signal::Live(0.7, EnergySource::VdjVu),
            "a 1 s stop is not confirmed yet: keep following the last level"
        );
        r.poller_last_playing = Some(now);
        r.poller_stopped_since = None;
        // Track ran out: the deck still says "play" but the meter is flat.
        r.vu = Some((0.0, now));
        r.smooth = 0.0;
        r.quiet_since = Some(now - Duration::from_millis(500));
        assert_eq!(
            r.signal(now),
            Signal::Live(0.0, EnergySource::VdjVu),
            "a short gap is not silence"
        );
        r.quiet_since = Some(now - Duration::from_millis(2500));
        assert_eq!(
            r.signal(now),
            Signal::Silent,
            "flat meter for 2 s = the track ended"
        );
    }

    #[test]
    fn os2l_beats_alone_drive_playing_and_silence() {
        let mut r = Runtime::new();
        r.cfg.stop_on_silence = true;
        r.cfg.energy_source = EnergySource::VdjVu;
        r.cfg.manual_energy = 0.4;
        let now = Instant::now();
        assert_eq!(
            r.signal(now),
            Signal::Fallback(0.4),
            "no VDJ link at all: manual energy"
        );
        r.os2l_up = true;
        assert_eq!(r.signal(now), Signal::Silent, "connected but no beats yet");
        r.os2l_beat_at = Some(now);
        r.vu = Some((0.7, now));
        r.smooth = 0.7;
        assert_eq!(r.signal(now), Signal::Live(0.7, EnergySource::VdjVu));
        assert_eq!(
            r.signal(now + Duration::from_secs(5)),
            Signal::Silent,
            "beats stopped: the track ended"
        );
    }

    #[test]
    fn manual_energy_still_goes_dark_when_vdj_stops() {
        let mut r = Runtime::new();
        r.cfg.stop_on_silence = true;
        r.cfg.energy_source = EnergySource::Manual;
        r.cfg.manual_energy = 0.5;
        let now = Instant::now();
        assert_eq!(
            r.signal(now),
            Signal::Live(0.5, EnergySource::Manual),
            "no VDJ link: manual runs freely"
        );
        r.poller_up = true;
        assert_eq!(
            r.signal(now),
            Signal::Silent,
            "VDJ observed and nothing plays"
        );
        r.poller_last_playing = Some(now);
        r.poller_last_contact = Some(now);
        assert_eq!(r.signal(now), Signal::Live(0.5, EnergySource::Manual));
    }

    #[test]
    fn silence_detection_is_opt_in_and_the_level_holds() {
        let mut r = Runtime::new();
        r.cfg.energy_source = EnergySource::VdjVu;
        r.cfg.manual_energy = 0.4;
        r.poller_up = true;
        let now = Instant::now();
        assert_eq!(
            r.signal(now),
            Signal::Fallback(0.4),
            "default: VDJ silent never blacks out"
        );
        r.poller_stopped_since = Some(now - Duration::from_secs(60));
        r.poller_last_contact = Some(now);
        r.vu = Some((0.8, now - Duration::from_secs(10)));
        r.smooth = 0.8;
        assert_eq!(
            r.signal(now),
            Signal::Live(0.8, EnergySource::VdjVu),
            "no fresh sample for 10 s: hold the last level instead of jumping"
        );
        assert_eq!(
            r.signal(now + Duration::from_secs(25)),
            Signal::Fallback(0.4),
            "after the hold window the slider takes over"
        );
        r.quiet_since = Some(now - Duration::from_secs(5));
        r.vu = Some((0.0, now));
        assert_eq!(
            r.signal(now),
            Signal::Live(0.8, EnergySource::VdjVu),
            "a flat meter is not a blackout unless opted in"
        );
    }

    #[test]
    fn silence_fades_to_black_holds_and_resumes() {
        let mut r = Runtime::new();
        r.rng = StdRng::seed_from_u64(3);
        r.cfg.stop_on_silence = true;
        r.cfg.enabled = true;
        r.cfg.energy_source = EnergySource::VdjVu;
        r.cfg.use_vocabulary = false;
        r.poller_up = true;
        r.profiles = vec![par_profile("a", 0, 0), par_profile("b", 1, 10)];
        let t0 = Instant::now();
        r.vu = Some((0.9, t0));
        r.poller_last_playing = Some(t0);
        r.poller_last_contact = Some(t0);
        r.smooth = 0.9;
        let dim =
            |out: &HashMap<u16, ChannelOverlay>| out.get(&0).and_then(|ov| ov[0]).unwrap_or(0);
        let mut out = HashMap::new();
        for i in 0..40 {
            out = r.tick(t0 + Duration::from_millis(i * 50), Some(120.0));
        }
        assert!(dim(&out) > 0, "playing: our fixtures are lit");
        assert!(r.silent_since.is_none());
        assert!((r.gain - 1.0).abs() < 1e-6, "fade-in complete: {}", r.gain);

        // The deck stops: VDJ has been answering "nothing plays" for 3 s.
        r.vu = None;
        r.poller_last_playing = None;
        r.poller_stopped_since = Some(t0 - Duration::from_secs(1));
        r.poller_last_contact = Some(t0 + Duration::from_millis(2100));
        let t1 = t0 + Duration::from_millis(2100);
        r.tick(t1, Some(120.0));
        assert!(r.silent_since.is_some(), "silence detected");
        assert!(r.gain < 1.0 && r.gain > 0.0, "fading out: {}", r.gain);
        for i in 1..=60 {
            out = r.tick(t1 + Duration::from_millis(i * 50), Some(120.0));
        }
        assert_eq!(r.gain, 0.0);
        assert_eq!(dim(&out), 0, "held dark");
        assert!(
            out.get(&0).and_then(|ov| ov[10]).is_some(),
            "second fixture owned too"
        );

        // Music is back.
        let t2 = t1 + Duration::from_millis(4000);
        r.vu = Some((0.8, t2));
        r.poller_last_playing = Some(t2);
        r.poller_last_contact = Some(t2);
        r.poller_stopped_since = None;
        r.smooth = 0.8;
        for i in 0..30 {
            out = r.tick(t2 + Duration::from_millis(i * 50), Some(120.0));
        }
        assert!(r.silent_since.is_none(), "resumed");
        assert!(dim(&out) > 0, "lit again after the fade-in");
    }

    #[test]
    fn auto_palette_sticks_to_a_theme_for_a_few_looks() {
        let mut r = Runtime::new();
        r.rng = StdRng::seed_from_u64(5);
        r.cfg.palette = PaletteStyle::Auto;
        let first = r.theme_style();
        assert_ne!(first, PaletteStyle::Auto);
        let (_, left) = r.theme.unwrap();
        for _ in 0..left {
            assert_eq!(r.theme_style(), first, "the theme holds");
        }
        r.cfg.palette = PaletteStyle::Mono;
        assert_eq!(
            r.theme_style(),
            PaletteStyle::Mono,
            "an explicit palette wins"
        );
    }

    #[test]
    fn look_changes_crossfade_when_the_new_look_asks_for_it() {
        let mut r = Runtime::new();
        r.rng = StdRng::seed_from_u64(17);
        r.cfg.enabled = true;
        r.cfg.variety = 1.0;
        r.decide(0.0, 0.5, true);
        assert!(
            r.prev_look.is_none(),
            "nothing to fade from on the first look"
        );
        for i in 1..=6 {
            r.pending_nudge = Some(Nudge::Next);
            r.decide(i as f64 * 8.0, 0.5, false);
            let new = r.look.as_ref().unwrap();
            if new.transition_beats > 0.0 {
                let (_, from, len) = r.prev_look.as_ref().expect("crossfade armed");
                assert_eq!(*from, i as f64 * 8.0);
                assert_eq!(*len, new.transition_beats as f64);
            } else {
                assert!(r.prev_look.is_none(), "a hard cut has nothing to fade");
            }
        }
    }

    #[test]
    fn blend_overlays_interpolates_and_lets_old_channels_go() {
        let mut from = HashMap::new();
        let mut a = crate::engine::empty_overlay();
        a[0] = Some(200);
        a[1] = Some(100);
        from.insert(0u16, a);
        let mut to = HashMap::new();
        let mut b = crate::engine::empty_overlay();
        b[0] = Some(0);
        b[2] = Some(50);
        to.insert(0u16, b);
        let early = blend_overlays(&from, to.clone(), 0.25);
        assert_eq!(early[&0][0], Some(150));
        assert_eq!(early[&0][1], Some(100), "old-only channel lingers early");
        let half = blend_overlays(&from, to.clone(), 0.5);
        assert_eq!(half[&0][0], Some(100));
        assert_eq!(half[&0][2], Some(50));
        let late = blend_overlays(&from, to.clone(), 0.75);
        assert_eq!(late[&0][0], Some(50));
        assert_eq!(late[&0][1], None, "old-only channel released late");
        let start = blend_overlays(&from, to, 0.0);
        assert_eq!(start[&0][0], Some(200));
    }

    #[test]
    fn director_changes_on_cadence_and_remembers() {
        let mut r = Runtime::new();
        r.rng = StdRng::seed_from_u64(42);
        r.cfg.enabled = true;
        r.cfg.cadence_beats = 4;
        r.cfg.variety = 1.0;
        r.decide(0.0, 0.8, true);
        let first = r.look.clone().expect("a look");
        // Force "new" twice, then "back" must restore the previous one.
        r.pending_nudge = Some(Nudge::Next);
        r.decide(4.0, 0.8, false);
        let second = r.look.clone().unwrap();
        assert_ne!(first.id, second.id);
        r.pending_nudge = Some(Nudge::Back);
        r.decide(8.0, 0.8, false);
        assert_eq!(r.look.as_ref().unwrap().id, first.id);
    }

    #[test]
    fn vocabulary_weights_prefer_the_band_and_learned_use() {
        let mut r = Runtime::new();
        r.cfg.vocabulary = vec![
            VocabEntry {
                action: ButtonAction::RecallScene { id: "low".into() },
                energy: EnergyBand::Low,
                weight: 1.0,
            },
            VocabEntry {
                action: ButtonAction::RecallScene { id: "high".into() },
                energy: EnergyBand::High,
                weight: 1.0,
            },
        ];
        let high = r.vocab_candidates(EnergyBand::High);
        assert_eq!(high.len(), 1);
        assert_eq!(high[0].0, 1);
        let mid = r.vocab_candidates(EnergyBand::Mid);
        assert_eq!(
            mid.len(),
            2,
            "neighbouring bands are eligible at lower weight"
        );
        r.usage.insert(
            usage_key(&ButtonAction::RecallScene { id: "high".into() }),
            5,
        );
        let boosted = r.vocab_candidates(EnergyBand::High);
        assert!(boosted[0].1 > high[0].1);
    }
}
