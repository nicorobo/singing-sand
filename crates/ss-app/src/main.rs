use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use slint::Model as _;
use tracing::info;

use ss_audio::{analyze_track, AudioEngine};
use ss_core::{AudioCommand, AudioEvent, Track};
use ss_db::Db;
use ss_library::Scanner;

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

/// Render frequency-band waveform data into a `SharedPixelBuffer`.
///
/// Each bucket is `[low_rms, mid_rms, high_rms]`. The bar height is the mean
/// amplitude and its color is an additive blend of three band colors:
///   Low  → Catppuccin Peach    rgb(250, 179, 135)
///   Mid  → Catppuccin Blue     rgb(137, 180, 250)  (legacy colour)
///   High → Catppuccin Lavender rgb(203, 166, 247)
fn render_waveform_buffer(bands: &[[f32; 3]]) -> slint::SharedPixelBuffer<slint::Rgb8Pixel> {
    const W: u32 = 1000;
    const H: u32 = 96;
    let bg = slint::Rgb8Pixel { r: 24, g: 24, b: 37 };

    let mut buf = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(W, H);
    let pixels = buf.make_mut_slice();
    for p in pixels.iter_mut() {
        *p = bg;
    }

    for x in 0..W as usize {
        let bucket = (x * bands.len()) / W as usize;
        let [low, mid, high] = bands.get(bucket).copied().unwrap_or([0.0; 3]);

        // Bar height driven by mean amplitude.
        let amplitude = ((low + mid + high) / 3.0).clamp(0.0, 1.0);
        let bar_half = ((amplitude * H as f32) / 2.0) as usize;
        let center = H as usize / 2;
        let top = center.saturating_sub(bar_half);
        let bottom = (center + bar_half).min(H as usize);

        // Additive-blend color: weighted average of band colours.
        let total = low + mid + high + 1e-6;
        let r = ((low * 250.0 + mid * 137.0 + high * 203.0) / total) as u8;
        let g = ((low * 179.0 + mid * 180.0 + high * 166.0) / total) as u8;
        let b = ((low * 135.0 + mid * 250.0 + high * 247.0) / total) as u8;
        let bar_color = slint::Rgb8Pixel { r, g, b };

        for y in top..bottom {
            pixels[y * W as usize + x] = bar_color;
        }
    }

    buf
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
        bpm: t.bpm.unwrap_or(0.0),
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
    let now_bpm = track.bpm.unwrap_or(0.0);

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
            w.set_now_playing_bpm(now_bpm);
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
                    // Save BPM if not already set.
                    if track.bpm.is_none() && result.bpm > 0.0 {
                        if let Err(e) = db.save_bpm(track_id, result.bpm).await {
                            tracing::warn!("failed to save bpm: {e}");
                        }
                        // Update BPM in now-playing panel.
                        let bpm = result.bpm;
                        let weak3 = weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = weak3.upgrade() {
                                w.set_now_playing_bpm(bpm);
                            }
                        });
                    }
                    arrays
                }
                _ => vec![],
            }
        }
    };

    let pixel_buf = render_waveform_buffer(&bands);
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(w) = weak.upgrade() {
            w.set_waveform_image(slint::Image::from_rgb8(pixel_buf));
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

    // Analyse new tracks: generate frequency-band waveforms and detect BPM.
    let mut analysed = 0usize;
    let mut analysis_errors = 0usize;
    for (track_id, path) in stats.upserted_tracks {
        // Skip if both waveform and BPM are already cached.
        let has_waveform = matches!(db.get_waveform_bands(track_id).await, Ok(Some(_)));
        let has_bpm = matches!(
            db.get_track(track_id).await,
            Ok(Some(ref t)) if t.bpm.is_some()
        );
        if has_waveform && has_bpm {
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
                if result.bpm > 0.0 {
                    if let Err(e) = db.save_bpm(track_id, result.bpm).await {
                        tracing::warn!(track_id, error = %e, "failed to save bpm");
                    }
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
/// Send `(track_id, path)` pairs via [`AnalysisQueue::enqueue`]. A single
/// tokio task drains the queue sequentially, running FFT + BPM analysis for
/// each track and saving results to the DB. The Slint `pending-analysis-count`
/// property is updated in real-time via `invoke_from_event_loop`.
struct AnalysisQueue {
    tx: tokio::sync::mpsc::UnboundedSender<(i64, std::path::PathBuf)>,
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

        rt.spawn(async move {
            while let Some((track_id, path)) = rx.recv().await {
                let count = pending.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                update_analysis_counter(&weak, count);

                let result =
                    tokio::task::spawn_blocking(move || analyze_track(&path, 1000)).await;

                match result {
                    Ok(Ok(analysis)) => {
                        let arrays: Vec<[f32; 3]> =
                            analysis.waveform.iter().map(|b| b.to_array()).collect();
                        if let Err(e) = db.save_waveform_bands(track_id, &arrays).await {
                            tracing::warn!(track_id, error = %e, "analysis: save waveform failed");
                        }
                        if analysis.bpm > 0.0 {
                            if let Err(e) = db.save_bpm(track_id, analysis.bpm).await {
                                tracing::warn!(track_id, error = %e, "analysis: save bpm failed");
                            }
                            // Update BPM in track list row if it's currently visible.
                            let bpm = analysis.bpm;
                            let weak2 = weak.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                let Some(w) = weak2.upgrade() else { return };
                                let model = w.get_tracks();
                                if let Some(idx) = (0..model.row_count())
                                    .find(|&i| model.row_data(i).map(|r| r.id) == Some(track_id as i32))
                                {
                                    if let Some(mut row) = model.row_data(idx) {
                                        row.bpm = bpm;
                                        model.set_row_data(idx, row);
                                    }
                                }
                            });
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(track_id, error = %e, "analysis failed");
                    }
                    Err(e) => {
                        tracing::warn!(track_id, error = %e, "analysis task panicked");
                    }
                }

                let remaining = pending.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) - 1;
                update_analysis_counter(&weak, remaining);
            }
        });

        Self { tx }
    }

    fn enqueue(&self, tracks: impl IntoIterator<Item = (i64, std::path::PathBuf)>) {
        for item in tracks {
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

/// Launch the Slint GUI.
fn cmd_gui() -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let db = rt.block_on(open_db())?;
    let engine = Arc::new(AudioEngine::spawn()?);

    let window = AppWindow::new().context("failed to create window")?;
    let rt_handle = rt.handle().clone();

    // ── Initial data load ────────────────────────────────────────────────────

    let initial_tracks = rt.block_on(db.list_tracks())?;
    window.set_tracks(tracks_to_model_rc(&initial_tracks));
    // Clone for art loader so the move into spawn_art_loader is independent.
    let initial_tracks_art = initial_tracks.clone();
    spawn_art_loader(initial_tracks_art, window.as_weak(), Arc::clone(&db), rt_handle.clone());

    let dirs = rt.block_on(db.list_scanned_dirs())?;
    let dir_names: Vec<slint::SharedString> = dirs
        .iter()
        .map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.to_string_lossy().into_owned())
                .into()
        })
        .collect();
    let dir_paths: Vec<slint::SharedString> =
        dirs.iter().map(|p| p.to_string_lossy().into_owned().into()).collect();
    window.set_sidebar_dir_names(slint::ModelRc::new(slint::VecModel::from(dir_names)));
    window.set_sidebar_dir_paths(slint::ModelRc::new(slint::VecModel::from(dir_paths)));

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
        window.on_play_track(move |id| {
            let engine = Arc::clone(&engine);
            let db = Arc::clone(&db);
            let weak = weak.clone();
            rt_handle.spawn(start_playback(id as i64, db, engine, weak));
        });
    }

    // ── Next / Prev track ────────────────────────────────────────────────────

    {
        let engine = Arc::clone(&engine);
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        window.on_next_track(move || {
            let engine = Arc::clone(&engine);
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let rt_handle = rt_handle.clone();
            if let Some(w) = weak.upgrade() {
                let tracks = w.get_tracks();
                let current = w.get_current_track_id();
                if let Some(next_id) = find_adjacent_track(&tracks, current, 1) {
                    w.set_current_track_id(next_id);
                    w.set_is_playing(true);
                    rt_handle.spawn(start_playback(next_id as i64, db, engine, weak));
                }
            }
        });
    }

    {
        let engine = Arc::clone(&engine);
        let db = Arc::clone(&db);
        let weak = window.as_weak();
        let rt_handle = rt_handle.clone();
        window.on_prev_track(move || {
            let engine = Arc::clone(&engine);
            let db = Arc::clone(&db);
            let weak = weak.clone();
            let rt_handle = rt_handle.clone();
            if let Some(w) = weak.upgrade() {
                let tracks = w.get_tracks();
                let current = w.get_current_track_id();
                if let Some(prev_id) = find_adjacent_track(&tracks, current, -1) {
                    w.set_current_track_id(prev_id);
                    w.set_is_playing(true);
                    rt_handle.spawn(start_playback(prev_id as i64, db, engine, weak));
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
