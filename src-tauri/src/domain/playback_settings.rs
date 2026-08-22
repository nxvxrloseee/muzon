use crate::data::audio::pipeline::EQ_BAND_COUNT;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackSettings {
    /// 0 = gapless (no fade), > 0 = crossfade duration in seconds.
    pub crossfade_secs: f64,
    /// 10-band equalizer gains in dB, roughly -24..12 each.
    pub eq_gains: [f64; EQ_BAND_COUNT],
    /// Playback speed multiplier without pitch shift; 1.0 = normal.
    pub tempo: f64,
}

impl PlaybackSettings {
    pub fn default_settings() -> Self {
        Self {
            crossfade_secs: 0.0,
            eq_gains: [0.0; EQ_BAND_COUNT],
            tempo: 1.0,
        }
    }
}
