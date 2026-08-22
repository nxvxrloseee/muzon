use crate::data::covers;
use crate::domain::{library, Track};
use crate::state::AppState;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
#[specta::specta]
pub async fn add_music_folder(app: tauri::AppHandle) -> Result<library::ScanReport, String> {
    // The blocking dialog API (`blocking_pick_folder`) crashes this app's GTK backend
    // when called from a tauri command thread against our undecorated/transparent
    // window; the async callback API avoids that reentrant-dialog codepath.
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    let folder = rx.await.map_err(|e| e.to_string())?;

    let Some(path) = folder else {
        return Ok(library::ScanReport::default());
    };
    let path_buf = path.into_path().map_err(|e| e.to_string())?;

    // Walking the folder tree, reading tags off every file, and writing them to
    // the DB is blocking work; on Linux/webkitgtk this command's IPC dispatch
    // runs on the GTK main thread, so this would otherwise freeze the whole UI
    // for the duration of the scan (same class of bug fixed for cover art).
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        library::add_root_and_scan(&state.db, &path_buf).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[specta::specta]
pub fn get_tracks(state: State<AppState>) -> Result<Vec<Track>, String> {
    library::list_tracks(&state.db).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_track_cover(state: State<'_, AppState>, path: String) -> Result<Option<String>, ()> {
    if let Some(cached) = state.cover_cache.lock().unwrap().get(&path) {
        return Ok(cached.clone());
    }
    // Custom URI-scheme IPC on Linux/webkitgtk runs sync commands on the GTK
    // main thread; decoding/resizing cover art there would freeze the whole UI.
    let cover_path = path.clone();
    let result = tokio::task::spawn_blocking(move || {
        covers::read_cover_data_url(std::path::Path::new(&cover_path))
    })
    .await
    .unwrap_or(None);
    state
        .cover_cache
        .lock()
        .unwrap()
        .insert(path, result.clone());
    Ok(result)
}

#[tauri::command]
#[specta::specta]
pub fn toggle_favorite(state: State<AppState>, track_id: i32) -> Result<bool, String> {
    library::toggle_favorite(&state.db, track_id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub fn update_track_tags(
    state: State<AppState>,
    path: String,
    title: String,
    artist: Option<String>,
    album: Option<String>,
    track_no: Option<i32>,
    cover_path: Option<String>,
) -> Result<Track, String> {
    state.cover_cache.lock().unwrap().remove(&path);
    library::update_tags(
        &state.db,
        std::path::Path::new(&path),
        library::TrackEditInput {
            title,
            artist,
            album,
            track_no,
            cover_path,
        },
    )
    .map_err(|e| e.to_string())
}
