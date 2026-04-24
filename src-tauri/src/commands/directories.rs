use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use ss_core::Track;
use ss_library::Scanner;
use tauri::{AppHandle, Emitter, State};

use crate::{dtos::DirTreeItemDto, state::AppState};

fn track_dirs_from(tracks: &[Track]) -> HashSet<String> {
    tracks
        .iter()
        .filter_map(|t| t.path.parent().map(|p| p.to_string_lossy().into_owned()))
        .collect()
}

fn append_dir_node(
    result: &mut Vec<DirTreeItemDto>,
    dir: &str,
    all_dirs: &[&str],
    indent: u32,
    expanded: &HashMap<String, bool>,
    is_root: bool,
) {
    let prefix = format!("{dir}/");
    let direct_children: Vec<&str> = all_dirs
        .iter()
        .copied()
        .filter(|&d| d.starts_with(&prefix) && !d[prefix.len()..].contains('/'))
        .collect();

    let has_children = !direct_children.is_empty();
    let is_expanded = expanded.get(dir).copied().unwrap_or(false);
    let name = Path::new(dir)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.to_string());

    result.push(DirTreeItemDto {
        path: dir.to_string(),
        name,
        indent: indent.min(4),
        has_children,
        is_expanded,
        is_root,
    });

    if is_expanded {
        for child in direct_children {
            append_dir_node(result, child, all_dirs, indent + 1, expanded, false);
        }
    }
}

pub fn build_dir_tree(
    roots: &[PathBuf],
    track_dirs: &HashSet<String>,
    expanded: &HashMap<String, bool>,
) -> Vec<DirTreeItemDto> {
    let mut sorted: Vec<&str> = track_dirs.iter().map(|s| s.as_str()).collect();
    sorted.sort_unstable();
    let mut result = Vec::new();
    for root in roots {
        let root_str = root.to_string_lossy();
        append_dir_node(&mut result, root_str.as_ref(), &sorted, 0, expanded, true);
    }
    result
}

pub async fn emit_dir_tree(
    db: &Arc<ss_db::Db>,
    expanded: &Arc<Mutex<HashMap<String, bool>>>,
    app: &AppHandle,
) {
    let dirs = db.list_scanned_dirs().await.unwrap_or_default();
    let all_tracks = db.list_tracks().await.unwrap_or_default();
    let tdirs = track_dirs_from(&all_tracks);
    let items = {
        let exp = expanded.lock().unwrap();
        build_dir_tree(&dirs, &tdirs, &exp)
    };
    app.emit("dir-tree-updated", &items).ok();
}

fn check_duplicate_dir(path: &str, existing: &[PathBuf]) -> Option<String> {
    let p = path.trim_end_matches('/');
    for r in existing {
        let r_str = r.to_string_lossy();
        let r = r_str.trim_end_matches('/');
        if p == r || p.starts_with(&format!("{r}/")) || r.starts_with(&format!("{p}/")) {
            return Some("This directory is already in your library.".into());
        }
    }
    None
}

#[tauri::command]
pub async fn add_directory(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await else {
        return Ok(());
    };
    let path = folder.path().to_path_buf();
    let path_str = path.to_string_lossy().to_string();

    let existing = state.db.list_scanned_dirs().await.unwrap_or_default();
    if let Some(msg) = check_duplicate_dir(&path_str, &existing) {
        app.emit("dir-duplicate", serde_json::json!({ "message": msg })).ok();
        return Ok(());
    }

    if let Err(e) = state.db.record_scanned_dir(&path).await {
        tracing::warn!("record_scanned_dir failed for {path_str}: {e}");
    }

    if let Ok(mut fw) = state.file_watcher.lock() {
        if let Err(e) = fw.watch(&path) {
            tracing::warn!("watch failed for {path_str}: {e}");
        }
    }

    emit_dir_tree(&state.db, &state.expanded_dirs, &app).await;

    let db2 = Arc::clone(&state.db);
    let aq = Arc::clone(&state.analysis_queue);
    let expanded = Arc::clone(&state.expanded_dirs);
    let app2 = app.clone();
    tokio::spawn(async move {
        let scanner = Scanner::new(Arc::clone(&db2));
        match scanner.scan_dir(&path).await {
            Ok(stats) => {
                tracing::info!("added dir {path_str} — {} upserted, {} errors", stats.upserted, stats.errors);
                aq.enqueue(stats.upserted_tracks);
            }
            Err(e) => tracing::warn!("scan_dir failed for {path_str}: {e}"),
        }
        emit_dir_tree(&db2, &expanded, &app2).await;
        app2.emit("library-changed", serde_json::json!({})).ok();
    });
    Ok(())
}

#[tauri::command]
pub async fn add_directory_path(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = PathBuf::from(&path);
    if !path.is_dir() {
        return Ok(());
    }
    let path_str = path.to_string_lossy().to_string();

    let existing = state.db.list_scanned_dirs().await.unwrap_or_default();
    if let Some(msg) = check_duplicate_dir(&path_str, &existing) {
        app.emit("dir-duplicate", serde_json::json!({ "message": msg })).ok();
        return Ok(());
    }

    if let Err(e) = state.db.record_scanned_dir(&path).await {
        tracing::warn!("record_scanned_dir failed for {path_str}: {e}");
    }

    if let Ok(mut fw) = state.file_watcher.lock() {
        if let Err(e) = fw.watch(&path) {
            tracing::warn!("watch failed for {path_str}: {e}");
        }
    }

    emit_dir_tree(&state.db, &state.expanded_dirs, &app).await;

    let db2 = Arc::clone(&state.db);
    let aq = Arc::clone(&state.analysis_queue);
    let expanded = Arc::clone(&state.expanded_dirs);
    let app2 = app.clone();
    tokio::spawn(async move {
        let scanner = Scanner::new(Arc::clone(&db2));
        match scanner.scan_dir(&path).await {
            Ok(stats) => {
                tracing::info!("added dir {path_str} — {} upserted, {} errors", stats.upserted, stats.errors);
                aq.enqueue(stats.upserted_tracks);
            }
            Err(e) => tracing::warn!("scan_dir failed for {path_str}: {e}"),
        }
        emit_dir_tree(&db2, &expanded, &app2).await;
        app2.emit("library-changed", serde_json::json!({})).ok();
    });
    Ok(())
}

#[tauri::command]
pub async fn toggle_dir_expanded(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    {
        let mut exp = state.expanded_dirs.lock().unwrap();
        let e = exp.entry(path).or_insert(false);
        *e = !*e;
    }
    emit_dir_tree(&state.db, &state.expanded_dirs, &app).await;
    Ok(())
}

#[tauri::command]
pub async fn remove_scanned_dir(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let tracks = state.db.list_tracks_in_dir(&path).await.unwrap_or_default();
    for t in &tracks {
        if let Err(e) = state.db.delete_track_by_path(&t.path.to_string_lossy()).await {
            tracing::warn!("delete_track failed: {e}");
        }
    }
    if let Err(e) = state.db.remove_scanned_dir(&path).await {
        tracing::warn!("remove_scanned_dir failed: {e}");
    }
    if let Ok(mut fw) = state.file_watcher.lock() {
        let _ = fw.unwatch(Path::new(&path));
    }
    state.expanded_dirs.lock().unwrap().remove(&path);
    emit_dir_tree(&state.db, &state.expanded_dirs, &app).await;
    app.emit("library-changed", serde_json::json!({})).ok();
    Ok(())
}
