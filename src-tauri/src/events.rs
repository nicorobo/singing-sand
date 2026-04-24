use std::sync::Arc;

use ss_audio::AudioEngine;
use ss_core::AudioEvent;
use tauri::{AppHandle, Emitter};

pub fn spawn_audio_event_forwarder(engine: Arc<AudioEngine>, app: AppHandle) {
    let event_rx = engine.event_rx.clone();
    tokio::spawn(async move {
        while let Ok(event) = event_rx.recv_async().await {
            match event {
                AudioEvent::PositionChanged(pos) => {
                    app.emit("position-changed", serde_json::json!({ "position": pos })).ok();
                }
                AudioEvent::TrackFinished => {
                    app.emit("track-finished", serde_json::json!({})).ok();
                }
                AudioEvent::Error(msg) => {
                    tracing::error!("audio error: {msg}");
                }
            }
        }
    });
}
