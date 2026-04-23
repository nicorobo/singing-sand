use crate::settings::{ColorScheme, DisplayStyle, NormalizeMode, WaveformRenderSettings};
use crate::types::{ViewPort, WaveformBucket};

const BG: [u8; 3] = [24, 24, 37];

/// Render waveform band data into a raw RGB pixel buffer (3 bytes per pixel, row-major).
pub fn render_to_pixels(
    data: &[WaveformBucket],
    settings: &WaveformRenderSettings,
    viewport: ViewPort,
) -> Vec<u8> {
    let w = viewport.width as usize;
    let h = viewport.height as usize;

    let mut buf = vec![0u8; w * h * 3];
    for px in buf.chunks_exact_mut(3) {
        px.copy_from_slice(&BG);
    }

    if data.is_empty() || w == 0 || h == 0 {
        return buf;
    }

    let start = (data.len() as f32 * viewport.start_pct) as usize;
    let end = (data.len() as f32 * viewport.end_pct) as usize;
    let end = end.min(data.len());
    let slice = if start >= end { return buf; } else { &data[start..end] };

    let normalized: Vec<WaveformBucket>;
    let slice: &[WaveformBucket] = match settings.normalize_mode {
        NormalizeMode::None => slice,
        NormalizeMode::PerBand => {
            let peak_low  = slice.iter().map(|b| b.low).fold(0.0_f32, f32::max);
            let peak_mid  = slice.iter().map(|b| b.mid).fold(0.0_f32, f32::max);
            let peak_high = slice.iter().map(|b| b.high).fold(0.0_f32, f32::max);
            normalized = slice.iter().map(|b| WaveformBucket {
                low:  if peak_low  > 0.0 { b.low  / peak_low  } else { 0.0 },
                mid:  if peak_mid  > 0.0 { b.mid  / peak_mid  } else { 0.0 },
                high: if peak_high > 0.0 { b.high / peak_high } else { 0.0 },
            }).collect();
            &normalized
        }
        NormalizeMode::Global => {
            let peak = slice.iter().flat_map(|b| [b.low, b.mid, b.high]).fold(0.0_f32, f32::max);
            normalized = slice.iter().map(|b| WaveformBucket {
                low:  if peak > 0.0 { b.low  / peak } else { 0.0 },
                mid:  if peak > 0.0 { b.mid  / peak } else { 0.0 },
                high: if peak > 0.0 { b.high / peak } else { 0.0 },
            }).collect();
            &normalized
        }
    };

    let slice_len = slice.len();

    for x in 0..w {
        let bucket = (x * slice_len) / w;
        let b = if settings.smoothing <= 1 {
            slice.get(bucket).copied().unwrap_or_default()
        } else {
            let half = settings.smoothing as usize / 2;
            let lo = bucket.saturating_sub(half);
            let hi = (bucket + half).min(slice_len - 1);
            let count = (hi - lo + 1) as f32;
            let sum = slice[lo..=hi].iter().fold(WaveformBucket::default(), |acc, b| WaveformBucket {
                low:  acc.low  + b.low,
                mid:  acc.mid  + b.mid,
                high: acc.high + b.high,
            });
            WaveformBucket { low: sum.low / count, mid: sum.mid / count, high: sum.high / count }
        };

        let low  = (b.low  * settings.low_gain).clamp(0.0, 1.0);
        let mid  = (b.mid  * settings.mid_gain).clamp(0.0, 1.0);
        let high = (b.high * settings.high_gain).clamp(0.0, 1.0);

        let peak = low.max(mid).max(high);
        let amplitude = (peak.powf(settings.gamma) * settings.amplitude_scale).clamp(0.0, 1.0);
        if amplitude <= settings.noise_floor {
            continue;
        }

        let color = pick_color(low, mid, high, amplitude, &settings.color_scheme);

        match settings.display_style {
            DisplayStyle::Mirrored => {
                let bar_half = ((amplitude * h as f32) / 2.0) as usize;
                let center = h / 2;
                let top = center.saturating_sub(bar_half);
                let bottom = (center + bar_half).min(h);
                for y in top..bottom {
                    let i = (y * w + x) * 3;
                    buf[i..i + 3].copy_from_slice(&color);
                }
            }
            DisplayStyle::TopHalf => {
                let bar_h = (amplitude * h as f32) as usize;
                for y in 0..bar_h.min(h) {
                    let i = (y * w + x) * 3;
                    buf[i..i + 3].copy_from_slice(&color);
                }
            }
        }
    }

    // Flip vertically so bars grow from the bottom up.
    for row in 0..h / 2 {
        for col in 0..w {
            let a = (row * w + col) * 3;
            let b = ((h - 1 - row) * w + col) * 3;
            buf.swap(a,     b);
            buf.swap(a + 1, b + 1);
            buf.swap(a + 2, b + 2);
        }
    }

    buf
}

fn pick_color(low: f32, mid: f32, high: f32, amplitude: f32, scheme: &ColorScheme) -> [u8; 3] {
    match scheme {
        ColorScheme::AdditivePeachBlueLavender => [
            (low * 250.0 + mid * 137.0 + high * 203.0).min(255.0) as u8,
            (low * 179.0 + mid * 180.0 + high * 166.0).min(255.0) as u8,
            (low * 135.0 + mid * 250.0 + high * 247.0).min(255.0) as u8,
        ],
        ColorScheme::Monochrome => {
            let v = (amplitude * 255.0) as u8;
            [(v as f32 * 0.54) as u8, (v as f32 * 0.71) as u8, v]
        }
        ColorScheme::Grayscale => {
            let v = (amplitude * 255.0) as u8;
            [v, v, v]
        }
        ColorScheme::PerBandSolid => {
            if low >= mid && low >= high {
                [250, 179, 135] // Peach
            } else if mid >= high {
                [137, 180, 250] // Blue
            } else {
                [203, 166, 247] // Lavender
            }
        }
    }
}
