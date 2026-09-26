mod commands;
mod data;
mod domain;
mod state;
#[cfg(test)]
mod testing;

use data::audio::pipeline::{AudioPlayer, PlaybackError, PlaybackTick};
use data::db::Db;
use data::hotkeys_store::HotkeysStore;
use data::mpris::{self, MprisPlayer};
use data::playback_settings_store::PlaybackSettingsStore;
use data::session_store::{SessionState, SessionStore};
use data::theme_store::{ThemeSource, ThemeStore};
use mpris_server::Property;
use state::AppState;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use tauri_specta::{collect_commands, Builder};

const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// While a crossfade is ramping, the poll rate *is* the ramp's step rate, and a
/// short fade stepped five times a second is audibly a staircase rather than a
/// fade. Only in force while one is actually in flight.
const CROSSFADE_POLL_INTERVAL: Duration = Duration::from_millis(40);

/// How often the frontend hears about playback - deliberately unchanged by the
/// faster polling above, since every one of these wakes its subscribers.
const FRONTEND_TICK_INTERVAL: Duration = Duration::from_millis(200);

/// How often the polling thread flushes the session to disk. The frontend keeps
/// the backend's in-memory copy current continuously and writes it out exactly
/// on window close, so this only bounds what an unclean exit can lose.
const SESSION_FLUSH_INTERVAL: Duration = Duration::from_secs(60);

/// The one-shot fields of every tick since the last one the frontend was sent.
///
/// Polling faster than we report means a pulse - a track ending, a gapless
/// hand-off, an error - can land on a tick that is never forwarded. Collecting
/// them here means the next reported tick still carries it.
#[derive(Default)]
struct PendingPulses {
    ended: bool,
    auto_advanced_to: Option<String>,
    error: Option<PlaybackError>,
}

impl PendingPulses {
    fn absorb(&mut self, tick: &PlaybackTick) {
        self.ended |= tick.ended;
        if tick.auto_advanced_to.is_some() {
            self.auto_advanced_to = tick.auto_advanced_to.clone();
        }
        if tick.error.is_some() {
            self.error = tick.error.clone();
        }
    }

    /// Moves everything collected onto the tick about to be reported, leaving
    /// the accumulator empty.
    fn apply_to(&mut self, tick: &mut PlaybackTick) {
        tick.ended = self.ended;
        tick.auto_advanced_to = self.auto_advanced_to.take();
        tick.error = self.error.take();
        self.ended = false;
    }
}

/// `muzon ctl …`: talks to the running instance's control socket and exits.
pub fn control_cli(args: &[String]) -> i32 {
    data::control::cli(args)
}

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
        commands::library::get_track_palette,
        commands::library::update_track_tags,
        commands::library::toggle_favorite,
        commands::library::record_play,
        commands::lyrics::get_lyrics,
        commands::lyrics::get_lyrics_source_text,
        commands::lyrics::save_lyrics,
        commands::lyrics::fetch_online_lyrics,
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
        commands::player::save_crossfade_seconds,
        commands::player::set_equalizer_bands,
        commands::player::preview_track_tempo,
        commands::player::set_track_tempo,
        commands::player::restore_track,
        commands::session::get_session,
        commands::session::set_session_queue,
        commands::session::set_session_progress,
        commands::session::save_session,
        commands::window::get_window_controls_visible,
        commands::theme::get_theme,
        commands::theme::set_theme,
        commands::sync::get_s3_config,
        commands::sync::set_s3_config,
        commands::sync::set_s3_credentials,
        commands::sync::forget_s3_credentials,
        commands::sync::has_s3_credentials,
        commands::sync::check_s3_connection,
        commands::sync::sync_now,
        commands::theme::get_theme_source,
        commands::theme::set_theme_source,
        commands::theme::system_theme_available,
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
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .register_asynchronous_uri_scheme_protocol(
            data::cover_protocol::SCHEME,
            data::cover_protocol::handle,
        )
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
            let theme_source = theme_store.load_source();
            // Following the shell's palette: if it can't be read (no shell, file
            // gone) fall back to the saved colours rather than refusing to start
            let theme = match theme_source {
                ThemeSource::System => data::system_theme::load().unwrap_or_else(|e| {
                    eprintln!("[muzon theme] палитра системы недоступна: {e}");
                    theme_store.load_or_default()
                }),
                ThemeSource::Manual => theme_store.load_or_default(),
            };
            let hotkeys_store = HotkeysStore::new(&app_config_dir);
            let hotkeys = hotkeys_store.load_or_default();
            let playback_settings_store = PlaybackSettingsStore::new(&app_config_dir);
            let playback_settings = playback_settings_store.load_or_default();
            player.set_crossfade_seconds(playback_settings.crossfade_secs);
            player.set_equalizer_bands(playback_settings.eq_gains);

            let s3_config_store = data::s3_config_store::S3ConfigStore::new(&app_config_dir);
            let s3_config = s3_config_store.load_or_default();

            let session = SessionState::new(SessionStore::new(&app_config_dir));
            // The frontend restores the rest of the session for itself, but the
            // volume has to be in place before anything can be loaded onto a
            // deck, or the first restored track starts at full blast.
            player.set_volume(session.snapshot().volume)?;

            app.manage(AppState {
                db,
                player,
                tick_channel: Mutex::new(None),
                theme_store,
                theme: Mutex::new(theme),
                theme_source: Mutex::new(theme_source),
                cover_cache: Mutex::new(HashMap::new()),
                hotkeys_store,
                hotkeys: Mutex::new(hotkeys),
                playback_settings_store,
                s3_config_store,
                s3_config: Mutex::new(s3_config),
                session,
                mpris: Default::default(),
                control: Default::default(),
            });

            // Queue, favourites and lyrics for desktop shells (see data/control.rs)
            data::control::serve(app.handle().clone());

            // Recolouring the desktop recolours the window: watch the shell's
            // palette while the user is following it
            let theme_handle = app.handle().clone();
            let stop_handle = app.handle().clone();
            data::system_theme::watch(
                move |theme| {
                    let state = theme_handle.state::<AppState>();
                    if *state.theme_source.lock().unwrap() != ThemeSource::System {
                        return;
                    }
                    *state.theme.lock().unwrap() = theme.clone();
                    let _ = theme_handle.emit("theme-changed", theme);
                },
                move || {
                    let state = stop_handle.state::<AppState>();
                    // Only stops when the user leaves the system palette
                    let following = *state.theme_source.lock().unwrap() == ThemeSource::System;
                    !following
                },
            );

            let mpris_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match mpris_server::Server::new("muzon", MprisPlayer::new(mpris_handle.clone())).await
                {
                    Ok(server) => {
                        // Handing the server to the bridge is also what keeps it
                        // (and its D-Bus connection) alive for the app's
                        // lifetime; message dispatch runs on the tokio runtime
                        // independently from here.
                        mpris_handle
                            .state::<AppState>()
                            .mpris
                            .attach(Arc::new(server));
                    }
                    Err(e) => eprintln!("Failed to start MPRIS server: {e}"),
                }
            });

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                // What the desktop shell has last been told. Ticks almost always
                // say the same thing as the one before, so we announce only the
                // transitions.
                let mut announced: Option<(bool, Option<String>, bool)> = None;
                let mut pending = PendingPulses::default();
                let mut last_frontend_tick = Instant::now();
                let mut last_session_flush = Instant::now();
                let mut interval = POLL_INTERVAL;

                loop {
                    std::thread::sleep(interval);
                    let state = handle.state::<AppState>();
                    let mut tick = state.player.tick();
                    pending.absorb(&tick);
                    interval = if state.player.is_crossfading() {
                        CROSSFADE_POLL_INTERVAL
                    } else {
                        POLL_INTERVAL
                    };

                    if last_frontend_tick.elapsed() < FRONTEND_TICK_INTERVAL {
                        continue;
                    }
                    last_frontend_tick = Instant::now();
                    pending.apply_to(&mut tick);

                    let mut channel_guard = state.tick_channel.lock().unwrap();
                    if let Some(channel) = channel_guard.as_ref() {
                        if channel.send(tick.clone()).is_err() {
                            *channel_guard = None;
                        }
                    }
                    drop(channel_guard);

                    // Duration is part of the signature because a track's length
                    // isn't queryable for the first tick or two after it starts:
                    // announcing metadata only on the track change would leave
                    // the shell showing a zero-length song forever.
                    let current = (
                        tick.is_playing,
                        tick.path.clone(),
                        tick.duration_secs > 0.0,
                    );
                    if announced.as_ref() != Some(&current) {
                        let playing_changed = announced.as_ref().map(|(p, _, _)| *p) != Some(current.0);
                        let metadata_changed = announced
                            .as_ref()
                            .map(|(_, path, had_duration)| {
                                path != &current.1 || *had_duration != current.2
                            })
                            .unwrap_or(true);
                        let mut properties = vec![Property::PlaybackStatus(
                            mpris::playback_status_of(&state.player.status()),
                        )];
                        if metadata_changed {
                            properties.push(Property::Metadata(mpris::build_metadata(&handle)));
                            state.control.publish("track", data::control::status(&handle));
                        }
                        if playing_changed {
                            state
                                .control
                                .publish("playback", serde_json::json!({ "isPlaying": current.0 }));
                        }
                        state.mpris.notify(properties);
                        announced = Some(current);
                    }

                    if last_session_flush.elapsed() >= SESSION_FLUSH_INTERVAL {
                        last_session_flush = Instant::now();
                        if let Err(e) = state.session.flush() {
                            eprintln!("[muzon] failed to save session: {e}");
                        }
                    }
                }
            });


            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
