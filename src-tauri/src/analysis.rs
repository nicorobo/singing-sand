use std::{path::PathBuf, sync::{atomic::{AtomicUsize, Ordering}, Arc}};

use ss_audio::analyze_track;
use ss_db::Db;
use tauri::{AppHandle, Emitter};

pub struct AnalysisQueue {
    tx: tokio::sync::mpsc::UnboundedSender<(i64, PathBuf)>,
    pending: Arc<AtomicUsize>,
}

impl AnalysisQueue {
    pub fn spawn(db: Arc<Db>, app: AppHandle, rt: &tokio::runtime::Handle) -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(i64, PathBuf)>();
        let pending = Arc::new(AtomicUsize::new(0));
        let pending_clone = Arc::clone(&pending);

        rt.spawn(async move {
            let parallelism = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            let sem = Arc::new(tokio::sync::Semaphore::new(parallelism));

            while let Some((track_id, path)) = rx.recv().await {
                let permit = Arc::clone(&sem).acquire_owned().await.unwrap();
                let db2 = Arc::clone(&db);
                let app2 = app.clone();
                let pending2 = Arc::clone(&pending_clone);

                tokio::spawn(async move {
                    let _permit = permit;
                    match tokio::task::spawn_blocking(move || analyze_track(&path, 1000)).await {
                        Ok(Ok(analysis)) => {
                            let arrays: Vec<[f32; 3]> =
                                analysis.waveform.iter().map(|b| b.to_array()).collect();
                            if let Err(e) = db2.save_waveform_bands(track_id, &arrays).await {
                                tracing::warn!(track_id, error = %e, "analysis: save waveform failed");
                            } else {
                                app2.emit("waveform-ready", serde_json::json!({ "track_id": track_id })).ok();
                            }
                        }
                        Ok(Err(e)) => tracing::warn!(track_id, error = %e, "analysis failed"),
                        Err(e)    => tracing::warn!(track_id, error = %e, "analysis task panicked"),
                    }
                    let remaining = pending2.fetch_sub(1, Ordering::Relaxed).saturating_sub(1);
                    app2.emit("analysis-progress", serde_json::json!({ "pending_count": remaining })).ok();
                });
            }
        });

        Self { tx, pending }
    }

    pub fn enqueue(&self, tracks: impl IntoIterator<Item = (i64, PathBuf)>) {
        for item in tracks {
            let count = self.pending.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = self.tx.send(item);
            // Emit updated count via a fire-and-forget approach; actual count emitted per-task completion too
            tracing::debug!("analysis queue: {count} pending");
        }
    }

    pub fn pending_count(&self) -> usize {
        self.pending.load(Ordering::Relaxed)
    }
}
