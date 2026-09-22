use crate::data::audio::pipeline::{PlaybackTick, EQ_BAND_COUNT};
use crate::domain::playback_settings::PlaybackSettings;
use crate::state::AppState;
use mpris_server::Property;
use tauri::ipc::Channel;
use tauri::{Manager, State};

#[tauri::command]
#[specta::specta]
pub fn play_track(state: State<AppState>, path: String) -> Result<(), String> {
    state.player.load_and_play(&path).map_err(|e| e.to_string())?;
    let tempo = state
        .db
        .track_tempo_by_path(&path)
        .map_err(|e| e.to_string())?
        .unwrap_or(1.0);
    state.player.apply_tempo_for_path(&path, tempo);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn toggle_play(state: State<AppState>) -> Result<(), String> {
    let status = state.player.status();
    if status.is_playing {
        state.player.pause().map_err(|e| e.to_string())
    } else {
        state.player.play().map_err(|e| e.to_string())
    }
}

#[tauri::command]
#[specta::specta]
pub fn pause_playback(state: State<AppState>) -> Result<(), String> {
    state.player.pause().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn seek(state: State<AppState>, position_secs: f64) -> Result<(), String> {
    state.player.seek(position_secs).map_err(|e| e.to_string())?;
    // A shell can't infer a jump from the position property alone - MPRIS has a
    // dedicated signal for exactly this case.
    state.mpris.seeked(position_secs);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_volume(state: State<AppState>, volume: f64) -> Result<(), String> {
    state.player.set_volume(volume).map_err(|e| e.to_string())?;
    state
        .mpris
        .notify(vec![Property::Volume(state.player.volume())]);
    Ok(())
}

/// Loads `path` paused at `position_secs`. This is session restore: the app
/// comes back with the track it was closed on sitting exactly where it was,
/// without starting to play by itself.
#[tauri::command]
#[specta::specta]
pub async fn restore_track(
    app: tauri::AppHandle,
    path: String,
    position_secs: f64,
) -> Result<(), String> {
    // Pre-rolling blocks until the pipeline is ready to answer a seek, and on
    // Linux/webkitgtk this command's IPC dispatch runs on the GTK main thread -
    // same class of freeze already fixed for cover art and library scans.
    tokio::task::spawn_blocking(move || {
        let state = app.state::<AppState>();
        state
            .player
            .load_paused_at(&path, position_secs)
            .map_err(|e| e.to_string())?;
        let tempo = state
            .db
            .track_tempo_by_path(&path)
            .map_err(|e| e.to_string())?
            .unwrap_or(1.0);
        state.player.apply_tempo_for_path(&path, tempo);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[specta::specta]
pub fn stop_playback(state: State<AppState>) -> Result<(), String> {
    state.player.stop().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn subscribe_playback_ticks(state: State<AppState>, channel: Channel<PlaybackTick>) {
    *state.tick_channel.lock().unwrap() = Some(channel);
}

#[tauri::command]
#[specta::specta]
pub fn set_next_track(state: State<AppState>, path: Option<String>) -> Result<(), String> {
    state.player.set_next_track(path.clone());
    if let Some(next) = path {
        // Pre-apply the next track's saved tempo to the standby deck right away,
        // so it's already correct by the time a gapless/crossfade switch lands -
        // otherwise the tempo would visibly/audibly jump mid-transition.
        let tempo = state
            .db
            .track_tempo_by_path(&next)
            .map_err(|e| e.to_string())?
            .unwrap_or(1.0);
        state.player.apply_tempo_for_path(&next, tempo);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_playback_settings(state: State<AppState>) -> PlaybackSettings {
    state.playback_settings_store.load_or_default()
}

#[tauri::command]
#[specta::specta]
pub fn set_crossfade_seconds(state: State<AppState>, secs: f64) {
    // In-memory only, cheap - safe to call on every slider drag event. The
    // frontend debounces the actual disk write via `save_crossfade_seconds`.
    state.player.set_crossfade_seconds(secs);
}

#[tauri::command]
#[specta::specta]
pub fn save_crossfade_seconds(state: State<AppState>, secs: f64) -> Result<(), String> {
    let mut settings = state.playback_settings_store.load_or_default();
    settings.crossfade_secs = secs;
    state
        .playback_settings_store
        .save(&settings)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn set_equalizer_bands(state: State<AppState>, gains: [f64; EQ_BAND_COUNT]) -> Result<(), String> {
    state.player.set_equalizer_bands(gains);
    let mut settings = state.playback_settings_store.load_or_default();
    settings.eq_gains = gains;
    state
        .playback_settings_store
        .save(&settings)
        .map_err(|e| e.to_string())
}

/// Applies `tempo` live to whichever deck currently holds `path` (the audible
/// part of a drag) without touching the DB - called on every slider event.
#[tauri::command]
#[specta::specta]
pub fn preview_track_tempo(state: State<AppState>, path: String, tempo: f64) {
    state.player.apply_tempo_for_path(&path, tempo);
}

/// Persists a track's tempo to the DB - the frontend debounces this so a drag
/// doesn't write on every event; `preview_track_tempo` already handled the
/// live audio side immediately.
#[tauri::command]
#[specta::specta]
pub fn set_track_tempo(state: State<AppState>, track_id: i32, tempo: f64) -> Result<(), String> {
    state
        .db
        .set_track_tempo(track_id, tempo)
        .map_err(|e| e.to_string())
}
