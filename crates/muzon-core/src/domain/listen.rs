use crate::domain::Track;
use serde::{Deserialize, Serialize};
/// One listen, as both services need it. Kept in the offline queue, so it
/// has to survive a round trip through JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Listen {
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub duration_secs: Option<f64>,
    pub track_no: Option<i32>,
    /// Unix seconds of when the listen *started*, which is what both
    /// services ask for.
    pub listened_at: i64,
}

impl Listen {
    /// `None` for a track with no artist: neither service accepts one, and
    /// queueing it would only block the queue behind a permanent error.
    pub fn from_track(track: &Track, listened_at: i64) -> Option<Self> {
        let artist = track.artist.as_deref()?.trim();
        if artist.is_empty()
            || artist.len() > 4096
            || track.title.trim().is_empty()
            || track.title.len() > 4096
        {
            return None;
        }
        Some(Self {
            artist: artist.to_string(),
            track: track.title.trim().to_string(),
            album: track
                .album
                .clone()
                .filter(|a| !a.trim().is_empty() && a.len() <= 4096),
            duration_secs: track.duration_secs.filter(|d| d.is_finite() && *d > 0.0),
            track_no: track.track_no.filter(|n| *n > 0),
            listened_at,
        })
    }
}
