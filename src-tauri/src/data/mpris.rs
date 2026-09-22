use crate::data::audio::pipeline::{PlaybackStatus as PipelineStatus, MAX_TEMPO, MIN_TEMPO};
use crate::data::covers;
use crate::domain::session::RepeatMode;
use crate::state::AppState;
use gstreamer as gst;
use mpris_server::zbus::fdo;
use mpris_server::{
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, Property, RootInterface,
    Server, Signal, Time, TrackId, Volume,
};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Emitter, Manager};

/// Bridges the OS media-key/MPRIS surface (headphone remote buttons, GNOME/KDE
/// media widgets) to playback. Track advancement (Next/Previous) is delegated back
/// to the frontend via events since queue order/shuffle/repeat live in the queue
/// store there, not in Rust.
pub struct MprisPlayer {
    app: AppHandle,
}

impl MprisPlayer {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }
}

/// Holds the live MPRIS server once its D-Bus connection is up, so the rest of
/// the app can push `PropertiesChanged` and `Seeked` at it.
///
/// Without this the server answered property *reads* correctly but announced
/// nothing, and desktop shells read MPRIS properties once and then follow the
/// signals - so the shell's widget froze on whatever happened to be playing the
/// first time it looked, and never learned about a track change, a pause, or a
/// seek again.
#[derive(Default)]
pub struct MprisBridge {
    server: OnceLock<Arc<Server<MprisPlayer>>>,
}

impl MprisBridge {
    pub fn attach(&self, server: Arc<Server<MprisPlayer>>) {
        let _ = self.server.set(server);
    }

    /// Fire-and-forget: emitting is async, while every caller here is either a
    /// synchronous Tauri command or the playback polling thread.
    pub fn notify(&self, properties: Vec<Property>) {
        let Some(server) = self.server.get().cloned() else {
            return;
        };
        tauri::async_runtime::spawn(async move {
            if let Err(e) = server.properties_changed(properties).await {
                eprintln!("[muzon mpris] failed to emit PropertiesChanged: {e}");
            }
        });
    }

    /// A jump the shell could not have predicted from the playing state alone.
    pub fn seeked(&self, position_secs: f64) {
        let Some(server) = self.server.get().cloned() else {
            return;
        };
        let position = Time::from_micros((position_secs * 1_000_000.0) as i64);
        tauri::async_runtime::spawn(async move {
            if let Err(e) = server.emit(Signal::Seeked { position }).await {
                eprintln!("[muzon mpris] failed to emit Seeked: {e}");
            }
        });
    }
}

pub fn loop_status_of(repeat: RepeatMode) -> LoopStatus {
    match repeat {
        RepeatMode::Off => LoopStatus::None,
        RepeatMode::All => LoopStatus::Playlist,
        RepeatMode::One => LoopStatus::Track,
    }
}

fn repeat_mode_of(loop_status: LoopStatus) -> RepeatMode {
    match loop_status {
        LoopStatus::None => RepeatMode::Off,
        LoopStatus::Playlist => RepeatMode::All,
        LoopStatus::Track => RepeatMode::One,
    }
}

/// Nothing loaded reads as Stopped rather than Paused: a shell offering a
/// resume button for a player with no track is just wrong.
pub fn playback_status_of(status: &PipelineStatus) -> PlaybackStatus {
    if status.is_playing {
        PlaybackStatus::Playing
    } else if status.path.is_some() {
        PlaybackStatus::Paused
    } else {
        PlaybackStatus::Stopped
    }
}

/// Metadata for the shell's media widget. A free function because both the
/// D-Bus property getter and the polling thread that announces changes need it.
pub fn build_metadata(app: &AppHandle) -> Metadata {
    let state = app.state::<AppState>();
    let status = state.player.status();
    let mut builder = Metadata::builder();

    let Some(path) = status.path.as_deref() else {
        return builder.trackid(TrackId::NO_TRACK).build();
    };

    match state.db.track_by_path(path).ok().flatten() {
        Some(track) => {
            // A stable per-track object path: shells use `mpris:trackid` to tell
            // "same track, moved position" apart from "different track".
            builder = builder
                .trackid(
                    TrackId::try_from(format!("/com/nxvxrloseee/muzon/track/{}", track.id))
                        .unwrap_or(TrackId::NO_TRACK),
                )
                .title(track.title);
            if let Some(artist) = track.artist {
                builder = builder.artist([artist]);
            }
            if let Some(album) = track.album {
                builder = builder.album(album);
            }
        }
        None => builder = builder.trackid(TrackId::NO_TRACK),
    }

    if let Ok(uri) = gst::glib::filename_to_uri(path, None) {
        builder = builder.url(uri.to_string());
    }
    if let Some(art) = art_url(app, path) {
        builder = builder.art_url(art);
    }
    if status.duration_secs > 0.0 {
        builder = builder.length(Time::from_micros(
            (status.duration_secs * 1_000_000.0) as i64,
        ));
    }

    builder.build()
}

/// `file://` URI of the cached cover thumbnail - the same one the in-app list
/// rows are served, so the shell gets artwork without decoding the cover again.
fn art_url(app: &AppHandle, track_path: &str) -> Option<String> {
    let cache_dir = app.path().app_cache_dir().ok()?.join("covers");
    let thumbnail = covers::ensure_thumbnail_path(&cache_dir, Path::new(track_path))?;
    Some(gst::glib::filename_to_uri(&thumbnail, None).ok()?.to_string())
}

impl RootInterface for MprisPlayer {
    async fn raise(&self) -> fdo::Result<()> {
        if let Some(window) = self.app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
        Ok(())
    }

    async fn quit(&self) -> fdo::Result<()> {
        // `close`, not `destroy`: this has to go through the same close request
        // the app intercepts to flush settings and write the session out.
        if let Some(window) = self.app.get_webview_window("main") {
            let _ = window.close();
        }
        Ok(())
    }

    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn set_fullscreen(&self, _fullscreen: bool) -> mpris_server::zbus::Result<()> {
        Ok(())
    }

    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn identity(&self) -> fdo::Result<String> {
        Ok("Muzon".into())
    }

    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok("muzon".into())
    }

    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        Ok(vec!["file".into()])
    }

    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok(vec![])
    }
}

impl PlayerInterface for MprisPlayer {
    async fn next(&self) -> fdo::Result<()> {
        let _ = self.app.emit("mpris-next", ());
        Ok(())
    }

    async fn previous(&self) -> fdo::Result<()> {
        let _ = self.app.emit("mpris-previous", ());
        Ok(())
    }

    async fn pause(&self) -> fdo::Result<()> {
        let _ = self.state().player.pause();
        Ok(())
    }

    async fn play_pause(&self) -> fdo::Result<()> {
        let state = self.state();
        if state.player.status().is_playing {
            let _ = state.player.pause();
        } else {
            let _ = state.player.play();
        }
        Ok(())
    }

    async fn stop(&self) -> fdo::Result<()> {
        let _ = self.state().player.stop();
        Ok(())
    }

    async fn play(&self) -> fdo::Result<()> {
        let _ = self.state().player.play();
        Ok(())
    }

    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        let state = self.state();
        let current = state.player.status().position_secs;
        let new_pos = (current + offset.as_micros() as f64 / 1_000_000.0).max(0.0);
        let _ = state.player.seek(new_pos);
        state.mpris.seeked(new_pos);
        Ok(())
    }

    async fn set_position(&self, _track_id: TrackId, position: Time) -> fdo::Result<()> {
        let secs = (position.as_micros() as f64 / 1_000_000.0).max(0.0);
        let state = self.state();
        let _ = state.player.seek(secs);
        state.mpris.seeked(secs);
        Ok(())
    }

    async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        Ok(())
    }

    async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
        Ok(playback_status_of(&self.state().player.status()))
    }

    // Repeat and shuffle live in the frontend's queue store, but the session
    // the frontend keeps the backend supplied with already mirrors both - so
    // reads are answered from there rather than by a round trip to the webview.
    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(loop_status_of(self.state().session.snapshot().repeat))
    }

    async fn set_loop_status(&self, loop_status: LoopStatus) -> mpris_server::zbus::Result<()> {
        // Delegated the same way Next/Previous are: only the queue store can
        // actually change what plays next, and it reports back through the
        // session, which is what re-announces the property.
        //
        // The session's copy is updated here too, though, and not left to that
        // round trip: zbus re-reads the property to build its own
        // PropertiesChanged the moment this returns, so without it the shell is
        // handed the *old* value and only corrected a moment later.
        let repeat = repeat_mode_of(loop_status);
        self.state().session.update(|session| session.repeat = repeat);
        let _ = self.app.emit("mpris-set-repeat", repeat);
        Ok(())
    }

    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(self.state().player.current_tempo())
    }

    async fn set_rate(&self, rate: PlaybackRate) -> mpris_server::zbus::Result<()> {
        // Live only, exactly like dragging the in-app speed slider before it
        // settles - the per-track tempo the library stores is not something a
        // media widget should be rewriting.
        let state = self.state();
        if let Some(path) = state.player.status().path {
            state.player.apply_tempo_for_path(&path, rate);
        }
        Ok(())
    }

    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(self.state().session.snapshot().shuffle)
    }

    async fn set_shuffle(&self, shuffle: bool) -> mpris_server::zbus::Result<()> {
        // Optimistic for the same reason as `set_loop_status`.
        self.state().session.update(|session| session.shuffle = shuffle);
        let _ = self.app.emit("mpris-set-shuffle", shuffle);
        Ok(())
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        let app = self.app.clone();
        // Building this can decode a cover the first time a track is seen; the
        // zbus dispatch task must not block on that.
        Ok(tokio::task::spawn_blocking(move || build_metadata(&app))
            .await
            .unwrap_or_else(|_| Metadata::builder().trackid(TrackId::NO_TRACK).build()))
    }

    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(self.state().player.volume())
    }

    async fn set_volume(&self, volume: Volume) -> mpris_server::zbus::Result<()> {
        // No explicit notify here: zbus emits PropertiesChanged for a property
        // set through D-Bus by itself, and announcing it again just sends the
        // shell the same value twice. The app's own volume command has no such
        // help and does notify.
        let _ = self.state().player.set_volume(volume);
        Ok(())
    }

    async fn position(&self) -> fdo::Result<Time> {
        let secs = self.state().player.status().position_secs;
        Ok(Time::from_micros((secs * 1_000_000.0) as i64))
    }

    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(MIN_TEMPO)
    }

    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(MAX_TEMPO)
    }

    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }
}
