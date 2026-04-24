use std::sync::Arc;

use ss_audio::analyze_track;
use ss_core::AudioCommand;
use ss_waveform::WaveformBucket;
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

#[tauri::command]
pub async fn play_track(
    track_id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let track = state
        .db
        .get_track(track_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("track not found")?;

    state.engine.send(AudioCommand::LoadAndPlay {
        path: track.path.clone(),
        start_sec: 0.0,
    });

    let duration = track.duration_secs.unwrap_or(0.0);
    *state.current_track_id.lock().unwrap() = Some(track_id);
    *state.current_duration.lock().unwrap() = duration;

    app.emit(
        "track-loaded",
        serde_json::json!({
            "track_id": track_id,
            "duration": duration,
            "title":  track.title.unwrap_or_default(),
            "artist": track.artist.unwrap_or_default(),
        }),
    )
    .ok();

    // Load or compute waveform bands in the background, then emit waveform-ready.
    let db = Arc::clone(&state.db);
    let render_settings = Arc::clone(&state.render_settings);
    let current_bands = Arc::clone(&state.current_bands);
    let path = track.path.clone();
    tokio::spawn(async move {
        let bands: Vec<[f32; 3]> = match db.get_waveform_bands(track_id).await {
            Ok(Some(cached)) => cached,
            _ => {
                let path2 = path.clone();
                match tokio::task::spawn_blocking(move || analyze_track(&path2, 1000)).await {
                    Ok(Ok(result)) => {
                        let arrays: Vec<[f32; 3]> = result.waveform.iter().map(|b| b.to_array()).collect();
                        if let Err(e) = db.save_waveform_bands(track_id, &arrays).await {
                            tracing::warn!("failed to cache waveform bands: {e}");
                        }
                        arrays
                    }
                    _ => return,
                }
            }
        };

        let buckets: Vec<WaveformBucket> = bands.iter().map(|&a| WaveformBucket::from_array(a)).collect();
        *current_bands.lock().unwrap() = buckets;
        let _ = render_settings; // settings used lazily via get_waveform command
        app.emit("waveform-ready", serde_json::json!({ "track_id": track_id })).ok();
    });

    Ok(())
}

#[tauri::command]
pub async fn play(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.send(AudioCommand::Play);
    Ok(())
}

#[tauri::command]
pub async fn pause(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.send(AudioCommand::Pause);
    Ok(())
}

#[tauri::command]
pub async fn stop(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.send(AudioCommand::Stop);
    Ok(())
}

#[tauri::command]
pub async fn seek(fraction: f64, state: State<'_, AppState>) -> Result<(), String> {
    let duration = *state.current_duration.lock().unwrap();
    let target = (fraction * duration).max(0.0);
    state.engine.send(AudioCommand::Seek(target));
    Ok(())
}
