use std::path::PathBuf;

/// Commands sent from the application to the audio engine thread.
#[derive(Debug)]
pub enum AudioCommand {
    /// Load a file and start playing from the given offset (seconds).
    LoadAndPlay { path: PathBuf, start_sec: f64 },
    Play,
    Pause,
    Stop,
    /// Seek to the given position in seconds (stop + restart).
    Seek(f64),
    /// Volume in [0.0, 1.0].
    SetVolume(f32),
}

/// Events emitted by the audio engine back to the application.
#[derive(Debug, Clone)]
pub enum AudioEvent {
    /// Current playback position, emitted ~10 Hz while playing.
    PositionChanged(f64),
    TrackFinished,
    Error(String),
}

/// Snapshot of current playback state.
#[derive(Debug, Clone, Default)]
pub struct PlaybackState {
    pub playing: bool,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub volume: f32,
    pub current_path: Option<std::path::PathBuf>,
}

impl PlaybackState {
    pub fn new() -> Self {
        Self {
            volume: 1.0,
            ..Default::default()
        }
    }
}
