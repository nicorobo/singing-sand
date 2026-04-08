# ss-waveform + Phase 11 Settings — Handoff Document

This document captures the full implementation plan and codebase context needed to pick up this work in a new session. No re-exploration required.

---

## What we're building

**Two related pieces:**

1. **`ss-waveform` crate** — extract the waveform rendering logic from `ss-app/src/main.rs` into a standalone, shippable crate with a clean public API and configurable rendering settings.

2. **Phase 11 settings** — persist waveform visual settings to SQLite and expose them via a second OS window (Slint `SettingsWindow`) accessible from a gear icon in the sidebar.

---

## Current state (what exists today)

- `render_waveform_buffer(bands: &[[f32; 3]]) -> slint::SharedPixelBuffer<slint::Rgb8Pixel>` is a private function in `crates/ss-app/src/main.rs:45–80`. It's hardcoded: 1000×96px, mirrored bars, additive Peach/Blue/Lavender blend, no settings.
- `WaveformBucket { low: f32, mid: f32, high: f32 }` is defined in `crates/ss-audio/src/analyze.rs:16–23` and re-exported from `crates/ss-audio/src/lib.rs`.
- Waveform data in DB is `Vec<[f32; 3]>` (interleaved f32 blobs, 12 bytes/bucket). Retrieved via `db.get_waveform_bands(track_id) -> Result<Option<Vec<[f32; 3]>>>`.
- Highest migration: `0006` (`crates/ss-db/migrations/`). Next is `0007`.
- No settings infrastructure exists yet.

### Current render algorithm (must be preserved as the default)
```rust
// Background: rgb(24, 24, 37)
// Per column x (0..1000):
//   bucket = (x * bands.len()) / 1000
//   [low, mid, high] = bands[bucket]
//   amplitude = ((low + mid + high) / 3.0).clamp(0.0, 1.0)
//   bar_half = (amplitude * 96) / 2  → symmetrical around center
//   total = low + mid + high + 1e-6
//   r = (low*250 + mid*137 + high*203) / total
//   g = (low*179 + mid*180 + high*166) / total
//   b = (low*135 + mid*250 + high*247) / total
```

### Slint UI structure (relevant parts)
- Sidebar: `Rectangle { width: 200px }` containing a `ScrollView` that fills it entirely — needs to become a `VerticalLayout` with `ScrollView { vertical-stretch: 1 }` + a fixed footer for the gear button.
- Waveform display: `waveform-rect := Rectangle { height: 48px; clip: true }` containing an `Image { height: 96px }` — the clip shows only the top half of the symmetric render.
- The playhead is a Slint overlay `Rectangle` inside `waveform-rect`, **not** painted in the pixel buffer.
- `w.set_waveform_image(slint::Image::from_rgb8(pixel_buf))` is how the pixel buffer reaches the UI.

---

## Part A: `ss-waveform` crate

### New crate structure

```
crates/ss-waveform/
  Cargo.toml
  src/
    lib.rs       ← public re-exports
    types.rs     ← WaveformBucket, ViewPort
    settings.rs  ← WaveformRenderSettings, DisplayStyle, ColorScheme
    render.rs    ← render_to_pixels (pure fn)
    renderer.rs  ← stateful Renderer (cache + dirty tracking)
```

### `Cargo.toml`

```toml
[package]
name    = "ss-waveform"
version = "0.1.0"
edition = "2021"

[dependencies]
slint = { workspace = true }

[dependencies.image]
workspace = true
optional = true

[features]
render-png = ["dep:image"]
```

No dependency on ss-core, ss-audio, or ss-db. The only external dep is `slint` (for `SharedPixelBuffer<Rgb8Pixel>`).

### `types.rs`

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WaveformBucket {
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}
impl WaveformBucket {
    pub fn to_array(self) -> [f32; 3] { [self.low, self.mid, self.high] }
    pub fn from_array(arr: [f32; 3]) -> Self { Self { low: arr[0], mid: arr[1], high: arr[2] } }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewPort {
    pub width: u32,
    pub height: u32,
    pub start_pct: f32,  // [0.0, 1.0] fraction of data to render
    pub end_pct: f32,
}
impl Default for ViewPort {
    fn default() -> Self { Self { width: 1000, height: 96, start_pct: 0.0, end_pct: 1.0 } }
}
```

### `settings.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayStyle { Mirrored, TopHalf }  // Mirrored = current behavior

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorScheme {
    AdditivePeachBlueLavender,  // current behavior
    Monochrome,
    PerBandSolid,
    Grayscale,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WaveformRenderSettings {
    pub show_low:        bool,   // default: true
    pub show_mid:        bool,   // default: true
    pub show_high:       bool,   // default: true
    pub amplitude_scale: f32,   // default: 1.0
    pub low_gain:        f32,   // default: 1.0
    pub mid_gain:        f32,   // default: 1.0
    pub high_gain:       f32,   // default: 1.0
    pub display_style:   DisplayStyle,  // default: Mirrored
    pub color_scheme:    ColorScheme,   // default: AdditivePeachBlueLavender
    pub normalize:       bool,   // default: false
}
// impl Default → all defaults above → produces byte-identical output to current render_waveform_buffer
```

### `render.rs`

```rust
pub fn render_to_pixels(
    data: &[WaveformBucket],
    settings: &WaveformRenderSettings,
    viewport: ViewPort,
) -> slint::SharedPixelBuffer<slint::Rgb8Pixel>
```

Logic:
1. Slice data: `start = (data.len() as f32 * viewport.start_pct) as usize`, `end = ...`
2. Zero out disabled bands (`show_low/mid/high`)
3. Apply `low_gain`, `mid_gain`, `high_gain`, then `amplitude_scale`
4. If `normalize: true`, scan slice for per-band peaks, divide through
5. Per pixel column `x` → map to bucket index → compute bar height + color
6. `DisplayStyle::Mirrored`: `center ± bar_half` (current)
7. `DisplayStyle::TopHalf`: bar from bottom, height = amplitude * H
8. `AdditivePeachBlueLavender` color formula — **byte-identical to current**:
   ```rust
   let total = low + mid + high + 1e-6;
   let r = ((low * 250.0 + mid * 137.0 + high * 203.0) / total) as u8;
   let g = ((low * 179.0 + mid * 180.0 + high * 166.0) / total) as u8;
   let b = ((low * 135.0 + mid * 250.0 + high * 247.0) / total) as u8;
   ```
9. `Monochrome`: `let v = (mean_amplitude * 255.0) as u8; Rgb8Pixel { r: v, g: v, b: v }`
10. `PerBandSolid`: stacked bars — low fills bottom portion in Peach, mid fills middle in Blue, high fills top in Lavender (proportional to each band's value)
11. `Grayscale`: same as Monochrome
12. Background: `Rgb8Pixel { r: 24, g: 24, b: 37 }`

### `renderer.rs` (stateful cache)

```rust
pub struct Renderer {
    settings: WaveformRenderSettings,
    data: Vec<WaveformBucket>,
    viewport: ViewPort,
    cached: Option<slint::SharedPixelBuffer<slint::Rgb8Pixel>>,
}
impl Renderer {
    pub fn new() -> Self;
    pub fn set_settings(&mut self, s: WaveformRenderSettings) -> bool;  // true if dirty
    pub fn set_data(&mut self, data: Vec<WaveformBucket>);
    pub fn set_viewport(&mut self, vp: ViewPort);
    pub fn get_or_render(&mut self) -> &slint::SharedPixelBuffer<slint::Rgb8Pixel>;
}
```

Invalidates cache on any setter that changes state. Note: the `Renderer` is not used in the initial wiring in `main.rs` (it calls `render_to_pixels` directly); the `Renderer` is included as a public API for future use.

### `lib.rs`

```rust
mod types; mod settings; mod render; mod renderer;
#[cfg(feature = "render-png")] mod png;

pub use types::{WaveformBucket, ViewPort};
pub use settings::{WaveformRenderSettings, DisplayStyle, ColorScheme};
pub use render::render_to_pixels;
pub use renderer::Renderer;
#[cfg(feature = "render-png")] pub use png::render_to_png;
```

---

## Part B: DB settings + persistence

### `crates/ss-db/migrations/0007_settings.sql`

```sql
CREATE TABLE settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
```

### New methods on `Db` in `crates/ss-db/src/lib.rs`

```rust
pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
    let row = sqlx::query("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
    Ok(row.map(|r| r.get("value")))
}

pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value"
    )
    .bind(key).bind(value).execute(&self.pool).await?;
    Ok(())
}
```

---

## Part C: AppSettings + Settings UI

### New file: `crates/ss-app/src/settings.rs`

```rust
use ss_waveform::{ColorScheme, DisplayStyle, WaveformRenderSettings};
use ss_db::Db;
use anyhow::Result;

// Key constants
const KEY_SHOW_LOW:        &str = "waveform.show_low";
const KEY_SHOW_MID:        &str = "waveform.show_mid";
const KEY_SHOW_HIGH:       &str = "waveform.show_high";
const KEY_AMPLITUDE_SCALE: &str = "waveform.amplitude_scale";
const KEY_LOW_GAIN:        &str = "waveform.low_gain";
const KEY_MID_GAIN:        &str = "waveform.mid_gain";
const KEY_HIGH_GAIN:       &str = "waveform.high_gain";
const KEY_DISPLAY_STYLE:   &str = "waveform.display_style";
const KEY_COLOR_SCHEME:    &str = "waveform.color_scheme";
const KEY_NORMALIZE:       &str = "waveform.normalize";

pub struct AppSettings { pub waveform: WaveformRenderSettings }
impl Default for AppSettings { fn default() -> Self { Self { waveform: WaveformRenderSettings::default() } } }

pub async fn load_settings(db: &Db) -> Result<AppSettings> { /* read each key with fallback to Default */ }
pub async fn save_settings(db: &Db, s: &AppSettings) -> Result<()> { /* set_setting for all keys */ }
```

### New file: `crates/ss-app/ui/settings.slint`

```slint
import { CheckBox, Slider, ComboBox } from "std-widgets.slint";

export component SettingsWindow inherits Window {
    title: "Waveform Settings";
    background: #1e1e2e;
    preferred-width: 320px;
    preferred-height: 420px;

    // in-out properties (initialized from Rust on open)
    in-out property <bool>  show-low:        true;
    in-out property <bool>  show-mid:        true;
    in-out property <bool>  show-high:       true;
    in-out property <float> amplitude-scale: 1.0;
    in-out property <float> low-gain:        1.0;
    in-out property <float> mid-gain:        1.0;
    in-out property <float> high-gain:       1.0;
    in-out property <int>   display-style:   0;  // 0=Mirrored, 1=TopHalf
    in-out property <int>   color-scheme:    0;  // 0..3
    in-out property <bool>  normalize:       false;

    // callbacks (fired immediately on change, no Apply button)
    callback show-low-changed(bool);
    callback show-mid-changed(bool);
    callback show-high-changed(bool);
    callback amplitude-scale-changed(float);
    callback low-gain-changed(float);
    callback mid-gain-changed(float);
    callback high-gain-changed(float);
    callback display-style-changed(int);
    callback color-scheme-changed(int);
    callback normalize-changed(bool);

    // ... VerticalLayout with sections for each control group
}
```

### Changes to `crates/ss-app/ui/main.slint`

**1. Import at top:**
```slint
import { SettingsWindow } from "settings.slint";
```

**2. Sidebar layout** — the sidebar `Rectangle` currently has only a `ScrollView`. Change it to:
```slint
Rectangle {
    width: 200px;
    background: #11111b;

    VerticalLayout {
        // Scrollable section takes all available space
        sidebar-scroll := ScrollView {
            vertical-stretch: 1;
            viewport-width: 200px;
            // ... existing VerticalLayout content unchanged ...
        }

        // Gear icon footer
        Rectangle {
            height: 36px;
            background: #0d0d17;
            HorizontalLayout {
                padding-left: 12px;
                alignment: start;
                Text {
                    text: "⚙";
                    color: gear-ta.has-hover ? #cdd6f4 : #6c7086;
                    font-size: 14px;
                    vertical-alignment: center;
                }
            }
            gear-ta := TouchArea {
                clicked => { root.settings-clicked(); }
            }
        }
    }
}
```

**3. Add to `AppWindow`:**
```slint
callback settings-clicked();

// Settings window instance (not visible by default)
settings-win := SettingsWindow {
    show-low-changed(v)         => { root.settings-show-low-changed(v); }
    show-mid-changed(v)         => { root.settings-show-mid-changed(v); }
    // ... all 10 callbacks forwarded ...
}

// Forwarding callbacks (so Rust only wires to AppWindow)
callback settings-show-low-changed(bool);
callback settings-show-mid-changed(bool);
// ... etc.
```

**4. Wire gear button** to show the settings window:
```slint
// In AppWindow, add a function or direct show call
// Rust wires: window.on_settings_clicked(|| { settings_win.show(); })
// Since settings-win is a sub-instance, Rust accesses it via invoke_show_settings() or
// by adding a bool property: in-out property <bool> settings-visible: false;
// and binding: settings-win.visible <=> root.settings-visible;
```

### Changes to `crates/ss-app/src/main.rs`

**Imports to add:**
```rust
use ss_waveform::{render_to_pixels, WaveformBucket, WaveformRenderSettings, ViewPort};
mod settings;
use settings::{AppSettings, load_settings, save_settings};
```

**Remove:** `render_waveform_buffer` function (lines 38–80).

**In `cmd_gui`**, after `open_db()`:
```rust
let app_settings = rt.block_on(load_settings(&db))?;
let render_settings = Arc::new(Mutex::new(app_settings.waveform.clone()));
let current_bands: Arc<Mutex<Vec<WaveformBucket>>> = Arc::new(Mutex::new(vec![]));
```

**In `start_playback`**, replace the waveform section:
```rust
// Convert Vec<[f32;3]> from DB → Vec<WaveformBucket>
let buckets: Vec<WaveformBucket> = bands.iter().map(|&arr| WaveformBucket::from_array(arr)).collect();
// Store for settings re-renders
*current_bands.lock().unwrap() = buckets.clone();
// Render with current settings
let settings_snap = render_settings.lock().unwrap().clone();
let pixel_buf = render_to_pixels(&buckets, &settings_snap, ViewPort::default());
```

**Settings callback pattern** (repeat for all 10 settings):
```rust
{
    let render_settings = Arc::clone(&render_settings);
    let current_bands = Arc::clone(&current_bands);
    let weak = window.as_weak();
    let db = Arc::clone(&db);
    let rt_handle = rt_handle.clone();
    window.on_settings_show_low_changed(move |val| {
        render_settings.lock().unwrap().show_low = val;
        let bands = current_bands.lock().unwrap().clone();
        if !bands.is_empty() {
            let s = render_settings.lock().unwrap().clone();
            let buf = render_to_pixels(&bands, &s, ViewPort::default());
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(w) = weak.upgrade() {
                    w.set_waveform_image(slint::Image::from_rgb8(buf));
                }
            });
        }
        let s = render_settings.lock().unwrap().clone();
        let db = Arc::clone(&db);
        rt_handle.spawn(async move {
            let _ = save_settings(&db, &AppSettings { waveform: s }).await;
        });
    });
}
```

**Initialize window properties from loaded settings:**
```rust
window.set_settings_show_low(app_settings.waveform.show_low);
// ... all 10 fields
```

---

## Cargo.toml changes

**Root `Cargo.toml`:**
```toml
[workspace]
members = [
    "crates/ss-core",
    "crates/ss-db",
    "crates/ss-audio",
    "crates/ss-library",
    "crates/ss-api",
    "crates/ss-app",
    "crates/ss-waveform",   # ← add
]

[workspace.dependencies]
ss-waveform = { path = "crates/ss-waveform" }   # ← add
```

**`crates/ss-audio/Cargo.toml`:** add `ss-waveform = { path = "../ss-waveform" }`

**`crates/ss-app/Cargo.toml`:** add `ss-waveform = { workspace = true }`

---

## ss-audio changes (backward-compatible)

**`crates/ss-audio/src/analyze.rs`:**
- Remove the `WaveformBucket` struct definition (lines 14–30)
- Add at top: `use ss_waveform::WaveformBucket;`

**`crates/ss-audio/src/lib.rs`:**
```rust
// Change from:
pub use analyze::{analyze_track, AnalysisResult, WaveformBucket};
// To:
pub use analyze::{analyze_track, AnalysisResult};
pub use ss_waveform::WaveformBucket;  // re-export keeps public API identical
```

---

## Implementation order

1. Create `crates/ss-waveform/` with all source files
2. Register in root `Cargo.toml`
3. `cargo build -p ss-waveform` — verify standalone compile
4. Update `ss-audio` (move `WaveformBucket`, re-export)
5. Create `crates/ss-db/migrations/0007_settings.sql`
6. Add `get_setting`/`set_setting` to `crates/ss-db/src/lib.rs`
7. Create `crates/ss-app/src/settings.rs`
8. Add `ss-waveform` to `crates/ss-app/Cargo.toml`
9. Create `crates/ss-app/ui/settings.slint`
10. Update `crates/ss-app/ui/main.slint`
11. Update `crates/ss-app/src/main.rs`
12. `cargo build` + `cargo test` + run end-to-end

## Verification

```bash
cargo build -p ss-waveform          # standalone crate compiles cleanly
cargo build                         # full workspace
cargo test                          # existing tests still pass
cargo run                           # app starts, waveforms render identically with default settings
# Open settings via gear icon → toggle bands → waveform re-renders instantly
# Quit and reopen → settings persist
```

---

## Critical files summary

| File | Change |
|---|---|
| `crates/ss-waveform/` | **New crate** — entire directory |
| `Cargo.toml` (root) | Add workspace member + dep |
| `crates/ss-audio/src/analyze.rs` | Remove `WaveformBucket` def, import from ss-waveform |
| `crates/ss-audio/src/lib.rs` | Re-export `WaveformBucket` from ss-waveform |
| `crates/ss-audio/Cargo.toml` | Add ss-waveform dep |
| `crates/ss-db/migrations/0007_settings.sql` | **New migration** |
| `crates/ss-db/src/lib.rs` | Add `get_setting` / `set_setting` |
| `crates/ss-app/src/settings.rs` | **New**: `AppSettings`, `load_settings`, `save_settings` |
| `crates/ss-app/ui/settings.slint` | **New**: `SettingsWindow` component |
| `crates/ss-app/ui/main.slint` | Import, gear button in sidebar footer, forwarding callbacks |
| `crates/ss-app/src/main.rs` | Replace `render_waveform_buffer`, wire settings, `current_bands` |
| `crates/ss-app/Cargo.toml` | Add ss-waveform dep |
