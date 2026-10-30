//! Waveform peak extraction for the cue editor.
//!
//! Decodes the audio file VirtualDJ is playing (we get its path from
//! the cue follower) with symphonia and reduces it to a per-bucket
//! peak envelope the frontend draws on a canvas: N buckets per second
//! (50 for the compact view, up to 1000 for the zoomable editor), each
//! the max absolute amplitude across all channels, quantised to u8. A
//! 4-minute track is ~12 KB at 50/s and ~100 KB at 400/s.
//!
//! Decoding a full MP3 takes ~1-2 s of CPU, so results are cached by
//! path (small LRU) and the Tauri command runs on a blocking thread.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use ts_rs::TS;

pub const PEAKS_PER_SECOND: u32 = 50;
pub const MAX_PEAKS_PER_SECOND: u32 = 1000;
/// Cap the envelope at one hour of audio — beyond that something is
/// wrong (a DJ set file?) and we'd rather truncate than balloon.
const MAX_SECONDS: usize = 3600;
const MAX_CACHE: usize = 6;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct WaveformData {
    pub duration_secs: f64,
    /// Peak amplitude per bucket, 0..=255.
    pub peaks: Vec<u8>,
    pub peaks_per_second: u32,
}

type WaveCache = Mutex<Vec<((String, u32), Arc<WaveformData>)>>;

fn cache() -> &'static WaveCache {
    static C: OnceLock<WaveCache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(Vec::new()))
}

/// Drop every cached envelope (the cue editor module was switched off).
pub fn clear_cache() {
    cache().lock().expect("waveform cache").clear();
}

pub fn waveform_for(path: &str, pps: u32) -> Result<Arc<WaveformData>, String> {
    let pps = pps.clamp(1, MAX_PEAKS_PER_SECOND);
    {
        let g = cache().lock().expect("waveform cache");
        if let Some((_, data)) = g.iter().find(|((p, r), _)| p == path && *r == pps) {
            return Ok(data.clone());
        }
    }
    let data = Arc::new(decode(path, pps)?);
    {
        let mut g = cache().lock().expect("waveform cache");
        if g.len() >= MAX_CACHE {
            g.remove(0);
        }
        g.push(((path.to_string(), pps), data.clone()));
    }
    Ok(data)
}

fn decode(path: &str, pps: u32) -> Result<WaveformData, String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).map_err(|e| format!("no se pudo abrir {path}: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("formato no reconocido: {e}"))?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or("el archivo no tiene pista de audio")?
        .clone();
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &Default::default())
        .map_err(|e| format!("codec no soportado: {e}"))?;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44_100).max(1);
    let samples_per_peak = (sample_rate / pps).max(1) as usize;
    let max_peaks = MAX_SECONDS * pps as usize;

    let mut peaks: Vec<u8> = Vec::new();
    let mut acc_max = 0.0f32;
    let mut acc_n = 0usize;
    let mut total_frames = 0u64;
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    // Runs until EOF (or a broken tail — either way we render what we got).
    'decode: while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // Skip corrupt frames instead of giving up on the file.
            Err(_) => continue,
        };
        let spec = *decoded.spec();
        let cap = decoded.capacity() as u64;
        let needs_realloc = sample_buf
            .as_ref()
            .map(|b| b.capacity() < (cap as usize) * spec.channels.count())
            .unwrap_or(true);
        if needs_realloc {
            sample_buf = Some(SampleBuffer::<f32>::new(cap, spec));
        }
        let buf = sample_buf.as_mut().expect("sample buffer");
        buf.copy_interleaved_ref(decoded);
        let channels = spec.channels.count().max(1);
        for frame in buf.samples().chunks(channels) {
            let amp = frame.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            acc_max = acc_max.max(amp);
            acc_n += 1;
            total_frames += 1;
            if acc_n >= samples_per_peak {
                peaks.push((acc_max.min(1.0) * 255.0) as u8);
                acc_max = 0.0;
                acc_n = 0;
                if peaks.len() >= max_peaks {
                    break 'decode;
                }
            }
        }
    }
    if acc_n > 0 {
        peaks.push((acc_max.min(1.0) * 255.0) as u8);
    }
    if peaks.is_empty() {
        return Err("no se pudo decodificar ningún audio del archivo".into());
    }

    Ok(WaveformData {
        duration_secs: total_frames as f64 / sample_rate as f64,
        peaks,
        peaks_per_second: pps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal 16-bit mono PCM WAV writer for the test.
    fn write_wav(path: &Path, sample_rate: u32, samples: &[i16]) {
        let data_len = (samples.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for s in samples {
            out.extend_from_slice(&s.to_le_bytes());
        }
        std::fs::write(path, out).unwrap();
    }

    #[test]
    fn decodes_wav_into_expected_peak_envelope() {
        let dir = std::env::temp_dir().join(format!("dmx-wave-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("test.wav");
        // 2 s at 8 kHz: first second silence, second second a loud sine.
        let sr = 8_000u32;
        let mut samples = vec![0i16; sr as usize];
        for i in 0..sr as usize {
            let v = (f32::sin(i as f32 * 0.5) * 30_000.0) as i16;
            samples.push(v);
        }
        write_wav(&wav, sr, &samples);

        let w = decode(wav.to_str().unwrap(), PEAKS_PER_SECOND).unwrap();
        assert!(
            (w.duration_secs - 2.0).abs() < 0.05,
            "dur {}",
            w.duration_secs
        );
        let expected = 2 * PEAKS_PER_SECOND as usize;
        assert!(
            (w.peaks.len() as i64 - expected as i64).abs() <= 2,
            "peaks {}",
            w.peaks.len()
        );
        // First half quiet, second half loud.
        let half = w.peaks.len() / 2;
        assert!(w.peaks[..half - 1].iter().all(|&p| p < 8));
        assert!(w.peaks[half + 1..].iter().any(|&p| p > 180));

        std::fs::remove_dir_all(&dir).ok();
    }
}
