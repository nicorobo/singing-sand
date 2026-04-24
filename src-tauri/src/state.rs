use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use ss_audio::AudioEngine;
use ss_db::Db;
use ss_library::FileWatcher;
use ss_waveform::{WaveformBucket, WaveformRenderSettings};

use crate::analysis::AnalysisQueue;

pub struct AppState {
    pub db: Arc<Db>,
    pub engine: Arc<AudioEngine>,
    pub render_settings: Arc<Mutex<WaveformRenderSettings>>,
    pub current_bands: Arc<Mutex<Vec<WaveformBucket>>>,
    pub current_track_id: Arc<Mutex<Option<i64>>>,
    pub current_duration: Arc<Mutex<f64>>,
    pub selection: Arc<Mutex<HashSet<i64>>>,
    pub last_selected_id: Arc<Mutex<Option<i64>>>,
    pub current_track_ids: Arc<Mutex<Vec<i64>>>,
    pub expanded_dirs: Arc<Mutex<HashMap<String, bool>>>,
    pub file_watcher: Arc<Mutex<FileWatcher>>,
    pub analysis_queue: Arc<AnalysisQueue>,
    pub notes_tx: flume::Sender<(i64, String)>,
    pub art_cache: Arc<Mutex<HashMap<i64, Vec<u8>>>>,
}
