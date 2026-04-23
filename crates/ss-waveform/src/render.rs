use slint::{Rgb8Pixel, SharedPixelBuffer};

use crate::settings::{ColorScheme, DisplayStyle, NormalizeMode, WaveformRenderSettings};
use crate::types::{ViewPort, WaveformBucket};

const BG: Rgb8Pixel = Rgb8Pixel { r: 24, g: 24, b: 37 };

/// Render waveform band data into a pixel buffer.
///
/// This is a pure function — no side effects, no allocation beyond the buffer.
/// With default settings the output is byte-identical to the original
/// `render_waveform_buffer` in ss-app.
pub fn render_to_pixels(
    data: &[WaveformBucket],
    settings: &WaveformRenderSettings,
    viewport: ViewPort,
) -> SharedPixelBuffer<Rgb8Pixel> {
    let w = viewport.width as usize;
    let h = viewport.height as usize;

    let mut buf = SharedPixelBuffer::<Rgb8Pixel>::new(viewport.width, viewport.height);
    let pixels = buf.make_mut_slice();
    for p in pixels.iter_mut() {
        *p = BG;
    }

    if data.is_empty() || w == 0 || h == 0 {
        return buf;
    }

    // Slice the data to the requested viewport range.
    let start = (data.len() as f32 * viewport.start_pct) as usize;
    let end = (data.len() as f32 * viewport.end_pct) as usize;
    let end = end.min(data.len());
    let slice = if start >= end { return buf; } else { &data[start..end] };

    // Apply normalization if requested.
    let normalized: Vec<WaveformBucket>;
    let slice: &[WaveformBucket] = match settings.normalize_mode {
        NormalizeMode::None => slice,
        NormalizeMode::PerBand => {
            let peak_low  = slice.iter().map(|b| b.low).fold(0.0_f32, f32::max);
            let peak_mid  = slice.iter().map(|b| b.mid).fold(0.0_f32, f32::max);
            let peak_high = slice.iter().map(|b| b.high).fold(0.0_f32, f32::max);
            normalized = slice
                .iter()
                .map(|b| WaveformBucket {
                    low:  if peak_low  > 0.0 { b.low  / peak_low  } else { 0.0 },
                    mid:  if peak_mid  > 0.0 { b.mid  / peak_mid  } else { 0.0 },
                    high: if peak_high > 0.0 { b.high / peak_high } else { 0.0 },
                })
                .collect();
            &normalized
        }
        NormalizeMode::Global => {
            let peak = slice
                .iter()
                .flat_map(|b| [b.low, b.mid, b.high])
                .fold(0.0_f32, f32::max);
            normalized = slice
                .iter()
                .map(|b| WaveformBucket {
                    low:  if peak > 0.0 { b.low  / peak } else { 0.0 },
                    mid:  if peak > 0.0 { b.mid  / peak } else { 0.0 },
                    high: if peak > 0.0 { b.high / peak } else { 0.0 },
                })
                .collect();
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

        // Per-band gain only — these values drive COLOR (amplitude_scale must not affect hue).
        let low  = (b.low  * settings.low_gain).clamp(0.0, 1.0);
        let mid  = (b.mid  * settings.mid_gain).clamp(0.0, 1.0);
        let high = (b.high * settings.high_gain).clamp(0.0, 1.0);

        // Bar HEIGHT: peak of bands → gamma curve → global amplitude scale.
        let peak = low.max(mid).max(high);
        let amplitude = (peak.powf(settings.gamma) * settings.amplitude_scale).clamp(0.0, 1.0);
        if amplitude <= settings.noise_floor {
            continue;
        }

        match settings.display_style {
            DisplayStyle::Mirrored => {
                let bar_half = ((amplitude * h as f32) / 2.0) as usize;
                let center = h / 2;
                let top = center.saturating_sub(bar_half);
                let bottom = (center + bar_half).min(h);
                let color = pick_color(low, mid, high, amplitude, &settings.color_scheme);
                for y in top..bottom {
                    pixels[y * w + x] = color;
                }
            }
            DisplayStyle::TopHalf => {
                let bar_h = (amplitude * h as f32) as usize;
                let color = pick_color(low, mid, high, amplitude, &settings.color_scheme);
                for y in 0..bar_h.min(h) {
                    pixels[y * w + x] = color;
                }
            }
        }
    }

    // Flip vertically so bars grow from the bottom up.
    let pixels = buf.make_mut_slice();
    for row in 0..h / 2 {
        for col in 0..w {
            pixels.swap(row * w + col, (h - 1 - row) * w + col);
        }
    }

    buf
}

fn pick_color(
    low: f32,
    mid: f32,
    high: f32,
    amplitude: f32,
    scheme: &ColorScheme,
) -> Rgb8Pixel {
    match scheme {
        ColorScheme::AdditivePeachBlueLavender => {
            Rgb8Pixel {
                r: (low * 250.0 + mid * 137.0 + high * 203.0).min(255.0) as u8,
                g: (low * 179.0 + mid * 180.0 + high * 166.0).min(255.0) as u8,
                b: (low * 135.0 + mid * 250.0 + high * 247.0).min(255.0) as u8,
            }
        }
        ColorScheme::Monochrome => {
            // Blue intensity scaled by amplitude.
            let v = (amplitude * 255.0) as u8;
            Rgb8Pixel { r: (v as f32 * 0.54) as u8, g: (v as f32 * 0.71) as u8, b: v }
        }
        ColorScheme::Grayscale => {
            let v = (amplitude * 255.0) as u8;
            Rgb8Pixel { r: v, g: v, b: v }
        }
        ColorScheme::PerBandSolid => {
            // Dominant band determines color: Peach/Blue/Lavender.
            if low >= mid && low >= high {
                Rgb8Pixel { r: 250, g: 179, b: 135 } // Peach
            } else if mid >= high {
                Rgb8Pixel { r: 137, g: 180, b: 250 } // Blue
            } else {
                Rgb8Pixel { r: 203, g: 166, b: 247 } // Lavender
            }
        }
    }
}
