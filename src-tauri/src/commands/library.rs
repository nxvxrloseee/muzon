use crate::data::covers;
use crate::domain::{library, palette, Track, TrackPalette};
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

/// Off the UI thread for the same reason as the scan and the cover decode: on
/// Linux/webkitgtk a synchronous command's IPC dispatch runs on the GTK main
/// thread, and serialising an entire library there freezes the window for as
/// long as it takes.
#[tauri::command]
#[specta::specta]
pub async fn get_tracks(app: tauri::AppHandle) -> Result<Vec<Track>, String> {
    tokio::task::spawn_blocking(move || {
        let state = app.state::<AppState>();
        library::list_tracks(&state.db).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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

/// Colors for the Now Playing gradient, taken from the same cached thumbnail
/// the cover art is served from - so this costs a small decode at most once per
/// track, off the UI thread, instead of decoding and quantizing in the webview.
#[tauri::command]
#[specta::specta]
pub async fn get_track_palette(
    app: tauri::AppHandle,
    path: String,
) -> Result<Option<TrackPalette>, ()> {
    let Ok(cache_dir) = app.path().app_cache_dir() else {
        return Ok(None);
    };
    let cache_dir = cache_dir.join("covers");

    Ok(tokio::task::spawn_blocking(move || {
        let (_, bytes) = covers::read_cover_thumbnail(&cache_dir, std::path::Path::new(&path))?;
        palette::extract(&bytes)
    })
    .await
    .unwrap_or(None))
}

#[tauri::command]
#[specta::specta]
pub fn toggle_favorite(state: State<AppState>, track_id: i32) -> Result<bool, String> {
    library::toggle_favorite(&state.db, track_id).map_err(|e| e.to_string())
}

/// Counts one listen. A single indexed UPDATE, so it stays synchronous - the
/// reason the two commands above went async is the work they do, not the fact
/// that they touch the database.
#[tauri::command]
#[specta::specta]
pub fn record_play(state: State<AppState>, track_id: i32) -> Result<(), String> {
    state.db.record_play(track_id).map_err(|e| e.to_string())
}

/// Rewrites the file's tags, which is blocking file I/O - and so has no
/// business running on the GTK main thread either.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn update_track_tags(
    app: tauri::AppHandle,
    path: String,
    title: String,
    artist: Option<String>,
    album: Option<String>,
    track_no: Option<i32>,
    cover_path: Option<String>,
) -> Result<Track, String> {
    tokio::task::spawn_blocking(move || {
        let state = app.state::<AppState>();
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
    })
    .await
    .map_err(|e| e.to_string())?
}
