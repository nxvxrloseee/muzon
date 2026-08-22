mod commands;
mod data;
mod domain;
mod state;

use data::audio::pipeline::AudioPlayer;
use data::db::Db;
use data::hotkeys_store::HotkeysStore;
use data::mpris::MprisPlayer;
use data::playback_settings_store::PlaybackSettingsStore;
use data::theme_store::ThemeStore;
use state::AppState;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tauri::Manager;
use tauri_specta::{collect_commands, Builder};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let specta_builder = Builder::<tauri::Wry>::new()
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        // We never produce NaN/Infinity for our f64 fields (position/duration/time
        // are always real numbers), so assert lossless floats to get plain `number`
        // instead of the conservative `number | null` specta defaults to.
        .semantic_types(
            specta_typescript::semantic::Configuration::default().enable_lossless_floats(),
        )
        .commands(collect_commands![
        commands::library::add_music_folder,
        commands::library::get_tracks,
        commands::library::get_track_cover,
        commands::library::update_track_tags,
        commands::library::toggle_favorite,
        commands::lyrics::get_lyrics,
        commands::lyrics::get_lyrics_source_text,
        commands::lyrics::save_lyrics,
        commands::player::play_track,
        commands::player::toggle_play,
        commands::player::pause_playback,
        commands::player::seek,
        commands::player::set_volume,
        commands::player::stop_playback,
        commands::player::subscribe_playback_ticks,
        commands::player::set_next_track,
        commands::player::get_playback_settings,
        commands::player::set_crossfade_seconds,
        commands::player::set_equalizer_bands,
        commands::player::set_tempo,
        commands::theme::get_theme,
        commands::theme::set_theme,
        commands::theme::get_default_theme,
        commands::theme::export_theme,
        commands::theme::import_theme,
        commands::hotkeys::get_hotkeys,
        commands::hotkeys::set_hotkeys,
        commands::playlists::get_playlists,
        commands::playlists::create_playlist,
        commands::playlists::rename_playlist,
        commands::playlists::delete_playlist,
        commands::playlists::get_playlist_tracks,
        commands::playlists::add_track_to_playlist,
        commands::playlists::remove_track_from_playlist,
        commands::playlists::reorder_playlist_tracks,
    ]);

    #[cfg(debug_assertions)]
    specta_builder
        .export(specta_typescript::Typescript::default(), "../src/bindings.ts")
        .expect("Failed to export typescript bindings");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(specta_builder.invoke_handler())
        .setup(move |app| {
            specta_builder.mount_events(app);

            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            let db = Db::open(&app_data_dir.join("muzon.sqlite3"))?;
            let player = AudioPlayer::new()?;

            let app_config_dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&app_config_dir)?;
            let theme_store = ThemeStore::new(&app_config_dir);
            let theme = theme_store.load_or_default();
            let hotkeys_store = HotkeysStore::new(&app_config_dir);
            let hotkeys = hotkeys_store.load_or_default();
            let playback_settings_store = PlaybackSettingsStore::new(&app_config_dir);
            let playback_settings = playback_settings_store.load_or_default();
            player.set_crossfade_seconds(playback_settings.crossfade_secs);
            player.set_equalizer_bands(playback_settings.eq_gains);
            player.set_tempo(playback_settings.tempo);

            app.manage(AppState {
                db,
                player,
                tick_channel: Mutex::new(None),
                theme_store,
                theme: Mutex::new(theme),
                cover_cache: Mutex::new(HashMap::new()),
                hotkeys_store,
                hotkeys: Mutex::new(hotkeys),
                playback_settings_store,
            });

            let mpris_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match mpris_server::Server::new("muzon", MprisPlayer::new(mpris_handle)).await {
                    Ok(server) => {
                        // The connection's message dispatch runs independently on the
                        // tokio runtime once created; this just keeps `server` (and
                        // thus the connection) alive for the app's lifetime.
                        std::future::pending::<()>().await;
                        drop(server);
                    }
                    Err(e) => eprintln!("Failed to start MPRIS server: {e}"),
                }
            });

            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_millis(200));
                let state = handle.state::<AppState>();
                let tick = state.player.tick();
                let mut channel_guard = state.tick_channel.lock().unwrap();
                if let Some(channel) = channel_guard.as_ref() {
                    if channel.send(tick).is_err() {
                        *channel_guard = None;
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
