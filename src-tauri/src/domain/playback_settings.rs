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
    /// Absent from files written before it existed; without the default those
    /// would fail to parse and take the EQ down with them.
    #[serde(default)]
    pub replay_gain: ReplayGainSettings,
}

impl PlaybackSettings {
    pub fn default_settings() -> Self {
        Self {
            crossfade_secs: 0.0,
            eq_gains: [0.0; EQ_BAND_COUNT],
            replay_gain: ReplayGainSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ReplayGainMode {
    /// Off by default: turning it on makes most music several dB quieter,
    /// which should be the listener's choice rather than a surprise.
    #[default]
    Off,
    /// Every track at the same loudness.
    Track,
    /// Albums at the same loudness, keeping the dynamics between their tracks.
    Album,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReplayGainSettings {
    pub mode: ReplayGainMode,
    /// Added on top of the tagged gain, in dB. ReplayGain's reference level is
    /// fairly quiet, so this is the knob for bringing it back up.
    pub preamp_db: f64,
}

pub const MIN_PREAMP_DB: f64 = -6.0;
pub const MAX_PREAMP_DB: f64 = 12.0;

impl ReplayGainSettings {
    pub fn normalized(self) -> Self {
        Self {
            preamp_db: if self.preamp_db.is_finite() {
                self.preamp_db.clamp(MIN_PREAMP_DB, MAX_PREAMP_DB)
            } else {
                0.0
            },
            ..self
        }
    }
}

/// What a stream's tags say about its loudness. Every field is optional:
/// files tagged by different tools carry any subset of them.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StreamGains {
    pub track_gain: Option<f64>,
    pub track_peak: Option<f64>,
    pub album_gain: Option<f64>,
    pub album_peak: Option<f64>,
}

/// The linear volume factor to play a stream at.
///
/// Album mode falls back to the track values and vice versa, so a file tagged
/// only one way still gets levelled. Untagged streams play as they are: there
/// is nothing to go on, and guessing would be wrong in both directions. The
/// peak, where known, caps the gain so levelling a quiet track up never clips.
pub fn gain_factor(settings: ReplayGainSettings, gains: &StreamGains) -> f64 {
    let (gain, peak) = match settings.mode {
        ReplayGainMode::Off => return 1.0,
        ReplayGainMode::Track => (
            gains.track_gain.or(gains.album_gain),
            gains.track_peak.or(gains.album_peak),
        ),
        ReplayGainMode::Album => (
            gains.album_gain.or(gains.track_gain),
            gains.album_peak.or(gains.track_peak),
        ),
    };
    let Some(gain) = gain else {
        return 1.0;
    };
    let factor = 10f64.powf((gain + settings.preamp_db) / 20.0);
    match peak {
        Some(peak) if peak > 0.0 => factor.min(1.0 / peak),
        _ => factor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(mode: ReplayGainMode, preamp_db: f64) -> ReplayGainSettings {
        ReplayGainSettings { mode, preamp_db }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    const TAGGED: StreamGains = StreamGains {
        track_gain: Some(-6.0),
        track_peak: Some(0.5),
        album_gain: Some(-3.0),
        album_peak: Some(0.9),
    };

    #[test]
    fn off_leaves_the_stream_alone() {
        assert_eq!(
            gain_factor(settings(ReplayGainMode::Off, 6.0), &TAGGED),
            1.0
        );
    }

    #[test]
    fn track_and_album_modes_use_their_own_gain() {
        // -6 dB is half the amplitude, -3 dB about 0.708
        assert!(close(
            gain_factor(settings(ReplayGainMode::Track, 0.0), &TAGGED),
            0.501
        ));
        assert!(close(
            gain_factor(settings(ReplayGainMode::Album, 0.0), &TAGGED),
            0.708
        ));
    }

    #[test]
    fn preamp_adds_to_the_tagged_gain() {
        assert!(close(
            gain_factor(settings(ReplayGainMode::Track, 6.0), &TAGGED),
            1.0
        ));
    }

    #[test]
    fn each_mode_falls_back_to_the_other_tag() {
        let only_track = StreamGains {
            track_gain: Some(-6.0),
            ..StreamGains::default()
        };
        assert!(close(
            gain_factor(settings(ReplayGainMode::Album, 0.0), &only_track),
            0.501
        ));
    }

    #[test]
    fn untagged_streams_play_unchanged() {
        let none = StreamGains::default();
        assert_eq!(
            gain_factor(settings(ReplayGainMode::Track, 6.0), &none),
            1.0
        );
    }

    #[test]
    fn the_peak_caps_a_boost_so_it_cannot_clip() {
        let quiet = StreamGains {
            track_gain: Some(12.0),
            track_peak: Some(0.8),
            ..StreamGains::default()
        };
        // +12 dB would be ~3.98x; a 0.8 peak only has room for 1.25x
        assert!(close(
            gain_factor(settings(ReplayGainMode::Track, 0.0), &quiet),
            1.25
        ));
    }

    #[test]
    fn a_settings_file_from_before_replay_gain_still_loads() {
        let json = r#"{"crossfadeSecs": 2.0, "eqGains": [1,2,3,4,5,6,7,8,9,10]}"#;
        let parsed: PlaybackSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.eq_gains[9], 10.0);
        assert_eq!(parsed.replay_gain, ReplayGainSettings::default());
    }
}
