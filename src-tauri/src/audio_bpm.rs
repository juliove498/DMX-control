//! Microphone / line-in BPM counter.
//!
//! Pipeline:
//! 1. cpal captures the selected input device (mono-mixed to f32) and
//!    pushes chunks over a bounded channel — the audio callback does
//!    nothing else, so it can never glitch the driver.
//! 2. A worker thread runs a **multiband spectral-flux onset detector**:
//!    2048-point Hann/FFT frames every 512 samples, grouped into ~24
//!    log-spaced bands (60 Hz–8 kHz); the onset envelope is the mean of
//!    per-band positive `ln(energy)` differences. Two properties matter:
//!    - *Gain invariance*: input level scales every band multiplicatively,
//!      so the log differences don't move — a whisper-level laptop mic
//!      and a hot line feed produce the same envelope.
//!    - *Spectral indifference*: every band votes equally, so the beat
//!      is caught whether it lives in a club kick, in the mids of a
//!      phone speaker (which reproduces nothing under ~400 Hz), or in
//!      hi-hats only.
//! 3. Every ~500 ms the envelope is autocorrelated over the last ~8 s
//!    (mean/variance normalised — again gain invariant) with harmonic
//!    scoring (lag + 2·lag + 3·lag) so the true period beats its
//!    subdivisions; octave folding prefers the 82–165 range DJs play,
//!    and a parabolic fit sharpens the lag to sub-frame precision.
//! 4. A short median filter stabilises the reported BPM; a confidence
//!    score (autocorrelation peak height) gates auto-apply so silence
//!    or chatter never drags the Overall BPM around.
//!
//! The only absolute threshold in the chain is a −60 dBFS silence gate:
//! below that there is genuinely nothing to track.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use ts_rs::TS;

use crate::engine::output_thread::SharedGlobals;
use crate::show::ShowState;

pub const MIN_BPM: f32 = 60.0;
pub const MAX_BPM: f32 = 200.0;
/// Analysis hop in samples. At 48 kHz → ~93.7 envelope frames/second.
const HOP: usize = 512;
/// FFT frame length (Hann window, 75% overlap at HOP=512).
const FRAME: usize = 2048;
/// Log-spaced analysis bands between [`BAND_LO_HZ`] and [`BAND_HI_HZ`].
const BANDS: usize = 24;
const BAND_LO_HZ: f32 = 60.0;
const BAND_HI_HZ: f32 = 8_000.0;
/// Envelope history used by the autocorrelation, in seconds.
const ENV_WINDOW_S: f32 = 8.0;
/// RMS below this is treated as "no signal" (−60 dBFS).
const SILENCE_RMS: f32 = 0.001;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct AudioInputInfo {
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../bindings/")]
pub struct AudioBpmStatus {
    pub running: bool,
    pub device: Option<String>,
    /// Smoothed estimate, one decimal. `None` until confident enough
    /// (or while the input is silent).
    pub bpm: Option<f32>,
    /// 0..1 — autocorrelation peak height of the last estimate.
    pub confidence: f32,
    /// 0..1 input meter (−50..0 dBFS mapped linearly).
    pub level: f32,
    /// True for ~150 ms after each detected onset — UI beat flash.
    pub beat: bool,
    /// While true the worker writes confident estimates straight into
    /// the Overall BPM (rounded to 0.1, with hysteresis).
    pub auto_apply: bool,
    pub error: Option<String>,
}

#[derive(Default)]
struct SharedInner {
    running: bool,
    device: Option<String>,
    bpm: Option<f32>,
    confidence: f32,
    level: f32,
    last_beat: Option<Instant>,
    auto_apply: bool,
    error: Option<String>,
}

#[derive(Default)]
pub struct AudioBpmRuntime {
    shared: Arc<Mutex<SharedInner>>,
    stop: Option<Arc<AtomicBool>>,
}

pub type SharedAudioBpm = Arc<Mutex<AudioBpmRuntime>>;

pub fn shared_audio_bpm() -> SharedAudioBpm {
    Arc::new(Mutex::new(AudioBpmRuntime::default()))
}

pub fn list_input_devices() -> Vec<AudioInputInfo> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();
    let mut out = Vec::new();
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            if let Ok(name) = d.name() {
                let is_default = name == default_name;
                out.push(AudioInputInfo { name, is_default });
            }
        }
    }
    out
}

pub fn status(state: &SharedAudioBpm) -> AudioBpmStatus {
    let rt = state.lock();
    let s = rt.shared.lock();
    AudioBpmStatus {
        running: s.running,
        device: s.device.clone(),
        bpm: s.bpm,
        confidence: s.confidence,
        level: s.level,
        beat: s
            .last_beat
            .map(|t| t.elapsed() < Duration::from_millis(150))
            .unwrap_or(false),
        auto_apply: s.auto_apply,
        error: s.error.clone(),
    }
}

pub fn set_auto_apply(state: &SharedAudioBpm, enabled: bool) {
    let rt = state.lock();
    rt.shared.lock().auto_apply = enabled;
}

pub fn stop(state: &SharedAudioBpm) {
    let mut rt = state.lock();
    if let Some(flag) = rt.stop.take() {
        flag.store(true, Ordering::Relaxed);
    }
    let mut s = rt.shared.lock();
    s.running = false;
    s.bpm = None;
    s.confidence = 0.0;
    s.level = 0.0;
}

/// Start listening on `device_name` (or the system default input).
/// Spawns a dedicated thread that owns the cpal stream (`Stream` is
/// !Send, so it must be created and dropped on the same thread).
pub fn start(
    app: AppHandle,
    show: ShowState,
    globals: SharedGlobals,
    state: &SharedAudioBpm,
    device_name: Option<String>,
) {
    stop(state);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let shared = {
        let mut rt = state.lock();
        rt.stop = Some(stop_flag.clone());
        let mut s = rt.shared.lock();
        s.running = true;
        s.error = None;
        s.device = device_name.clone();
        drop(s);
        rt.shared.clone()
    };

    std::thread::Builder::new()
        .name("dmx-audio-bpm".into())
        .spawn(move || {
            if let Err(err) = run_listener(&app, &show, &globals, &shared, &stop_flag, device_name)
            {
                tracing::warn!(%err, "audio bpm listener stopped with error");
                let mut s = shared.lock();
                s.error = Some(err);
                s.running = false;
            }
        })
        .expect("spawn audio bpm thread");
}

fn run_listener(
    app: &AppHandle,
    show: &ShowState,
    globals: &SharedGlobals,
    shared: &Arc<Mutex<SharedInner>>,
    stop_flag: &Arc<AtomicBool>,
    device_name: Option<String>,
) -> Result<(), String> {
    let host = cpal::default_host();
    let device = match &device_name {
        Some(wanted) => host
            .input_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().map(|n| &n == wanted).unwrap_or(false))
            .ok_or_else(|| format!("input device '{wanted}' not found"))?,
        None => host
            .default_input_device()
            .ok_or_else(|| "no default input device".to_string())?,
    };
    let resolved_name = device.name().unwrap_or_else(|_| "?".into());
    shared.lock().device = Some(resolved_name.clone());

    let config = device.default_input_config().map_err(|e| e.to_string())?;
    let sample_rate = config.sample_rate().0 as f32;
    let channels = config.channels() as usize;

    // Bounded channel: if the worker ever falls behind, the callback
    // drops chunks instead of blocking the audio driver.
    let (tx, rx) = crossbeam_channel::bounded::<Vec<f32>>(64);
    let err_flag: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let err_for_cb = err_flag.clone();
    let err_fn = move |e: cpal::StreamError| {
        *err_for_cb.lock() = Some(e.to_string());
    };

    macro_rules! build {
        ($t:ty, $conv:expr) => {{
            let tx = tx.clone();
            let conv = $conv;
            device
                .build_input_stream(
                    &config.clone().into(),
                    move |data: &[$t], _| {
                        let mono: Vec<f32> = data
                            .chunks(channels.max(1))
                            .map(|frame| {
                                frame.iter().map(|s| conv(*s)).sum::<f32>()
                                    / frame.len().max(1) as f32
                            })
                            .collect();
                        let _ = tx.try_send(mono);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?
        }};
    }
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build!(f32, |s: f32| s),
        cpal::SampleFormat::I16 => build!(i16, |s: i16| s as f32 / 32_768.0),
        cpal::SampleFormat::U16 => build!(u16, |s: u16| (s as f32 - 32_768.0) / 32_768.0),
        other => return Err(format!("unsupported sample format {other:?}")),
    };
    stream.play().map_err(|e| e.to_string())?;
    tracing::info!(device = %resolved_name, sample_rate, channels, "audio bpm listener started");

    let mut detector = OnsetDetector::new(sample_rate);
    let mut estimates: VecDeque<f32> = VecDeque::with_capacity(5);
    let mut last_estimate_at = Instant::now();
    let mut last_applied: f32 = 0.0;

    while !stop_flag.load(Ordering::Relaxed) {
        if let Some(e) = err_flag.lock().take() {
            return Err(e);
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => {
                if detector.process(&chunk) {
                    shared.lock().last_beat = Some(Instant::now());
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }

        if last_estimate_at.elapsed() >= Duration::from_millis(500) {
            last_estimate_at = Instant::now();
            let level = detector.level();
            let silent = detector.is_silent();
            let est = if silent {
                None
            } else {
                estimate_bpm(&detector.envelope(), detector.env_rate())
            };
            let mut s = shared.lock();
            s.level = level;
            match est {
                Some((bpm, conf)) if conf >= 0.2 => {
                    if estimates.len() == 5 {
                        estimates.pop_front();
                    }
                    estimates.push_back(bpm);
                    let mut sorted: Vec<f32> = estimates.iter().copied().collect();
                    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    let median = sorted[sorted.len() / 2];
                    let rounded = (median * 10.0).round() / 10.0;
                    s.bpm = Some(rounded);
                    s.confidence = conf;
                    let auto = s.auto_apply;
                    drop(s);
                    // Auto-apply with hysteresis: confident, and moved
                    // at least 0.3 BPM since the last write — keeps the
                    // show file from being re-persisted twice a second.
                    if auto && conf >= 0.5 && (rounded - last_applied).abs() >= 0.3 {
                        last_applied = rounded;
                        if let Err(err) =
                            crate::commands::set_overall_bpm_impl(app, show, globals, rounded)
                        {
                            tracing::warn!(?err, "audio bpm auto-apply failed");
                        }
                    }
                }
                _ => {
                    if silent {
                        estimates.clear();
                        s.bpm = None;
                    }
                    s.confidence = 0.0;
                }
            }
        }
    }
    drop(stream);
    shared.lock().running = false;
    tracing::info!("audio bpm listener stopped");
    Ok(())
}

// ---- DSP: onset envelope ---------------------------------------------------

pub struct OnsetDetector {
    sample_rate: f32,
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    window: Vec<f32>,
    /// Rolling frame of the last `FRAME` mono samples.
    buf: VecDeque<f32>,
    hop_fill: usize,
    /// Half-open FFT-bin ranges per analysis band.
    band_bins: Vec<(usize, usize)>,
    prev_log_bands: Vec<f32>,
    have_prev: bool,
    env: VecDeque<f32>,
    env_cap: usize,
    rms_acc: f64,
    rms_fill: usize,
    rms_smooth: f32,
    frame_count: usize,
    last_beat_frame: usize,
    scratch: Vec<rustfft::num_complex::Complex<f32>>,
}

impl OnsetDetector {
    pub fn new(sample_rate: f32) -> Self {
        let env_rate = sample_rate / HOP as f32;
        let fft = rustfft::FftPlanner::new().plan_fft_forward(FRAME);
        let window: Vec<f32> = (0..FRAME)
            .map(|i| {
                let t = i as f32 / (FRAME - 1) as f32;
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * t).cos()
            })
            .collect();
        // Log-spaced band edges → bin ranges. Every band gets at least
        // one bin; bands beyond Nyquist are dropped (low sample rates).
        let hi = BAND_HI_HZ.min(sample_rate * 0.45);
        let bin_of = |f: f32| ((f * FRAME as f32 / sample_rate) as usize).max(1);
        let mut band_bins: Vec<(usize, usize)> = Vec::with_capacity(BANDS);
        let ratio = hi / BAND_LO_HZ;
        let mut start = bin_of(BAND_LO_HZ);
        for k in 1..=BANDS {
            let edge = BAND_LO_HZ * ratio.powf(k as f32 / BANDS as f32);
            let mut end = bin_of(edge).max(start + 1);
            end = end.min(FRAME / 2);
            if start >= FRAME / 2 {
                break;
            }
            band_bins.push((start, end));
            start = end;
        }
        let n_bands = band_bins.len();
        Self {
            sample_rate,
            fft,
            window,
            buf: VecDeque::with_capacity(FRAME),
            hop_fill: 0,
            band_bins,
            prev_log_bands: vec![0.0; n_bands],
            have_prev: false,
            env: VecDeque::new(),
            env_cap: (env_rate * (ENV_WINDOW_S + 2.0)) as usize,
            rms_acc: 0.0,
            rms_fill: 0,
            rms_smooth: 0.0,
            frame_count: 0,
            last_beat_frame: 0,
            scratch: vec![rustfft::num_complex::Complex::new(0.0, 0.0); FRAME],
        }
    }

    pub fn env_rate(&self) -> f32 {
        self.sample_rate / HOP as f32
    }

    pub fn envelope(&self) -> Vec<f32> {
        self.env.iter().copied().collect()
    }

    /// Smoothed input level for the UI meter, 0..1 over −50..0 dBFS.
    pub fn level(&self) -> f32 {
        let db = 20.0 * (self.rms_smooth.max(1e-6)).log10();
        ((db + 50.0) / 50.0).clamp(0.0, 1.0)
    }

    pub fn is_silent(&self) -> bool {
        self.rms_smooth < SILENCE_RMS
    }

    /// Feed mono samples. Returns true when a beat tick (onset above the
    /// adaptive threshold) fired inside this chunk.
    pub fn process(&mut self, samples: &[f32]) -> bool {
        let mut beat = false;
        for &s in samples {
            if self.buf.len() == FRAME {
                self.buf.pop_front();
            }
            self.buf.push_back(s);
            self.rms_acc += (s as f64) * (s as f64);
            self.rms_fill += 1;
            self.hop_fill += 1;
            if self.hop_fill == HOP {
                self.hop_fill = 0;
                if self.buf.len() == FRAME {
                    beat |= self.finish_frame();
                }
            }
        }
        beat
    }

    fn finish_frame(&mut self) -> bool {
        const EPS: f32 = 1e-12;
        // Input meter from the raw time domain (accumulated since the
        // previous frame).
        if self.rms_fill > 0 {
            let rms = ((self.rms_acc / self.rms_fill as f64) as f32).sqrt();
            self.rms_smooth = 0.9 * self.rms_smooth + 0.1 * rms;
            self.rms_acc = 0.0;
            self.rms_fill = 0;
        }

        // Windowed FFT of the rolling frame.
        for (i, (c, s)) in self.scratch.iter_mut().zip(self.buf.iter()).enumerate() {
            c.re = s * self.window[i];
            c.im = 0.0;
        }
        self.fft.process(&mut self.scratch);

        // Multiband positive log-energy flux. Gain shifts every band's
        // log by the same constant, so the difference is level-proof;
        // averaging across bands means kick-heavy, mid-heavy (phone
        // speaker) and hat-only material all light the envelope up.
        let mut flux = 0.0f32;
        for (b, &(lo, hi)) in self.band_bins.iter().enumerate() {
            let mut e = 0.0f32;
            for bin in lo..hi {
                e += self.scratch[bin].norm_sqr();
            }
            let l = (e + EPS).ln();
            if self.have_prev {
                flux += (l - self.prev_log_bands[b]).max(0.0);
            }
            self.prev_log_bands[b] = l;
        }
        flux /= self.band_bins.len().max(1) as f32;
        if !self.have_prev {
            self.have_prev = true;
            flux = 0.0;
        }

        if self.env.len() == self.env_cap {
            self.env.pop_front();
        }
        self.env.push_back(flux);
        self.frame_count += 1;

        // Beat tick: flux above mean + 2σ of the last ~1.5 s, with a
        // 250 ms refractory so one kick doesn't double-fire. Purely
        // cosmetic (UI flash) — tempo comes from the autocorrelation.
        let env_rate = self.env_rate();
        let win = (env_rate * 1.5) as usize;
        if self.env.len() > win && !self.is_silent() {
            let recent: Vec<f32> = self.env.iter().rev().take(win).copied().collect();
            let mean = recent.iter().sum::<f32>() / recent.len() as f32;
            let var =
                recent.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / recent.len() as f32;
            let thresh = mean + 2.0 * var.sqrt();
            let refractory = (env_rate * 0.25) as usize;
            if flux > thresh && flux > 0.02 && self.frame_count - self.last_beat_frame > refractory
            {
                self.last_beat_frame = self.frame_count;
                return true;
            }
        }
        false
    }
}

// ---- DSP: tempo estimation --------------------------------------------------

/// Autocorrelate the onset envelope and return `(bpm, confidence)`.
/// The envelope is mean/variance normalised first, so the result is
/// independent of input gain by construction.
pub fn estimate_bpm(env: &[f32], env_rate: f32) -> Option<(f32, f32)> {
    let n = env.len();
    if (n as f32) < env_rate * 4.0 {
        return None; // need ≥4 s of context
    }
    let mean = env.iter().sum::<f32>() / n as f32;
    let var = env.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n as f32;
    if var < 1e-12 {
        return None; // flat envelope — silence or constant tone
    }
    let std = var.sqrt();
    let x: Vec<f32> = env.iter().map(|v| (v - mean) / std).collect();

    let min_lag = (env_rate * 60.0 / MAX_BPM).floor().max(2.0) as usize;
    let max_lag = ((env_rate * 60.0 / MIN_BPM).ceil() as usize).min(n / 2);
    if max_lag <= min_lag + 2 {
        return None;
    }

    // Autocorrelation out to 3× the slowest period (capped so every
    // lag still overlaps at least ~1 s of envelope) — the harmonic
    // terms below read corr at 2·lag and 3·lag.
    let ext_max = (3 * max_lag).min(n.saturating_sub(env_rate as usize));
    let mut corr = vec![0.0f32; ext_max + 2];
    for lag in min_lag..=ext_max {
        let m = n - lag;
        let c = (0..m).map(|i| x[i] * x[i + lag]).sum::<f32>() / m as f32;
        corr[lag] = c;
    }

    // Harmonic scoring: real tempos also correlate at their multiples
    // (2 bars later is still on the grid), while spurious subdivision
    // peaks (eighth notes) don't get reinforced the same way. This is
    // what keeps sparse, bass-less material (phone speakers) from
    // locking onto double-time hats.
    let at = |lag: usize| if lag <= ext_max { corr[lag] } else { 0.0 };
    let score_of = |lag: usize| corr[lag] + 0.5 * at(2 * lag) + 0.25 * at(3 * lag);
    let mut best_lag = 0usize;
    let mut best_score = f32::MIN;
    for lag in min_lag..=max_lag {
        let s = score_of(lag);
        if s > best_score {
            best_score = s;
            best_lag = lag;
        }
    }
    if best_score <= 0.0 || corr[best_lag] <= 0.0 {
        return None;
    }

    // Octave disambiguation: fold extremes back into the 82–165 range
    // DJs play — a "70 BPM" peak whose half-lag also correlates is
    // really 140.
    let bpm_of = |lag: f32| env_rate * 60.0 / lag;
    let mut lag = best_lag;
    if bpm_of(lag as f32) < 82.0 {
        let half = lag / 2;
        if half >= min_lag && corr[half] > 0.6 * corr[lag] {
            lag = half;
        }
    } else if bpm_of(lag as f32) > 165.0 {
        let dbl = lag * 2;
        if dbl <= max_lag && corr[dbl] > 0.8 * corr[lag] {
            lag = dbl;
        }
    }

    // Parabolic interpolation over the score for sub-frame lag
    // precision (~±0.1 BPM at 128).
    let refined = if lag > min_lag && lag < max_lag {
        let (a, b, c) = (score_of(lag - 1), score_of(lag), score_of(lag + 1));
        let denom = a - 2.0 * b + c;
        if denom.abs() > 1e-9 {
            lag as f32 + 0.5 * (a - c) / denom
        } else {
            lag as f32
        }
    } else {
        lag as f32
    };

    let bpm = bpm_of(refined).clamp(MIN_BPM, MAX_BPM);
    Some((bpm, corr[lag].clamp(0.0, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// Synthesize `seconds` of audio with a kick-like burst (60 Hz sine,
    /// exponential decay) on every beat at `bpm`, scaled by `gain`, over
    /// a noise floor well below the bursts.
    fn synth_kicks(bpm: f32, seconds: f32, gain: f32) -> Vec<f32> {
        let total = (SR * seconds) as usize;
        let period = (SR * 60.0 / bpm) as usize;
        let burst_len = (SR * 0.09) as usize;
        let mut out = vec![0.0f32; total];
        // Deterministic pseudo-noise floor at −40 dB relative to gain.
        let mut seed: u32 = 0x1234_5678;
        for (i, o) in out.iter_mut().enumerate() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = ((seed >> 16) as f32 / 32_768.0 - 1.0) * gain * 0.01;
            *o = noise;
            let pos = i % period;
            if pos < burst_len {
                let t = pos as f32 / SR;
                let envl = (-t * 40.0).exp();
                *o += gain * envl * (2.0 * std::f32::consts::PI * 60.0 * t).sin();
            }
        }
        out
    }

    fn detect(audio: &[f32]) -> Option<(f32, f32)> {
        let mut det = OnsetDetector::new(SR);
        det.process(audio);
        assert!(!det.is_silent(), "synth signal must clear the gate");
        estimate_bpm(&det.envelope(), det.env_rate())
    }

    #[test]
    fn detects_128_bpm_at_full_level() {
        let audio = synth_kicks(128.0, 9.0, 0.9);
        let (bpm, conf) = detect(&audio).expect("estimate");
        assert!((bpm - 128.0).abs() < 2.0, "got {bpm}");
        assert!(conf > 0.3, "confidence {conf}");
    }

    #[test]
    fn detects_same_bpm_at_whisper_level() {
        // 30 dB quieter than the full-level case — the log-flux front
        // end and normalised autocorrelation must land on the same
        // tempo. This is the "works at different intensities" contract.
        let loud = detect(&synth_kicks(128.0, 9.0, 0.9)).expect("loud");
        let quiet = detect(&synth_kicks(128.0, 9.0, 0.028)).expect("quiet");
        assert!(
            (loud.0 - quiet.0).abs() < 1.0,
            "loud {} vs quiet {}",
            loud.0,
            quiet.0
        );
        assert!((quiet.0 - 128.0).abs() < 2.0, "quiet got {}", quiet.0);
    }

    #[test]
    fn detects_slow_and_fast_tempos() {
        let (bpm, _) = detect(&synth_kicks(90.0, 10.0, 0.5)).expect("90");
        assert!((bpm - 90.0).abs() < 2.0, "got {bpm}");
        let (bpm, _) = detect(&synth_kicks(174.0, 9.0, 0.5)).expect("174");
        // Accept the tempo or its half — 174 sits above the fold window,
        // but it must never come back as something unrelated.
        assert!(
            (bpm - 174.0).abs() < 3.0 || (bpm - 87.0).abs() < 2.0,
            "got {bpm}"
        );
    }

    /// Phone-speaker simulation: NOTHING below ~400 Hz. Beats are a
    /// 1.4 kHz click; off-beat eighth notes are a quieter 6 kHz "hat";
    /// a sustained 800 Hz tone masks the mix like a melody would.
    fn synth_phone_speaker(bpm: f32, seconds: f32, gain: f32) -> Vec<f32> {
        let total = (SR * seconds) as usize;
        let period = (SR * 60.0 / bpm) as usize;
        let click_len = (SR * 0.03) as usize;
        let mut out = vec![0.0f32; total];
        for (i, o) in out.iter_mut().enumerate() {
            let t_abs = i as f32 / SR;
            // Melody-ish sustained tone, well above phone HP cutoff.
            *o = gain * 0.15 * (2.0 * std::f32::consts::PI * 800.0 * t_abs).sin();
            let pos = i % period;
            if pos < click_len {
                let t = pos as f32 / SR;
                let envl = (-t * 120.0).exp();
                *o += gain * envl * (2.0 * std::f32::consts::PI * 1_400.0 * t).sin();
            }
            // Off-beat hats at half strength (eighth-note grid).
            let pos8 = (i + period / 2) % period;
            if pos8 < click_len / 2 {
                let t = pos8 as f32 / SR;
                let envl = (-t * 200.0).exp();
                *o += gain * 0.4 * envl * (2.0 * std::f32::consts::PI * 6_000.0 * t).sin();
            }
        }
        out
    }

    #[test]
    fn detects_bpm_from_bassless_phone_speaker_audio() {
        // The original two-band detector weighted a <150 Hz kick band —
        // exactly what a phone speaker cannot reproduce. The multiband
        // spectral flux must find the tempo from mids/highs alone, at
        // full level AND 26 dB quieter.
        let loud = detect(&synth_phone_speaker(128.0, 10.0, 0.6)).expect("loud phone");
        assert!((loud.0 - 128.0).abs() < 2.0, "loud got {}", loud.0);
        let quiet = detect(&synth_phone_speaker(128.0, 10.0, 0.03)).expect("quiet phone");
        assert!((quiet.0 - 128.0).abs() < 2.0, "quiet got {}", quiet.0);
        assert!(
            (loud.0 - quiet.0).abs() < 1.0,
            "loud {} vs quiet {}",
            loud.0,
            quiet.0
        );
    }

    #[test]
    fn silence_reports_nothing() {
        let audio = vec![0.0f32; (SR * 6.0) as usize];
        let mut det = OnsetDetector::new(SR);
        det.process(&audio);
        assert!(det.is_silent());
        // Flat envelope must not fabricate a tempo either.
        assert!(estimate_bpm(&det.envelope(), det.env_rate()).is_none());
    }

    #[test]
    fn short_context_reports_nothing() {
        let audio = synth_kicks(128.0, 2.0, 0.9);
        let mut det = OnsetDetector::new(SR);
        det.process(&audio);
        assert!(estimate_bpm(&det.envelope(), det.env_rate()).is_none());
    }

    #[test]
    fn beat_ticks_fire_at_both_levels() {
        for gain in [0.9f32, 0.03] {
            let audio = synth_kicks(120.0, 6.0, gain);
            let mut det = OnsetDetector::new(SR);
            let mut ticks = 0;
            for chunk in audio.chunks(2048) {
                if det.process(chunk) {
                    ticks += 1;
                }
            }
            // 6 s at 120 BPM = 12 beats; the first ~1.5 s warm the
            // adaptive threshold up, so expect at least half of them.
            assert!(ticks >= 6, "gain {gain}: only {ticks} ticks");
        }
    }
}
