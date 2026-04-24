use tauri::{AppHandle, Emitter, State};

use crate::{dtos::PlaylistDto, state::AppState};

#[tauri::command]
pub async fn create_playlist(
    name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<PlaylistDto>, String> {
    state.db.insert_playlist(&name).await.map_err(|e| e.to_string())?;
    let playlists: Vec<PlaylistDto> = state.db.list_playlists().await.map_err(|e| e.to_string())?.iter().map(PlaylistDto::from).collect();
    app.emit("sidebar-playlists-updated", &playlists).ok();
    Ok(playlists)
}

#[tauri::command]
pub async fn add_to_playlist(
    playlist_id: i64,
    track_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.db.add_track_to_playlist(track_id, playlist_id).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn remove_from_playlist(
    playlist_id: i64,
    track_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.db.remove_track_from_playlist(track_id, playlist_id).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn reorder_playlist_tracks(
    playlist_id: i64,
    new_order: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.db.reorder_playlist_tracks(playlist_id, &new_order).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn delete_playlist(
    playlist_id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<PlaylistDto>, String> {
    state.db.delete_playlist(playlist_id).await.map_err(|e| e.to_string())?;
    let playlists: Vec<PlaylistDto> = state.db.list_playlists().await.map_err(|e| e.to_string())?.iter().map(PlaylistDto::from).collect();
    app.emit("sidebar-playlists-updated", &playlists).ok();
    Ok(playlists)
}

#[tauri::command]
pub async fn add_selected_to_playlist(
    playlist_id: i64,
    sel_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    for track_id in sel_ids {
        if let Err(e) = state.db.add_track_to_playlist(track_id, playlist_id).await {
            tracing::warn!("add_selected_to_playlist failed for {track_id}: {e}");
        }
    }
    Ok(())
}
