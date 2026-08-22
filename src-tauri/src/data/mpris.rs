use crate::state::AppState;
use mpris_server::zbus::fdo;
use mpris_server::{
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, RootInterface, Time,
    TrackId, Volume,
};
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

    fn build_metadata(&self) -> Metadata {
        let state = self.state();
        let status = state.player.status();
        let mut builder = Metadata::builder().trackid(TrackId::NO_TRACK);

        if let Some(path) = &status.path {
            if let Ok(tracks) = state.db.list_tracks() {
                if let Some(track) = tracks.iter().find(|t| &t.path == path) {
                    builder = builder.title(track.title.clone());
                    if let Some(artist) = &track.artist {
                        builder = builder.artist([artist.clone()]);
                    }
                    if let Some(album) = &track.album {
                        builder = builder.album(album.clone());
                    }
                }
            }
            if status.duration_secs > 0.0 {
                builder = builder.length(Time::from_micros(
                    (status.duration_secs * 1_000_000.0) as i64,
                ));
            }
        }

        builder.build()
    }
}

impl RootInterface for MprisPlayer {
    async fn raise(&self) -> fdo::Result<()> {
        Ok(())
    }

    async fn quit(&self) -> fdo::Result<()> {
        Ok(())
    }

    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(false)
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
        Ok(false)
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
        Ok(())
    }

    async fn set_position(&self, _track_id: TrackId, position: Time) -> fdo::Result<()> {
        let secs = (position.as_micros() as f64 / 1_000_000.0).max(0.0);
        let _ = self.state().player.seek(secs);
        Ok(())
    }

    async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        Ok(())
    }

    async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
        Ok(if self.state().player.status().is_playing {
            PlaybackStatus::Playing
        } else {
            PlaybackStatus::Paused
        })
    }

    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(LoopStatus::None)
    }

    async fn set_loop_status(&self, _loop_status: LoopStatus) -> mpris_server::zbus::Result<()> {
        Ok(())
    }

    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn set_rate(&self, _rate: PlaybackRate) -> mpris_server::zbus::Result<()> {
        Ok(())
    }

    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn set_shuffle(&self, _shuffle: bool) -> mpris_server::zbus::Result<()> {
        Ok(())
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        Ok(self.build_metadata())
    }

    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(self.state().player.volume())
    }

    async fn set_volume(&self, volume: Volume) -> mpris_server::zbus::Result<()> {
        let _ = self.state().player.set_volume(volume);
        Ok(())
    }

    async fn position(&self) -> fdo::Result<Time> {
        let secs = self.state().player.status().position_secs;
        Ok(Time::from_micros((secs * 1_000_000.0) as i64))
    }

    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
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
