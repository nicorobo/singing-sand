-- Migration 0009: store raw (un-normalized) waveform data so that render-time
-- normalization modes (None / PerBand / Global) are meaningful.
-- Clearing waveforms forces re-analysis on next startup.
DELETE FROM waveforms;
