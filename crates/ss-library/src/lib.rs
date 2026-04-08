use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use lofty::prelude::{AudioFile, TaggedFileExt};
use lofty::tag::Accessor;
use tracing::{debug, warn};
use walkdir::WalkDir;

use ss_db::{Db, NewTrack};

/// Audio file extensions that the scanner will process.
const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "opus", "wav", "aiff", "aif", "m4a", "wv", "ape",
];

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
                            if let Some(art_bytes) = extract_primary_art(&path) {
                                if let Err(e) = self.db.save_album_art(t.id, &art_bytes).await {
                                    warn!(path = %path.display(), error = %e, "failed to save album art");
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
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Walk `dir` and return paths of recognised audio files.
fn collect_audio_paths(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| AUDIO_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
                .unwrap_or(false)
        })
        .map(|e| e.into_path())
        .collect()
}

/// Extract the first embedded picture from an audio file.
/// Returns raw JPEG/PNG bytes, or `None` if the file has no embedded art.
fn extract_primary_art(path: &Path) -> Option<Vec<u8>> {
    let tagged_file = lofty::read_from_path(path).ok()?;
    let tag = tagged_file.primary_tag()?;
    tag.pictures().first().map(|p| p.data().to_vec())
}

/// Read metadata from an audio file using lofty.
fn read_metadata(path: &Path) -> Result<NewTrack> {
    let tagged_file = lofty::read_from_path(path)?;
    let props = tagged_file.properties();
    let duration_secs = Some(props.duration().as_secs_f64());

    let tag = tagged_file.primary_tag();
    let title = tag.and_then(|t| t.title().map(|s| s.into_owned()));
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
