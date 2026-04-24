# Slint → Tauri Migration

**Goal:** Replace Slint GUI with Tauri 2.0 + React + Vite + Bun + SCSS. Feature parity with the existing app. All Slint code removed by the end.

**Stack:** Bun · Vite · React 18 · SCSS Modules · Tauri 2.0 · Zustand · @tanstack/react-virtual

---

## Progress

### Phase 1 — Scaffold ✅
*Goal: blank Tauri window opens, Vite dev server starts.*

- [x] Create `src-tauri/` (Cargo.toml, build.rs, tauri.conf.json, capabilities/, src/main.rs, src/lib.rs)
- [x] Add `src-tauri` to workspace `Cargo.toml`; remove `ss-app` from members (not deleted yet)
- [x] Create root `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`
- [x] Create `src/main.tsx`, `src/App.tsx` (blank shell)
- [x] Create 4 Zustand stores (`playerStore`, `libraryStore`, `sidebarStore`, `uiStore`)
- [x] Create `src/hooks/useTauriEvents.ts` (event wiring stubs)
- [x] `bun install` — install all frontend deps
- [x] Verify: `bun run dev` starts at :5173; `cargo build -p singing-sand` compiles clean

---

### Phase 2 — Remove Slint from `ss-waveform` ✅
*Goal: `ss-waveform` has no Slint dependency; `render_to_pixels` returns `Vec<u8>` (raw RGB).*

- [x] `crates/ss-waveform/src/render.rs` — change return type from `SharedPixelBuffer<Rgb8Pixel>` to `Vec<u8>`; replace `Rgb8Pixel` with direct byte writes
- [x] `crates/ss-waveform/src/renderer.rs` — update cached field type to `Option<Vec<u8>>`
- [x] `crates/ss-waveform/Cargo.toml` — remove `slint`; make `image` non-optional
- [x] Verify: `cargo build -p ss-waveform` compiles clean

---

### Phase 3 — Core Rust backend (`src-tauri`) ✅
*Goal: all Tauri commands and events compile; `art://` URI scheme registered.*

- [x] `src-tauri/src/state.rs` — `AppState` struct with all `Arc`/`Mutex` fields
- [x] `src-tauri/src/dtos.rs` — all serializable DTO types for IPC boundary
- [x] `src-tauri/src/events.rs` — `spawn_audio_event_forwarder`
- [x] `src-tauri/src/analysis.rs` — `AnalysisQueue` with `AppHandle` (emits `waveform-ready`, `analysis-progress`)
- [x] `src-tauri/src/settings.rs` — load/save waveform settings from SQLite
- [x] Register `art://localhost/{track_id}` async URI scheme in `lib.rs`
- [x] `commands/settings.rs` — `get_settings`, `update_waveform_setting`
- [x] `commands/library.rs` — `nav_all`, `nav_select_dir`, `nav_playlist`, `nav_tag`, `search_tracks`
- [x] `commands/directories.rs` — `add_directory` (rfd), `toggle_dir_expanded`, `remove_scanned_dir`
- [x] `commands/waveform.rs` — `get_waveform` → PNG via `ipc::Response`
- [x] `commands/transport.rs` — `play_track`, `play`, `pause`, `stop`, `seek`
- [x] `commands/tracks.rs` — `expand_track`, `remove_tag_from_expanded`, `save_notes`, `track_clicked`
- [x] `commands/tags.rs` — `create_tag`, `delete_tag`, `update_tag`, `toggle_tag_for_selection`
- [x] `commands/playlists.rs` — `create_playlist`, `add_to_playlist`, `remove_from_playlist`, `reorder_playlist_tracks`, `add_selected_to_playlist`
- [x] Wire all into `lib.rs`: `manage(state)`, URI scheme, `invoke_handler`
- [x] Verify: `cargo build -p singing-sand` compiles clean

---

### Phase 4 — Library UI ✅
*Goal: track list, sidebar navigation, and selection all work.*

- [x] `src/styles/` — `_variables.scss` (Catppuccin Mocha tokens), `_reset.scss`, `global.scss`
- [x] `src/types.ts` — all TypeScript interfaces mirroring DTOs
- [x] `<Sidebar>` + `<DirTree>` + `<PlaylistList>` + `<TagPills>` + `<SidebarFooter>`
- [x] `<SearchBar>` (debounced, calls `search_tracks`)
- [x] `<TrackList>` (virtualized with `@tanstack/react-virtual`) + `<TrackRow>` + `<ExpandedTrackRow>`
- [x] Wire nav clicks → `invoke(nav_all/nav_select_dir/…)` → `libraryStore.setTracks`
- [x] Wire album art: `<img src={`art://localhost/${id}`}>` per row
- [x] Wire `track_clicked` (plain/shift/meta selection), `expand_track`, `save_notes` (debounced)
- [x] Added `get_sidebar_data` Tauri command for initial sidebar load

---

### Phase 5 — Transport and Waveform ✅
*Goal: click-to-play, waveform display, seek, playhead animation, prev/next.*

- [x] `<Waveform>` — displays PNG from `get_waveform`; click → `seek(x/width)`; CSS playhead line
- [x] `<NowPlaying>` — `art://` album art, title/artist, Prev/Play-Pause/Next buttons
- [x] `<PlayerPanel>` — wraps both
- [x] Wire `position-changed` event → playhead position
- [x] Wire `waveform-ready` event → call `get_waveform(id, width, height)` → `<img>`
- [x] Prev/Next: resolve adjacent ID from `libraryStore.tracks` in frontend, call `play_track(id)`
- [x] Wire `track-loaded` event → update playerStore (title/artist/duration/isPlaying)

---

### Phase 6 — Playlists and Tags ✅
*Goal: full CRUD for playlists and tags; selection-based tag assignment; drag-to-reorder.*

- [x] Playlist creation form in sidebar (inline input)
- [x] `add_to_playlist` via drag-to-sidebar-pill; `remove_from_playlist` (× in expanded row)
- [x] `reorder_playlist_tracks` via drag-and-drop within track list (playlist nav only)
- [x] Tag assignment panel (visible when `selectedIds.size > 0`): solid/faded/empty pills, click toggles
- [x] Tag creation, editing (inline + color picker), deletion in sidebar
- [x] `<TagChip>` shared component

---

### Phase 7 — Analysis Queue + Settings Drawer ✅
*Goal: analysis progress overlay; live waveform settings.*

- [x] `AnalysisQueue` already in `src-tauri/src/analysis.rs` with `AppHandle`
- [x] `<AnalysisOverlay>` — bottom-right "Analyzing N tracks…" badge
- [x] `<SettingsDrawer>` — 10 waveform controls (sliders + selects); calls `update_waveform_setting`
- [x] Gear icon in `<SidebarFooter>` toggles `playerStore.settingsOpen`
- [x] Settings changes emit `waveform-ready` → waveform refreshes live

---

### Phase 8 — Drag-and-Drop + Polish ✅
*Goal: file/dir drop, keyboard shortcuts, waveform resize, duplicate dir modal.*

- [x] `getCurrentWebview().onDragDropEvent` → `add_directory_path(path)` (bypasses rfd)
- [x] Duplicate directory confirmation modal (replaces alert())
- [x] Keyboard shortcuts: `Space` = play/pause, `←`/`→` = seek ±5s via `useEffect` listener
- [x] `ResizeObserver` on waveform container → re-fetch PNG on resize
- [x] SCSS polish — antialiased fonts, button reset, consistent spacing

---

### Phase 9 — Delete `ss-app` + Final Cleanup ✅
*Goal: zero Slint code in the workspace.*

- [x] Remove `crates/ss-app/` from filesystem
- [x] Remove `slint` from workspace `Cargo.toml` entirely
- [x] `cargo build` + `cargo clippy` clean with no Slint anywhere
- [x] `bun run build` produces clean production bundle
- [x] Confirmed: no `.slint` files, no `slint` in any `Cargo.toml`

---

## Architecture Reference

### Image delivery
| Asset | Mechanism | Frontend usage |
|-------|-----------|----------------|
| Album art | `art://localhost/{track_id}` custom URI scheme | `<img src="art://localhost/42">` |
| Waveform | `get_waveform(id, w, h)` → `ipc::Response` (PNG bytes → `ArrayBuffer`) | `URL.createObjectURL(blob)` → `<img>` |

### Rust → React events
| Event | Payload | Trigger |
|-------|---------|---------|
| `position-changed` | `{position, duration}` | Audio engine 100ms tick |
| `track-finished` | `{}` | Audio engine |
| `analysis-progress` | `{pending_count}` | AnalysisQueue worker |
| `waveform-ready` | `{track_id}` | After render (frontend calls `get_waveform`) |
| `library-changed` | `{}` | File watcher / scan complete |
| `dir-tree-updated` | `{items}` | After dir mutations |
| `sidebar-playlists-updated` | `{playlists}` | After playlist mutations |
| `sidebar-tags-updated` | `{tags}` | After tag mutations |

### What's skipped / deferred
- Waveform thumbnails per track-list row
- Canvas/WebGL waveform (PNG approach used; three.js can be layered on later)
- Separate Tauri `SettingsWindow` (replaced by slide-in drawer)
- `ss-api` HTTP server (already unused)
- Markers system
