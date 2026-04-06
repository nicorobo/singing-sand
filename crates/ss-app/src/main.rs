use anyhow::{Context, Result};
use ss_audio::AudioEngine;
use ss_core::{AudioCommand, AudioEvent};
use std::{path::PathBuf, time::Duration};
use tracing::info;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("singing_sand=debug".parse().unwrap())
                .add_directive("ss_audio=debug".parse().unwrap()),
        )
        .init();

    let path: PathBuf = std::env::args()
        .nth(1)
        .context("Usage: singing-sand <audio-file>")?
        .into();

    anyhow::ensure!(path.exists(), "file not found: {}", path.display());

    info!("starting audio engine");
    let engine = AudioEngine::spawn()?;

    engine.send(AudioCommand::LoadAndPlay {
        path: path.clone(),
        start_sec: 0.0,
    });

    info!("playing {} — press Ctrl-C to stop", path.display());

    // Block until the track finishes or the user interrupts.
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
            Err(flume::RecvTimeoutError::Timeout) => {
                // Continue polling.
            }
            Err(flume::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    println!();
    Ok(())
}
