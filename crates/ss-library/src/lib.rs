use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use lofty::prelude::{AudioFile, TaggedFileExt};
use lofty::tag::Accessor;
use tracing::{debug, warn};
use walkdir::WalkDir;

use notify::Watcher as _;
use ss_db::{Db, NewTrack};

/// Audio file extensions that the scanner will process.
const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "opus", "wav", "aiff", "aif", "m4a", "wv", "ape",
];

/// Returns true if `path` has a recognised audio file extension.
pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| AUDIO_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Summary of a completed scan.
#[derive(Debug, Default)]
pub struct ScanStats {
    /// Files successfully inserted or updated in the database.
    pub upserted: usize,
    /// Files skipped because they are not recognised audio files.
    pub skipped: usize,
    /// Files that failed to read (I/O or metadata errors).
    pub errors: usize,
    /// IDs of tracks that were upserted, paired with their file path.
    pub upserted_tracks: Vec<(i64, std::path::PathBuf)>,
}

/// Walks a directory tree and upserts track metadata into the database.
pub struct Scanner {
    db: Arc<Db>,
}

impl Scanner {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// Recursively scan `dir`, reading metadata and upserting each audio file.
    pub async fn scan_dir(&self, dir: &Path) -> Result<ScanStats> {
        let mut stats = ScanStats::default();

        // Record that this directory was scanned.
        if let Err(e) = self.db.record_scanned_dir(dir).await {
            warn!(dir = %dir.display(), error = %e, "failed to record scanned dir");
        }

        // Collect paths first so we can hand them to blocking tasks without
        // holding a WalkDir iterator across await points.
        let paths = collect_audio_paths(dir);

        for path in paths {
            match read_metadata(&path) {
                Ok(track) => {
                    match self.db.upsert_track(&track).await {
                        Ok(t) => {
                            debug!(id = t.id, path = %path.display(), "upserted");
                            stats.upserted += 1;
                            stats.upserted_tracks.push((t.id, path.clone()));
                            if let Some((art_bytes, thumb_bytes)) = extract_primary_art(&path) {
                                if let Err(e) = self.db.save_album_art(t.id, &art_bytes).await {
                                    warn!(path = %path.display(), error = %e, "failed to save album art");
                                }
                                if let Err(e) = self.db.save_thumbnail_44(t.id, &thumb_bytes).await {
                                    warn!(path = %path.display(), error = %e, "failed to save thumbnail");
                                }
                            }
                        }
                        Err(e) => {
                            warn!(path = %path.display(), error = %e, "db upsert failed");
                            stats.errors += 1;
                        }
                    }
                }
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "metadata read failed");
                    stats.errors += 1;
                }
            }
        }

        Ok(stats)
    }

    /// Scan a single audio file and upsert it into the database.
    /// Returns `Some((track_id, path))` on success, `None` if not an audio file.
    pub async fn scan_file(&self, path: &Path) -> Result<Option<(i64, PathBuf)>> {
        if !is_audio_file(path) {
            return Ok(None);
        }
        let path_buf = path.to_path_buf();
        let track = tokio::task::spawn_blocking(move || read_metadata(&path_buf))
            .await
            .context("spawn_blocking panicked")??;
        let t = self.db.upsert_track(&track).await?;
        let result_path = track.path.clone();
        let art_path = track.path.clone();
        if let Some((art_bytes, thumb_bytes)) =
            tokio::task::spawn_blocking(move || extract_primary_art(&art_path))
                .await
                .ok()
                .flatten()
        {
            if let Err(e) = self.db.save_album_art(t.id, &art_bytes).await {
                warn!(path = %result_path.display(), error = %e, "failed to save album art (single file)");
            }
            if let Err(e) = self.db.save_thumbnail_44(t.id, &thumb_bytes).await {
                warn!(path = %result_path.display(), error = %e, "failed to save thumbnail (single file)");
            }
        }
        Ok(Some((t.id, result_path)))
    }
}

// ── Library events ────────────────────────────────────────────────────────────

/// Events emitted by [`FileWatcher`] when the library changes on disk.
pub enum LibraryEvent {
    /// A new audio file was detected and upserted into the database.
    TrackAdded { id: i64, path: PathBuf },
    /// An audio file was removed from disk and deleted from the database.
    TrackRemoved(PathBuf),
}

// ── File watcher ──────────────────────────────────────────────────────────────

/// Watches registered directories for filesystem changes and emits
/// [`LibraryEvent`]s via a Tokio channel.
pub struct FileWatcher {
    watcher: notify::RecommendedWatcher,
}

impl FileWatcher {
    /// Create a new watcher.  Call [`FileWatcher::watch`] to add directories.
    ///
    /// * `db` — database handle used by the internal scanner.
    /// * `rt_handle` — Tokio runtime handle for spawning async work.
    /// * `event_tx` — channel for emitting library change events.
    pub fn new(
        db: Arc<Db>,
        rt_handle: tokio::runtime::Handle,
        event_tx: tokio::sync::mpsc::UnboundedSender<LibraryEvent>,
    ) -> Result<Self> {
        let (std_tx, std_rx) =
            std::sync::mpsc::channel::<notify::Result<notify::Event>>();

        let watcher = notify::recommended_watcher(std_tx)?;

        // Bridge: std::sync::mpsc → tokio::sync::mpsc (runs in a dedicated OS thread)
        let (raw_tx, mut raw_rx) =
            tokio::sync::mpsc::unbounded_channel::<notify::Event>();
        std::thread::spawn(move || {
            for event in std_rx.into_iter().flatten() {
                let _ = raw_tx.send(event);
            }
        });

        // Async task: process raw events using the scanner
        let scanner = Arc::new(Scanner::new(Arc::clone(&db)));
        rt_handle.spawn(async move {
            while let Some(event) = raw_rx.recv().await {
                process_fs_event(event, Arc::clone(&db), Arc::clone(&scanner), event_tx.clone())
                    .await;
            }
        });

        Ok(Self { watcher })
    }

    /// Begin watching `path` recursively.
    pub fn watch(&mut self, path: &Path) -> Result<()> {
        self.watcher.watch(path, notify::RecursiveMode::Recursive)?;
        Ok(())
    }

    /// Stop watching `path`.
    pub fn unwatch(&mut self, path: &Path) -> Result<()> {
        self.watcher.unwatch(path)?;
        Ok(())
    }
}

async fn process_fs_event(
    event: notify::Event,
    db: Arc<Db>,
    scanner: Arc<Scanner>,
    tx: tokio::sync::mpsc::UnboundedSender<LibraryEvent>,
) {
    use notify::EventKind;
    match &event.kind {
        EventKind::Create(_) => {
            for path in &event.paths {
                if !is_audio_file(path) {
                    continue;
                }
                match scanner.scan_file(path).await {
                    Ok(Some((id, p))) => {
                        let _ = tx.send(LibraryEvent::TrackAdded { id, path: p });
                    }
                    Ok(None) => {}
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "watcher: scan_file failed");
                    }
                }
            }
        }
        EventKind::Remove(_) => {
            for path in &event.paths {
                if !is_audio_file(path) {
                    continue;
                }
                if let Err(e) = db.delete_track_by_path(&path.to_string_lossy()).await {
                    warn!(path = %path.display(), error = %e, "watcher: delete_track failed");
                }
                let _ = tx.send(LibraryEvent::TrackRemoved(path.clone()));
            }
        }
        EventKind::Modify(notify::event::ModifyKind::Name(notify::event::RenameMode::Both)) => {
            if let [from, to] = event.paths.as_slice() {
                if is_audio_file(from) {
                    if let Err(e) = db.delete_track_by_path(&from.to_string_lossy()).await {
                        warn!(path = %from.display(), error = %e, "watcher: delete renamed track failed");
                    }
                    let _ = tx.send(LibraryEvent::TrackRemoved(from.clone()));
                }
                if is_audio_file(to) {
                    match scanner.scan_file(to).await {
                        Ok(Some((id, p))) => {
                            let _ = tx.send(LibraryEvent::TrackAdded { id, path: p });
                        }
                        Ok(None) => {}
                        Err(e) => {
                            warn!(path = %to.display(), error = %e, "watcher: scan renamed track failed");
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Walk `dir` and return paths of recognised audio files.
fn collect_audio_paths(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| is_audio_file(e.path()))
        .map(|e| e.into_path())
        .collect()
}

/// Extract the first embedded picture from an audio file.
/// Returns `(raw_bytes, thumbnail_rgb)`, or `None` if the file has no embedded art.
/// `thumbnail_rgb` is an 88×88 raw RGB byte buffer ready for `SharedPixelBuffer`.
fn extract_primary_art(path: &Path) -> Option<(Vec<u8>, Vec<u8>)> {
    let tagged_file = lofty::read_from_path(path).ok()?;
    let tag = tagged_file.primary_tag()?;
    let raw = tag.pictures().first().map(|p| p.data().to_vec())?;
    let thumb = compute_thumbnail(&raw).unwrap_or_default();
    Some((raw, thumb))
}

/// Decode `raw` JPEG/PNG bytes and produce an 88×88 raw RGB byte vec.
/// 88px = 2× the 44px display size, so artwork is sharp on retina screens.
fn compute_thumbnail(raw: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(raw).ok()?;
    let resized = img.resize_to_fill(88, 88, image::imageops::FilterType::Triangle);
    Some(resized.to_rgb8().into_raw())
}

/// Read metadata from an audio file using lofty.
fn read_metadata(path: &Path) -> Result<NewTrack> {
    let tagged_file = lofty::read_from_path(path)?;
    let props = tagged_file.properties();
    let duration_secs = Some(props.duration().as_secs_f64());

    // primary_tag() returns the format's preferred tag type (e.g. ID3v2 for MP3).
    // first_tag() is the fallback for files that only carry a secondary tag (e.g. ID3v1-only).
    let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag());
    let title = tag
        .and_then(|t| t.title().map(|s| s.into_owned()))
        .or_else(|| {
            // Last resort: use the filename stem so the track is never blank.
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_owned())
        });
    let artist = tag.and_then(|t| t.artist().map(|s| s.into_owned()));
    let album = tag.and_then(|t| t.album().map(|s| s.into_owned()));

    Ok(NewTrack {
        path: path.to_path_buf(),
        title,
        artist,
        album,
        duration_secs,
    })
}

use anyhow::Context as _;
