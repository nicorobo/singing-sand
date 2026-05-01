mod analysis;
mod commands;
mod dtos;
mod events;
mod settings;
mod state;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::Context as _;
use ss_audio::AudioEngine;
use ss_db::Db;
use ss_library::{FileWatcher, LibraryEvent};
use tauri::{AppHandle, Emitter, Manager};

use analysis::AnalysisQueue;
use commands::{
    directories::{add_directory, add_directory_path, emit_dir_tree, remove_scanned_dir, toggle_dir_expanded},
    library::{get_sidebar_data, nav_all, nav_playlist, nav_select_dir, nav_tag, search_tracks},
    playlists::{
        add_selected_to_playlist, add_to_playlist,
        create_playlist, create_playlist_group,
        delete_playlist, delete_playlist_group,
        move_playlist_node,
        nav_group,
        remove_from_playlist, reorder_playlist_tracks,
        rename_playlist, rename_playlist_group,
    },
    settings::{get_settings, update_waveform_setting},
    tags::{create_tag, delete_tag, toggle_tag_for_selection, update_tag},
    tracks::{expand_track, remove_tag_from_expanded, save_notes, track_clicked},
    transport::{pause, play, play_track, seek, stop},
    waveform::get_waveform,
};
use settings::load_settings;
use state::AppState;

async fn open_db() -> anyhow::Result<Arc<Db>> {
    let db_path = PathBuf::from("singing-sand.db");
    let db = Db::open(&db_path).await.context("failed to open database")?;
    db.migrate().await.context("database migration failed")?;
    Ok(Arc::new(db))
}

pub fn run() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    let db = rt.block_on(open_db()).expect("failed to open database");
    let engine = Arc::new(AudioEngine::spawn().expect("failed to start audio engine"));

    let app_settings = rt.block_on(load_settings(&db)).unwrap_or_default();
    let render_settings = Arc::new(Mutex::new(app_settings.waveform.clone()));

    let (notes_tx, notes_rx) = flume::unbounded::<(i64, String)>();
    {
        let db = Arc::clone(&db);
        rt.spawn(async move {
            loop {
                let Ok((mut id, mut text)) = notes_rx.recv_async().await else { break };
                loop {
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(600),
                        notes_rx.recv_async(),
                    )
                    .await
                    {
                        Ok(Ok((new_id, new_text))) => { id = new_id; text = new_text; }
                        _ => break,
                    }
                }
                if let Err(e) = db.update_track_notes(id, &text).await {
                    tracing::warn!("update_track_notes failed: {e}");
                }
            }
        });
    }

    let (lib_event_tx, lib_event_rx) = tokio::sync::mpsc::unbounded_channel::<LibraryEvent>();
    let dirs = rt.block_on(db.list_scanned_dirs()).unwrap_or_default();

    let file_watcher = {
        let mut fw = FileWatcher::new(Arc::clone(&db), rt.handle().clone(), lib_event_tx)
            .expect("failed to create file watcher");
        for dir in &dirs {
            if let Err(e) = fw.watch(dir) {
                tracing::warn!("failed to watch {}: {e}", dir.display());
            }
        }
        Arc::new(Mutex::new(fw))
    };

    let rt_handle = rt.handle().clone();
    let rt_handle_art = rt.handle().clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let _rt_guard = rt_handle.enter();
            let app_handle = app.handle().clone();

            // Analysis queue needs AppHandle — created after setup so we have the handle.
            let analysis_queue = Arc::new(AnalysisQueue::spawn(
                Arc::clone(&db),
                app_handle.clone(),
                &rt_handle,
            ));

            // Enqueue tracks missing analysis.
            {
                let needs = rt_handle.block_on(db.list_tracks_needing_analysis()).unwrap_or_default();
                tracing::info!("{} tracks queued for background analysis", needs.len());
                analysis_queue.enqueue(needs);
            }

            // Library event handler (file watcher → frontend events).
            {
                let db2 = Arc::clone(&db);
                let aq = Arc::clone(&analysis_queue);
                let expanded = Arc::new(Mutex::new(HashMap::<String, bool>::new()));
                let app2 = app_handle.clone();
                let mut lib_event_rx = lib_event_rx;
                rt_handle.spawn(async move {
                    while let Some(event) = lib_event_rx.recv().await {
                        if let LibraryEvent::TrackAdded { id, path } = event {
                            aq.enqueue(std::iter::once((id, path)));
                        }
                        emit_dir_tree(&db2, &expanded, &app2).await;
                        app2.emit("library-changed", serde_json::json!({})).ok();
                    }
                });
            }

            // Audio event forwarder.
            events::spawn_audio_event_forwarder(Arc::clone(&engine), app_handle.clone());

            // Build and register AppState.
            let state = AppState {
                db: Arc::clone(&db),
                engine: Arc::clone(&engine),
                render_settings,
                current_bands: Arc::new(Mutex::new(vec![])),
                current_track_id: Arc::new(Mutex::new(None)),
                current_duration: Arc::new(Mutex::new(0.0)),
                selection: Arc::new(Mutex::new(std::collections::HashSet::new())),
                last_selected_id: Arc::new(Mutex::new(None)),
                current_track_ids: Arc::new(Mutex::new(vec![])),
                expanded_dirs: Arc::new(Mutex::new(HashMap::new())),
                file_watcher,
                analysis_queue,
                notes_tx,
                art_cache: Arc::new(Mutex::new(HashMap::new())),
            };
            app.manage(state);
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("art", move |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let url = request.uri().to_string();
            rt_handle_art.spawn(async move {
                let response = serve_art(&app, &url).await;
                responder.respond(response);
            });
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            update_waveform_setting,
            get_sidebar_data,
            nav_all,
            nav_select_dir,
            nav_playlist,
            nav_tag,
            search_tracks,
            add_directory,
            add_directory_path,
            toggle_dir_expanded,
            remove_scanned_dir,
            get_waveform,
            play_track,
            play,
            pause,
            stop,
            seek,
            expand_track,
            remove_tag_from_expanded,
            save_notes,
            track_clicked,
            create_tag,
            delete_tag,
            update_tag,
            toggle_tag_for_selection,
            create_playlist,
            delete_playlist,
            rename_playlist,
            create_playlist_group,
            delete_playlist_group,
            rename_playlist_group,
            move_playlist_node,
            nav_group,
            add_to_playlist,
            remove_from_playlist,
            reorder_playlist_tracks,
            add_selected_to_playlist,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn serve_art(app: &AppHandle, url: &str) -> tauri::http::Response<Vec<u8>> {
    // URL format: art://localhost/{track_id}
    let track_id: Option<i64> = url
        .trim_start_matches("art://localhost/")
        .trim_start_matches("art://localhost:")
        .split('/')
        .last()
        .and_then(|s| s.parse().ok());

    let Some(track_id) = track_id else {
        return tauri::http::Response::builder()
            .status(400)
            .body(vec![])
            .unwrap();
    };

    let state = app.state::<AppState>();

    // Check in-memory cache first.
    if let Some(cached) = state.art_cache.lock().unwrap().get(&track_id).cloned() {
        return jpeg_response(cached);
    }

    match state.db.get_album_art(track_id).await {
        Ok(Some(bytes)) => {
            state.art_cache.lock().unwrap().insert(track_id, bytes.clone());
            jpeg_response(bytes)
        }
        _ => tauri::http::Response::builder()
            .status(404)
            .body(vec![])
            .unwrap(),
    }
}

fn jpeg_response(body: Vec<u8>) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(200)
        .header("Content-Type", "image/jpeg")
        .header("Cache-Control", "max-age=86400")
        .body(body)
        .unwrap()
}
