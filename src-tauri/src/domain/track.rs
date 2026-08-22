use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Track {
    pub id: i32,
    pub path: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_secs: Option<f64>,
    pub track_no: Option<i32>,
    pub is_favorite: bool,
    /// Playback speed multiplier without pitch shift; 1.0 = normal.
    pub tempo: f64,
}
