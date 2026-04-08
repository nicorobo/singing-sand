/// Per-bucket frequency-band RMS values, normalised to [0, 1].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WaveformBucket {
    /// Low band (20–250 Hz) RMS.
    pub low: f32,
    /// Mid band (250–4 000 Hz) RMS.
    pub mid: f32,
    /// High band (4 000–20 000 Hz) RMS.
    pub high: f32,
}

impl WaveformBucket {
    pub fn to_array(self) -> [f32; 3] {
        [self.low, self.mid, self.high]
    }
    pub fn from_array(arr: [f32; 3]) -> Self {
        Self { low: arr[0], mid: arr[1], high: arr[2] }
    }
}

/// Viewport for rendering: which region of the data to render and at what size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewPort {
    pub width: u32,
    pub height: u32,
    /// Fraction of the data to start from [0.0, 1.0].
    pub start_pct: f32,
    /// Fraction of the data to end at [0.0, 1.0].
    pub end_pct: f32,
}

impl Default for ViewPort {
    fn default() -> Self {
        Self { width: 1000, height: 48, start_pct: 0.0, end_pct: 1.0 }
    }
}
