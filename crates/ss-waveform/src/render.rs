use slint::{Rgb8Pixel, SharedPixelBuffer};

use crate::settings::{ColorScheme, DisplayStyle, WaveformRenderSettings};
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
    let slice: &[WaveformBucket] = if settings.normalize {
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
    } else {
        slice
    };

    let slice_len = slice.len();

    for x in 0..w {
        let bucket = (x * slice_len) / w;
        let b = slice.get(bucket).copied().unwrap_or_default();

        // Zero out disabled bands.
        let low  = if settings.show_low  { b.low  * settings.low_gain  } else { 0.0 };
        let mid  = if settings.show_mid  { b.mid  * settings.mid_gain  } else { 0.0 };
        let high = if settings.show_high { b.high * settings.high_gain } else { 0.0 };

        let low  = (low  * settings.amplitude_scale).clamp(0.0, 1.0);
        let mid  = (mid  * settings.amplitude_scale).clamp(0.0, 1.0);
        let high = (high * settings.amplitude_scale).clamp(0.0, 1.0);

        let amplitude = ((low + mid + high) / 3.0).clamp(0.0, 1.0);
        if amplitude == 0.0 {
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
                let top = h.saturating_sub(bar_h);
                let color = pick_color(low, mid, high, amplitude, &settings.color_scheme);
                for y in top..h {
                    pixels[y * w + x] = color;
                }
            }
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
            let total = low + mid + high + 1e-6;
            Rgb8Pixel {
                r: ((low * 250.0 + mid * 137.0 + high * 203.0) / total) as u8,
                g: ((low * 179.0 + mid * 180.0 + high * 166.0) / total) as u8,
                b: ((low * 135.0 + mid * 250.0 + high * 247.0) / total) as u8,
            }
        }
        ColorScheme::Monochrome | ColorScheme::Grayscale => {
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
