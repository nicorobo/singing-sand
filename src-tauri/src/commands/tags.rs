use std::collections::HashSet;

use tauri::{AppHandle, Emitter, State};

use crate::{
    dtos::{SelectedTagDto, TagDto},
    state::AppState,
};

#[tauri::command]
pub async fn create_tag(
    name: String,
    color: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<TagDto>, String> {
    state.db.insert_tag(&name, &color).await.map_err(|e| e.to_string())?;
    let tags: Vec<TagDto> = state.db.list_tags().await.map_err(|e| e.to_string())?.iter().map(TagDto::from).collect();
    app.emit("sidebar-tags-updated", &tags).ok();
    Ok(tags)
}

#[tauri::command]
pub async fn delete_tag(
    tag_id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<TagDto>, String> {
    state.db.delete_tag(tag_id).await.map_err(|e| e.to_string())?;
    let tags: Vec<TagDto> = state.db.list_tags().await.map_err(|e| e.to_string())?.iter().map(TagDto::from).collect();
    app.emit("sidebar-tags-updated", &tags).ok();
    Ok(tags)
}

#[tauri::command]
pub async fn update_tag(
    tag_id: i64,
    name: String,
    color: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<TagDto>, String> {
    state.db.update_tag(tag_id, &name, &color).await.map_err(|e| e.to_string())?;
    let tags: Vec<TagDto> = state.db.list_tags().await.map_err(|e| e.to_string())?.iter().map(TagDto::from).collect();
    app.emit("sidebar-tags-updated", &tags).ok();
    Ok(tags)
}

#[tauri::command]
pub async fn toggle_tag_for_selection(
    tag_id: i64,
    sel_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<SelectedTagDto>, String> {
    if sel_ids.is_empty() {
        return Ok(vec![]);
    }

    let track_tags = state.db.list_tags_for_tracks(&sel_ids).await.map_err(|e| e.to_string())?;
    let tracks_with_tag: HashSet<i64> = track_tags
        .iter()
        .filter(|(_, t)| t.id == tag_id)
        .map(|(tid, _)| *tid)
        .collect();

    let all_have_it = tracks_with_tag.len() == sel_ids.len();
    for &track_id in &sel_ids {
        if all_have_it {
            state.db.unassign_tag(track_id, tag_id).await.ok();
        } else if !tracks_with_tag.contains(&track_id) {
            state.db.assign_tag(track_id, tag_id).await.ok();
        }
    }

    let all_tags = state.db.list_tags().await.map_err(|e| e.to_string())?;
    let updated_track_tags = state.db.list_tags_for_tracks(&sel_ids).await.map_err(|e| e.to_string())?;
    let sel_set: HashSet<i64> = sel_ids.iter().copied().collect();
    let total = sel_set.len();

    let items = all_tags
        .iter()
        .map(|tag| {
            let count = updated_track_tags
                .iter()
                .filter(|(tid, t)| sel_set.contains(tid) && t.id == tag.id)
                .count();
            SelectedTagDto {
                id:       tag.id,
                name:     tag.name.clone(),
                color:    tag.color.clone(),
                assigned: count == total && total > 0,
                partial:  count > 0 && count < total,
            }
        })
        .collect();

    Ok(items)
}
