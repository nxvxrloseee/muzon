use crate::data::{lrc_source, lrclib, tag_writer};
use crate::domain::lyrics::{self, Lyrics};
use crate::state::AppState;
use tauri::Manager;

#[tauri::command]
#[specta::specta]
pub fn get_lyrics(path: String) -> Option<Lyrics> {
    lrc_source::load_for_track(std::path::Path::new(&path))
}

/// Raw LRC text for the in-app editor to prefill (same tag-then-file source
/// priority as `get_lyrics`, but returns the unparsed text so edits round-trip
/// exactly, and is an empty string rather than null when nothing exists yet).
#[tauri::command]
#[specta::specta]
pub fn get_lyrics_source_text(path: String) -> String {
    lrc_source::load_raw_text_for_editing(std::path::Path::new(&path))
}

#[tauri::command]
#[specta::specta]
pub fn save_lyrics(path: String, content: String, store_in_tag: bool) -> Result<(), String> {
    let track_path = std::path::Path::new(&path);
    if store_in_tag {
        tag_writer::write_lyrics_to_tag(track_path, &content).map_err(|e| e.to_string())
    } else {
        lrc_source::save_sidecar_file(track_path, &content).map_err(|e| e.to_string())
    }
}

/// Looks the track's lyrics up on LRCLIB and returns the LRC text, or `None`
/// when nothing convincing was found.
///
/// Deliberately an explicit action rather than something that happens on every
/// track change: this is the only outbound request the app makes, and a local
/// music player reaching for the network unprompted is a surprise. Saving is
/// left to the caller too, so the existing tag-or-sidecar choice still applies.
#[tauri::command]
#[specta::specta]
pub async fn fetch_online_lyrics(
    app: tauri::AppHandle,
    path: String,
) -> Result<Option<String>, String> {
    let query = {
        let state = app.state::<AppState>();
        let track = state
            .db
            .track_by_path(&path)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "трек не найден в библиотеке".to_string())?;
        lrclib::Query {
            title: track.title,
            artist: track.artist,
            album: track.album,
            duration_secs: track.duration_secs,
        }
    };
    let duration_secs = query.duration_secs;

    let candidates = lrclib::find(&query).await.map_err(|e| e.to_string())?;
    let usable: Vec<lrclib::Candidate> = candidates
        .into_iter()
        .filter(|candidate| candidate.text().is_some())
        .collect();
    let matches: Vec<lyrics::Match> = usable
        .iter()
        .map(|candidate| lyrics::Match {
            has_synced: candidate.has_synced(),
            duration_secs: candidate.duration,
        })
        .collect();

    Ok(lyrics::pick_best_match(&matches, duration_secs)
        .and_then(|index| usable.get(index))
        .and_then(|candidate| candidate.text())
        .map(str::to_string))
}
