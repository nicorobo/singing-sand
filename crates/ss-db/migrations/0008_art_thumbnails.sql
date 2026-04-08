-- Add pre-computed 44×44 raw RGB thumbnail to album_art.
-- 44×44×3 = 5,808 bytes vs 200–500 KB raw art blob — no decode needed at display time.
ALTER TABLE album_art ADD COLUMN thumbnail_44 BLOB;
