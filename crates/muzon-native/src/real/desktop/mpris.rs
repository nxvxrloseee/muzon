use super::{window_request, WindowRequest};
use mpris_server::{
    zbus::{self, fdo},
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, Property, RootInterface,
    Server, Signal, Time, TrackId, Volume,
};
use muzon_core::{
    controller::{ControllerClient, Snapshot},
    data::{
        audio::pipeline::{MAX_TEMPO, MIN_TEMPO},
        covers,
    },
};
use muzon_queue::RepeatMode;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
        Arc,
    },
    time::Duration,
};

struct Player {
    client: ControllerClient,
    covers: PathBuf,
    window: SyncSender<WindowRequest>,
    stop: Arc<AtomicBool>,
}
fn track_id(s: &Snapshot) -> TrackId {
    if s.current_path.is_none() {
        return TrackId::NO_TRACK;
    }
    s.current_track()
        .map(|t| {
            TrackId::try_from(format!(
                "/com/nxvxrloseee/muzon/track/t{}/entry/e{}",
                t.id,
                s.cursor.unwrap_or(0)
            ))
            .unwrap_or(TrackId::NO_TRACK)
        })
        .unwrap_or(TrackId::NO_TRACK)
}
fn playback(s: &Snapshot) -> PlaybackStatus {
    if s.is_playing {
        PlaybackStatus::Playing
    } else if s.current_path.is_some() {
        PlaybackStatus::Paused
    } else {
        PlaybackStatus::Stopped
    }
}
fn loop_status(repeat: RepeatMode) -> LoopStatus {
    match repeat {
        RepeatMode::Off => LoopStatus::None,
        RepeatMode::All => LoopStatus::Playlist,
        RepeatMode::One => LoopStatus::Track,
    }
}
fn micros(secs: f64) -> Time {
    Time::from_micros((secs.max(0.0) * 1_000_000.0) as i64)
}
fn metadata(s: &Snapshot, cache: &Path) -> Metadata {
    let Some(track) = s.current_track().filter(|_| s.current_path.is_some()) else {
        return Metadata::builder().trackid(TrackId::NO_TRACK).build();
    };
    let mut builder = Metadata::builder()
        .trackid(track_id(s))
        .title(track.title.clone())
        .length(micros(s.display_duration()));
    if let Some(artist) = &track.artist {
        builder = builder.artist([artist.clone()]);
    }
    if let Some(album) = &track.album {
        builder = builder.album(album.clone());
    }
    if let Some(genre) = &track.genre {
        builder = builder.genre([genre.clone()]);
    }
    if let Ok(uri) = url::Url::from_file_path(&track.path) {
        builder = builder.url(uri.to_string());
    }
    if let Some(path) = covers::ensure_thumbnail_path(cache, Path::new(&track.path)) {
        if let Ok(uri) = url::Url::from_file_path(path) {
            builder = builder.art_url(uri.to_string());
        }
    }
    builder.build()
}
async fn read_metadata(
    s: Arc<Snapshot>,
    cache: PathBuf,
    stop: &AtomicBool,
) -> anyhow::Result<Metadata> {
    let rx = std::sync::Mutex::new(
        super::read_job(move || metadata(&s, &cache)).map_err(anyhow::Error::msg)?,
    );
    super::bounded(stop, Duration::from_secs(2), async move {
        loop {
            let received = rx.lock().unwrap().try_recv();
            match received {
                Ok(value) => return Ok(value),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    tokio::time::sleep(Duration::from_millis(10)).await
                }
                Err(error) => return Err(anyhow::anyhow!(error)),
            }
        }
    })
    .await?
}

fn can_next(s: &Snapshot) -> bool {
    s.cursor
        .and_then(|i| s.order.iter().position(|&q| q == i))
        .is_some_and(|p| p + 1 < s.order.len() || s.repeat != RepeatMode::Off)
}
impl Player {
    async fn apply(&self, f: impl FnOnce(&ControllerClient) + Send + 'static) -> fdo::Result<()> {
        if self.stop.load(Ordering::Acquire) {
            return Err(fdo::Error::Failed("desktop integration stopped".into()));
        }
        let client = self.client.clone();
        tokio::task::spawn_blocking(move || {
            f(&client);
            client.barrier()
        })
        .await
        .map_err(|e| fdo::Error::Failed(e.to_string()))?
        .map(|_| ())
        .map_err(fdo::Error::Failed)
    }
}
impl RootInterface for Player {
    async fn raise(&self) -> fdo::Result<()> {
        window_request(&self.window, WindowRequest::Raise)
    }
    async fn quit(&self) -> fdo::Result<()> {
        window_request(&self.window, WindowRequest::Quit)
    }
    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn set_fullscreen(&self, _: bool) -> zbus::Result<()> {
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
        Ok("muzon-native".into())
    }
    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        Ok(vec!["file".into()])
    }
    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok(vec![])
    }
}
impl PlayerInterface for Player {
    async fn next(&self) -> fdo::Result<()> {
        self.apply(|c| c.next()).await
    }
    async fn previous(&self) -> fdo::Result<()> {
        self.apply(|c| c.previous()).await
    }
    async fn pause(&self) -> fdo::Result<()> {
        self.apply(|c| c.pause()).await
    }
    async fn play_pause(&self) -> fdo::Result<()> {
        self.apply(|c| c.toggle_play()).await
    }
    async fn stop(&self) -> fdo::Result<()> {
        self.apply(|c| c.stop()).await
    }
    async fn play(&self) -> fdo::Result<()> {
        self.apply(|c| c.play()).await
    }
    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        let s = self.client.snapshot();
        let pos = (s.position_secs + offset.as_micros() as f64 / 1_000_000.0).max(0.0);
        if let Some(path) = s.current_path.clone() {
            if s.display_duration() > 0.0 && pos > s.display_duration() {
                return self.next().await;
            }
            self.apply(move |c| c.seek_for(path, s.cursor, pos)).await?;
        }
        Ok(())
    }
    async fn set_position(&self, id: TrackId, position: Time) -> fdo::Result<()> {
        let s = self.client.snapshot();
        let pos = position.as_micros() as f64 / 1_000_000.0;
        if id != track_id(&s)
            || pos < 0.0
            || (s.display_duration() > 0.0 && pos > s.display_duration())
        {
            return Ok(());
        }
        if let Some(path) = s.current_path.clone() {
            self.apply(move |c| c.seek_for(path, s.cursor, pos)).await?;
        }
        Ok(())
    }
    async fn open_uri(&self, uri: String) -> fdo::Result<()> {
        let path = url::Url::parse(&uri)
            .ok()
            .and_then(|u| u.to_file_path().ok())
            .ok_or_else(|| {
                fdo::Error::NotSupported("only library file URIs are supported".into())
            })?;
        let s = self.client.snapshot();
        let id = s
            .library
            .iter()
            .find(|t| Path::new(&t.path) == path)
            .map(|t| t.id)
            .ok_or_else(|| {
                fdo::Error::NotSupported("file is not in this profile's library".into())
            })?;
        self.apply(move |c| c.set_queue(vec![id], id)).await
    }
    async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
        Ok(playback(&self.client.snapshot()))
    }
    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(loop_status(self.client.snapshot().repeat))
    }
    async fn set_loop_status(&self, status: LoopStatus) -> zbus::Result<()> {
        let repeat = match status {
            LoopStatus::None => RepeatMode::Off,
            LoopStatus::Track => RepeatMode::One,
            LoopStatus::Playlist => RepeatMode::All,
        };
        self.apply(move |c| c.set_repeat(repeat))
            .await
            .map_err(Into::into)
    }
    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(self.client.snapshot().tempo)
    }
    async fn set_rate(&self, rate: PlaybackRate) -> zbus::Result<()> {
        if !rate.is_finite() || !(MIN_TEMPO..=MAX_TEMPO).contains(&rate) {
            return Err(zbus::Error::Failure("rate out of range".into()));
        }
        self.apply(move |c| c.set_tempo(rate))
            .await
            .map_err(Into::into)
    }
    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(self.client.snapshot().shuffle)
    }
    async fn set_shuffle(&self, on: bool) -> zbus::Result<()> {
        self.apply(move |c| c.set_shuffle(on))
            .await
            .map_err(Into::into)
    }
    async fn metadata(&self) -> fdo::Result<Metadata> {
        let s = self.client.snapshot();
        let cache = self.covers.clone();
        read_metadata(s, cache, &self.stop)
            .await
            .map_err(|e| fdo::Error::Failed(e.to_string()))
    }
    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(self.client.snapshot().volume)
    }
    async fn set_volume(&self, volume: Volume) -> zbus::Result<()> {
        if !volume.is_finite() {
            return Err(zbus::Error::Failure("volume must be finite".into()));
        }
        self.apply(move |c| c.set_volume(volume.max(0.0)))
            .await
            .map_err(Into::into)
    }
    async fn position(&self) -> fdo::Result<Time> {
        Ok(micros(self.client.snapshot().position_secs))
    }
    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(MIN_TEMPO)
    }
    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(MAX_TEMPO)
    }
    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(can_next(&self.client.snapshot()))
    }
    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(!self.client.snapshot().queue.is_empty())
    }
    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(!self.client.snapshot().queue.is_empty())
    }
    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(self.client.snapshot().current_path.is_some())
    }
    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(self.client.snapshot().current_path.is_some())
    }
    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }
}
pub async fn serve(
    client: ControllerClient,
    covers: PathBuf,
    name: String,
    window: SyncSender<WindowRequest>,
    stop: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let player = Player {
        client: client.clone(),
        covers,
        window,
        stop: stop.clone(),
    };
    let server =
        super::bounded(&stop, Duration::from_secs(3), Server::new(&name, player)).await??;
    eprintln!("[muzon mpris] {}", server.bus_name());
    let mut prev = client.snapshot();
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if stop.load(Ordering::Acquire) {
            break;
        }
        let s = client.snapshot();
        let mut properties = Vec::new();
        if playback(&s) != playback(&prev) {
            properties.push(Property::PlaybackStatus(playback(&s)));
        }
        if s.volume != prev.volume {
            properties.push(Property::Volume(s.volume));
        }
        if s.shuffle != prev.shuffle {
            properties.push(Property::Shuffle(s.shuffle));
        }
        if s.repeat != prev.repeat {
            properties.push(Property::LoopStatus(loop_status(s.repeat)));
        }
        if s.tempo != prev.tempo {
            properties.push(Property::Rate(s.tempo));
        }
        if s.current_path != prev.current_path
            || s.cursor != prev.cursor
            || !Arc::ptr_eq(&s.queue, &prev.queue)
            || s.display_duration() != prev.display_duration()
        {
            let snapshot = s.clone();
            let cache = server.imp().covers.clone();
            let metadata = read_metadata(snapshot, cache, &stop).await?;
            properties.push(Property::Metadata(metadata));
            properties.push(Property::CanPlay(!s.queue.is_empty()));
            properties.push(Property::CanPause(s.current_path.is_some()));
            properties.push(Property::CanSeek(s.current_path.is_some()));
            properties.push(Property::CanGoNext(can_next(&s)));
            properties.push(Property::CanGoPrevious(!s.queue.is_empty()));
        }
        if can_next(&s) != can_next(&prev) {
            properties.push(Property::CanGoNext(can_next(&s)));
        }
        if !properties.is_empty() {
            super::bounded(
                &stop,
                Duration::from_secs(2),
                server.properties_changed(properties),
            )
            .await??;
        }
        if s.seek_revision != prev.seek_revision {
            super::bounded(
                &stop,
                Duration::from_secs(2),
                server.emit(Signal::Seeked {
                    position: micros(
                        if s.current_path != prev.current_path || s.cursor != prev.cursor {
                            s.position_secs
                        } else {
                            s.seek_position_secs
                        },
                    ),
                }),
            )
            .await??;
        }
        prev = s;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mpris_setters_wait_for_state_and_stale_track_ids_cannot_seek() {
        let _gst = muzon_core::testing::gst_lock();
        let (dir, core) = super::super::tests::fixture();
        let (tx, rx) = std::sync::mpsc::sync_channel(4);
        let player = Player {
            client: core.client(),
            covers: dir.join("covers"),
            window: tx,
            stop: Arc::new(AtomicBool::new(false)),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            assert_eq!(
                player.playback_status().await.unwrap(),
                PlaybackStatus::Paused
            );
            let old = track_id(&core.snapshot());
            player.set_shuffle(true).await.unwrap();
            assert!(player.shuffle().await.unwrap());
            player.set_loop_status(LoopStatus::Playlist).await.unwrap();
            assert_eq!(player.loop_status().await.unwrap(), LoopStatus::Playlist);
            player.next().await.unwrap();
            player.pause().await.unwrap();
            let before = core.snapshot().seek_revision;
            player.set_position(old, micros(7.0)).await.unwrap();
            assert_eq!(core.snapshot().seek_revision, before);
            player
                .set_position(track_id(&core.snapshot()), micros(2.0))
                .await
                .unwrap();
            assert_eq!(core.snapshot().seek_revision, before + 1);
            player
                .set_position(track_id(&core.snapshot()), micros(200.0))
                .await
                .unwrap();
            assert_eq!(core.snapshot().seek_revision, before + 1);
            player.set_volume(0.3).await.unwrap();
            assert_eq!(player.volume().await.unwrap(), 0.3);
            assert!(player.set_volume(f64::NAN).await.is_err());
            let metadata = player.metadata().await.unwrap();
            assert!(metadata.trackid().is_some());
            assert!(player
                .open_uri("https://example.invalid/music.mp3".into())
                .await
                .is_err());
            player.raise().await.unwrap();
            assert!(matches!(rx.try_recv().unwrap(), WindowRequest::Raise));
            player.quit().await.unwrap();
            assert!(matches!(rx.try_recv().unwrap(), WindowRequest::Quit));
        });
    }
}
