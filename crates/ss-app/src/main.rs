use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use slint::Model as _;
use tracing::info;

use ss_audio::{analyze_track, AudioEngine};
use ss_core::{AudioCommand, AudioEvent, Track};
use ss_db::Db;
use ss_library::{FileWatcher, LibraryEvent, Scanner};
use ss_waveform::{render_to_pixels, ViewPort, WaveformBucket, WaveformRenderSettings};

mod settings;
use settings::{load_settings, save_settings, AppSettings};

slint::include_modules!();

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("singing_sand=debug".parse().unwrap())
                .add_directive("ss_audio=debug".parse().unwrap())
                .add_directive("ss_library=debug".parse().unwrap()),
        )
        .init();
}

async fn open_db() -> Result<Arc<Db>> {
    let db_path = PathBuf::from("singing-sand.db");
    let db = Db::open(&db_path)
        .await
        .context("failed to open database")?;
    db.migrate().await.context("database migration failed")?;
    Ok(Arc::new(db))
}

/// Decode raw JPEG/PNG bytes and resize to `size × size`.
/// Returns a `SharedPixelBuffer` (Send) so this can be called in `spawn_blocking`.
/// Call `slint::Image::from_rgb8(buf)` on the Slint thread to get an Image.
fn decode_art_buf(bytes: &[u8], size: u32) -> Option<slint::SharedPixelBuffer<slint::Rgb8Pixel>> {
    let img = image::load_from_memory(bytes).ok()?;
    let resized = img.resize_to_fill(size, size, image::imageops::FilterType::Triangle);
    let rgb = resized.to_rgb8();
    let (w, h) = (rgb.width(), rgb.height());
    let mut buf = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(w, h);
    buf.make_mut_bytes().copy_from_slice(rgb.as_raw());
    Some(buf)
}

fn track_to_item(t: &Track) -> TrackItem {
    TrackItem {
        id: t.id as i32,
        title: t.title.clone().unwrap_or_default().into(),
        artist: t.artist.clone().unwrap_or_default().into(),
        album: t.album.clone().unwrap_or_default().into(),
        duration_secs: t.duration_secs.unwrap_or(0.0) as f32,
        art: slint::Image::default(),
    }
}

fn tracks_to_model_rc(tracks: &[Track]) -> slint::ModelRc<TrackItem> {
    slint::ModelRc::new(slint::VecModel::from(
        tracks.iter().map(track_to_item).collect::<Vec<_>>(),
    ))
}

/// Spawn a background task that progressively fills in album art thumbnails.
/// For each track with cached art, decodes a 44px buffer and updates that row
/// in the window's current track model (verified by track ID to guard against
/// nav changes that replace the model mid-load).
fn spawn_art_loader(
    tracks: Vec<Track>,
    weak: slint::Weak<AppWindow>,
    db: Arc<Db>,
    rt_handle: tokio::runtime::Handle,
) {
    rt_handle.clone().spawn(async move {
        for (i, track) in tracks.iter().enumerate() {
            let track_id = track.id;
            let bytes = match db.get_album_art(track_id).await {
                Ok(Some(b)) => b,
                _ => continue,
            };
            let buf = tokio::task::spawn_blocking(move || decode_art_buf(&bytes, 44))
                .await
                .ok()
                .flatten();
            let Some(buf) = buf else { continue };

            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(w) = weak.upgrade() else { return };
                let model = w.get_tracks();
                let Some(mut row) = model.row_data(i) else { return };
                if row.id == track_id as i32 {
                    row.art = slint::Image::from_rgb8(buf);
                    model.set_row_data(i, row);
                }
            });
        }
    });
}

/// Core playback pipeline — shared by play-track, next-track, and prev-track.
async fn start_playback(
    track_id: i64,
    db: Arc<Db>,
    engine: Arc<AudioEngine>,
    weak: slint::Weak<AppWindow>,
    render_settings: Arc<Mutex<WaveformRenderSettings>>,
    current_bands: Arc<Mutex<Vec<WaveformBucket>>>,
) {
    let track = match db.get_track(track_id).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            tracing::warn!("start_playback: track {track_id} not found");
            return;
        }
        Err(e) => {
            tracing::error!("start_playback db error: {e}");
            return;
        }
    };

    engine.send(AudioCommand::LoadAndPlay {
        path: track.path.clone(),
        start_sec: 0.0,
    });

    let duration = track.duration_secs.unwrap_or(0.0) as f32;
    let now_title: slint::SharedString = track.title.clone().unwrap_or_default().into();
    let now_artist: slint::SharedString = track.artist.clone().unwrap_or_default().into();

    // Load tag chips.
    let all_tags = db.list_tags().await.unwrap_or_default();
    let assigned = db.list_tags_for_track(track_id).await.unwrap_or_default();
    let assigned_ids: std::collections::HashSet<i64> = assigned.iter().map(|t| t.id).collect();
    let tag_items: Vec<TagItem> = all_tags
        .iter()
        .map(|t| TagItem {
            id: t.id as i32,
            name: t.name.clone().into(),
            assigned: assigned_ids.contains(&t.id),
        })
        .collect();

    // Decode now-playing art to a pixel buffer (Send).
    let now_art_buf = match db.get_album_art(track_id).await.unwrap_or(None) {
        Some(bytes) => {
            tokio::task::spawn_blocking(move || decode_art_buf(&bytes, 80))
                .await
                .ok()
                .flatten()
        }
        None => None,
    };

    // Push immediate state to UI (create Image on Slint thread).
    let tag_items_clone = tag_items.clone();
    let now_title_clone = now_title.clone();
    let now_artist_clone = now_artist.clone();
    let weak2 = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(w) = weak2.upgrade() {
            let now_art = now_art_buf
                .map(slint::Image::from_rgb8)
                .unwrap_or_default();
            w.set_duration(duration);
            w.set_position(0.0);
            w.set_current_track_tags(slint::ModelRc::new(slint::VecModel::from(tag_items_clone)));
            w.set_now_playing_art(now_art);
            w.set_now_playing_title(now_title_clone);
            w.set_now_playing_artist(now_artist_clone);
        }
    });

    // Load or compute frequency-band waveform.
    let path = track.path.clone();
    let bands: Vec<[f32; 3]> = match db.get_waveform_bands(track_id).await {
        Ok(Some(cached)) => cached,
        _ => {
            let path2 = path.clone();
            match tokio::task::spawn_blocking(move || analyze_track(&path2, 1000)).await {
                Ok(Ok(result)) => {
                    let arrays: Vec<[f32; 3]> =
                        result.waveform.iter().map(|b| b.to_array()).collect();
                    if let Err(e) = db.save_waveform_bands(track_id, &arrays).await {
                        tracing::warn!("failed to cache waveform bands: {e}");
                    }
                    arrays
                }
                _ => vec![],
            }
        }
    };

    // Convert [f32;3] arrays → WaveformBucket for rendering.
    let buckets: Vec<WaveformBucket> =
        bands.iter().map(|&arr| WaveformBucket::from_array(arr)).collect();
    *current_bands.lock().unwrap() = buckets.clone();

    let settings_snap = render_settings.lock().unwrap().clone();
    let pixel_buf = render_to_pixels(&buckets, &settings_snap, ViewPort::default());

    let _ = slint::invoke_from_event_loop(move || {
        if let Some(w) = weak.upgrade() {
            w.set_waveform_image(slint::Image::from_rgb8(pixel_buf));
        }
    });
}

/// Returns an error message if `path` is already covered by any of `existing` roots,
/// either as an exact match, a child, or a parent.
fn check_duplicate_dir(path: &str, existing: &[PathBuf]) -> Option<String> {
    let p = path.trim_end_matches('/');
    for r in existing {
        let r_str = r.to_string_lossy();
        let r = r_str.trim_end_matches('/');
        if p == r || p.starts_with(&format!("{r}/")) || r.starts_with(&format!("{p}/")) {
            return Some("This directory is already in your library.".into());
        }
    }
    None
}

/// Recursively append a directory node (and its expanded children) to `result`.
fn append_dir_node(
    result: &mut Vec<DirTreeItem>,
    dir: &str,
    all_dirs: &[&str],
    indent: i32,
    expanded: &HashMap<String, bool>,
    is_root: bool,
) {
    let prefix = format!("{dir}/");
    let direct_children: Vec<&str> = all_dirs
        .iter()
        .copied()
        .filter(|&d| d.starts_with(&prefix) && !d[prefix.len()..].contains('/'))
        .collect();

    let has_children = !direct_children.is_empty();
    let is_expanded = expanded.get(dir).copied().unwrap_or(false);

    let name: slint::SharedString = Path::new(dir)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.to_string())
        .into();

    result.push(DirTreeItem {
        path: dir.to_string().into(),
        name,
        indent: indent.min(4),
        has_children,
        is_expanded,
        is_root,
    });

    if is_expanded {
        for child in direct_children {
            append_dir_node(result, child, all_dirs, indent + 1, expanded, false);
        }
    }
}

/// Build the flat `DirTreeItem` list from scanned roots and track directory paths.
fn build_dir_tree_items(
    roots: &[PathBuf],
    track_dirs: &HashSet<String>,
    expanded: &HashMap<String, bool>,
) -> Vec<DirTreeItem> {
    let mut sorted: Vec<&str> = track_dirs.iter().map(|s| s.as_str()).collect();
    sorted.sort_unstable();

    let mut result = Vec::new();
    for root in roots {
        let root_str = root.to_string_lossy();
        append_dir_node(&mut result, root_str.as_ref(), &sorted, 0, expanded, true);
    }
    result
}

/// Unique parent directories for all tracks (used to build the sidebar tree).
fn track_dirs_from(tracks: &[Track]) -> HashSet<String> {
    tracks
        .iter()
        .filter_map(|t| t.path.parent().map(|p| p.to_string_lossy().into_owned()))
        .collect()
}

/// Rebuild the dir tree from the DB and push the new list to the Slint UI.
async fn refresh_dir_tree(
    db: &Arc<Db>,
    weak: &slint::Weak<AppWindow>,
    expanded: &Arc<Mutex<HashMap<String, bool>>>,
) {
    let dirs = db.list_scanned_dirs().await.unwrap_or_default();
    let all_tracks = db.list_tracks().await.unwrap_or_default();
    let tdirs = track_dirs_from(&all_tracks);
    let items = {
        let exp = expanded.lock().unwrap();
        build_dir_tree_items(&dirs, &tdirs, &exp)
    };
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(w) = weak.upgrade() {
            w.set_dir_tree_items(slint::ModelRc::new(slint::VecModel::from(items)));
        }
    });
}

fn main() -> Result<()> {
    init_tracing();

    let mut args = std::env::args().skip(1).peekable();

    match args.peek().map(String::as_str) {
        Some("scan") => {
            args.next();
            let parts: Vec<String> = args.collect();
            anyhow::ensure!(!parts.is_empty(), "Usage: singing-sand scan <directory>");
            let dir: PathBuf = parts.join(" ").into();
            anyhow::ensure!(dir.is_dir(), "not a directory: {}", dir.display());
            tokio::runtime::Runtime::new()?.block_on(cmd_scan(dir))
        }
        Some("play") => {
            args.next();
            let path: PathBuf = args
                .next()
                .context("Usage: singing-sand play <audio-file>")?
                .into();
            anyhow::ensure!(path.exists(), "file not found: {}", path.display());
            cmd_play_cli(path)
        }
        _ => cmd_gui(),
    }
}

// ── Commands ──────────────────────────────────────────────────────────────────

async fn cmd_scan(dir: PathBuf) -> Result<()> {
    info!("scanning {}", dir.display());
    let db = open_db().await?;
    let scanner = Scanner::new(Arc::clone(&db));
    let stats = scanner.scan_dir(&dir).await?;
    println!(
        "scan complete — {} upserted, {} errors, {} skipped",
        stats.upserted, stats.errors, stats.skipped
    );

    // Analyse new tracks: generate frequency-band waveforms.
    let mut analysed = 0usize;
    let mut analysis_errors = 0usize;
    for (track_id, path) in stats.upserted_tracks {
        // Skip if waveform is already cached.
        let has_waveform = matches!(db.get_waveform_bands(track_id).await, Ok(Some(_)));
        if has_waveform {
            continue;
        }

        match tokio::task::spawn_blocking(move || analyze_track(&path, 1000)).await {
            Ok(Ok(result)) => {
                let arrays: Vec<[f32; 3]> =
                    result.waveform.iter().map(|b| b.to_array()).collect();
                if let Err(e) = db.save_waveform_bands(track_id, &arrays).await {
                    tracing::warn!(track_id, error = %e, "failed to save waveform bands");
                    analysis_errors += 1;
                    continue;
                }
                analysed += 1;
            }
            Ok(Err(e)) => {
                tracing::warn!(track_id, error = %e, "analysis failed");
                analysis_errors += 1;
            }
            Err(e) => {
                tracing::warn!(track_id, error = %e, "analysis task panicked");
                analysis_errors += 1;
            }
        }
    }
    if analysed > 0 || analysis_errors > 0 {
        println!("analysis — {} done, {} errors", analysed, analysis_errors);
    }

    Ok(())
}

/// CLI playback (kept for debugging).
fn cmd_play_cli(path: PathBuf) -> Result<()> {
    use std::time::Duration;
    info!("starting audio engine");
    let engine = AudioEngine::spawn()?;
    engine.send(AudioCommand::LoadAndPlay {
        path: path.clone(),
        start_sec: 0.0,
    });
    info!("playing {} — press Ctrl-C to stop", path.display());
    loop {
        match engine.event_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(AudioEvent::TrackFinished) => {
                info!("playback finished");
                break;
            }
            Ok(AudioEvent::PositionChanged(pos)) => {
                print!("\r  {:.1}s", pos);
                use std::io::Write;
                std::io::stdout().flush().ok();
            }
            Ok(AudioEvent::Error(e)) => {
                anyhow::bail!("audio error: {e}");
            }
            Err(flume::RecvTimeoutError::Timeout) => {}
            Err(flume::RecvTimeoutError::Disconnected) => break,
        }
    }
    println!();
    Ok(())
}

/// Long-lived background analysis queue.
///
/// Send `(track_id, path)` pairs via [`AnalysisQueue::enqueue`]. A pool of up
/// to N concurrent tokio `spawn_blocking` tasks (where N = CPU count) processes
/// the queue in parallel. The Slint `pending-analysis-count` property is
/// updated in real-time via `invoke_from_event_loop`.
struct AnalysisQueue {
    tx: tokio::sync::mpsc::UnboundedSender<(i64, std::path::PathBuf)>,
    pending: Arc<std::sync::atomic::AtomicUsize>,
    weak: slint::Weak<AppWindow>,
}

impl AnalysisQueue {
    fn spawn(
        db: Arc<Db>,
        weak: slint::Weak<AppWindow>,
        rt: &tokio::runtime::Handle,
    ) -> Self {
        let (tx, mut rx) =
            tokio::sync::mpsc::unbounded_channel::<(i64, std::path::PathBuf)>();
        let pending = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let pending_for_struct = Arc::clone(&pending);
        let weak_for_struct = weak.clone();

        rt.spawn(async move {
            let parallelism = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);
            let sem = Arc::new(tokio::sync::Semaphore::new(parallelism));

            while let Some((track_id, path)) = rx.recv().await {
                // Acquire a slot; blocks the loop (and stops reading the channel)
                // when all workers are busy, providing natural backpressure.
                let permit = Arc::clone(&sem).acquire_owned().await.unwrap();
                let db2 = Arc::clone(&db);
                let weak2 = weak.clone();
                let pending2 = Arc::clone(&pending);

                tokio::spawn(async move {
                    let _permit = permit; // released when this task completes

                    let result =
                        tokio::task::spawn_blocking(move || analyze_track(&path, 1000)).await;

                    match result {
                        Ok(Ok(analysis)) => {
                            let arrays: Vec<[f32; 3]> =
                                analysis.waveform.iter().map(|b| b.to_array()).collect();
                            if let Err(e) = db2.save_waveform_bands(track_id, &arrays).await {
                                tracing::warn!(track_id, error = %e, "analysis: save waveform failed");
                            }
                        }
                        Ok(Err(e)) => {
                            tracing::warn!(track_id, error = %e, "analysis failed");
                        }
                        Err(e) => {
                            tracing::warn!(track_id, error = %e, "analysis task panicked");
                        }
                    }

                    let remaining =
                        pending2.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) - 1;
                    update_analysis_counter(&weak2, remaining);
                });
            }
        });

        Self { tx, pending: pending_for_struct, weak: weak_for_struct }
    }

    fn enqueue(&self, tracks: impl IntoIterator<Item = (i64, std::path::PathBuf)>) {
        for item in tracks {
            let count = self.pending.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            update_analysis_counter(&self.weak, count);
            let _ = self.tx.send(item);
        }
    }
}

/// Push the current analysis count to the Slint UI.
fn update_analysis_counter(weak: &slint::Weak<AppWindow>, count: usize) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(w) = weak.upgrade() {
            w.set_pending_analysis_count(count as i32);
        }
    });
}

fn display_style_to_int(s: ss_waveform::DisplayStyle) -> i32 {
    match s {
        ss_waveform::DisplayStyle::Mirrored => 0,
        ss_waveform::DisplayStyle::TopHalf  => 1,
    }
}

fn int_to_display_style(v: i32) -> ss_waveform::DisplayStyle {
    match v {
        1 => ss_waveform::DisplayStyle::TopHalf,
        _ => ss_waveform::DisplayStyle::Mirrored,
    }
}

fn color_scheme_to_int(s: ss_waveform::ColorScheme) -> i32 {
    match s {
        ss_waveform::ColorScheme::AdditivePeachBlueLavender => 0,
        ss_waveform::ColorScheme::Monochrome                => 1,
        ss_waveform::ColorScheme::PerBandSolid              => 2,
        ss_waveform::ColorScheme::Grayscale                 => 3,
    }
}

fn int_to_color_scheme(v: i32) -> ss_waveform::ColorScheme {
    match v {
        1 => ss_waveform::ColorScheme::Monochrome,
        2 => ss_waveform::ColorScheme::PerBandSolid,
        3 => ss_waveform::ColorScheme::Grayscale,
        _ => ss_waveform::ColorScheme::AdditivePeachBlueLavender,
    }
}

/// Re-render the waveform with current settings and push it to the UI.
fn rerender_waveform(
    render_settings: &Arc<Mutex<WaveformRenderSettings>>,
    current_bands: &Arc<Mutex<Vec<WaveformBucket>>>,
    weak: &slint::Weak<AppWindow>,
) {
    let bands = current_bands.lock().unwrap().clone();
    if bands.is_empty() {
        return;
    }
    let s = render_settings.lock().unwrap().clone();
    let buf = render_to_pixels(&bands, &s, ViewPort::default());
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(w) = weak.upgrade() {
            w.set_waveform_image(slint::Image::from_rgb8(buf));
        }
    });
}

/// Launch the Slint GUI.
fn cmd_gui() -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let db = rt.block_on(open_db())?;
    let engine = Arc::new(AudioEngine::spawn()?);

    let window = AppWindow::new().context("failed to create window")?;
    let rt_handle = rt.handle().clone();

    // ── Load persisted settings ──────────────────────────────────────────────
    let app_settings = rt.block_on(load_settings(&db)).unwrap_or_default();
    let render_settings = Arc::new(Mutex::new(app_settings.waveform.clone()));
    let current_bands: Arc<Mutex<Vec<WaveformBucket>>> = Arc::new(Mutex::new(vec![]));

    // Create the settings window (separate OS window, shown on demand).
    let settings_win = SettingsWindow::new().context("failed to create settings window")?;
    {
        let w = &app_settings.waveform;
        settings_win.set_show_low(w.show_low);
        settings_win.set_show_mid(w.show_mid);
        settings_win.set_show_high(w.show_high);
        settings_win.set_amplitude_scale(w.amplitude_scale);
        settings_win.set_low_gain(w.low_gain);
        settings_win.set_mid_gain(w.mid_gain);
        settings_win.set_high_gain(w.high_gain);
        settings_win.set_display_style(display_style_to_int(w.display_style));
        settings_win.set_color_scheme(color_scheme_to_int(w.color_scheme));
        settings_win.set_normalize(w.normalize);
    }
    let settings_win = Arc::new(settings_win);

    // ── Initial data load ────────────────────────────────────────────────────

    let initial_tracks = rt.block_on(db.list_tracks())?;
    window.set_tracks(tracks_to_model_rc(&initial_tracks));
    // Clone for art loader so the move into spawn_art_loader is independent.
    let initial_tracks_art = initial_tracks.clone();
    spawn_art_loader(initial_tracks_art, window.as_weak(), Arc::clone(&db), rt_handle.clone());

    let dirs = rt.block_on(db.list_scanned_dirs())?;

    // Build initial directory tree from existing tracks
    let expanded_state = Arc::new(Mutex::new(HashMap::<String, bool>::new()));
    {
        let tdirs = track_dirs_from(&initial_tracks);
        let exp = expanded_state.lock().unwrap();
        let tree_items = build_dir_tree_items(&dirs, &tdirs, &exp);
        window.set_dir_tree_items(slint::ModelRc::new(slint::VecModel::from(tree_items)));
    }

    let playlists = rt.block_on(db.list_playlists())?;
    let pl_entries: Vec<SidebarEntry> = playlists
        .iter()
        .map(|p| SidebarEntry { id: p.id as i32, label: p.name.clone().into() })
        .collect();
    window.set_sidebar_playlists(slint::ModelRc::new(slint::VecModel::from(pl_entries)));

    let tags = rt.block_on(db.list_tags())?;
    let tag_entries: Vec<SidebarEntry> = tags
        .iter()
        .map(|t| SidebarEntry { id: t.id as i32, label: t.name.clone().into() })
        .collect();
    window.set_sidebar_tags(slint::ModelRc::new(slint::VecModel::from(tag_entries)));

    info!(
        "loaded {} tracks, {} dirs, {} playlists, {} tags",
        dirs.len(),
        dirs.len(),
        playlists.len(),
        tags.len()
    );

    // ── Background analysis queue ────────────────────────────────────────────

    let analysis_queue = Arc::new(AnalysisQueue::spawn(
        Arc::clone(&db),
        window.as_weak(),
        &rt_handle,
    ));

    // Enqueue all tracks that are missing a waveform or BPM.
    {
        let needs_analysis = rt.block_on(db.list_tracks_needing_analysis()).unwrap_or_default();
        info!("{} tracks queued for background analysis", needs_analysis.len());
        analysis_queue.enqueue(needs_analysis);
    }

    // ── File watcher ─────────────────────────────────────────────────────────

    let (lib_event_tx, lib_event_rx) =
        tokio::sync::mpsc::unbounded_channel::<LibraryEvent>();

    let mut fw = FileWatcher::new(
        Arc::clone(&db),
        rt_handle.clone(),
        lib_event_tx,
    )?;
    for dir in &dirs {
        if let Err(e) = fw.watch(dir) {
            tracing::warn!("failed to watch {}: {e}", dir.display());
        }
    }
    let file_watcher = Arc::new(Mutex::new(fw));

    // Spawn a task that reacts to library events from the file watcher
    {
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let expanded = Arc::clone(&expanded_state);
        let aq = Arc::clone(&analysis_queue);
        let mut lib_event_rx = lib_event_rx;
        rt_handle.spawn(async move {
            while let Some(event) = lib_event_rx.recv().await {
                match event {
                    LibraryEvent::TrackAdded { id, path } => {
                        aq.enqueue(std::iter::once((id, path)));
                    }
                    LibraryEvent::TrackRemoved(_) => {}
                }
                // Refresh dir tree and current track list view
                refresh_dir_tree(&db, &weak, &expanded).await;
                let _ = slint::invoke_from_event_loop({
                    let weak = weak.clone();
                    move || {
                        let Some(w) = weak.upgrade() else { return };
                        match w.get_nav_kind() {
                            0 => { w.invoke_nav_all(); }
                            1 => { w.invoke_nav_select_dir(w.get_nav_dir()); }
                            2 => { w.invoke_nav_playlist(w.get_nav_id()); }
                            3 => { w.invoke_nav_tag(w.get_nav_id()); }
                            _ => {}
                        }
                    }
                });
            }
        });
    }

    // ── Directory management callbacks ───────────────────────────────────────

    {
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        let expanded = Arc::clone(&expanded_state);
        let aq = Arc::clone(&analysis_queue);
        let fw = Arc::clone(&file_watcher);
        window.on_add_directory_clicked(move || {
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let expanded = Arc::clone(&expanded);
            let aq = Arc::clone(&aq);
            let fw = Arc::clone(&fw);
            rt_handle.spawn(async move {
                let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await else {
                    return;
                };
                let path = folder.path().to_path_buf();
                let path_str = path.to_string_lossy().to_string();

                // Duplicate check
                let existing = db.list_scanned_dirs().await.unwrap_or_default();
                if let Some(msg) = check_duplicate_dir(&path_str, &existing) {
                    let msg: slint::SharedString = msg.into();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak.upgrade() {
                            w.set_dir_duplicate_message(msg);
                            w.set_show_dir_duplicate_dialog(true);
                        }
                    });
                    return;
                }

                // Scan + record in DB
                let scanner = Scanner::new(Arc::clone(&db));
                match scanner.scan_dir(&path).await {
                    Ok(stats) => {
                        info!(
                            "added dir {path_str} — {} upserted, {} errors",
                            stats.upserted, stats.errors
                        );
                        aq.enqueue(stats.upserted_tracks);
                    }
                    Err(e) => tracing::warn!("scan_dir failed for {path_str}: {e}"),
                }

                // Start watching the new directory
                if let Ok(mut fw) = fw.lock() {
                    if let Err(e) = fw.watch(&path) {
                        tracing::warn!("watch failed for {path_str}: {e}");
                    }
                }

                // Refresh the sidebar tree and trigger a nav reload
                refresh_dir_tree(&db, &weak, &expanded).await;
                let _ = slint::invoke_from_event_loop({
                    let weak = weak.clone();
                    move || {
                        let Some(w) = weak.upgrade() else { return };
                        if w.get_nav_kind() == 0 {
                            w.invoke_nav_all();
                        }
                    }
                });
            });
        });
    }

    {
        let expanded = Arc::clone(&expanded_state);
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        window.on_toggle_dir_expanded(move |path| {
            let path_str = path.to_string();
            {
                let mut exp = expanded.lock().unwrap();
                let e = exp.entry(path_str).or_insert(false);
                *e = !*e;
            }
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let expanded = Arc::clone(&expanded);
            rt_handle.spawn(async move {
                refresh_dir_tree(&db, &weak, &expanded).await;
            });
        });
    }

    {
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        let expanded = Arc::clone(&expanded_state);
        let fw = Arc::clone(&file_watcher);
        window.on_remove_scanned_dir(move |path| {
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let expanded = Arc::clone(&expanded);
            let fw = Arc::clone(&fw);
            let path_str = path.to_string();
            rt_handle.spawn(async move {
                // Delete all tracks under this root
                let tracks = db.list_tracks_in_dir(&path_str).await.unwrap_or_default();
                for t in &tracks {
                    if let Err(e) =
                        db.delete_track_by_path(&t.path.to_string_lossy()).await
                    {
                        tracing::warn!("delete_track failed: {e}");
                    }
                }
                if let Err(e) = db.remove_scanned_dir(&path_str).await {
                    tracing::warn!("remove_scanned_dir failed: {e}");
                }
                // Stop watching
                if let Ok(mut fw) = fw.lock() {
                    let _ = fw.unwatch(Path::new(&path_str));
                }
                // Remove from expanded state
                expanded.lock().unwrap().remove(&path_str);
                // Rebuild tree
                refresh_dir_tree(&db, &weak, &expanded).await;
                // If currently viewing the removed dir, switch to All Tracks
                let _ = slint::invoke_from_event_loop({
                    let weak = weak.clone();
                    move || {
                        let Some(w) = weak.upgrade() else { return };
                        if w.get_nav_kind() == 1 && w.get_nav_dir() == path_str.as_str() {
                            w.set_nav_kind(0);
                            w.set_nav_dir("".into());
                            w.invoke_nav_all();
                        } else if w.get_nav_kind() == 0 {
                            w.invoke_nav_all();
                        }
                    }
                });
            });
        });
    }

    // ── Nav callbacks ────────────────────────────────────────────────────────

    {
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        window.on_nav_all(move || {
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let rth = rt_handle.clone();
            let rth2 = rth.clone();
            rth.spawn(async move {
                let tracks = db.list_tracks().await.unwrap_or_default();
                let tracks_for_ui = tracks.clone();
                let weak2 = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak2.upgrade() {
                        w.set_tracks(tracks_to_model_rc(&tracks_for_ui));
                        w.set_expanded_track_id(-1);
                    }
                });
                spawn_art_loader(tracks, weak, Arc::clone(&db), rth2);
            });
        });
    }

    {
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        window.on_nav_select_dir(move |dir_path| {
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let rth = rt_handle.clone();
            let rth2 = rth.clone();
            let dir_str = dir_path.to_string();
            rth.spawn(async move {
                let tracks = db.list_tracks_in_dir(&dir_str).await.unwrap_or_default();
                let tracks_for_ui = tracks.clone();
                let weak2 = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak2.upgrade() {
                        w.set_tracks(tracks_to_model_rc(&tracks_for_ui));
                        w.set_expanded_track_id(-1);
                    }
                });
                spawn_art_loader(tracks, weak, Arc::clone(&db), rth2);
            });
        });
    }

    {
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        window.on_nav_playlist(move |playlist_id| {
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let rth = rt_handle.clone();
            let rth2 = rth.clone();
            rth.spawn(async move {
                let tracks =
                    db.list_tracks_in_playlist(playlist_id as i64).await.unwrap_or_default();
                let tracks_for_ui = tracks.clone();
                let weak2 = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak2.upgrade() {
                        w.set_tracks(tracks_to_model_rc(&tracks_for_ui));
                        w.set_expanded_track_id(-1);
                    }
                });
                spawn_art_loader(tracks, weak, Arc::clone(&db), rth2);
            });
        });
    }

    {
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        window.on_nav_tag(move |tag_id| {
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let rth = rt_handle.clone();
            let rth2 = rth.clone();
            rth.spawn(async move {
                let tracks = db.list_tracks_with_tag(tag_id as i64).await.unwrap_or_default();
                let tracks_for_ui = tracks.clone();
                let weak2 = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak2.upgrade() {
                        w.set_tracks(tracks_to_model_rc(&tracks_for_ui));
                        w.set_expanded_track_id(-1);
                    }
                });
                spawn_art_loader(tracks, weak, Arc::clone(&db), rth2);
            });
        });
    }

    // ── Playlist / tag creation ──────────────────────────────────────────────

    {
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        window.on_create_playlist(move |name| {
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let name_str = name.to_string();
            rt_handle.spawn(async move {
                if let Err(e) = db.insert_playlist(&name_str).await {
                    tracing::warn!("create_playlist failed: {e}");
                    return;
                }
                let playlists = db.list_playlists().await.unwrap_or_default();
                let entries: Vec<SidebarEntry> = playlists
                    .iter()
                    .map(|p| SidebarEntry { id: p.id as i32, label: p.name.clone().into() })
                    .collect();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        w.set_sidebar_playlists(slint::ModelRc::new(slint::VecModel::from(
                            entries,
                        )));
                    }
                });
            });
        });
    }

    {
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        window.on_create_tag(move |name| {
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let name_str = name.to_string();
            rt_handle.spawn(async move {
                if let Err(e) = db.insert_tag(&name_str).await {
                    tracing::warn!("create_tag failed: {e}");
                    return;
                }
                let tags = db.list_tags().await.unwrap_or_default();
                let entries: Vec<SidebarEntry> = tags
                    .iter()
                    .map(|t| SidebarEntry { id: t.id as i32, label: t.name.clone().into() })
                    .collect();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        w.set_sidebar_tags(slint::ModelRc::new(slint::VecModel::from(entries)));
                    }
                });
            });
        });
    }

    // ── Track → playlist ─────────────────────────────────────────────────────

    {
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        window.on_add_to_playlist(move |playlist_id, track_id| {
            let db = Arc::clone(&db);
            let weak = weak.clone();
            rt_handle.spawn(async move {
                if track_id < 0 {
                    return;
                }
                if let Err(e) =
                    db.add_track_to_playlist(track_id as i64, playlist_id as i64).await
                {
                    tracing::warn!("add_to_playlist failed: {e}");
                }
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        if w.get_nav_kind() == 2 && w.get_nav_id() == playlist_id {
                            w.invoke_nav_playlist(playlist_id);
                        }
                    }
                });
            });
        });
    }

    // ── Remove track from playlist ───────────────────────────────────────────

    {
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        window.on_remove_from_playlist(move |playlist_id, track_id| {
            let db = Arc::clone(&db);
            let weak = weak.clone();
            rt_handle.spawn(async move {
                if playlist_id < 0 || track_id < 0 {
                    return;
                }
                if let Err(e) =
                    db.remove_track_from_playlist(track_id as i64, playlist_id as i64).await
                {
                    tracing::warn!("remove_from_playlist failed: {e}");
                    return;
                }
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        w.invoke_nav_playlist(playlist_id);
                    }
                });
            });
        });
    }

    // ── Tag toggle ───────────────────────────────────────────────────────────

    {
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        window.on_toggle_tag(move |tag_id, should_assign| {
            let db = Arc::clone(&db);
            let weak = weak.clone();
            rt_handle.spawn(async move {
                let track_id = {
                    if let Some(w) = weak.upgrade() {
                        w.get_current_track_id() as i64
                    } else {
                        return;
                    }
                };
                if track_id < 0 {
                    return;
                }
                let result = if should_assign {
                    db.assign_tag(track_id, tag_id as i64).await
                } else {
                    db.unassign_tag(track_id, tag_id as i64).await.map(|_| ())
                };
                if let Err(e) = result {
                    tracing::warn!("toggle_tag failed: {e}");
                    return;
                }
                let all_tags = db.list_tags().await.unwrap_or_default();
                let assigned = db.list_tags_for_track(track_id).await.unwrap_or_default();
                let assigned_ids: std::collections::HashSet<i64> =
                    assigned.iter().map(|t| t.id).collect();
                let tag_items: Vec<TagItem> = all_tags
                    .iter()
                    .map(|t| TagItem {
                        id: t.id as i32,
                        name: t.name.clone().into(),
                        assigned: assigned_ids.contains(&t.id),
                    })
                    .collect();
                let nav_kind = weak.upgrade().map(|w| w.get_nav_kind()).unwrap_or(0);
                let nav_tag_id = weak.upgrade().map(|w| w.get_nav_id()).unwrap_or(-1);
                let tracks_opt = if nav_kind == 3 {
                    Some(
                        db.list_tracks_with_tag(nav_tag_id as i64)
                            .await
                            .unwrap_or_default(),
                    )
                } else {
                    None
                };
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        w.set_current_track_tags(slint::ModelRc::new(slint::VecModel::from(
                            tag_items,
                        )));
                        if let Some(ref tracks) = tracks_opt {
                            w.set_tracks(tracks_to_model_rc(tracks));
                        }
                    }
                });
            });
        });
    }

    // ── Expand track row ─────────────────────────────────────────────────────

    {
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        window.on_expand_track(move |track_id| {
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let track_id = track_id as i64;
            rt_handle.spawn(async move {
                let (tags_res, playlists_res, notes_res, track_res) = tokio::join!(
                    db.list_tags_for_track(track_id),
                    db.list_playlists_for_track(track_id),
                    db.get_track_notes(track_id),
                    db.get_track(track_id),
                );

                let tag_items: Vec<ExpandedTagItem> = tags_res
                    .unwrap_or_default()
                    .iter()
                    .map(|t| ExpandedTagItem { id: t.id as i32, name: t.name.clone().into() })
                    .collect();

                let pl_items: Vec<ExpandedPlaylistItem> = playlists_res
                    .unwrap_or_default()
                    .iter()
                    .map(|p| ExpandedPlaylistItem { id: p.id as i32, name: p.name.clone().into() })
                    .collect();

                let notes: slint::SharedString =
                    notes_res.unwrap_or(None).unwrap_or_default().into();

                let duration_fmt: slint::SharedString = track_res
                    .ok()
                    .flatten()
                    .and_then(|t| t.duration_secs)
                    .map(|s| {
                        let total = s as u64;
                        format!("{}:{:02}", total / 60, total % 60)
                    })
                    .unwrap_or_default()
                    .into();

                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        // Only update if the user hasn't switched to a different row
                        if w.get_expanded_track_id() == track_id as i32 {
                            w.set_expanded_track_tags(slint::ModelRc::new(
                                slint::VecModel::from(tag_items),
                            ));
                            w.set_expanded_track_playlists(slint::ModelRc::new(
                                slint::VecModel::from(pl_items),
                            ));
                            w.set_expanded_track_notes(notes);
                            w.set_expanded_track_duration_formatted(duration_fmt);
                        }
                    }
                });
            });
        });
    }

    // ── Remove tag from expanded row ─────────────────────────────────────────

    {
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        window.on_remove_tag_from_expanded(move |tag_id| {
            let db = Arc::clone(&db);
            let weak = weak.clone();
            // Read track ID on Slint thread before spawning
            let track_id = match weak.upgrade() {
                Some(w) => w.get_expanded_track_id() as i64,
                None => return,
            };
            if track_id < 0 {
                return;
            }
            rt_handle.spawn(async move {
                if let Err(e) = db.unassign_tag(track_id, tag_id as i64).await {
                    tracing::warn!("remove_tag_from_expanded failed: {e}");
                    return;
                }
                let tags = db.list_tags_for_track(track_id).await.unwrap_or_default();
                let tag_items: Vec<ExpandedTagItem> = tags
                    .iter()
                    .map(|t| ExpandedTagItem { id: t.id as i32, name: t.name.clone().into() })
                    .collect();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        if w.get_expanded_track_id() == track_id as i32 {
                            w.set_expanded_track_tags(slint::ModelRc::new(
                                slint::VecModel::from(tag_items),
                            ));
                        }
                    }
                });
            });
        });
    }

    // ── Notes autosave (debounced) ───────────────────────────────────────────

    let (notes_tx, notes_rx) = flume::unbounded::<(i64, String)>();

    {
        let db = Arc::clone(&db);
        rt_handle.spawn(async move {
            loop {
                let Ok((mut id, mut text)) = notes_rx.recv_async().await else { break };
                // Drain trailing keystrokes within 600ms
                loop {
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(600),
                        notes_rx.recv_async(),
                    )
                    .await
                    {
                        Ok(Ok((new_id, new_text))) => {
                            id = new_id;
                            text = new_text;
                        }
                        _ => break,
                    }
                }
                if let Err(e) = db.update_track_notes(id, &text).await {
                    tracing::warn!("update_track_notes failed: {e}");
                }
            }
        });
    }

    {
        window.on_save_notes(move |track_id, text| {
            let _ = notes_tx.send((track_id as i64, text.to_string()));
        });
    }

    // ── Play track ───────────────────────────────────────────────────────────

    {
        let engine = Arc::clone(&engine);
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        let render_settings = Arc::clone(&render_settings);
        let current_bands = Arc::clone(&current_bands);
        window.on_play_track(move |id| {
            let engine = Arc::clone(&engine);
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let rs = Arc::clone(&render_settings);
            let cb = Arc::clone(&current_bands);
            rt_handle.spawn(start_playback(id as i64, db, engine, weak, rs, cb));
        });
    }

    // ── Next / Prev track ────────────────────────────────────────────────────

    {
        let engine = Arc::clone(&engine);
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        let render_settings = Arc::clone(&render_settings);
        let current_bands = Arc::clone(&current_bands);
        window.on_next_track(move || {
            let engine = Arc::clone(&engine);
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let rt_handle = rt_handle.clone();
            let rs = Arc::clone(&render_settings);
            let cb = Arc::clone(&current_bands);
            if let Some(w) = weak.upgrade() {
                let tracks = w.get_tracks();
                let current = w.get_current_track_id();
                if let Some(next_id) = find_adjacent_track(&tracks, current, 1) {
                    w.set_current_track_id(next_id);
                    w.set_is_playing(true);
                    rt_handle.spawn(start_playback(next_id as i64, db, engine, weak, rs, cb));
                }
            }
        });
    }

    {
        let engine = Arc::clone(&engine);
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        let render_settings = Arc::clone(&render_settings);
        let current_bands = Arc::clone(&current_bands);
        window.on_prev_track(move || {
            let engine = Arc::clone(&engine);
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let rt_handle = rt_handle.clone();
            let rs = Arc::clone(&render_settings);
            let cb = Arc::clone(&current_bands);
            if let Some(w) = weak.upgrade() {
                let tracks = w.get_tracks();
                let current = w.get_current_track_id();
                if let Some(prev_id) = find_adjacent_track(&tracks, current, -1) {
                    w.set_current_track_id(prev_id);
                    w.set_is_playing(true);
                    rt_handle.spawn(start_playback(prev_id as i64, db, engine, weak, rs, cb));
                }
            }
        });
    }

    // ── Transport ────────────────────────────────────────────────────────────

    {
        let engine = Arc::clone(&engine);
        window.on_play(move || engine.send(AudioCommand::Play));
    }
    {
        let engine = Arc::clone(&engine);
        window.on_pause(move || engine.send(AudioCommand::Pause));
    }
    {
        let engine = Arc::clone(&engine);
        window.on_stop(move || engine.send(AudioCommand::Stop));
    }
    {
        let engine = Arc::clone(&engine);
        let weak = window.as_weak();
        window.on_seek(move |fraction| {
            if let Some(w) = weak.upgrade() {
                let duration = w.get_duration() as f64;
                let target = (fraction as f64 * duration).max(0.0);
                engine.send(AudioCommand::Seek(target));
            }
        });
    }

    // ── Settings window callbacks ────────────────────────────────────────────

    {
        let settings_win_weak: Arc<SettingsWindow> = Arc::clone(&settings_win);
        window.on_settings_clicked(move || {
            settings_win_weak.show().ok();
        });
    }

    macro_rules! settings_cb {
        ($method:ident, $field:ident, $val_ty:ty, $convert:expr) => {{
            let render_settings = Arc::clone(&render_settings);
            let current_bands = Arc::clone(&current_bands);
            let weak = window.as_weak();
            let db = Arc::clone(&db);
            let rt_handle = rt_handle.clone();
            settings_win.$method(move |val| {
                let converted: $val_ty = ($convert)(val);
                render_settings.lock().unwrap().$field = converted;
                rerender_waveform(&render_settings, &current_bands, &weak);
                let s = render_settings.lock().unwrap().clone();
                let db = Arc::clone(&db);
                rt_handle.spawn(async move {
                    let _ = save_settings(&db, &AppSettings { waveform: s }).await;
                });
            });
        }};
    }

    settings_cb!(on_show_low_changed,        show_low,        bool, |v| v);
    settings_cb!(on_show_mid_changed,        show_mid,        bool, |v| v);
    settings_cb!(on_show_high_changed,       show_high,       bool, |v| v);
    settings_cb!(on_amplitude_scale_changed, amplitude_scale, f32,  |v| v);
    settings_cb!(on_low_gain_changed,        low_gain,        f32,  |v| v);
    settings_cb!(on_mid_gain_changed,        mid_gain,        f32,  |v| v);
    settings_cb!(on_high_gain_changed,       high_gain,       f32,  |v| v);
    settings_cb!(on_display_style_changed,   display_style,   ss_waveform::DisplayStyle, |v: i32| int_to_display_style(v));
    settings_cb!(on_color_scheme_changed,    color_scheme,    ss_waveform::ColorScheme,  |v: i32| int_to_color_scheme(v));
    settings_cb!(on_normalize_changed,       normalize,       bool, |v| v);

    // ── Audio event forwarding ───────────────────────────────────────────────

    {
        let event_rx = engine.event_rx.clone();
        let weak = window.as_weak();
        rt.spawn(async move {
            while let Ok(event) = event_rx.recv_async().await {
                let weak = weak.clone();
                match event {
                    AudioEvent::PositionChanged(pos) => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = weak.upgrade() {
                                w.set_position(pos as f32);
                            }
                        });
                    }
                    AudioEvent::TrackFinished => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = weak.upgrade() {
                                w.set_is_playing(false);
                            }
                        });
                    }
                    AudioEvent::Error(msg) => {
                        tracing::error!("audio error: {msg}");
                    }
                }
            }
        });
    }

    window.run().context("window run failed")?;
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Find the ID of the track adjacent to `current_id` in the model.
/// `offset` = +1 for next, -1 for prev; wraps around.
fn find_adjacent_track(
    model: &slint::ModelRc<TrackItem>,
    current_id: i32,
    offset: i32,
) -> Option<i32> {
    let len = model.row_count();
    if len == 0 {
        return None;
    }
    let current_idx = (0..len).find(|&i| model.row_data(i).map(|r| r.id) == Some(current_id))?;
    let next_idx = (current_idx as i32 + offset).rem_euclid(len as i32) as usize;
    model.row_data(next_idx).map(|r| r.id)
}
