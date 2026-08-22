use crate::domain::playlist::{self, Playlist};
use crate::domain::Track;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub fn get_playlists(state: State<AppState>) -> Result<Vec<Playlist>, String> {
    playlist::list_playlists(&state.db).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn create_playlist(state: State<AppState>, name: String) -> Result<Playlist, String> {
    playlist::create_playlist(&state.db, &name).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn rename_playlist(state: State<AppState>, id: i32, name: String) -> Result<(), String> {
    playlist::rename_playlist(&state.db, id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn delete_playlist(state: State<AppState>, id: i32) -> Result<(), String> {
    playlist::delete_playlist(&state.db, id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn get_playlist_tracks(state: State<AppState>, id: i32) -> Result<Vec<Track>, String> {
    playlist::playlist_tracks(&state.db, id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn add_track_to_playlist(
    state: State<AppState>,
    playlist_id: i32,
    track_id: i32,
) -> Result<(), String> {
    playlist::add_track(&state.db, playlist_id, track_id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn remove_track_from_playlist(
    state: State<AppState>,
    playlist_id: i32,
    track_id: i32,
) -> Result<(), String> {
    playlist::remove_track(&state.db, playlist_id, track_id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn reorder_playlist_tracks(
    state: State<AppState>,
    playlist_id: i32,
    track_ids: Vec<i32>,
) -> Result<(), String> {
    playlist::reorder_tracks(&state.db, playlist_id, &track_ids).map_err(|e| e.to_string())
}
