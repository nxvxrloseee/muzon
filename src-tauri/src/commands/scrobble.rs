use crate::domain::scrobble::ScrobbleStatus;
use crate::state::AppState;
use tauri::{Manager, State};

#[tauri::command]
#[specta::specta]
pub fn get_scrobble_status(state: State<AppState>) -> ScrobbleStatus {
    state.scrobbler.status()
}

/// Checks the token with ListenBrainz before keeping it.
#[tauri::command]
#[specta::specta]
pub async fn connect_listenbrainz(
    app: tauri::AppHandle,
    token: String,
) -> Result<ScrobbleStatus, String> {
    let state = app.state::<AppState>();
    state
        .scrobbler
        .connect_listenbrainz(&token)
        .await
        .map_err(|e| e.to_string())?;
    state.scrobbler.wake();
    Ok(state.scrobbler.status())
}

#[tauri::command]
#[specta::specta]
pub fn disconnect_listenbrainz(state: State<AppState>) -> ScrobbleStatus {
    state.scrobbler.disconnect_listenbrainz();
    state.scrobbler.status()
}

/// First half of connecting Last.fm: returns the page on last.fm where the
/// user approves access. `lastfm_finish_auth` completes it.
#[tauri::command]
#[specta::specta]
pub async fn lastfm_begin_auth(
    app: tauri::AppHandle,
    api_key: String,
    secret: String,
) -> Result<String, String> {
    app.state::<AppState>()
        .scrobbler
        .lastfm_begin_auth(&api_key, &secret)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn lastfm_finish_auth(app: tauri::AppHandle) -> Result<ScrobbleStatus, String> {
    let state = app.state::<AppState>();
    state
        .scrobbler
        .lastfm_finish_auth()
        .await
        .map_err(|e| e.to_string())?;
    state.scrobbler.wake();
    Ok(state.scrobbler.status())
}

#[tauri::command]
#[specta::specta]
pub fn disconnect_lastfm(state: State<AppState>) -> ScrobbleStatus {
    state.scrobbler.disconnect_lastfm();
    state.scrobbler.status()
}
