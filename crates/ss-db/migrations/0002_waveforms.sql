CREATE TABLE IF NOT EXISTS waveforms (
    track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
    data     BLOB NOT NULL   -- little-endian f32 RMS values
);
