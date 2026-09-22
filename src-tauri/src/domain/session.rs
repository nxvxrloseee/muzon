use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    Off,
    All,
    One,
}

/// What the app puts back after a restart: the queue you were listening to, the
/// track you were on and how far into it, plus the switches that shape playback.
/// Restored paused - an app that starts blaring the moment it launches is not
/// what anyone means by "remember where I was".
///
/// Tracks are stored as paths rather than row ids so a re-scan that renumbers
/// the library (or a database rebuilt from scratch) still restores the same
/// files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Session {
    /// The queue in library order, exactly as the queue store holds it.
    pub queue_paths: Vec<String>,
    /// The shuffle permutation, kept so relaunching mid-shuffle continues the
    /// order you were part-way through instead of drawing a new one.
    pub shuffle_order: Vec<u32>,
    /// Index into `queue_paths`, or -1 when nothing was playing.
    pub cursor: i32,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub position_secs: f64,
    pub volume: f64,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            queue_paths: Vec::new(),
            shuffle_order: Vec::new(),
            cursor: -1,
            shuffle: false,
            repeat: RepeatMode::Off,
            position_secs: 0.0,
            volume: 1.0,
        }
    }
}
