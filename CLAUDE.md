# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

singing-sand is a Rust audio player and manager application.

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

### ⬜ Phase 2 — Database layer
sqlx + SQLite, migrations, CRUD for all entities, integration tests.

### ⬜ Phase 3 — Library scanner
walkdir + lofty metadata, batch inserts, `cargo run -- scan <dir>`.

### ⬜ Phase 4 — Minimal Slint GUI
Track list, click to play, play/pause/stop, progress bar.

### ⬜ Phase 5 — Waveform analysis + display
Background RMS analysis → SQLite blob → pixel buffer rendered in UI.

### ⬜ Phase 6 — Waveform seek + playhead
Click waveform to seek; animated playhead.

### ⬜ Phase 7 — Playlists + tag system
Create playlists, assign tags, sidebar with virtual tag directories.

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
