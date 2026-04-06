use anyhow::{Context, Result};
use rodio::Source;
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_NULL},
    errors::Error as SymphoniaError,
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
    units::Time,
};

/// The number of seconds between position-tick emissions.
const POSITION_TICK_INTERVAL_SECS: f64 = 0.1;

/// A rodio `Source` backed by a Symphonia decoder.
///
/// This is a streaming source: samples are decoded on demand as rodio's mixer
/// calls `next()`. Seek is handled by stop+restart in the engine, so no
/// seek channel is needed here.
pub struct SymphoniaSource {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    sample_rate: u32,
    channels: usize,
    /// Pre-converted f32 samples for the current decoded packet.
    buf: Vec<f32>,
    buf_pos: usize,
    /// Current playback position in seconds, shared with the engine.
    position_secs: Arc<Mutex<f64>>,
    /// Total duration in seconds, if known.
    pub duration_secs: Option<f64>,
    /// Samples consumed since last position tick.
    samples_since_tick: u64,
    samples_per_tick: u64,
}

impl SymphoniaSource {
    pub fn new(path: &Path, start_sec: f64, position_secs: Arc<Mutex<f64>>) -> Result<Self> {
        let file = std::fs::File::open(path)
            .with_context(|| format!("opening audio file: {}", path.display()))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .context("probing audio format")?;

        let format = probed.format;

        // Find the first audio track.
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .context("no supported audio track found")?;

        let track_id = track.id;
        let codec_params = track.codec_params.clone();

        let sample_rate = codec_params.sample_rate.unwrap_or(44100);
        let channels = codec_params
            .channels
            .map(|c| c.count())
            .unwrap_or(2);

        // Duration in seconds (may not be available for all formats).
        let duration_secs = codec_params
            .n_frames
            .zip(codec_params.sample_rate)
            .map(|(frames, rate)| frames as f64 / rate as f64);

        let decoder = symphonia::default::get_codecs()
            .make(&codec_params, &DecoderOptions::default())
            .context("creating decoder")?;

        let samples_per_tick = (sample_rate as f64 * POSITION_TICK_INTERVAL_SECS) as u64;

        let mut source = Self {
            format,
            decoder,
            track_id,
            sample_rate,
            channels,
            buf: Vec::new(),
            buf_pos: 0,
            position_secs,
            duration_secs,
            samples_since_tick: 0,
            samples_per_tick,
        };

        // Seek to start position if non-zero.
        if start_sec > 0.0 {
            source.seek_to(start_sec)?;
        }

        Ok(source)
    }

    fn seek_to(&mut self, secs: f64) -> Result<()> {
        self.format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time: Time::from(secs),
                track_id: Some(self.track_id),
            },
        )?;
        self.decoder.reset();
        self.buf.clear();
        self.buf_pos = 0;
        *self.position_secs.lock().unwrap() = secs;
        Ok(())
    }

    /// Decode the next packet and fill `self.buf`. Returns false if EOF.
    fn decode_next_packet(&mut self) -> bool {
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(SymphoniaError::IoError(_)) | Err(SymphoniaError::ResetRequired) => {
                    return false;
                }
                Err(_) => return false,
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let mut sample_buf =
                        SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                    sample_buf.copy_interleaved_ref(decoded);
                    self.buf = sample_buf.samples().to_vec();
                    self.buf_pos = 0;
                    return true;
                }
                Err(SymphoniaError::DecodeError(_)) => {
                    // Non-fatal: skip the packet.
                    continue;
                }
                Err(_) => return false,
            }
        }
    }
}

impl Iterator for SymphoniaSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        // Refill buffer when exhausted.
        if self.buf_pos >= self.buf.len() {
            if !self.decode_next_packet() {
                return None;
            }
        }

        let sample = self.buf[self.buf_pos];
        self.buf_pos += 1;

        // Update position counter every `samples_per_tick` samples.
        // We divide by channels because position is per-frame, not per-sample.
        self.samples_since_tick += 1;
        if self.samples_since_tick >= self.samples_per_tick * self.channels as u64 {
            self.samples_since_tick = 0;
            // Approximate position from buffer offset within the current packet.
            // A more accurate approach is to track the packet's timestamp; this
            // is a good-enough approximation for UI display.
            let frames_played = {
                let mut pos = self.position_secs.lock().unwrap();
                *pos += POSITION_TICK_INTERVAL_SECS;
                *pos
            };
            let _ = frames_played; // used via the lock above
        }

        Some(sample)
    }
}

impl Source for SymphoniaSource {
    fn current_frame_len(&self) -> Option<usize> {
        let remaining = self.buf.len().saturating_sub(self.buf_pos);
        if remaining == 0 {
            None
        } else {
            Some(remaining / self.channels)
        }
    }

    fn channels(&self) -> u16 {
        self.channels as u16
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        self.duration_secs.map(Duration::from_secs_f64)
    }
}
