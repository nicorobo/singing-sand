use anyhow::{Context, Result};
use realfft::RealFftPlanner;
use realfft::num_complex::Complex;
use std::path::Path;
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_NULL},
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};

pub use ss_waveform::WaveformBucket;

/// Result of a full analysis pass over one audio file.
pub struct AnalysisResult {
    /// `num_buckets` frequency-band waveform buckets.
    pub waveform: Vec<WaveformBucket>,
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

    // Rolling window for FFT — size must be a power of 2 ≤ 16384.
    const FFT_SIZE: usize = 2048;
    let mut fft_window: Vec<f32> = Vec::with_capacity(FFT_SIZE);

    // Reusable FFT plan + output/scratch buffers (allocated once, reused per window).
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let mut fft_output = fft.make_output_vec();
    let mut fft_scratch = fft.make_scratch_vec();

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
                    &fft,
                    &mut fft_output,
                    &mut fft_scratch,
                    &mut buckets[bucket_center],
                );

                // No overlap — clear the window entirely (2× fewer FFTs vs 50% hop).
                fft_window.clear();
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
        accumulate_fft_bands(
            &fft_window,
            sample_rate,
            &fft,
            &mut fft_output,
            &mut fft_scratch,
            &mut buckets[last_bucket],
        );
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

    Ok(AnalysisResult { waveform })
}

// ── FFT band accumulation ──────────────────────────────────────────────────────

/// Run an FFT on `window` (must be exactly FFT_SIZE samples) and add the
/// squared magnitudes of each frequency band into `acc`.
/// `acc` = (sum_sq_low, sum_sq_mid, sum_sq_high, count).
///
/// Uses a real-valued FFT (N/2+1 complex outputs) for ~2× faster computation
/// vs a full complex FFT.
fn accumulate_fft_bands(
    window: &[f32],
    sample_rate: u32,
    fft: &std::sync::Arc<dyn realfft::RealToComplex<f32>>,
    output: &mut Vec<Complex<f32>>,
    scratch: &mut Vec<Complex<f32>>,
    acc: &mut (f64, f64, f64, u64),
) {
    let n = window.len();
    // Apply Hann window into a mutable copy (realfft modifies input in-place).
    let mut input: Vec<f32> = window
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let w = 0.5
                * (1.0
                    - (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos());
            s * w
        })
        .collect();

    if fft.process_with_scratch(&mut input, output, scratch).is_err() {
        return;
    }

    let freq_resolution = sample_rate as f32 / n as f32;

    let mut sum_low = 0.0f64;
    let mut cnt_low = 0u32;
    let mut sum_mid = 0.0f64;
    let mut cnt_mid = 0u32;
    let mut sum_high = 0.0f64;
    let mut cnt_high = 0u32;

    for (i, c) in output.iter().enumerate() {
        let f = i as f32 * freq_resolution;
        let power = c.norm_sqr() as f64;
        if f < 250.0 {
            sum_low += power;
            cnt_low += 1;
        } else if f < 4000.0 {
            sum_mid += power;
            cnt_mid += 1;
        } else if f <= 20000.0 {
            sum_high += power;
            cnt_high += 1;
        }
    }

    // Average squared magnitude per band → RMS-compatible accumulation.
    if cnt_low  > 0 { acc.0 += sum_low  / cnt_low  as f64; }
    if cnt_mid  > 0 { acc.1 += sum_mid  / cnt_mid  as f64; }
    if cnt_high > 0 { acc.2 += sum_high / cnt_high as f64; }
}

