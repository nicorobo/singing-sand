use anyhow::{Context, Result};
use std::path::Path;
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_NULL},
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};

/// Decode an audio file and return `num_buckets` normalised RMS values in [0, 1].
///
/// Runs synchronously — call from `spawn_blocking` in async contexts.
pub fn analyze_waveform(path: &Path, num_buckets: usize) -> Result<Vec<f32>> {
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

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .context("analyze: creating decoder")?;

    // Estimate total frames to pre-size accumulators.
    let total_frames = codec_params
        .n_frames
        .unwrap_or(0) as usize;

    let frames_per_bucket = if total_frames > 0 {
        (total_frames / num_buckets).max(1)
    } else {
        // Unknown length: accumulate all samples then downsample.
        0
    };

    let mut buckets: Vec<(f64, u64)> = vec![(0.0, 0); num_buckets]; // (sum_sq, count)
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

        // Iterate per frame (interleaved channels).
        let frames_in_packet = samples.len() / channels;
        for frame_idx in 0..frames_in_packet {
            // Mix-down channels to mono for RMS.
            let mono: f64 = (0..channels)
                .map(|ch| samples[frame_idx * channels + ch] as f64)
                .sum::<f64>()
                / channels as f64;

            let bucket_idx = if frames_per_bucket > 0 {
                (frame_cursor / frames_per_bucket).min(num_buckets - 1)
            } else {
                // Will downsample after full decode.
                (frame_cursor / 1024).min(num_buckets - 1)
            };

            buckets[bucket_idx].0 += mono * mono;
            buckets[bucket_idx].1 += 1;

            frame_cursor += 1;
        }
    }

    // Compute RMS per bucket.
    let mut rms: Vec<f32> = buckets
        .iter()
        .map(|(sum_sq, count)| {
            if *count == 0 {
                0.0_f32
            } else {
                (sum_sq / *count as f64).sqrt() as f32
            }
        })
        .collect();

    // Normalise to [0, 1] against the peak bucket.
    let peak = rms.iter().cloned().fold(0.0_f32, f32::max);
    if peak > 0.0 {
        for v in rms.iter_mut() {
            *v /= peak;
        }
    }

    Ok(rms)
}
