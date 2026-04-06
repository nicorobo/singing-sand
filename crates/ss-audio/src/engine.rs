use crate::source::SymphoniaSource;
use anyhow::Result;
use crossbeam_channel::Sender as CbSender;
use flume::{Receiver, Sender};
use rodio::{OutputStream, OutputStreamHandle, Sink};
use ss_core::{AudioCommand, AudioEvent};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tracing::{error, info};

/// The audio engine. Owns a dedicated audio thread with a Rodio `Sink`.
///
/// Commands are sent via a crossbeam bounded channel (compatible with
/// `crossbeam_channel::select!`); events are returned via a `flume` channel
/// (which supports async `.recv_async()` for use in tokio tasks).
pub struct AudioEngine {
    cmd_tx: CbSender<AudioCommand>,
    /// Async-compatible event receiver for use in tokio tasks.
    pub event_rx: Receiver<AudioEvent>,
    /// Shared playback position, updated by the active SymphoniaSource.
    pub position: Arc<Mutex<f64>>,
}

impl AudioEngine {
    /// Spawn the audio thread and return an `AudioEngine` handle.
    pub fn spawn() -> Result<Self> {
        let (cmd_tx, cmd_rx) = crossbeam_channel::bounded::<AudioCommand>(32);
        let (event_tx, event_rx) = flume::unbounded::<AudioEvent>();
        let position = Arc::new(Mutex::new(0.0f64));
        let position_clone = Arc::clone(&position);

        thread::Builder::new()
            .name("audio-engine".into())
            .spawn(move || {
                audio_thread(cmd_rx, event_tx, position_clone);
            })?;

        Ok(Self {
            cmd_tx,
            event_rx,
            position,
        })
    }

    pub fn send(&self, cmd: AudioCommand) {
        if let Err(e) = self.cmd_tx.send(cmd) {
            error!("audio engine send failed: {e}");
        }
    }

    /// Current playback position in seconds.
    pub fn position_secs(&self) -> f64 {
        *self.position.lock().unwrap()
    }
}

// ---------------------------------------------------------------------------
// Audio thread implementation
// ---------------------------------------------------------------------------

struct AudioState {
    sink: Sink,
    #[allow(dead_code)]
    stream: OutputStream,
    #[allow(dead_code)]
    stream_handle: OutputStreamHandle,
    current_path: Option<PathBuf>,
    duration_secs: f64,
    position: Arc<Mutex<f64>>,
    event_tx: Sender<AudioEvent>,
}

impl AudioState {
    fn load_and_play(&mut self, path: PathBuf, start_sec: f64) {
        self.sink.stop();
        *self.position.lock().unwrap() = start_sec;

        let source = match SymphoniaSource::new(&path, start_sec, Arc::clone(&self.position)) {
            Ok(s) => s,
            Err(e) => {
                error!("failed to open {}: {e}", path.display());
                let _ = self.event_tx.send(AudioEvent::Error(e.to_string()));
                return;
            }
        };

        self.duration_secs = source.duration_secs.unwrap_or(0.0);
        self.current_path = Some(path.clone());

        info!("playing: {}", path.display());
        self.sink.append(source);
        self.sink.play();
    }

    fn seek(&mut self, target_secs: f64) {
        if let Some(path) = self.current_path.clone() {
            let was_paused = self.sink.is_paused();
            self.load_and_play(path, target_secs);
            if was_paused {
                self.sink.pause();
            }
        }
    }
}

fn audio_thread(
    cmd_rx: crossbeam_channel::Receiver<AudioCommand>,
    event_tx: Sender<AudioEvent>,
    position: Arc<Mutex<f64>>,
) {
    // OutputStream must be created on the audio thread.
    let (stream, stream_handle) = match OutputStream::try_default() {
        Ok(pair) => pair,
        Err(e) => {
            error!("failed to open audio output: {e}");
            let _ = event_tx.send(AudioEvent::Error(e.to_string()));
            return;
        }
    };

    let sink = Sink::try_new(&stream_handle).expect("failed to create rodio Sink");
    sink.pause();

    let mut state = AudioState {
        sink,
        stream,
        stream_handle,
        current_path: None,
        duration_secs: 0.0,
        position,
        event_tx: event_tx.clone(),
    };

    // Position polling ticker — emit PositionChanged at ~10 Hz.
    let ticker = crossbeam_channel::tick(Duration::from_millis(100));

    loop {
        crossbeam_channel::select! {
            recv(cmd_rx) -> msg => {
                match msg {
                    Ok(cmd) => handle_command(cmd, &mut state),
                    Err(_) => {
                        info!("audio engine command channel closed, exiting");
                        break;
                    }
                }
            }
            recv(ticker) -> _ => {
                if !state.sink.is_paused() && !state.sink.empty() {
                    let pos = *state.position.lock().unwrap();
                    let _ = event_tx.send(AudioEvent::PositionChanged(pos));
                }
                // Detect natural track end.
                if state.current_path.is_some() && state.sink.empty() {
                    state.current_path = None;
                    let _ = event_tx.send(AudioEvent::TrackFinished);
                }
            }
        }
    }
}

fn handle_command(cmd: AudioCommand, state: &mut AudioState) {
    match cmd {
        AudioCommand::LoadAndPlay { path, start_sec } => {
            state.load_and_play(path, start_sec);
        }
        AudioCommand::Play => {
            state.sink.play();
        }
        AudioCommand::Pause => {
            state.sink.pause();
        }
        AudioCommand::Stop => {
            state.sink.stop();
            state.current_path = None;
            *state.position.lock().unwrap() = 0.0;
        }
        AudioCommand::Seek(secs) => {
            state.seek(secs);
        }
        AudioCommand::SetVolume(vol) => {
            state.sink.set_volume(vol);
        }
    }
}
