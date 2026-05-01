-- Playlist groups for hierarchical organization.
-- Groups can contain other groups (self-referencing parent_id).
-- Deleting a group cascades to child groups; playlists in the group get group_id = NULL (moved to root).
CREATE TABLE playlist_groups (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    name      TEXT    NOT NULL,
    parent_id INTEGER REFERENCES playlist_groups(id) ON DELETE CASCADE,
    position  REAL    NOT NULL DEFAULT 0.0
);

-- Add group membership and fractional position ordering to playlists.
-- group_id NULL means the playlist lives at root level.
-- position REAL allows fractional repositioning with a single UPDATE.
ALTER TABLE playlists ADD COLUMN group_id INTEGER REFERENCES playlist_groups(id) ON DELETE SET NULL;
ALTER TABLE playlists ADD COLUMN position REAL NOT NULL DEFAULT 0.0;
