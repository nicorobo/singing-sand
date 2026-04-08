# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

singing-sand is a Rust audio player and manager application, and a high-performance, lightning-fast DJ organization tool. Speed and responsiveness are first-class priorities — UI interactions should feel instant, analysis should be non-blocking, and rendering should never stall the event loop.

## Commands

```bash
cargo build          # Build the project
cargo run            # Run the application
cargo test           # Run all tests
cargo test <name>    # Run a specific test by name (substring match)
cargo clippy         # Lint
cargo fmt            # Format code
```

## Architecture

Cargo workspace with six crates:

| Crate | Purpose |
|-------|---------|
| `ss-core` | Domain types, error enums — no I/O |
| `ss-db` | sqlx SQLite queries + migrations |
| `ss-audio` | Rodio + Symphonia audio engine, waveform analysis |
| `ss-library` | lofty metadata, notify watcher, scan pipeline |
| `ss-api` | axum HTTP server |
| `ss-app` | Binary entry point — wires everything together |

Dependency order: `ss-core` → `ss-db` → `ss-audio`, `ss-library` → `ss-api` → `ss-app`

### Threading model

- **Main thread**: Slint event loop (GUI mode)
- **Tokio runtime**: async I/O, DB, HTTP server, file watcher, analysis workers
- **Audio thread** (`std::thread`): Rodio `OutputStream` + `Sink`, driven by `crossbeam_channel`
- **Slint → Tokio**: capture `tokio::runtime::Handle`, call `handle.spawn(async { ... })` in callbacks
- **Tokio → Slint**: `slint::invoke_from_event_loop(|| { weak.upgrade()?.set_property(...) })`

### Audio engine

`AudioEngine` (`ss-audio`) owns a dedicated `std::thread` running a `crossbeam_channel::select!` loop over:
- `cmd_rx`: `AudioCommand` (LoadAndPlay, Play, Pause, Stop, Seek, SetVolume)
- `ticker`: 100ms tick → emits `AudioEvent::PositionChanged`

Events returned via `flume::Receiver<AudioEvent>` (supports async `.recv_async()`).

Seek strategy: **stop + restart** — `sink.stop()` then recreate `SymphoniaSource` at the new position.
Waveform rendering: **pre-render to pixel buffer** — drawn once in `spawn_blocking`, cached as `slint::Image`.

### Running the CLI player (Phase 1)

```bash
cargo run -p ss-app -- /path/to/audio.mp3
```

## Implementation Progress

### ✅ Phase 1 — Project skeleton + audio playback
Streaming Symphonia → Rodio pipeline. CLI plays any audio file.
Key files: `crates/ss-audio/src/engine.rs`, `crates/ss-audio/src/source.rs`

### ✅ Phase 2 — Database layer
sqlx + SQLite, migrations, CRUD for all entities, integration tests.
Key files: `crates/ss-db/src/lib.rs`, `crates/ss-db/migrations/0001_init.sql`

### ✅ Phase 3 — Library scanner
walkdir + lofty metadata, upsert into SQLite, `cargo run -- scan <dir>`.
Key files: `crates/ss-library/src/lib.rs`, `crates/ss-app/src/main.rs`

### ✅ Phase 4 — Minimal Slint GUI
Track list, click to play, play/pause/stop, progress bar.
Key files: `crates/ss-app/ui/main.slint`, `crates/ss-app/build.rs`, `crates/ss-app/src/main.rs`

### ✅ Phase 5 — Waveform analysis + display
Background RMS analysis → SQLite blob → pixel buffer rendered in UI.
Key files: `crates/ss-audio/src/analyze.rs`, `crates/ss-db/migrations/0002_waveforms.sql`, `crates/ss-app/ui/main.slint`

### ✅ Phase 6 — Waveform seek + playhead
Click waveform to seek; animated playhead.
The waveform is the only seeking UI required, we do not need a separate progress bar.
Key files: `crates/ss-app/ui/main.slint`, `crates/ss-app/src/main.rs`

### ✅ Phase 7 — Playlists + tag system
Sidebar navigation (All / Directories / Playlists / Tags), dynamic filtered track list, playlist creation, tag creation, per-track tag assignment chips, "Add to playlist" in transport.
Key files: `crates/ss-db/migrations/0003_playlists_tags.sql`, `crates/ss-db/src/lib.rs`, `crates/ss-app/ui/main.slint`, `crates/ss-app/src/main.rs`

### ✅ Phase 8 — Album art + now-playing panel
Album art extracted from embedded tags during scan (lofty `pictures()`), stored as raw JPEG/PNG bytes in SQLite (`album_art` table, migration 0004). 44px thumbnails loaded progressively per row via `invoke_from_event_loop` + `Model::set_row_data`. Now-playing panel below waveform shows 60px art, title, artist, and Prev/Play-Pause/Next buttons. Waveform halved to 48px (top half only via clip). `image` crate used for decode + resize. `slint::Image` is not Send — pixel buffers (`SharedPixelBuffer<Rgb8Pixel>`) are used across thread boundaries and converted to `Image` on the Slint thread.
Key files: `crates/ss-db/migrations/0004_album_art.sql`, `crates/ss-db/src/lib.rs`, `crates/ss-library/src/lib.rs`, `crates/ss-app/src/main.rs`, `crates/ss-app/ui/main.slint`

### ✅ Phase 9 — BPM detection + frequency-band waveforms
`analyze_track` (single Symphonia decode pass) replaces `analyze_waveform`. Per-bucket FFT via `realfft` (real-valued, N/2+1 output bins) produces [low/mid/high] RMS bands (20–250Hz / 250–4kHz / 4k–20kHz). BPM detected via energy-envelope autocorrelation (no external BPM lib — `bpm-analyzer` is device-capture only). Waveform rendered with additive colour blend (low=Peach, mid=Blue, high=Lavender). `AnalysisQueue` uses a `Semaphore`-bounded pool of `spawn_blocking` tasks (N = CPU count) for parallel analysis. `pending-analysis-count` Slint property drives a "Analyzing N tracks…" bottom-right overlay. BPM shown in track-list rows and now-playing panel. Migration 0006 adds `bpm REAL` to tracks and clears waveforms (format change: `[f32×3×N]` interleaved). FFT hop is non-overlapping (window cleared after each FFT) for 2× fewer FFT ops vs 50% overlap.
Key files: `crates/ss-db/migrations/0006_bpm_waveform_v2.sql`, `crates/ss-audio/src/analyze.rs`, `crates/ss-db/src/lib.rs`, `crates/ss-app/src/main.rs`, `crates/ss-app/ui/main.slint`

## For a future version

### ✅ Phase 11 — ss-waveform crate + settings window + persistent waveform controls
`ss-waveform` crate owns `WaveformBucket`, `ViewPort`, `WaveformRenderSettings`, `DisplayStyle`, `ColorScheme`, `render_to_pixels` (pure fn), and `Renderer` (stateful cache). `WaveformBucket` moved from `ss-audio`; `ss-audio` re-exports it. Settings persisted in SQLite (`settings` table, migration 0007). `SettingsWindow` is a second OS-level Slint `Window` instantiated from Rust, opened via gear icon in sidebar footer. All 10 waveform settings: band toggles (low/mid/high), amplitude scale, per-band gain (low/mid/high), display style (Mirrored/TopHalf), color scheme (4 options), normalize. Changes re-render waveform instantly and persist async.
Key files: `crates/ss-waveform/`, `crates/ss-db/migrations/0007_settings.sql`, `crates/ss-db/src/lib.rs`, `crates/ss-app/src/settings.rs`, `crates/ss-app/ui/settings.slint`, `crates/ss-app/ui/main.slint`, `crates/ss-app/src/main.rs`

**Implementation note:** Slint 1.x does not support Window components nested inside other Window components. `SettingsWindow` is instantiated separately via `SettingsWindow::new()` in Rust and wrapped in `Arc`. Opened via `settings_win.show()` from the `on_settings_clicked` callback.

### ⬜ Phase 8 — Waveform thumbnails
Per-track thumbnail in list view; click to seek/play.

### ⬜ Phase 9 — Markers
Timestamp markers on waveform with labels, tags, and filterable list.

### ⬜ Phase 10 — HTTP API + headless mode
axum REST API; `--headless` flag skips Slint.

### ⬜ Phase 11 — Filesystem watcher
notify watches source dirs; auto-updates library on file changes.

## Known Issues / Notes

- ~2s startup delay before audio begins (Symphonia file probe on audio thread). Acceptable for now; consider pre-buffering in a later phase.
