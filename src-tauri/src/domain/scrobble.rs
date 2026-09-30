//! What a listen is, and how ListenBrainz and Last.fm want to hear about it.
//! No I/O here; `data::scrobble` does the sending.

use crate::domain::Track;
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use specta::Type;

pub const CLIENT_NAME: &str = "Muzon";

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
        if artist.is_empty() || track.title.trim().is_empty() {
            return None;
        }
        Some(Self {
            artist: artist.to_string(),
            track: track.title.trim().to_string(),
            album: track.album.clone().filter(|a| !a.trim().is_empty()),
            duration_secs: track.duration_secs.filter(|d| *d > 0.0),
            track_no: track.track_no.filter(|n| *n > 0),
            listened_at,
        })
    }
}

/// ListenBrainz's submit-listens body. `now_playing` omits the timestamp, as
/// the API requires; more than one listen is an `import`.
pub fn listenbrainz_payload(listens: &[Listen], now_playing: bool) -> Value {
    let listen_type = if now_playing {
        "playing_now"
    } else if listens.len() == 1 {
        "single"
    } else {
        "import"
    };
    let payload: Vec<Value> = listens
        .iter()
        .map(|l| {
            let mut info = json!({
                "media_player": CLIENT_NAME,
                "submission_client": CLIENT_NAME,
                "submission_client_version": env!("CARGO_PKG_VERSION"),
            });
            if let Some(d) = l.duration_secs {
                info["duration_ms"] = json!((d * 1000.0).round() as i64);
            }
            if let Some(n) = l.track_no {
                info["tracknumber"] = json!(n);
            }
            let mut metadata = json!({
                "artist_name": l.artist,
                "track_name": l.track,
                "additional_info": info,
            });
            if let Some(album) = &l.album {
                metadata["release_name"] = json!(album);
            }
            let mut entry = json!({ "track_metadata": metadata });
            if !now_playing {
                entry["listened_at"] = json!(l.listened_at);
            }
            entry
        })
        .collect();
    json!({ "listen_type": listen_type, "payload": payload })
}

/// Last.fm's `api_sig`: every parameter except `format` and `callback`,
/// sorted by name, concatenated as name+value, the shared secret appended,
/// MD5 in lowercase hex.
pub fn lastfm_signature(params: &[(String, String)], secret: &str) -> String {
    let mut sorted: Vec<&(String, String)> = params
        .iter()
        .filter(|(k, _)| k != "format" && k != "callback")
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Md5::new();
    for (k, v) in sorted {
        hasher.update(k.as_bytes());
        hasher.update(v.as_bytes());
    }
    hasher.update(secret.as_bytes());
    hex::encode(hasher.finalize())
}

/// Last.fm takes at most this many scrobbles per request.
pub const LASTFM_BATCH: usize = 50;

/// The per-listen parameters of `track.scrobble` (indexed, for a batch) or
/// `track.updateNowPlaying` (unindexed, `index: None`).
pub fn lastfm_track_params(listen: &Listen, index: Option<usize>) -> Vec<(String, String)> {
    let key = |name: &str| match index {
        Some(i) => format!("{name}[{i}]"),
        None => name.to_string(),
    };
    let mut params = vec![
        (key("artist"), listen.artist.clone()),
        (key("track"), listen.track.clone()),
    ];
    if index.is_some() {
        params.push((key("timestamp"), listen.listened_at.to_string()));
    }
    if let Some(album) = &listen.album {
        params.push((key("album"), album.clone()));
    }
    if let Some(d) = listen.duration_secs {
        params.push((key("duration"), (d.round() as i64).to_string()));
    }
    if let Some(n) = listen.track_no {
        params.push((key("trackNumber"), n.to_string()));
    }
    params
}

/// What the settings page shows about one service.
#[derive(Debug, Clone, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    /// The account listens go to, or `None` when not connected.
    pub user: Option<String>,
    /// Listens waiting to be sent - kept across restarts.
    pub pending: u32,
    /// The last failure, cleared by the next success.
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScrobbleStatus {
    pub listenbrainz: ServiceStatus,
    pub lastfm: ServiceStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Listen {
        Listen {
            artist: "Brian Eno".into(),
            track: "Ascent".into(),
            album: Some("Apollo".into()),
            duration_secs: Some(254.6),
            track_no: Some(1),
            listened_at: 1_700_000_000,
        }
    }

    #[test]
    fn signature_matches_an_independently_computed_md5() {
        // Reference computed with Python's hashlib over the same parameters
        let params: Vec<(String, String)> = [
            ("method", "track.scrobble"),
            ("api_key", "KEY"),
            ("sk", "SESSION"),
            ("artist[0]", "Brian Eno"),
            ("track[0]", "Ascent"),
            ("timestamp[0]", "1700000000"),
            ("format", "json"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(
            lastfm_signature(&params, "SECRET"),
            "5e87526604c4faad31dd6b5a4157adc7"
        );
    }

    #[test]
    fn listenbrainz_single_listen_carries_its_timestamp() {
        let body = listenbrainz_payload(&[sample()], false);
        assert_eq!(body["listen_type"], "single");
        assert_eq!(body["payload"][0]["listened_at"], 1_700_000_000);
        let meta = &body["payload"][0]["track_metadata"];
        assert_eq!(meta["artist_name"], "Brian Eno");
        assert_eq!(meta["release_name"], "Apollo");
        assert_eq!(meta["additional_info"]["duration_ms"], 254_600);
    }

    #[test]
    fn listenbrainz_now_playing_has_no_timestamp_and_a_batch_is_an_import() {
        let now = listenbrainz_payload(&[sample()], true);
        assert_eq!(now["listen_type"], "playing_now");
        assert!(now["payload"][0].get("listened_at").is_none());

        let batch = listenbrainz_payload(&[sample(), sample()], false);
        assert_eq!(batch["listen_type"], "import");
    }

    #[test]
    fn lastfm_batch_params_are_indexed_and_now_playing_ones_are_not() {
        let batch = lastfm_track_params(&sample(), Some(3));
        assert!(batch.contains(&("timestamp[3]".into(), "1700000000".into())));
        assert!(batch.contains(&("duration[3]".into(), "255".into())));

        let now = lastfm_track_params(&sample(), None);
        assert!(now.contains(&("artist".into(), "Brian Eno".into())));
        assert!(!now.iter().any(|(k, _)| k.starts_with("timestamp")));
    }

    #[test]
    fn a_track_without_an_artist_is_not_a_listen() {
        let track = Track {
            id: 1,
            path: "/a.mp3".into(),
            title: "Untitled".into(),
            artist: None,
            album: None,
            duration_secs: Some(100.0),
            track_no: None,
            genre: None,
            year: None,
            is_favorite: false,
            tempo: 1.0,
            play_count: 0,
            last_played_at: None,
            added_at: 0.0,
        };
        assert!(Listen::from_track(&track, 0).is_none());
    }
}
