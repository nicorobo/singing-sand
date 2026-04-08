use anyhow::{Context, Result};
use spectrum_analyzer::{
    FrequencyLimit, samples_fft_to_spectrum,
    scaling::divide_by_N_sqrt,
    windows::hann_window,
};
use std::path::Path;
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_NULL},
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};

/// Per-bucket frequency-band RMS values, normalised to [0, 1].
#[derive(Debug, Clone, Copy, Default)]
pub struct WaveformBucket {
    /// Low band (20–250 Hz) RMS.
    pub low: f32,
    /// Mid band (250–4 000 Hz) RMS.
    pub mid: f32,
    /// High band (4 000–20 000 Hz) RMS.
    pub high: f32,
}

impl WaveformBucket {
    /// Flatten to [low, mid, high] for DB serialisation.
    pub fn to_array(self) -> [f32; 3] {
        [self.low, self.mid, self.high]
    }
}

/// Result of a full analysis pass over one audio file.
pub struct AnalysisResult {
    /// `num_buckets` frequency-band waveform buckets.
    pub waveform: Vec<WaveformBucket>,
    /// Detected tempo in beats per minute.
    pub bpm: f32,
}

/// Decode an audio file in a single pass and return both a frequency-band
/// waveform and a BPM estimate.
///
/// Runs synchronously — call from `spawn_blocking` in async contexts.
pub fn analyze_track(path: &Path, num_buckets: usize) -> Result<AnalysisResult> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("analyze: opening {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .context("analyze: probing format")?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .context("analyze: no supported audio track")?;

    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    let channels = codec_params.channels.map(|c| c.count()).unwrap_or(2);
    let sample_rate = codec_params.sample_rate.unwrap_or(44100);

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .context("analyze: creating decoder")?;

    let total_frames = codec_params.n_frames.unwrap_or(0) as usize;
    let frames_per_bucket = if total_frames > 0 {
        (total_frames / num_buckets).max(1)
    } else {
        0
    };

    // Accumulators for band RMS per bucket:
    // Each entry is (sum_sq_low, sum_sq_mid, sum_sq_high, count).
    let mut buckets: Vec<(f64, f64, f64, u64)> = vec![(0.0, 0.0, 0.0, 0); num_buckets];

    // All mono samples collected for BPM detection.
    let mut all_mono: Vec<f32> = Vec::with_capacity(total_frames.max(1));

    // Rolling window for FFT — size must be a power of 2 ≤ 16384.
    const FFT_SIZE: usize = 2048;
    let mut fft_window: Vec<f32> = Vec::with_capacity(FFT_SIZE);

    // Running frame counter (used to assign samples to buckets).
    let mut frame_cursor: usize = 0;

    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(_) => break,
        };

        let spec = *decoded.spec();
        let mut sbuf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        sbuf.copy_interleaved_ref(decoded);
        let samples = sbuf.samples();

        let frames_in_packet = samples.len() / channels;
        for frame_idx in 0..frames_in_packet {
            // Mix down to mono.
            let mono: f32 = (0..channels)
                .map(|ch| samples[frame_idx * channels + ch])
                .sum::<f32>()
                / channels as f32;

            all_mono.push(mono);

            // Feed into FFT window.
            fft_window.push(mono);
            if fft_window.len() == FFT_SIZE {
                let bucket_center = if frames_per_bucket > 0 {
                    (frame_cursor.saturating_sub(FFT_SIZE / 2) / frames_per_bucket)
                        .min(num_buckets - 1)
                } else {
                    (frame_cursor.saturating_sub(FFT_SIZE / 2) / 1024).min(num_buckets - 1)
                };

                accumulate_fft_bands(
                    &fft_window,
                    sample_rate,
                    &mut buckets[bucket_center],
                );

                // Hop by FFT_SIZE / 2 (50% overlap).
                fft_window.drain(..FFT_SIZE / 2);
            }

            let bucket_idx = if frames_per_bucket > 0 {
                (frame_cursor / frames_per_bucket).min(num_buckets - 1)
            } else {
                (frame_cursor / 1024).min(num_buckets - 1)
            };
            buckets[bucket_idx].3 += 1;

            frame_cursor += 1;
        }
    }

    // Process any leftover samples in the FFT window (zero-pad to FFT_SIZE).
    if fft_window.len() >= 4 {
        fft_window.resize(FFT_SIZE, 0.0);
        let last_bucket = num_buckets - 1;
        accumulate_fft_bands(&fft_window, sample_rate, &mut buckets[last_bucket]);
    }

    // Compute per-band RMS from accumulated sums.
    let band_rms_raw: Vec<(f32, f32, f32)> = buckets
        .iter()
        .map(|(sl, sm, sh, count)| {
            if *count == 0 {
                (0.0, 0.0, 0.0)
            } else {
                let n = *count as f64;
                (
                    (sl / n).sqrt() as f32,
                    (sm / n).sqrt() as f32,
                    (sh / n).sqrt() as f32,
                )
            }
        })
        .collect();

    // Normalise each band independently.
    let peak_low  = band_rms_raw.iter().map(|b| b.0).fold(0.0_f32, f32::max);
    let peak_mid  = band_rms_raw.iter().map(|b| b.1).fold(0.0_f32, f32::max);
    let peak_high = band_rms_raw.iter().map(|b| b.2).fold(0.0_f32, f32::max);

    let waveform: Vec<WaveformBucket> = band_rms_raw
        .iter()
        .map(|(l, m, h)| WaveformBucket {
            low:  if peak_low  > 0.0 { l / peak_low  } else { 0.0 },
            mid:  if peak_mid  > 0.0 { m / peak_mid  } else { 0.0 },
            high: if peak_high > 0.0 { h / peak_high } else { 0.0 },
        })
        .collect();

    let bpm = detect_bpm(&all_mono, sample_rate);

    Ok(AnalysisResult { waveform, bpm })
}

// ── FFT band accumulation ──────────────────────────────────────────────────────

/// Run an FFT on `window` (must be exactly FFT_SIZE samples) and add the
/// squared magnitudes of each frequency band into `acc`.
/// `acc` = (sum_sq_low, sum_sq_mid, sum_sq_high, count).
fn accumulate_fft_bands(
    window: &[f32],
    sample_rate: u32,
    acc: &mut (f64, f64, f64, u64),
) {
    let windowed = hann_window(window);
    let Ok(spectrum) = samples_fft_to_spectrum(
        &windowed,
        sample_rate,
        FrequencyLimit::All,
        Some(&divide_by_N_sqrt),
    ) else {
        return;
    };

    let mut sum_low = 0.0f64;
    let mut cnt_low = 0u32;
    let mut sum_mid = 0.0f64;
    let mut cnt_mid = 0u32;
    let mut sum_high = 0.0f64;
    let mut cnt_high = 0u32;

    for (freq, val) in spectrum.data() {
        let f = freq.val();
        let v = val.val() as f64;
        let sq = v * v;
        if f < 250.0 {
            sum_low += sq;
            cnt_low += 1;
        } else if f < 4000.0 {
            sum_mid += sq;
            cnt_mid += 1;
        } else if f <= 20000.0 {
            sum_high += sq;
            cnt_high += 1;
        }
    }

    // Average squared magnitude per band → RMS-compatible accumulation.
    if cnt_low  > 0 { acc.0 += sum_low  / cnt_low  as f64; }
    if cnt_mid  > 0 { acc.1 += sum_mid  / cnt_mid  as f64; }
    if cnt_high > 0 { acc.2 += sum_high / cnt_high as f64; }
}

// ── BPM detection ─────────────────────────────────────────────────────────────

/// Estimate the BPM of a mono audio signal via energy-envelope autocorrelation.
///
/// Algorithm:
/// 1. Compute RMS energy in overlapping 512-sample windows.
/// 2. Extract an onset-strength signal (positive energy differences).
/// 3. Autocorrelate the onset signal over lags corresponding to 60–200 BPM.
/// 4. Return the BPM at the highest-autocorrelation lag.
fn detect_bpm(samples: &[f32], sample_rate: u32) -> f32 {
    if samples.len() < 8192 {
        return 0.0; // Too short to estimate reliably.
    }

    const WINDOW: usize = 512;
    const HOP: usize = 256;

    // Step 1: energy envelope.
    let energy: Vec<f32> = samples
        .windows(WINDOW)
        .step_by(HOP)
        .map(|w| {
            let sq_sum: f32 = w.iter().map(|s| s * s).sum();
            (sq_sum / WINDOW as f32).sqrt()
        })
        .collect();

    if energy.len() < 4 {
        return 0.0;
    }

    // Step 2: onset strength (half-wave rectified first difference).
    let onset: Vec<f32> = energy
        .windows(2)
        .map(|w| (w[1] - w[0]).max(0.0))
        .collect();

    // Step 3: autocorrelation over BPM-relevant lags.
    let frame_rate = sample_rate as f32 / HOP as f32;
    let min_bpm = 60.0_f32;
    let max_bpm = 200.0_f32;
    let min_lag = (frame_rate * 60.0 / max_bpm).round() as usize;
    let max_lag = ((frame_rate * 60.0 / min_bpm).round() as usize).min(onset.len() / 2);

    if min_lag >= max_lag {
        return 0.0;
    }

    let n = onset.len();
    let mut best_lag = min_lag;
    let mut best_val = -1.0_f32;

    for lag in min_lag..=max_lag {
        let ac: f32 = onset[..n - lag]
            .iter()
            .zip(&onset[lag..])
            .map(|(a, b)| a * b)
            .sum::<f32>()
            / (n - lag) as f32;
        if ac > best_val {
            best_val = ac;
            best_lag = lag;
        }
    }

    // Convert lag to BPM.
    let raw_bpm = frame_rate * 60.0 / best_lag as f32;

    // Fold into [60, 200] range by halving/doubling.
    fold_bpm(raw_bpm)
}

/// Fold a raw BPM estimate into the [60, 200] range by doubling or halving.
fn fold_bpm(mut bpm: f32) -> f32 {
    while bpm > 200.0 { bpm /= 2.0; }
    while bpm < 60.0  { bpm *= 2.0; }
    bpm
}
