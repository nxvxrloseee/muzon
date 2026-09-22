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
    /// How many times this has been listened to far enough to count. See
    /// `lib/playThreshold` on the frontend for what "far enough" means.
    pub play_count: i32,
    /// Unix seconds, or null if never played. `f64` rather than `i64` because
    /// specta refuses to export 64-bit integers (they lose precision in JS) -
    /// and a second count is exact in a double for the next several million
    /// years, so nothing is given up by saying so.
    pub last_played_at: Option<f64>,
    /// Unix seconds of when the library first saw the file. Backfilled from the
    /// file's mtime for rows that predate the column.
    pub added_at: f64,
}
