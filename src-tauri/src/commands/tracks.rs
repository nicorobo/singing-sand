use std::collections::HashSet;

use tauri::State;

use crate::{
    dtos::{ExpandedPlaylistItemDto, ExpandedTagItemDto, ExpandedTrackDto, SelectedTagDto, SelectionChangedDto},
    state::AppState,
};

#[tauri::command]
pub async fn expand_track(
    track_id: i64,
    state: State<'_, AppState>,
) -> Result<ExpandedTrackDto, String> {
    let (tags_res, playlists_res, notes_res, track_res) = tokio::join!(
        state.db.list_tags_for_track(track_id),
        state.db.list_playlists_for_track(track_id),
        state.db.get_track_notes(track_id),
        state.db.get_track(track_id),
    );

    let tags = tags_res
        .unwrap_or_default()
        .iter()
        .map(|t| ExpandedTagItemDto { id: t.id, name: t.name.clone(), color: t.color.clone() })
        .collect();

    let playlists = playlists_res
        .unwrap_or_default()
        .iter()
        .map(|p| ExpandedPlaylistItemDto { id: p.id, name: p.name.clone() })
        .collect();

    let notes = notes_res.unwrap_or(None).unwrap_or_default();

    let duration_formatted = track_res
        .ok()
        .flatten()
        .and_then(|t| t.duration_secs)
        .map(|s| {
            let total = s as u64;
            format!("{}:{:02}", total / 60, total % 60)
        })
        .unwrap_or_default();

    Ok(ExpandedTrackDto { tags, playlists, notes, duration_formatted })
}

#[tauri::command]
pub async fn remove_tag_from_expanded(
    track_id: i64,
    tag_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<ExpandedTagItemDto>, String> {
    state.db.unassign_tag(track_id, tag_id).await.map_err(|e| e.to_string())?;
    let tags = state
        .db
        .list_tags_for_track(track_id)
        .await
        .unwrap_or_default()
        .iter()
        .map(|t| ExpandedTagItemDto { id: t.id, name: t.name.clone(), color: t.color.clone() })
        .collect();
    Ok(tags)
}

#[tauri::command]
pub async fn save_notes(
    track_id: i64,
    text: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _ = state.notes_tx.send((track_id, text));
    Ok(())
}

#[tauri::command]
pub async fn track_clicked(
    id: i64,
    shift: bool,
    meta: bool,
    state: State<'_, AppState>,
) -> Result<SelectionChangedDto, String> {
    let current_ids = state.current_track_ids.lock().unwrap().clone();
    let selected_ids: Vec<i64> = {
        let mut sel = state.selection.lock().unwrap();
        let mut last = state.last_selected_id.lock().unwrap();

        if meta {
            if sel.contains(&id) {
                sel.remove(&id);
            } else {
                sel.insert(id);
                *last = Some(id);
            }
        } else if shift {
            let anchor = last.unwrap_or(id);
            let anchor_pos = current_ids.iter().position(|&x| x == anchor);
            let clicked_pos = current_ids.iter().position(|&x| x == id);
            if let (Some(a), Some(b)) = (anchor_pos, clicked_pos) {
                let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                sel.clear();
                for &tid in &current_ids[lo..=hi] {
                    sel.insert(tid);
                }
            }
        } else {
            sel.clear();
            sel.insert(id);
            *last = Some(id);
        }

        sel.iter().copied().collect()
    }; // guards dropped here

    let all_tags = state.db.list_tags().await.map_err(|e| e.to_string())?;
    let track_tags = state.db.list_tags_for_tracks(&selected_ids).await.map_err(|e| e.to_string())?;
    let sel_set: HashSet<i64> = selected_ids.iter().copied().collect();
    let total = sel_set.len();

    let tag_items = all_tags
        .iter()
        .map(|tag| {
            let count = track_tags
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

    Ok(SelectionChangedDto { selected_ids, tag_items })
}
