use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A minimal track record. Extended with DB fields in later phases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: i64,
    pub path: PathBuf,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_secs: Option<f64>,
}
