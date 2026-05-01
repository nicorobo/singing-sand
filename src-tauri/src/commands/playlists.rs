use tauri::{AppHandle, Emitter, State};

use crate::{
    dtos::{PlaylistDto, PlaylistGroupDto, SidebarPlaylistDataDto, TrackDto},
    state::AppState,
};

async fn load_sidebar_playlist_data(state: &AppState) -> Result<SidebarPlaylistDataDto, String> {
    let (playlists_res, groups_res) = tokio::join!(
        state.db.list_playlists(),
        state.db.list_playlist_groups(),
    );
    let playlists = playlists_res.map_err(|e| e.to_string())?.iter().map(PlaylistDto::from).collect();
    let groups = groups_res.map_err(|e| e.to_string())?.iter().map(PlaylistGroupDto::from).collect();
    Ok(SidebarPlaylistDataDto { playlists, groups })
}

#[tauri::command]
pub async fn create_playlist(
    name: String,
    group_id: Option<i64>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SidebarPlaylistDataDto, String> {
    state.db.insert_playlist(&name, group_id).await.map_err(|e| e.to_string())?;
    let data = load_sidebar_playlist_data(&state).await?;
    app.emit("sidebar-playlists-updated", &data).ok();
    Ok(data)
}

#[tauri::command]
pub async fn delete_playlist(
    playlist_id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SidebarPlaylistDataDto, String> {
    state.db.delete_playlist(playlist_id).await.map_err(|e| e.to_string())?;
    let data = load_sidebar_playlist_data(&state).await?;
    app.emit("sidebar-playlists-updated", &data).ok();
    Ok(data)
}

#[tauri::command]
pub async fn rename_playlist(
    playlist_id: i64,
    name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SidebarPlaylistDataDto, String> {
    state.db.rename_playlist(playlist_id, &name).await.map_err(|e| e.to_string())?;
    let data = load_sidebar_playlist_data(&state).await?;
    app.emit("sidebar-playlists-updated", &data).ok();
    Ok(data)
}

#[tauri::command]
pub async fn create_playlist_group(
    name: String,
    parent_id: Option<i64>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SidebarPlaylistDataDto, String> {
    state.db.insert_playlist_group(&name, parent_id).await.map_err(|e| e.to_string())?;
    let data = load_sidebar_playlist_data(&state).await?;
    app.emit("sidebar-playlists-updated", &data).ok();
    Ok(data)
}

#[tauri::command]
pub async fn delete_playlist_group(
    group_id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SidebarPlaylistDataDto, String> {
    state.db.delete_playlist_group(group_id).await.map_err(|e| e.to_string())?;
    let data = load_sidebar_playlist_data(&state).await?;
    app.emit("sidebar-playlists-updated", &data).ok();
    Ok(data)
}

#[tauri::command]
pub async fn rename_playlist_group(
    group_id: i64,
    name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SidebarPlaylistDataDto, String> {
    state.db.rename_playlist_group(group_id, &name).await.map_err(|e| e.to_string())?;
    let data = load_sidebar_playlist_data(&state).await?;
    app.emit("sidebar-playlists-updated", &data).ok();
    Ok(data)
}

/// Move a playlist or group node within the tree.
/// node_type: "playlist" | "group"
/// before_index: 0-based insertion index among destination siblings.
#[tauri::command]
pub async fn move_playlist_node(
    node_type: String,
    node_id: i64,
    new_parent_id: Option<i64>,
    before_index: usize,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SidebarPlaylistDataDto, String> {
    match node_type.as_str() {
        "playlist" => state.db.move_playlist(node_id, new_parent_id, before_index).await,
        "group"    => state.db.move_playlist_group(node_id, new_parent_id, before_index).await,
        other      => Err(anyhow::anyhow!("unknown node_type: {other}")),
    }
    .map_err(|e| e.to_string())?;

    let data = load_sidebar_playlist_data(&state).await?;
    app.emit("sidebar-playlists-updated", &data).ok();
    Ok(data)
}

#[tauri::command]
pub async fn nav_group(
    group_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<TrackDto>, String> {
    let tracks = state.db.list_tracks_in_group(group_id).await.map_err(|e| e.to_string())?;
    *state.current_track_ids.lock().unwrap() = tracks.iter().map(|t| t.id).collect();
    Ok(tracks.iter().map(TrackDto::from).collect())
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
