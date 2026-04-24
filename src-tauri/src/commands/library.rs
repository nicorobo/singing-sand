use tauri::State;

use crate::{dtos::TrackDto, state::AppState};

fn tracks_to_dtos(tracks: &[ss_core::Track]) -> Vec<TrackDto> {
    tracks.iter().map(TrackDto::from).collect()
}

#[tauri::command]
pub async fn nav_all(state: State<'_, AppState>) -> Result<Vec<TrackDto>, String> {
    let tracks = state.db.list_tracks().await.map_err(|e| e.to_string())?;
    *state.current_track_ids.lock().unwrap() = tracks.iter().map(|t| t.id).collect();
    Ok(tracks_to_dtos(&tracks))
}

#[tauri::command]
pub async fn nav_select_dir(path: String, state: State<'_, AppState>) -> Result<Vec<TrackDto>, String> {
    let tracks = state.db.list_tracks_in_dir(&path).await.map_err(|e| e.to_string())?;
    *state.current_track_ids.lock().unwrap() = tracks.iter().map(|t| t.id).collect();
    Ok(tracks_to_dtos(&tracks))
}

#[tauri::command]
pub async fn nav_playlist(playlist_id: i64, state: State<'_, AppState>) -> Result<Vec<TrackDto>, String> {
    let tracks = state.db.list_tracks_in_playlist(playlist_id).await.map_err(|e| e.to_string())?;
    *state.current_track_ids.lock().unwrap() = tracks.iter().map(|t| t.id).collect();
    Ok(tracks_to_dtos(&tracks))
}

#[tauri::command]
pub async fn nav_tag(tag_id: i64, state: State<'_, AppState>) -> Result<Vec<TrackDto>, String> {
    let tracks = state.db.list_tracks_with_tag(tag_id).await.map_err(|e| e.to_string())?;
    *state.current_track_ids.lock().unwrap() = tracks.iter().map(|t| t.id).collect();
    Ok(tracks_to_dtos(&tracks))
}

#[tauri::command]
pub async fn search_tracks(
    query: String,
    nav_kind: u8,
    nav_id: i64,
    nav_dir: String,
    state: State<'_, AppState>,
) -> Result<Vec<TrackDto>, String> {
    let db = &state.db;
    let tracks = if query.is_empty() {
        match nav_kind {
            0 => db.list_tracks().await,
            1 => db.list_tracks_in_dir(&nav_dir).await,
            2 => db.list_tracks_in_playlist(nav_id).await,
            3 => db.list_tracks_with_tag(nav_id).await,
            _ => Ok(vec![]),
        }
    } else {
        match nav_kind {
            0 => db.list_tracks_filtered(&query).await,
            1 => db.list_tracks_in_dir_filtered(&nav_dir, &query).await,
            2 => db.list_tracks_in_playlist_filtered(nav_id, &query).await,
            3 => db.list_tracks_with_tag_filtered(nav_id, &query).await,
            _ => Ok(vec![]),
        }
    }
    .map_err(|e| e.to_string())?;
    *state.current_track_ids.lock().unwrap() = tracks.iter().map(|t| t.id).collect();
    Ok(tracks_to_dtos(&tracks))
}
