use slint::{Rgb8Pixel, SharedPixelBuffer};

use crate::render::render_to_pixels;
use crate::settings::WaveformRenderSettings;
use crate::types::{ViewPort, WaveformBucket};

/// Stateful waveform renderer with cache invalidation.
///
/// Call `get_or_render` to get a pixel buffer — it re-renders only when data,
/// settings, or viewport have changed since the last call.
pub struct Renderer {
    settings: WaveformRenderSettings,
    data:     Vec<WaveformBucket>,
    viewport: ViewPort,
    cached:   Option<SharedPixelBuffer<Rgb8Pixel>>,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            settings: WaveformRenderSettings::default(),
            data:     Vec::new(),
            viewport: ViewPort::default(),
            cached:   None,
        }
    }

    /// Update settings. Returns `true` if the cache was invalidated.
    pub fn set_settings(&mut self, s: WaveformRenderSettings) -> bool {
        if self.settings != s {
            self.settings = s;
            self.cached = None;
            true
        } else {
            false
        }
    }

    pub fn set_data(&mut self, data: Vec<WaveformBucket>) {
        self.data = data;
        self.cached = None;
    }

    pub fn set_viewport(&mut self, vp: ViewPort) {
        if self.viewport != vp {
            self.viewport = vp;
            self.cached = None;
        }
    }

    /// Return the cached pixel buffer, rendering first if the cache is stale.
    pub fn get_or_render(&mut self) -> &SharedPixelBuffer<Rgb8Pixel> {
        if self.cached.is_none() {
            self.cached = Some(render_to_pixels(&self.data, &self.settings, self.viewport));
        }
        self.cached.as_ref().unwrap()
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}
