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
    /// Normalize each track to its own peak amplitude before rendering.
    pub normalize:       bool,
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
            normalize:       false,
        }
    }
}
