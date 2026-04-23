/// How the waveform bar is drawn vertically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayStyle {
    /// Symmetric bar centered in the viewport (current default behavior).
    Mirrored,
    /// Bar grows from the bottom up.
    TopHalf,
}

/// Color scheme for rendered waveform bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    /// Additive blend: Low=Peach, Mid=Blue, High=Lavender (current default).
    AdditivePeachBlueLavender,
    Monochrome,
    PerBandSolid,
    Grayscale,
}

/// How amplitude values are normalized before rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizeMode {
    /// No normalization — use raw RMS values.
    None,
    /// Normalize each band independently to its own peak.
    PerBand,
    /// Normalize all bands to the global peak, preserving relative energy balance.
    Global,
}

/// All visual parameters for waveform rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct WaveformRenderSettings {
    /// Global amplitude multiplier applied after per-band gains.
    pub amplitude_scale: f32,
    pub low_gain:        f32,
    pub mid_gain:        f32,
    pub high_gain:       f32,
    pub display_style:   DisplayStyle,
    pub color_scheme:    ColorScheme,
    pub normalize_mode:  NormalizeMode,
    /// Power-curve exponent for bar height. <1.0 lifts quiet detail; >1.0 crushes it.
    /// Default 0.6 gives moderate dynamic compression.
    pub gamma:           f32,
    /// Amplitude threshold below which bars are hidden (noise gate). 0.0 = off.
    pub noise_floor:     f32,
    /// Number of adjacent buckets to average per column. 1 = no smoothing.
    pub smoothing:       u8,
}

impl Default for WaveformRenderSettings {
    fn default() -> Self {
        Self {
            amplitude_scale: 1.0,
            low_gain:        1.0,
            mid_gain:        1.0,
            high_gain:       1.0,
            display_style:   DisplayStyle::Mirrored,
            color_scheme:    ColorScheme::AdditivePeachBlueLavender,
            normalize_mode:  NormalizeMode::None,
            gamma:           0.6,
            noise_floor:     0.0,
            smoothing:       1,
        }
    }
}
