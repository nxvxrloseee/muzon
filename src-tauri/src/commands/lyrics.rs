use crate::data::{lrc_source, tag_writer};
use crate::domain::lyrics::Lyrics;

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
