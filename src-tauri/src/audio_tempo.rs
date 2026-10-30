//! Offline tempo + beat-grid estimation from a waveform envelope, for the
//! cue editor's "Detectar BPM" button when VirtualDJ isn't around to
//! tell us the grid.
//!
//! Deliberately simple DSP: an onset-strength signal (half-wave
//! rectified derivative of the peak envelope), an autocorrelation over
//! the 60–200 BPM lag range with parabolic peak refinement, an
//! octave-fold into the 80–160 range dance music lives in, and a phase
//! search for the first beat. Good enough for four-on-the-floor and
//! most pop; the operator can always correct it by hand or tap it.

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct GridEstimate {
    pub bpm: f64,
    /// Seconds into the file where the detected beat phase starts.
    pub first_beat_secs: f64,
    /// 0..=1: how much the winning tempo stood out from the rest.
    pub confidence: f64,
}

const MIN_BPM: f64 = 60.0;
const MAX_BPM: f64 = 200.0;
/// Phase search window: enough to average out fills, short enough that
/// slight tempo drift doesn't smear the grid.
const PHASE_HORIZON_SECS: f64 = 90.0;

pub fn estimate_grid(peaks: &[u8], pps: u32) -> Option<GridEstimate> {
    if pps == 0 || peaks.len() < (pps as usize) * 8 {
        return None; // need at least a few bars of audio
    }
    let pps_f = pps as f64;
    let env: Vec<f32> = peaks.iter().map(|&p| p as f32 / 255.0).collect();
    let mut onset = vec![0f32; env.len()];
    for i in 1..env.len() {
        onset[i] = (env[i] - env[i - 1]).max(0.0);
    }
    // Widen each onset spike over ±2 buckets (real transients already span
    // several 2.5 ms buckets): keeps a period that isn't a whole number of
    // buckets from correlating at half strength.
    let mut sm = vec![0f32; onset.len()];
    for (i, out) in sm.iter_mut().enumerate() {
        let mut acc = 0.0f32;
        let mut wsum = 0.0f32;
        for d in -2i32..=2 {
            let j = i as i32 + d;
            if j >= 0 && (j as usize) < onset.len() {
                let w = 3.0 - d.abs() as f32;
                acc += onset[j as usize] * w;
                wsum += w;
            }
        }
        *out = acc / wsum;
    }
    let mean = sm.iter().sum::<f32>() / sm.len() as f32;
    let o: Vec<f32> = sm.iter().map(|v| v - mean).collect();
    let energy: f64 = o.iter().map(|v| (*v as f64) * (*v as f64)).sum();
    if energy <= 1e-9 {
        return None;
    }

    let min_lag = (pps_f * 60.0 / MAX_BPM).floor().max(1.0) as usize;
    let max_lag = (pps_f * 60.0 / MIN_BPM).ceil() as usize;
    if max_lag + 2 >= o.len() {
        return None;
    }
    // Dance/pop tempos live in 80..=160; a periodic signal correlates just
    // as well at twice its period, so the global maximum alone would happily
    // report 64 for a 128 track. Take the best lag inside the preferred
    // range unless something outside it is overwhelmingly stronger.
    let pref_min = (pps_f * 60.0 / 160.0).floor() as usize;
    let pref_max = (pps_f * 60.0 / 80.0).ceil() as usize;
    let mut rs = vec![0.0f64; max_lag + 1];
    let mut best_lag = min_lag;
    let mut best_r = f64::MIN;
    let mut pref_lag = 0usize;
    let mut pref_r = f64::MIN;
    let mut sum_r = 0.0;
    for lag in min_lag..=max_lag {
        let mut acc = 0.0f64;
        for i in 0..(o.len() - lag) {
            acc += o[i] as f64 * o[i + lag] as f64;
        }
        let r = acc / energy;
        rs[lag] = r;
        sum_r += r;
        if r > best_r {
            best_r = r;
            best_lag = lag;
        }
        if lag >= pref_min && lag <= pref_max && r > pref_r {
            pref_r = r;
            pref_lag = lag;
        }
    }
    let mean_r = sum_r / (max_lag - min_lag + 1) as f64;
    let (chosen_lag, chosen_r) = if pref_lag > 0 && pref_r >= best_r * 0.5 {
        (pref_lag, pref_r)
    } else {
        (best_lag, best_r)
    };

    // Sub-bucket refinement of the winning lag.
    let mut lag_f = chosen_lag as f64;
    if chosen_lag > min_lag && chosen_lag < max_lag {
        let (a, b, c) = (rs[chosen_lag - 1], rs[chosen_lag], rs[chosen_lag + 1]);
        let denom = a - 2.0 * b + c;
        if denom.abs() > 1e-12 {
            let delta = 0.5 * (a - c) / denom;
            if delta.abs() < 1.0 {
                lag_f += delta;
            }
        }
    }
    let bpm = pps_f * 60.0 / lag_f;
    let confidence = ((chosen_r - mean_r) / (chosen_r.abs() + 1e-9)).clamp(0.0, 1.0);

    // Phase: the offset within one period whose grid lines land on the
    // most onset energy over the opening stretch of the track.
    let period = lag_f;
    let horizon = onset.len().min((pps_f * PHASE_HORIZON_SECS) as usize);
    let steps = period.round().max(1.0) as usize;
    let mut best_phase = 0usize;
    let mut best_sum = f64::MIN;
    for phase in 0..steps {
        let mut s = 0.0f64;
        let mut k = 0.0f64;
        loop {
            let idx = (phase as f64 + k * period).round() as usize;
            if idx >= horizon {
                break;
            }
            s += onset[idx] as f64;
            k += 1.0;
        }
        if s > best_sum {
            best_sum = s;
            best_phase = phase;
        }
    }

    Some(GridEstimate {
        bpm: (bpm * 100.0).round() / 100.0,
        first_beat_secs: best_phase as f64 / pps_f,
        confidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic envelope: decaying pulses every beat over a noisy floor.
    fn click_track(pps: u32, secs: f64, bpm: f64, offset_secs: f64) -> Vec<u8> {
        let n = (pps as f64 * secs) as usize;
        let period = pps as f64 * 60.0 / bpm;
        let mut v = vec![0u8; n];
        let mut seed: u32 = 12345;
        for (i, slot) in v.iter_mut().enumerate() {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            *slot = 18 + (seed >> 28) as u8; // 18..=33 floor noise
            let _ = i;
        }
        let mut t = pps as f64 * offset_secs;
        while t < n as f64 {
            let start = t.round() as usize;
            for k in 0..12 {
                let idx = start + k;
                if idx < n {
                    let amp = 255.0 * 0.72f64.powi(k as i32);
                    v[idx] = v[idx].max(amp as u8);
                }
            }
            t += period;
        }
        v
    }

    #[test]
    fn detects_128_bpm_and_phase() {
        let peaks = click_track(400, 60.0, 128.0, 0.1);
        let g = estimate_grid(&peaks, 400).expect("estimate");
        assert!((g.bpm - 128.0).abs() < 0.6, "bpm {}", g.bpm);
        assert!(
            (g.first_beat_secs - 0.1).abs() < 0.03,
            "phase {}",
            g.first_beat_secs
        );
        assert!(g.confidence > 0.3, "confidence {}", g.confidence);
    }

    #[test]
    fn detects_96_bpm_with_half_second_offset() {
        let peaks = click_track(400, 45.0, 96.0, 0.5);
        let g = estimate_grid(&peaks, 400).expect("estimate");
        assert!((g.bpm - 96.0).abs() < 0.6, "bpm {}", g.bpm);
        assert!(
            (g.first_beat_secs - 0.5).abs() < 0.03,
            "phase {}",
            g.first_beat_secs
        );
    }

    #[test]
    fn silence_has_no_grid() {
        assert!(estimate_grid(&vec![0u8; 400 * 30], 400).is_none());
        assert!(estimate_grid(&[100u8; 100], 400).is_none());
    }
}
