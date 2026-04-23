use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sqlx::{sqlite::SqliteConnectOptions, Row, SqlitePool};

use ss_core::Track;

#[derive(Debug, Clone)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub color: String,
}

/// Parameters for inserting or upserting a track (no ID yet).
#[derive(Debug, Clone)]
pub struct NewTrack {
    pub path: PathBuf,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_secs: Option<f64>,
}

/// Database handle wrapping a SQLite connection pool.
pub struct Db {
    pool: SqlitePool,
}

fn row_to_track(row: &sqlx::sqlite::SqliteRow) -> Track {
    Track {
        id: row.get("id"),
        path: PathBuf::from(row.get::<String, _>("path")),
        title: row.get("title"),
        artist: row.get("artist"),
        album: row.get("album"),
        duration_secs: row.get("duration_secs"),
        bpm: row.try_get("bpm").ok(),
    }
}

impl Db {
    /// Open (or create) a SQLite database at `path`.
    pub async fn open(path: &Path) -> Result<Self> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePool::connect_with(opts)
            .await
            .with_context(|| format!("failed to open database at {}", path.display()))?;
        Ok(Self { pool })
    }

    /// Open an in-memory database (useful for tests).
    pub async fn open_in_memory() -> Result<Self> {
        let pool = SqlitePool::connect(":memory:")
            .await
            .context("failed to open in-memory database")?;
        Ok(Self { pool })
    }

    /// Run pending migrations.
    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .context("database migration failed")
    }

    // ── Tracks ────────────────────────────────────────────────────────────────

    /// Insert a new track. Returns the full `Track` with its assigned ID.
    pub async fn insert_track(&self, t: &NewTrack) -> Result<Track> {
        let path = t.path.to_string_lossy().into_owned();
        let id: i64 = sqlx::query(
            "INSERT INTO tracks (path, title, artist, album, duration_secs) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&path)
        .bind(&t.title)
        .bind(&t.artist)
        .bind(&t.album)
        .bind(t.duration_secs)
        .execute(&self.pool)
        .await
        .context("insert_track failed")?
        .last_insert_rowid();

        Ok(Track {
            id,
            path: t.path.clone(),
            title: t.title.clone(),
            artist: t.artist.clone(),
            album: t.album.clone(),
            duration_secs: t.duration_secs,
            bpm: None,
        })
    }

    /// Fetch a track by its primary key. Returns `None` if not found.
    pub async fn get_track(&self, id: i64) -> Result<Option<Track>> {
        let row = sqlx::query(
            "SELECT id, path, title, artist, album, duration_secs, bpm FROM tracks WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("get_track failed")?;

        Ok(row.as_ref().map(row_to_track))
    }

    /// Fetch a track by its file path. Returns `None` if not found.
    pub async fn get_track_by_path(&self, path: &Path) -> Result<Option<Track>> {
        let path_str = path.to_string_lossy().into_owned();
        let row = sqlx::query(
            "SELECT id, path, title, artist, album, duration_secs, bpm FROM tracks WHERE path = ?",
        )
        .bind(&path_str)
        .fetch_optional(&self.pool)
        .await
        .context("get_track_by_path failed")?;

        Ok(row.as_ref().map(row_to_track))
    }

    /// Return all tracks ordered by id.
    pub async fn list_tracks(&self) -> Result<Vec<Track>> {
        let rows = sqlx::query(
            "SELECT id, path, title, artist, album, duration_secs, bpm FROM tracks ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
        .context("list_tracks failed")?;

        Ok(rows.iter().map(row_to_track).collect())
    }

    /// Update metadata for an existing track. Returns `true` if a row was modified.
    pub async fn update_track(&self, id: i64, t: &NewTrack) -> Result<bool> {
        let path = t.path.to_string_lossy().into_owned();
        let affected = sqlx::query(
            "UPDATE tracks SET path = ?, title = ?, artist = ?, album = ?, duration_secs = ? WHERE id = ?",
        )
        .bind(&path)
        .bind(&t.title)
        .bind(&t.artist)
        .bind(&t.album)
        .bind(t.duration_secs)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("update_track failed")?
        .rows_affected();

        Ok(affected > 0)
    }

    /// Insert or update a track matched by path. Returns the track with its ID.
    pub async fn upsert_track(&self, t: &NewTrack) -> Result<Track> {
        let path = t.path.to_string_lossy().into_owned();
        sqlx::query(
            r#"INSERT INTO tracks (path, title, artist, album, duration_secs)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(path) DO UPDATE SET
                   title         = excluded.title,
                   artist        = excluded.artist,
                   album         = excluded.album,
                   duration_secs = excluded.duration_secs"#,
        )
        .bind(&path)
        .bind(&t.title)
        .bind(&t.artist)
        .bind(&t.album)
        .bind(t.duration_secs)
        .execute(&self.pool)
        .await
        .context("upsert_track failed")?;

        // Fetch the row to get the stable id.
        self.get_track_by_path(&t.path)
            .await?
            .context("upsert_track: row missing after upsert")
    }

    /// Delete a track by ID. Returns `true` if a row was removed.
    pub async fn delete_track(&self, id: i64) -> Result<bool> {
        let affected = sqlx::query("DELETE FROM tracks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("delete_track failed")?
            .rows_affected();

        Ok(affected > 0)
    }

    // ── Waveforms ─────────────────────────────────────────────────────────────

    /// Persist a waveform (RMS bucket array) for a track.
    /// Values are stored as raw little-endian f32 bytes.
    pub async fn save_waveform(&self, track_id: i64, rms: &[f32]) -> Result<()> {
        let bytes: Vec<u8> = rms.iter().flat_map(|v| v.to_le_bytes()).collect();
        sqlx::query(
            r#"INSERT INTO waveforms (track_id, data) VALUES (?, ?)
               ON CONFLICT(track_id) DO UPDATE SET data = excluded.data"#,
        )
        .bind(track_id)
        .bind(&bytes)
        .execute(&self.pool)
        .await
        .context("save_waveform failed")?;
        Ok(())
    }

    /// Fetch a cached waveform for a track. Returns `None` if not yet analysed.
    pub async fn get_waveform(&self, track_id: i64) -> Result<Option<Vec<f32>>> {
        let row = sqlx::query("SELECT data FROM waveforms WHERE track_id = ?")
            .bind(track_id)
            .fetch_optional(&self.pool)
            .await
            .context("get_waveform failed")?;

        Ok(row.map(|r| {
            let bytes: Vec<u8> = r.get("data");
            bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect()
        }))
    }

    /// Persist a frequency-band waveform for a track.
    /// `bands` is a slice of [low_rms, mid_rms, high_rms] per bucket, stored
    /// interleaved as little-endian f32 bytes: [low0, mid0, high0, low1, ...].
    pub async fn save_waveform_bands(&self, track_id: i64, bands: &[[f32; 3]]) -> Result<()> {
        let bytes: Vec<u8> = bands
            .iter()
            .flat_map(|b| b.iter().flat_map(|v| v.to_le_bytes()))
            .collect();
        sqlx::query(
            r#"INSERT INTO waveforms (track_id, data) VALUES (?, ?)
               ON CONFLICT(track_id) DO UPDATE SET data = excluded.data"#,
        )
        .bind(track_id)
        .bind(&bytes)
        .execute(&self.pool)
        .await
        .context("save_waveform_bands failed")?;
        Ok(())
    }

    /// Fetch a frequency-band waveform for a track.
    /// Returns `None` if not yet analysed, or if the stored blob is in the
    /// old mono format (blob_len % 12 != 0).
    pub async fn get_waveform_bands(&self, track_id: i64) -> Result<Option<Vec<[f32; 3]>>> {
        let row = sqlx::query("SELECT data FROM waveforms WHERE track_id = ?")
            .bind(track_id)
            .fetch_optional(&self.pool)
            .await
            .context("get_waveform_bands failed")?;

        Ok(row.and_then(|r| {
            let bytes: Vec<u8> = r.get("data");
            if !bytes.len().is_multiple_of(12) {
                return None; // old mono format — will be regenerated
            }
            Some(
                bytes
                    .chunks_exact(12)
                    .map(|b| {
                        [
                            f32::from_le_bytes(b[0..4].try_into().unwrap()),
                            f32::from_le_bytes(b[4..8].try_into().unwrap()),
                            f32::from_le_bytes(b[8..12].try_into().unwrap()),
                        ]
                    })
                    .collect(),
            )
        }))
    }

    /// Save the detected BPM for a track.
    pub async fn save_bpm(&self, track_id: i64, bpm: f32) -> Result<()> {
        sqlx::query("UPDATE tracks SET bpm = ? WHERE id = ?")
            .bind(bpm)
            .bind(track_id)
            .execute(&self.pool)
            .await
            .context("save_bpm failed")?;
        Ok(())
    }

    /// Return all (track_id, path) pairs that are missing a waveform.
    /// These are queued for background analysis on startup and after each scan.
    pub async fn list_tracks_needing_analysis(&self) -> Result<Vec<(i64, std::path::PathBuf)>> {
        let rows = sqlx::query(
            r#"SELECT t.id, t.path FROM tracks t
               LEFT JOIN waveforms w ON w.track_id = t.id
               WHERE w.track_id IS NULL
               ORDER BY t.id"#,
        )
        .fetch_all(&self.pool)
        .await
        .context("list_tracks_needing_analysis failed")?;
        Ok(rows
            .iter()
            .map(|r| (r.get("id"), std::path::PathBuf::from(r.get::<String, _>("path"))))
            .collect())
    }

    // ── Album art ─────────────────────────────────────────────────────────────

    /// Persist raw JPEG/PNG bytes for a track's cover art. Upsert semantics.
    pub async fn save_album_art(&self, track_id: i64, data: &[u8]) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO album_art (track_id, data) VALUES (?, ?)
               ON CONFLICT(track_id) DO UPDATE SET data = excluded.data"#,
        )
        .bind(track_id)
        .bind(data)
        .execute(&self.pool)
        .await
        .context("save_album_art failed")?;
        Ok(())
    }

    /// Fetch raw JPEG/PNG bytes for a track's cover art. Returns `None` if not stored.
    pub async fn get_album_art(&self, track_id: i64) -> Result<Option<Vec<u8>>> {
        let row = sqlx::query("SELECT data FROM album_art WHERE track_id = ?")
            .bind(track_id)
            .fetch_optional(&self.pool)
            .await
            .context("get_album_art failed")?;
        Ok(row.map(|r| r.get("data")))
    }

    /// Persist a pre-decoded 44×44 raw RGB thumbnail (5,808 bytes). Upsert semantics.
    pub async fn save_thumbnail_44(&self, track_id: i64, data: &[u8]) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO album_art (track_id, data, thumbnail_44) VALUES (?, '', ?)
               ON CONFLICT(track_id) DO UPDATE SET thumbnail_44 = excluded.thumbnail_44"#,
        )
        .bind(track_id)
        .bind(data)
        .execute(&self.pool)
        .await
        .context("save_thumbnail_44 failed")?;
        Ok(())
    }

    /// Fetch pre-decoded 44×44 raw RGB thumbnail. Returns `None` if not yet computed.
    pub async fn get_thumbnail_44(&self, track_id: i64) -> Result<Option<Vec<u8>>> {
        let row = sqlx::query("SELECT thumbnail_44 FROM album_art WHERE track_id = ?")
            .bind(track_id)
            .fetch_optional(&self.pool)
            .await
            .context("get_thumbnail_44 failed")?;
        Ok(row.and_then(|r| r.get("thumbnail_44")))
    }

    /// Total number of tracks in the database.
    pub async fn track_count(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM tracks")
            .fetch_one(&self.pool)
            .await
            .context("track_count failed")?;
        Ok(row.get("count"))
    }

    // ── Scanned directories ───────────────────────────────────────────────────

    /// Record a directory as having been scanned. Idempotent.
    pub async fn record_scanned_dir(&self, path: &Path) -> Result<()> {
        let p = path.to_string_lossy().into_owned();
        sqlx::query(
            "INSERT INTO scanned_dirs (path) VALUES (?) ON CONFLICT(path) DO NOTHING",
        )
        .bind(&p)
        .execute(&self.pool)
        .await
        .context("record_scanned_dir failed")?;
        Ok(())
    }

    /// Return all recorded scanned directories ordered by path.
    pub async fn list_scanned_dirs(&self) -> Result<Vec<PathBuf>> {
        let rows = sqlx::query("SELECT path FROM scanned_dirs ORDER BY path")
            .fetch_all(&self.pool)
            .await
            .context("list_scanned_dirs failed")?;
        Ok(rows.iter().map(|r| PathBuf::from(r.get::<String, _>("path"))).collect())
    }

    /// Delete a single track by its exact file path. Cascades to waveforms, album_art, etc.
    pub async fn delete_track_by_path(&self, path: &str) -> Result<()> {
        sqlx::query("DELETE FROM tracks WHERE path = ?")
            .bind(path)
            .execute(&self.pool)
            .await
            .context("delete_track_by_path failed")?;
        Ok(())
    }

    /// Remove a scanned-directory record. Does not remove any tracks.
    pub async fn remove_scanned_dir(&self, path: &str) -> Result<()> {
        sqlx::query("DELETE FROM scanned_dirs WHERE path = ?")
            .bind(path)
            .execute(&self.pool)
            .await
            .context("remove_scanned_dir failed")?;
        Ok(())
    }

    /// Return all tracks whose path starts with the given directory prefix.
    pub async fn list_tracks_in_dir(&self, dir: &str) -> Result<Vec<Track>> {
        // Normalise: ensure trailing separator so "/foo" doesn't match "/foobar".
        let prefix = if dir.ends_with('/') {
            dir.to_owned()
        } else {
            format!("{}/", dir)
        };
        let rows = sqlx::query(
            "SELECT id, path, title, artist, album, duration_secs, bpm FROM tracks WHERE path LIKE ? || '%' ORDER BY id",
        )
        .bind(&prefix)
        .fetch_all(&self.pool)
        .await
        .context("list_tracks_in_dir failed")?;
        Ok(rows.iter().map(row_to_track).collect())
    }

    // ── Playlists ─────────────────────────────────────────────────────────────

    /// Create a new playlist. Returns the created playlist.
    pub async fn insert_playlist(&self, name: &str) -> Result<Playlist> {
        let id: i64 = sqlx::query("INSERT INTO playlists (name) VALUES (?)")
            .bind(name)
            .execute(&self.pool)
            .await
            .context("insert_playlist failed")?
            .last_insert_rowid();
        Ok(Playlist { id, name: name.to_owned() })
    }

    /// Return all playlists ordered by name.
    pub async fn list_playlists(&self) -> Result<Vec<Playlist>> {
        let rows = sqlx::query("SELECT id, name FROM playlists ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .context("list_playlists failed")?;
        Ok(rows.iter().map(|r| Playlist { id: r.get("id"), name: r.get("name") }).collect())
    }

    /// Delete a playlist by ID. Returns true if a row was removed.
    pub async fn delete_playlist(&self, id: i64) -> Result<bool> {
        let affected = sqlx::query("DELETE FROM playlists WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("delete_playlist failed")?
            .rows_affected();
        Ok(affected > 0)
    }

    /// Add a track to a playlist. Idempotent (ignores duplicate).
    pub async fn add_track_to_playlist(&self, track_id: i64, playlist_id: i64) -> Result<()> {
        let pos: Option<i64> = sqlx::query(
            "SELECT MAX(position) as m FROM playlist_tracks WHERE playlist_id = ?",
        )
        .bind(playlist_id)
        .fetch_one(&self.pool)
        .await
        .context("add_track_to_playlist: position query failed")?
        .get("m");
        let next_pos = pos.unwrap_or(-1) + 1;

        sqlx::query(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?, ?, ?)
             ON CONFLICT(playlist_id, track_id) DO NOTHING",
        )
        .bind(playlist_id)
        .bind(track_id)
        .bind(next_pos)
        .execute(&self.pool)
        .await
        .context("add_track_to_playlist failed")?;
        Ok(())
    }

    /// Reorder tracks in a playlist. `ordered_track_ids` must contain every
    /// track_id currently in the playlist, in the desired new order.
    /// Positions are assigned as 0, 1, 2, … matching the slice index.
    pub async fn reorder_playlist_tracks(
        &self,
        playlist_id: i64,
        ordered_track_ids: &[i64],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for (pos, &track_id) in ordered_track_ids.iter().enumerate() {
            sqlx::query(
                "UPDATE playlist_tracks SET position = ? WHERE playlist_id = ? AND track_id = ?",
            )
            .bind(pos as i64)
            .bind(playlist_id)
            .bind(track_id)
            .execute(&mut *tx)
            .await
            .context("reorder_playlist_tracks: update failed")?;
        }
        tx.commit().await.context("reorder_playlist_tracks: commit failed")?;
        Ok(())
    }

    /// Remove a track from a playlist.
    pub async fn remove_track_from_playlist(&self, track_id: i64, playlist_id: i64) -> Result<bool> {
        let affected =
            sqlx::query("DELETE FROM playlist_tracks WHERE playlist_id = ? AND track_id = ?")
                .bind(playlist_id)
                .bind(track_id)
                .execute(&self.pool)
                .await
                .context("remove_track_from_playlist failed")?
                .rows_affected();
        Ok(affected > 0)
    }

    /// Return tracks in a playlist ordered by position.
    pub async fn list_tracks_in_playlist(&self, playlist_id: i64) -> Result<Vec<Track>> {
        let rows = sqlx::query(
            r#"SELECT t.id, t.path, t.title, t.artist, t.album, t.duration_secs, t.bpm
               FROM tracks t
               JOIN playlist_tracks pt ON pt.track_id = t.id
               WHERE pt.playlist_id = ?
               ORDER BY pt.position"#,
        )
        .bind(playlist_id)
        .fetch_all(&self.pool)
        .await
        .context("list_tracks_in_playlist failed")?;
        Ok(rows.iter().map(row_to_track).collect())
    }

    // ── Tags ──────────────────────────────────────────────────────────────────

    /// Create a new tag. Returns the created tag.
    pub async fn insert_tag(&self, name: &str, color: &str) -> Result<Tag> {
        let id: i64 = sqlx::query("INSERT INTO tags (name, color) VALUES (?, ?)")
            .bind(name)
            .bind(color)
            .execute(&self.pool)
            .await
            .context("insert_tag failed")?
            .last_insert_rowid();
        Ok(Tag { id, name: name.to_owned(), color: color.to_owned() })
    }

    /// Return all tags ordered by name.
    pub async fn list_tags(&self) -> Result<Vec<Tag>> {
        let rows = sqlx::query("SELECT id, name, color FROM tags ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .context("list_tags failed")?;
        Ok(rows
            .iter()
            .map(|r| Tag { id: r.get("id"), name: r.get("name"), color: r.get("color") })
            .collect())
    }

    /// Delete a tag by ID. Returns true if a row was removed.
    pub async fn delete_tag(&self, id: i64) -> Result<bool> {
        let affected = sqlx::query("DELETE FROM tags WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("delete_tag failed")?
            .rows_affected();
        Ok(affected > 0)
    }

    /// Update a tag's name and color. Returns true if a row was modified.
    pub async fn update_tag(&self, id: i64, name: &str, color: &str) -> Result<bool> {
        let affected = sqlx::query("UPDATE tags SET name = ?, color = ? WHERE id = ?")
            .bind(name)
            .bind(color)
            .bind(id)
            .execute(&self.pool)
            .await
            .context("update_tag failed")?
            .rows_affected();
        Ok(affected > 0)
    }

    /// Assign a tag to a track. Idempotent.
    pub async fn assign_tag(&self, track_id: i64, tag_id: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO track_tags (track_id, tag_id) VALUES (?, ?) ON CONFLICT DO NOTHING",
        )
        .bind(track_id)
        .bind(tag_id)
        .execute(&self.pool)
        .await
        .context("assign_tag failed")?;
        Ok(())
    }

    /// Remove a tag from a track.
    pub async fn unassign_tag(&self, track_id: i64, tag_id: i64) -> Result<bool> {
        let affected =
            sqlx::query("DELETE FROM track_tags WHERE track_id = ? AND tag_id = ?")
                .bind(track_id)
                .bind(tag_id)
                .execute(&self.pool)
                .await
                .context("unassign_tag failed")?
                .rows_affected();
        Ok(affected > 0)
    }

    /// Return all tags assigned to a track.
    pub async fn list_tags_for_track(&self, track_id: i64) -> Result<Vec<Tag>> {
        let rows = sqlx::query(
            r#"SELECT t.id, t.name, t.color FROM tags t
               JOIN track_tags tt ON tt.tag_id = t.id
               WHERE tt.track_id = ?
               ORDER BY t.name"#,
        )
        .bind(track_id)
        .fetch_all(&self.pool)
        .await
        .context("list_tags_for_track failed")?;
        Ok(rows
            .iter()
            .map(|r| Tag { id: r.get("id"), name: r.get("name"), color: r.get("color") })
            .collect())
    }

    /// Return all (track_id, Tag) pairs for a set of track IDs.
    /// Used to compute per-selection tag assignment state efficiently.
    pub async fn list_tags_for_tracks(&self, track_ids: &[i64]) -> Result<Vec<(i64, Tag)>> {
        if track_ids.is_empty() {
            return Ok(vec![]);
        }
        // Build a parameterised IN clause
        let placeholders = track_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            r#"SELECT tt.track_id, t.id, t.name, t.color FROM tags t
               JOIN track_tags tt ON tt.tag_id = t.id
               WHERE tt.track_id IN ({placeholders})
               ORDER BY t.name"#
        );
        let mut q = sqlx::query(&sql);
        for id in track_ids {
            q = q.bind(id);
        }
        let rows = q.fetch_all(&self.pool).await.context("list_tags_for_tracks failed")?;
        Ok(rows
            .iter()
            .map(|r| {
                let tag = Tag { id: r.get("id"), name: r.get("name"), color: r.get("color") };
                (r.get::<i64, _>("track_id"), tag)
            })
            .collect())
    }

    /// Fetch the notes stored on a track. Returns `None` if no note has been written.
    pub async fn get_track_notes(&self, track_id: i64) -> Result<Option<String>> {
        let row = sqlx::query("SELECT notes FROM tracks WHERE id = ?")
            .bind(track_id)
            .fetch_optional(&self.pool)
            .await
            .context("get_track_notes failed")?;
        Ok(row.and_then(|r| r.get::<Option<String>, _>("notes")))
    }

    /// Write (or overwrite) the notes on a track. Returns `true` if the row was found.
    pub async fn update_track_notes(&self, track_id: i64, notes: &str) -> Result<bool> {
        let affected = sqlx::query("UPDATE tracks SET notes = ? WHERE id = ?")
            .bind(notes)
            .bind(track_id)
            .execute(&self.pool)
            .await
            .context("update_track_notes failed")?
            .rows_affected();
        Ok(affected > 0)
    }

    /// Return playlists that contain the given track, ordered by name.
    pub async fn list_playlists_for_track(&self, track_id: i64) -> Result<Vec<Playlist>> {
        let rows = sqlx::query(
            r#"SELECT p.id, p.name FROM playlists p
               JOIN playlist_tracks pt ON pt.playlist_id = p.id
               WHERE pt.track_id = ?
               ORDER BY p.name"#,
        )
        .bind(track_id)
        .fetch_all(&self.pool)
        .await
        .context("list_playlists_for_track failed")?;
        Ok(rows.iter().map(|r| Playlist { id: r.get("id"), name: r.get("name") }).collect())
    }

    // ── Settings ──────────────────────────────────────────────────────────────

    /// Fetch a setting value by key. Returns `None` if the key does not exist.
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .context("get_setting failed")?;
        Ok(row.map(|r| r.get("value")))
    }

    /// Insert or update a setting key-value pair.
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .context("set_setting failed")?;
        Ok(())
    }

    /// Return tracks that have a given tag assigned.
    pub async fn list_tracks_with_tag(&self, tag_id: i64) -> Result<Vec<Track>> {
        let rows = sqlx::query(
            r#"SELECT t.id, t.path, t.title, t.artist, t.album, t.duration_secs, t.bpm
               FROM tracks t
               JOIN track_tags tt ON tt.track_id = t.id
               WHERE tt.tag_id = ?
               ORDER BY t.id"#,
        )
        .bind(tag_id)
        .fetch_all(&self.pool)
        .await
        .context("list_tracks_with_tag failed")?;
        Ok(rows.iter().map(row_to_track).collect())
    }

    // ── Filtered queries ──────────────────────────────────────────────────────

    pub async fn list_tracks_filtered(&self, needle: &str) -> Result<Vec<Track>> {
        let pattern = format!("%{}%", needle);
        let rows = sqlx::query(
            r#"SELECT id, path, title, artist, album, duration_secs, bpm FROM tracks
               WHERE LOWER(COALESCE(title,  '')) LIKE LOWER(?)
                  OR LOWER(COALESCE(artist, '')) LIKE LOWER(?)
                  OR LOWER(COALESCE(album,  '')) LIKE LOWER(?)
               ORDER BY id"#,
        )
        .bind(&pattern)
        .bind(&pattern)
        .bind(&pattern)
        .fetch_all(&self.pool)
        .await
        .context("list_tracks_filtered failed")?;
        Ok(rows.iter().map(row_to_track).collect())
    }

    pub async fn list_tracks_in_dir_filtered(&self, dir: &str, needle: &str) -> Result<Vec<Track>> {
        let prefix = if dir.ends_with('/') { dir.to_owned() } else { format!("{}/", dir) };
        let pattern = format!("%{}%", needle);
        let rows = sqlx::query(
            r#"SELECT id, path, title, artist, album, duration_secs, bpm FROM tracks
               WHERE path LIKE ? || '%'
                 AND (LOWER(COALESCE(title,  '')) LIKE LOWER(?)
                   OR LOWER(COALESCE(artist, '')) LIKE LOWER(?)
                   OR LOWER(COALESCE(album,  '')) LIKE LOWER(?))
               ORDER BY id"#,
        )
        .bind(&prefix)
        .bind(&pattern)
        .bind(&pattern)
        .bind(&pattern)
        .fetch_all(&self.pool)
        .await
        .context("list_tracks_in_dir_filtered failed")?;
        Ok(rows.iter().map(row_to_track).collect())
    }

    pub async fn list_tracks_in_playlist_filtered(
        &self,
        playlist_id: i64,
        needle: &str,
    ) -> Result<Vec<Track>> {
        let pattern = format!("%{}%", needle);
        let rows = sqlx::query(
            r#"SELECT t.id, t.path, t.title, t.artist, t.album, t.duration_secs, t.bpm
               FROM tracks t
               JOIN playlist_tracks pt ON pt.track_id = t.id
               WHERE pt.playlist_id = ?
                 AND (LOWER(COALESCE(t.title,  '')) LIKE LOWER(?)
                   OR LOWER(COALESCE(t.artist, '')) LIKE LOWER(?)
                   OR LOWER(COALESCE(t.album,  '')) LIKE LOWER(?))
               ORDER BY pt.position"#,
        )
        .bind(playlist_id)
        .bind(&pattern)
        .bind(&pattern)
        .bind(&pattern)
        .fetch_all(&self.pool)
        .await
        .context("list_tracks_in_playlist_filtered failed")?;
        Ok(rows.iter().map(row_to_track).collect())
    }

    pub async fn list_tracks_with_tag_filtered(
        &self,
        tag_id: i64,
        needle: &str,
    ) -> Result<Vec<Track>> {
        let pattern = format!("%{}%", needle);
        let rows = sqlx::query(
            r#"SELECT t.id, t.path, t.title, t.artist, t.album, t.duration_secs, t.bpm
               FROM tracks t
               JOIN track_tags tt ON tt.track_id = t.id
               WHERE tt.tag_id = ?
                 AND (LOWER(COALESCE(t.title,  '')) LIKE LOWER(?)
                   OR LOWER(COALESCE(t.artist, '')) LIKE LOWER(?)
                   OR LOWER(COALESCE(t.album,  '')) LIKE LOWER(?))
               ORDER BY t.id"#,
        )
        .bind(tag_id)
        .bind(&pattern)
        .bind(&pattern)
        .bind(&pattern)
        .fetch_all(&self.pool)
        .await
        .context("list_tracks_with_tag_filtered failed")?;
        Ok(rows.iter().map(row_to_track).collect())
    }
}

// ── Integration tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup() -> Db {
        let db = Db::open_in_memory().await.unwrap();
        db.migrate().await.unwrap();
        db
    }

    fn new_track(path: &str) -> NewTrack {
        NewTrack {
            path: PathBuf::from(path),
            title: Some("Test Title".into()),
            artist: Some("Test Artist".into()),
            album: Some("Test Album".into()),
            duration_secs: Some(180.0),
        }
    }

    #[tokio::test]
    async fn insert_and_get_by_id() {
        let db = setup().await;
        let inserted = db.insert_track(&new_track("/music/a.mp3")).await.unwrap();
        assert!(inserted.id > 0);

        let fetched = db.get_track(inserted.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, inserted.id);
        assert_eq!(fetched.path, PathBuf::from("/music/a.mp3"));
        assert_eq!(fetched.title.as_deref(), Some("Test Title"));
        assert_eq!(fetched.duration_secs, Some(180.0));
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let db = setup().await;
        assert!(db.get_track(9999).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_by_path() {
        let db = setup().await;
        db.insert_track(&new_track("/music/b.flac")).await.unwrap();
        let fetched = db
            .get_track_by_path(Path::new("/music/b.flac"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.path, PathBuf::from("/music/b.flac"));
    }

    #[tokio::test]
    async fn list_tracks_ordered() {
        let db = setup().await;
        db.insert_track(&new_track("/music/c.mp3")).await.unwrap();
        db.insert_track(&new_track("/music/d.mp3")).await.unwrap();
        db.insert_track(&new_track("/music/e.mp3")).await.unwrap();

        let tracks = db.list_tracks().await.unwrap();
        assert_eq!(tracks.len(), 3);
        assert!(tracks[0].id < tracks[1].id);
        assert!(tracks[1].id < tracks[2].id);
    }

    #[tokio::test]
    async fn update_track() {
        let db = setup().await;
        let t = db.insert_track(&new_track("/music/f.mp3")).await.unwrap();

        let updated = NewTrack {
            path: PathBuf::from("/music/f.mp3"),
            title: Some("New Title".into()),
            artist: None,
            album: None,
            duration_secs: Some(200.0),
        };
        assert!(db.update_track(t.id, &updated).await.unwrap());

        let fetched = db.get_track(t.id).await.unwrap().unwrap();
        assert_eq!(fetched.title.as_deref(), Some("New Title"));
        assert_eq!(fetched.artist, None);
    }

    #[tokio::test]
    async fn upsert_inserts_then_updates() {
        let db = setup().await;
        let t = new_track("/music/g.mp3");

        let first = db.upsert_track(&t).await.unwrap();
        assert_eq!(first.title.as_deref(), Some("Test Title"));

        let modified = NewTrack {
            title: Some("Updated".into()),
            ..t
        };
        let second = db.upsert_track(&modified).await.unwrap();
        assert_eq!(first.id, second.id, "upsert must reuse the same row");
        assert_eq!(second.title.as_deref(), Some("Updated"));
        assert_eq!(db.track_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn delete_track() {
        let db = setup().await;
        let t = db.insert_track(&new_track("/music/h.mp3")).await.unwrap();

        assert!(db.delete_track(t.id).await.unwrap());
        assert!(!db.delete_track(t.id).await.unwrap(), "already deleted");
        assert!(db.get_track(t.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn track_notes_roundtrip() {
        let db = setup().await;
        let t = db.insert_track(&new_track("/music/noted.mp3")).await.unwrap();
        assert!(db.get_track_notes(t.id).await.unwrap().is_none());
        assert!(db.update_track_notes(t.id, "Great song").await.unwrap());
        assert_eq!(db.get_track_notes(t.id).await.unwrap().as_deref(), Some("Great song"));
        // Overwrite
        db.update_track_notes(t.id, "Even better").await.unwrap();
        assert_eq!(db.get_track_notes(t.id).await.unwrap().as_deref(), Some("Even better"));
    }

    #[tokio::test]
    async fn list_playlists_for_track_test() {
        let db = setup().await;
        let t = db.insert_track(&new_track("/music/playlist-track.mp3")).await.unwrap();
        let p1 = db.insert_playlist("Alpha").await.unwrap();
        let p2 = db.insert_playlist("Beta").await.unwrap();
        db.add_track_to_playlist(t.id, p1.id).await.unwrap();
        db.add_track_to_playlist(t.id, p2.id).await.unwrap();
        let pls = db.list_playlists_for_track(t.id).await.unwrap();
        assert_eq!(pls.len(), 2);
        assert_eq!(pls[0].name, "Alpha");
        assert_eq!(pls[1].name, "Beta");
        // Track in no playlists
        let t2 = db.insert_track(&new_track("/music/no-playlists.mp3")).await.unwrap();
        assert!(db.list_playlists_for_track(t2.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn reorder_playlist_tracks_test() {
        let db = setup().await;
        let t1 = db.insert_track(&new_track("/music/a.mp3")).await.unwrap();
        let t2 = db.insert_track(&new_track("/music/b.mp3")).await.unwrap();
        let t3 = db.insert_track(&new_track("/music/c.mp3")).await.unwrap();
        let pl = db.insert_playlist("reorder-test").await.unwrap();
        db.add_track_to_playlist(t1.id, pl.id).await.unwrap();
        db.add_track_to_playlist(t2.id, pl.id).await.unwrap();
        db.add_track_to_playlist(t3.id, pl.id).await.unwrap();
        // Reverse order
        db.reorder_playlist_tracks(pl.id, &[t3.id, t1.id, t2.id]).await.unwrap();
        let tracks = db.list_tracks_in_playlist(pl.id).await.unwrap();
        assert_eq!(
            tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![t3.id, t1.id, t2.id]
        );
    }

    #[tokio::test]
    async fn duplicate_path_is_rejected() {
        let db = setup().await;
        db.insert_track(&new_track("/music/dup.mp3")).await.unwrap();
        let result = db.insert_track(&new_track("/music/dup.mp3")).await;
        assert!(result.is_err(), "duplicate path must fail");
    }

    #[tokio::test]
    async fn track_count() {
        let db = setup().await;
        assert_eq!(db.track_count().await.unwrap(), 0);
        db.insert_track(&new_track("/music/i.mp3")).await.unwrap();
        db.insert_track(&new_track("/music/j.mp3")).await.unwrap();
        assert_eq!(db.track_count().await.unwrap(), 2);
    }
}
