use crate::data::audio::pipeline::{PlaybackTick, EQ_BAND_COUNT};
use crate::domain::playback_settings::PlaybackSettings;
use crate::state::AppState;
use tauri::ipc::Channel;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub fn play_track(state: State<AppState>, path: String) -> Result<(), String> {
    state.player.load_and_play(&path).map_err(|e| e.to_string())
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
    state.player.seek(position_secs).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn set_volume(state: State<AppState>, volume: f64) -> Result<(), String> {
    state.player.set_volume(volume).map_err(|e| e.to_string())
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
pub fn set_next_track(state: State<AppState>, path: Option<String>) {
    state.player.set_next_track(path);
}

#[tauri::command]
#[specta::specta]
pub fn get_playback_settings(state: State<AppState>) -> PlaybackSettings {
    state.playback_settings_store.load_or_default()
}

#[tauri::command]
#[specta::specta]
pub fn set_crossfade_seconds(state: State<AppState>, secs: f64) -> Result<(), String> {
    state.player.set_crossfade_seconds(secs);
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

#[tauri::command]
#[specta::specta]
pub fn set_tempo(state: State<AppState>, tempo: f64) -> Result<(), String> {
    state.player.set_tempo(tempo);
    let mut settings = state.playback_settings_store.load_or_default();
    settings.tempo = tempo;
    state
        .playback_settings_store
        .save(&settings)
        .map_err(|e| e.to_string())
}
