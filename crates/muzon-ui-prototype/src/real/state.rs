//! The native UI keeps presentation state; the core is the sole playback owner.
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::{
    audio_settings::{AudioUi, PRESETS},
    desktop::{Desktop, WindowRequest},
    message::Message,
};
use iced::widget::{image, scrollable};
use iced::{Subscription, Task};
use muzon_core::controller::{Controller, CoreConfig, Media, PlaylistEdit, Snapshot};
use muzon_core::data::{
    db::Db, playback_settings_store::PlaybackSettingsStore, session_store::SessionStore,
};

pub const LIBRARY_LIST: &str = "muzon-native/library";
pub const QUEUE_LIST: &str = "muzon-native/queue";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Library,
    Albums,
    Artists,
    Genres,
    Favorites,
    Smart,
    Playlists,
    Audio,
    Sleep,
    Shortcuts,
    Appearance,
    Cloud,
    Scrobble,
    NowPlaying,
    Queue,
}

#[derive(Debug, Clone, Copy)]
pub enum CollectionOverlay {
    AddTrack(i32),
    DeletePlaylist(i32),
}

pub struct QueueDrag {
    pub from: usize,
    pub to: usize,
    pub baseline: Arc<Vec<usize>>,
}
pub struct RealApp {
    controller: Option<Controller>,
    desktop: Option<Desktop>,
    tray: Option<super::tray::Tray>,
    pub(super) hidden: bool,
    // An explicit profile has one writer, even if two native windows are launched.
    _profile_lock: File,
    pub(super) window: crate::real::window_state::WindowMemory,
    pub snapshot: Arc<Snapshot>,
    pub media: Arc<Media>,
    pub cover: Option<image::Handle>,
    pub screen: Screen,
    pub audio: AudioUi,
    pub hotkeys: super::hotkeys::Hotkeys,
    pub appearance: crate::real::appearance::Appearance,
    pub services: crate::real::services::Services,
    services_hub: Option<Arc<muzon_services::hub::Hub>>,
    pub spectrum: Arc<Vec<f32>>,
    spectrum_enabled: bool,
    pub editor: Option<super::editor::Editor>,
    pub palette: Option<super::palette::Palette>,
    pub queue_drag: Option<QueueDrag>,
    editor_generation: u64,
    held_keys: HashSet<iced::keyboard::Key>,
    pub query: String,
    pub smart: super::smart::Smart,
    smart_refresh: std::time::Instant,
    pub album_grid: crate::real::album_grid::Grid,
    pub groups: Vec<super::groups::Group>,
    pub group_rows: Vec<usize>,
    pub selected_group: Option<super::groups::GroupKey>,
    pub group_scroll: f32,
    pub filtered: Vec<usize>,
    search_index: Vec<String>,
    library_by_id: HashMap<i32, usize>,
    pub selected_playlist: Option<i32>,
    pub new_playlist_name: String,
    pub rename_playlist_name: String,
    pub collection_overlay: Option<CollectionOverlay>,
    pub playlist_busy: bool,
    pub pending_favorites: HashSet<i32>,
    pub playlist_scroll: f32,
    pub picker_scroll: f32,
    pub folder_input: String,
    pub folder_picker_open: bool,
    pub track_covers: crate::real::track_covers::Covers,
    pub library_scroll: f32,
    pub queue_scroll: f32,
    pub seek_preview: Option<f64>,
    pub closing: bool,
    pub audio_close_failed: bool,
    pub ui_error: Option<String>,
}

/// Filtering names never become queue positions: resolve a click to a stable id.
pub fn filtered_track_id(snapshot: &Snapshot, filtered: &[usize], row: usize) -> Option<i32> {
    filtered
        .get(row)
        .and_then(|&i| snapshot.library.get(i))
        .map(|t| t.id)
}
/// Queue rows address queue occurrences, including repeated copies of a track.
pub fn displayed_queue_index(snapshot: &Snapshot, row: usize) -> Option<usize> {
    snapshot
        .display
        .get(row)
        .copied()
        .filter(|&i| i < snapshot.queue.len())
}

/// Collection views resolve membership through stable library ids, preserving
/// playlist order and never mutating the playback queue.
pub fn collection_indices(
    snapshot: &Snapshot,
    screen: Screen,
    selected: Option<i32>,
    by_id: &HashMap<i32, usize>,
    search: &[String],
    query: &str,
) -> Vec<usize> {
    let matches = |&i: &usize| {
        search
            .get(i)
            .is_some_and(|text| query.is_empty() || text.contains(query))
    };
    match screen {
        Screen::Playlists => selected
            .and_then(|id| snapshot.playlists.iter().find(|p| p.id == id))
            .map(|p| {
                p.track_ids
                    .iter()
                    .filter_map(|id| by_id.get(id).copied())
                    .filter(matches)
                    .collect()
            })
            .unwrap_or_default(),
        _ => snapshot
            .library
            .iter()
            .enumerate()
            .filter(|(_, t)| screen != Screen::Favorites || t.is_favorite)
            .map(|(i, _)| i)
            .filter(matches)
            .collect(),
    }
}

impl RealApp {
    pub fn open(profile: &Path) -> anyhow::Result<Option<Self>> {
        let Some(lock) = super::desktop::instance::claim(profile)? else {
            return Ok(None);
        };
        let (tx, requests) = std::sync::mpsc::sync_channel(16);
        let instance = super::desktop::instance::InstanceServer::start(profile, tx.clone())?;
        let config_dir = profile.join("config");
        let db = Arc::new(Db::open(&profile.join("muzon.sqlite3"))?);
        let controller = Controller::spawn(CoreConfig {
            db: db.clone(),
            session_store: SessionStore::new(&config_dir),
            playback_settings_store: PlaybackSettingsStore::new(&config_dir),
            cover_dir: profile.join("covers"),
        })?;
        let desktop = Desktop::start(controller.client(), profile, instance, tx, requests);
        let snapshot = controller.snapshot();
        let media = controller.media();
        let audio = AudioUi::new(&snapshot);
        let (services_hub, services_error) =
            match muzon_services::hub::Hub::new(profile, db, controller.client()) {
                Ok(hub) => (Some(Arc::new(hub)), None),
                Err(error) => (None, Some(error.to_string())),
            };
        let services =
            crate::real::services::Services::new(services_hub.as_deref(), services_error);
        let mut app = Self {
            controller: Some(controller),
            desktop: Some(desktop),
            tray: super::tray::Tray::new().unwrap_or_else(|e| {
                eprintln!("Muzon tray: {e}");
                None
            }),
            hidden: false,
            _profile_lock: lock,
            window: crate::real::window_state::WindowMemory::new(profile),
            snapshot,
            media,
            cover: None,
            screen: Screen::Library,
            audio,
            services,
            services_hub,
            hotkeys: crate::real::hotkeys::Hotkeys::new(profile),
            appearance: crate::real::appearance::Appearance::new(profile),
            spectrum: Arc::new(Vec::new()),
            spectrum_enabled: false,
            editor: None,
            palette: None,
            queue_drag: None,
            editor_generation: 0,
            held_keys: HashSet::new(),
            query: String::new(),
            smart: crate::real::smart::Smart::MostPlayed,
            smart_refresh: std::time::Instant::now(),
            album_grid: crate::real::album_grid::Grid::new(profile.join("covers/albums")),
            groups: Vec::new(),
            group_rows: Vec::new(),
            selected_group: None,
            group_scroll: 0.0,
            filtered: Vec::new(),
            search_index: Vec::new(),
            library_by_id: HashMap::new(),
            selected_playlist: None,
            new_playlist_name: String::new(),
            rename_playlist_name: String::new(),
            collection_overlay: None,
            playlist_busy: false,
            pending_favorites: HashSet::new(),
            playlist_scroll: 0.0,
            picker_scroll: 0.0,
            folder_input: String::new(),
            folder_picker_open: false,
            track_covers: crate::real::track_covers::Covers::new(profile.join("covers/rows")),
            library_scroll: 0.0,
            queue_scroll: 0.0,
            seek_preview: None,
            closing: false,
            audio_close_failed: false,
            ui_error: None,
        };
        app.album_grid.viewport = app.window.geometry.size();
        app.rebuild_search();
        app.request_current_media();
        Ok(Some(app))
    }
    fn rebuild_search(&mut self) {
        self.library_by_id = self
            .snapshot
            .library
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id, i))
            .collect();
        self.search_index = self
            .snapshot
            .library
            .iter()
            .map(|t| {
                format!(
                    "{}\n{}\n{}\n{}",
                    t.title,
                    t.artist.as_deref().unwrap_or(""),
                    t.album.as_deref().unwrap_or(""),
                    t.genre.as_deref().unwrap_or("")
                )
                .to_lowercase()
            })
            .collect();
        self.groups = super::groups::build(&self.snapshot.library, self.screen);
        self.filter();
    }
    fn filter(&mut self) {
        let query = self.query.trim().to_lowercase();
        if super::groups::is_grouped(self.screen) {
            self.group_rows = self
                .groups
                .iter()
                .enumerate()
                .filter(|(_, g)| g.matches(&query, &self.search_index))
                .map(|(i, _)| i)
                .collect();
            if !self
                .group_rows
                .iter()
                .any(|&i| self.selected_group.as_ref() == Some(&self.groups[i].key))
            {
                self.selected_group = self.group_rows.first().map(|&i| self.groups[i].key.clone());
            }
            self.filtered = self
                .selected_group
                .as_ref()
                .and_then(|key| self.groups.iter().find(|g| &g.key == key))
                .map(|g| g.tracks(&query, &self.search_index))
                .unwrap_or_default();
            return;
        }
        if self.screen == Screen::Smart {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.);
            self.filtered = super::smart::select(&self.snapshot.library, self.smart, now)
                .into_iter()
                .filter(|&i| {
                    query.is_empty() || self.search_index.get(i).is_some_and(|s| s.contains(&query))
                })
                .collect();
            return;
        }
        self.filtered = collection_indices(
            &self.snapshot,
            self.screen,
            self.selected_playlist,
            &self.library_by_id,
            &self.search_index,
            &query,
        );
    }
    pub fn library_track(&self, id: i32) -> Option<&muzon_core::domain::Track> {
        self.library_by_id
            .get(&id)
            .and_then(|&i| self.snapshot.library.get(i))
    }
    fn reset_list_scroll(&mut self) -> Task<Message> {
        self.library_scroll = 0.0;
        scrollable::snap_to(
            scrollable::Id::new(LIBRARY_LIST),
            scrollable::RelativeOffset::START,
        )
    }
    fn submit_playlist(&mut self, edit: PlaylistEdit) -> Task<Message> {
        if self.playlist_busy {
            return Task::none();
        }
        let Some(client) = self.controller.as_ref().map(Controller::client) else {
            return Task::none();
        };
        self.playlist_busy = true;
        self.ui_error = None;
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || client.edit_playlist(edit))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r)
            },
            Message::PlaylistFinished,
        )
    }
    fn save_audio(&mut self) -> Task<Message> {
        self.audio.tempo_target = None;
        if self.audio.saving {
            return Task::none();
        }
        let Some(client) = self.controller.as_ref().map(Controller::client) else {
            return Task::none();
        };
        self.audio.saving = true;
        self.ui_error = None;
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || client.save_audio())
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r)
            },
            Message::AudioSaved,
        )
    }
    fn edit_audio(&mut self, edit: muzon_core::domain::playback_settings::PlaybackEdit) {
        match self.audio.preview(&edit) {
            Ok(()) => {
                if let Some(core) = &self.controller {
                    core.preview_playback(edit);
                }
            }
            Err(error) => self.ui_error = Some(error),
        }
    }
    fn select_playlist(&mut self, id: Option<i32>) {
        self.selected_playlist = id;
        self.rename_playlist_name = id
            .and_then(|id| self.snapshot.playlists.iter().find(|p| p.id == id))
            .map(|p| p.name.clone())
            .unwrap_or_default();
        self.filter();
    }

    fn request_current_media(&self) {
        if let (Some(core), Some(path)) = (&self.controller, &self.snapshot.current_path) {
            core.request_media(path.clone());
        }
    }
    fn poll(&mut self) -> bool {
        let Some(core) = &self.controller else {
            return false;
        };
        let previous_count = self.filtered.len();
        let next = core.snapshot();
        let library_changed = !Arc::ptr_eq(&self.snapshot.library, &next.library);
        let playlists_changed = !Arc::ptr_eq(&self.snapshot.playlists, &next.playlists);
        let track_changed = self.snapshot.current_path != next.current_path;
        self.snapshot = next;
        self.audio.sync(&self.snapshot);
        if self.audio_close_failed
            && !self.snapshot.audio_dirty
            && self.snapshot.audio_error.is_none()
        {
            self.audio_close_failed = false;
            self.ui_error = None;
        }
        if track_changed {
            self.audio.track_changed();
            self.seek_preview = None;
            self.cover = None;
        }
        let media = core.media();
        if !Arc::ptr_eq(&media, &self.media) || track_changed {
            self.cover = media
                .cover
                .as_ref()
                .filter(|_| media.track_path == self.snapshot.current_path)
                .map(|bytes| image::Handle::from_bytes(bytes.as_ref().clone()));
            self.media = media;
        }
        if !core.is_running() {
            self.ui_error = Some("Playback worker stopped".into());
        }
        if playlists_changed
            && self
                .selected_playlist
                .is_some_and(|id| !self.snapshot.playlists.iter().any(|p| p.id == id))
        {
            self.select_playlist(None);
        }
        if library_changed {
            self.album_grid.invalidate();
            self.track_covers.invalidate();
            self.rebuild_search();
        } else if playlists_changed {
            self.filter();
        }
        if (library_changed || playlists_changed) && self.palette.is_some() {
            let query = self.palette.as_ref().unwrap().query.clone();
            let mut palette = super::palette::Palette::new(&self.snapshot);
            palette.query = query;
            palette.filter();
            self.palette = Some(palette);
        }
        if track_changed {
            self.request_current_media();
        }
        previous_count != self.filtered.len()
    }
    pub fn subscription(&self) -> Subscription<Message> {
        let close =
            iced::window::close_requests().map(|id| Message::WindowCloseRequested(Some(id)));
        if self.closing {
            close
        } else {
            Subscription::batch([
                iced::time::every(Duration::from_millis(
                    if self.spectrum_enabled && self.snapshot.is_playing {
                        40
                    } else {
                        200
                    },
                ))
                .map(|_| Message::Poll),
                iced::time::every(Duration::from_secs(5)).map(|_| Message::AppearanceRefresh),
                close,
                iced::event::listen_with(|event, status, id| match event {
                    iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                        key,
                        modifiers,
                        ..
                    }) => Some(Message::KeyPressed(
                        key,
                        modifiers,
                        status == iced::event::Status::Ignored,
                    )),
                    iced::Event::Keyboard(iced::keyboard::Event::KeyReleased { key, .. }) => {
                        Some(Message::KeyReleased(key))
                    }
                    iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                        iced::mouse::Button::Left,
                    )) => Some(Message::QueueDragRelease),
                    iced::Event::Window(iced::window::Event::Unfocused) => {
                        Some(Message::WindowFocusLost)
                    }
                    iced::Event::Mouse(iced::mouse::Event::CursorLeft) => {
                        Some(Message::WindowFocusLost)
                    }
                    iced::Event::Window(iced::window::Event::Opened { .. }) => {
                        Some(Message::WindowOpened(id))
                    }
                    iced::Event::Window(iced::window::Event::Resized(size)) => {
                        Some(Message::WindowResized(id, size))
                    }
                    _ => None,
                }),
            ])
        }
    }
    fn show_window(&mut self, focus: bool) -> Task<Message> {
        self.hidden = false;
        let maximized = self.window.geometry.maximized;
        iced::window::get_latest().then(move |id| {
            id.map(|id| {
                let task = iced::window::change_mode(id, iced::window::Mode::Windowed)
                    .chain(iced::window::minimize(id, false))
                    .chain(iced::window::maximize(id, maximized));
                if focus {
                    task.chain(iced::window::gain_focus(id))
                } else {
                    task
                }
            })
            .unwrap_or_else(Task::none)
        })
    }
    pub fn update(&mut self, message: Message) -> Task<Message> {
        if self.editor.is_some()
            && matches!(
                message,
                Message::WindowCloseRequested(_) | Message::QuitRequested(_)
            )
        {
            if self.editor.as_ref().is_some_and(|e| e.dirty() || e.saving) {
                let _ = self.update(Message::EditorClose);
                return self.show_window(true);
            }
            self.editor = None;
        }
        if matches!(
            message,
            Message::QuitRequested(_) | Message::DiscardAudioAndQuit
        ) && self.services.busy.is_some()
        {
            self.services.pending_quit = true;
            self.services.pending_discard = matches!(message, Message::DiscardAudioAndQuit);
            if let Some(cancel) = &self.services.cancel {
                cancel.cancel();
            }
            self.ui_error = Some("Завершаем фоновую операцию перед выходом…".into());
            return self.show_window(true);
        }
        if let Message::WindowCloseRequested(id) = message {
            if !self.closing
                && self.appearance.prefs.close_to_tray
                && self.tray.as_ref().is_some_and(|tray| tray.ready())
            {
                if let Some(id) = id {
                    self.hidden = true;
                    return iced::window::change_mode(id, iced::window::Mode::Hidden);
                }
            }
            return self.update(Message::QuitRequested(id));
        }
        if let Message::QuitRequested(id) = message {
            if self.closing {
                return Task::none();
            }
            self.closing = true;
            if self.hidden {
                return self.update(Message::WindowCloseReady(id, None));
            }
            if let Some(id) = id {
                return iced::window::get_size(id).then(move |size| {
                    iced::window::get_maximized(id).map(move |maximized| {
                        Message::WindowCloseReady(Some(id), Some((size, maximized)))
                    })
                });
            }
            return self.update(Message::WindowCloseReady(None, None));
        }
        if let Message::WindowCloseReady(id, geometry) = message {
            if let Some((size, maximized)) = geometry {
                let generation = self.window.next_observation();
                self.window.observe(generation, size, maximized);
            }
            self.closing = true;
            self.audio_close_failed = false;
            let hotkeys_flush = self.hotkeys.flush();
            let appearance_flush = self.appearance.flush();
            let client = self
                .controller
                .as_ref()
                .filter(|core| core.is_running())
                .map(Controller::client);
            return Task::perform(
                async move {
                    let result = tokio::task::spawn_blocking(move || {
                        if let Some(client) = client {
                            client.save_audio()?;
                        }
                        let (store, revision, values) = hotkeys_flush;
                        store.save(revision, &values)?;
                        let (store, revision, value) = appearance_flush;
                        store.save(revision, &value)
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r);
                    (id, result)
                },
                |(id, result)| Message::ShutdownAudioSaved(id, result),
            );
        }

        if let Message::ShutdownAudioSaved(id, result) = message {
            if let Err(error) = result {
                self.closing = false;
                self.audio_close_failed = true;
                self.audio.saving = false;
                self.audio.tempo_target = None;
                let _ = self.poll();
                self.ui_error = Some(format!(
                    "Не удалось сохранить настройки. Закрытие отменено: {error}"
                ));
                return self.show_window(true);
            }
            self.tray.take();
            let window_flush = self.window.flush();
            let desktop = self.desktop.take();
            let services_hub = self.services_hub.take();
            if let Some(mut core) = self.controller.take() {
                return Task::perform(
                    async move {
                        let result = tokio::task::spawn_blocking(move || {
                            let (store, revision, geometry) = window_flush;
                            if let Err(error) = store.save(revision, geometry) {
                                eprintln!("Muzon window settings: {error}");
                            }
                            if let Some(hub) = services_hub {
                                hub.shutdown();
                            }
                            drop(desktop);
                            core.shutdown();
                        })
                        .await
                        .map_err(|e| e.to_string());
                        (id, result)
                    },
                    |(id, result)| Message::ShutdownComplete(id, result),
                );
            }
            return id.map(iced::window::close).unwrap_or_else(iced::exit);
        }
        if let Message::ShutdownComplete(id, result) = message {
            if let Err(error) = result {
                eprintln!("Muzon shutdown: {error}");
            }
            return id.map(iced::window::close).unwrap_or_else(iced::exit);
        }
        if self.closing {
            return Task::none();
        }
        match message {
            Message::ServiceAction(action) => {
                if let Some(hub) = self.services_hub.clone() {
                    return self.services.start(hub, action);
                }
            }
            Message::ServiceFinished(generation, action, result) => {
                if self.services.finished(generation, action, result) {
                    if let Some(hub) = &self.services_hub {
                        self.services.poll(hub);
                    }
                    if self.services.pending_quit {
                        let discard = self.services.pending_discard;
                        self.services.pending_quit = false;
                        self.services.pending_discard = false;
                        self.ui_error = None;
                        return self.update(if discard {
                            Message::DiscardAudioAndQuit
                        } else {
                            Message::Quit
                        });
                    }
                }
            }
            Message::ServiceField(index, value) => {
                if self.services.busy.is_none() && value.len() <= 4096 {
                    match index {
                        0 => self.services.config.endpoint = value,
                        1 => self.services.config.region = value,
                        2 => self.services.config.bucket = value,
                        3 => self.services.config.prefix = value,
                        _ => {}
                    }
                }
            }
            Message::ServiceSecret(index, value) => {
                if self.services.busy.is_none() {
                    if value.0.len() > 4096 {
                        self.services.error = Some("Ключ превышает 4096 байт".into());
                    } else {
                        match index {
                            0 => self.services.access = value,
                            1 => self.services.secret = value,
                            2 => self.services.token = value,
                            3 => {
                                self.services.lastfm_key = value;
                                self.services.auth_url = None;
                            }
                            4 => {
                                self.services.lastfm_secret = value;
                                self.services.auth_url = None;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Message::ServicePathStyle(value) => {
                if self.services.busy.is_none() {
                    self.services.config.path_style = value;
                }
            }
            Message::ServiceCancel => {
                if let Some(cancel) = &self.services.cancel {
                    cancel.cancel();
                    self.services.message = Some("Отменяем синхронизацию…".into());
                }
            }
            Message::ServiceCopyAuth => {
                if let Some(url) = &self.services.auth_url {
                    return iced::clipboard::write(url.0.clone());
                }
            }
            Message::ServiceOpenAuth => {
                if let Some(url) = self.services.auth_url.clone() {
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || -> Result<(), String> {
                                let parsed = url::Url::parse(&url.0).map_err(|e| e.to_string())?;
                                if parsed.scheme() != "https"
                                    || parsed.host_str() != Some("www.last.fm")
                                {
                                    return Err("Некорректный адрес авторизации Last.fm".into());
                                }
                                let mut child = std::process::Command::new("xdg-open")
                                    .arg(&url.0)
                                    .stdout(std::process::Stdio::null())
                                    .stderr(std::process::Stdio::null())
                                    .spawn()
                                    .map_err(|e| e.to_string())?;
                                std::thread::spawn(move || {
                                    let _ = child.wait();
                                });
                                Ok(())
                            })
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|r| r)
                        },
                        Message::ServiceBrowser,
                    );
                }
            }
            Message::ServiceBrowser(result) => {
                if let Err(error) = result {
                    self.services.error = Some(format!(
                        "Не удалось открыть браузер: {error}. Скопируйте ссылку вручную"
                    ));
                }
            }
            Message::AppearanceChanged(change) => {
                let refresh = matches!(change, crate::real::appearance::Change::Mode(_));
                match self.appearance.change(change) {
                    Ok(()) => {}
                    Err(error) => {
                        self.appearance.error = Some(error);
                        return Task::none();
                    }
                }
                let save = self.update(Message::AppearanceSave);
                if refresh {
                    return Task::batch([save, self.update(Message::AppearanceRefresh)]);
                }
                return save;
            }
            Message::AppearanceAccentInput(value) => self.appearance.accent_input = value,
            Message::AppearanceAccentApply => {
                return self.update(Message::AppearanceChanged(
                    crate::real::appearance::Change::Accent(self.appearance.accent_input.clone()),
                ))
            }
            Message::AppearanceFileInput(value) => self.appearance.file_input = value,
            Message::AppearanceRyoku => {
                if let Some(path) = crate::real::appearance::ryoku_path() {
                    self.appearance.file_input = path.to_string_lossy().into_owned();
                    return self.update(Message::AppearanceImport);
                }
            }
            Message::AppearanceImport => {
                self.appearance.import();
                return Task::batch([
                    self.update(Message::AppearanceSave),
                    self.update(Message::AppearanceRefresh),
                ]);
            }
            Message::AppearanceSave => {
                let (store, revision, value) = self.appearance.flush();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || store.save(revision, &value))
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|r| r)
                    },
                    move |result| Message::AppearanceSaved(revision, result),
                );
            }
            Message::AppearanceSaved(revision, result) => self.appearance.saved(revision, result),
            Message::AppearanceRefresh => {
                let mut tasks = Vec::new();
                if self.appearance.prefs.mode == crate::real::appearance::Mode::System {
                    tasks.push(Task::perform(
                        crate::real::appearance::system_light(),
                        Message::AppearanceSystem,
                    ));
                }
                if !self.appearance.prefs.theme_file.is_empty() {
                    let path = std::path::PathBuf::from(&self.appearance.prefs.theme_file);
                    let previous = self.appearance.stamp;
                    let generation = self.appearance.generation;
                    let light = self.appearance.prefs.mode == crate::real::appearance::Mode::Light
                        || self.appearance.system_light;
                    tasks.push(Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                crate::real::appearance::read_source(path, previous, light)
                            })
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|r| r)
                        },
                        move |result| Message::AppearanceThemeLoaded(generation, result),
                    ));
                }
                return Task::batch(tasks);
            }
            Message::AppearanceThemeLoaded(generation, result) => {
                if generation == self.appearance.generation {
                    self.appearance.set_source(result);
                }
            }
            Message::AppearanceSystem(light) => {
                if self.appearance.prefs.mode == crate::real::appearance::Mode::System {
                    if let Some(light) = light {
                        let changed = light != self.appearance.system_light;
                        self.appearance.system_light = light;
                        self.appearance.apply();
                        if changed && !self.appearance.prefs.theme_file.is_empty() {
                            self.appearance.generation = self.appearance.generation.wrapping_add(1);
                            self.appearance.stamp = None;
                            return self.update(Message::AppearanceRefresh);
                        }
                    }
                }
            }
            Message::WindowFocusLost => {
                self.queue_drag = None;
                self.hotkeys.capture = None;
                self.held_keys.clear();
            }
            Message::QueueDragStart(from) => {
                if from < self.snapshot.display.len() {
                    self.queue_drag = Some(QueueDrag {
                        from,
                        to: from,
                        baseline: self.snapshot.display.clone(),
                    });
                }
            }
            Message::QueueDragHover(to) => {
                if to < self.snapshot.display.len() {
                    if let Some(drag) = &mut self.queue_drag {
                        drag.to = to;
                    }
                }
            }
            Message::QueueDragRelease => {
                if let Some(drag) = self.queue_drag.take() {
                    if Arc::ptr_eq(&drag.baseline, &self.snapshot.display) && drag.from != drag.to {
                        return self.update(Message::QueueMove(drag.from, drag.to));
                    }
                }
            }
            Message::PaletteQuery(query) => {
                if let Some(palette) = &mut self.palette {
                    palette.query = query;
                    palette.filter();
                }
                return scrollable::snap_to(
                    scrollable::Id::new("muzon-palette-results"),
                    scrollable::RelativeOffset::START,
                );
            }
            Message::PaletteClose => self.palette = None,
            Message::PaletteExecute => {
                if let Some(palette) = &self.palette {
                    return self.update(Message::PaletteRun(palette.selected));
                }
            }
            Message::PaletteRun(position) => {
                let action = self
                    .palette
                    .as_ref()
                    .and_then(|p| p.results.get(position).and_then(|&i| p.entries.get(i)))
                    .map(|entry| entry.action.clone());
                self.palette = None;
                if let Some(action) = action {
                    use super::palette::Action;
                    match action {
                        Action::Screen(screen) => {
                            return self.update(Message::ScreenSelected(screen))
                        }
                        Action::Toggle => return self.update(Message::TogglePlay),
                        Action::Next => return self.update(Message::Next),
                        Action::Previous => return self.update(Message::Previous),
                        Action::Rescan => return self.update(Message::Rescan),
                        Action::PickFolder => return self.update(Message::PickFolder),
                        Action::Quit => return self.update(Message::Quit),
                        Action::EditCurrent => {
                            if let Some(track) = self.snapshot.current_track() {
                                return self.update(Message::OpenEditor(track.id));
                            }
                        }
                        Action::Track(id) => {
                            if let Some(core) = &self.controller {
                                core.set_queue(
                                    self.snapshot.library.iter().map(|t| t.id).collect(),
                                    id,
                                );
                            }
                        }
                        Action::Group(screen, key) => {
                            self.query.clear();
                            let task = self.update(Message::ScreenSelected(screen));
                            self.album_grid.browsing = false;
                            self.selected_group = Some(key);
                            self.filter();
                            return task;
                        }
                        Action::Playlist(id) => {
                            self.query.clear();
                            let task = self.update(Message::ScreenSelected(Screen::Playlists));
                            let select = self.update(Message::PlaylistSelected(id));
                            return Task::batch([task, select]);
                        }
                        Action::Smart(kind) => {
                            self.query.clear();
                            self.smart = kind;
                            return self.update(Message::ScreenSelected(Screen::Smart));
                        }
                    }
                }
            }
            Message::HotkeyCapture(index) => {
                if index < 8 {
                    self.hotkeys.capture = Some(index);
                    self.hotkeys.error = None;
                }
            }
            Message::HotkeysReset => {
                self.hotkeys.reset();
                return self.update(Message::HotkeysSave);
            }
            Message::HotkeysSave => {
                let (store, revision, values) = self.hotkeys.flush();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || store.save(revision, &values))
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|r| r)
                    },
                    move |result| Message::HotkeysSaved(revision, result),
                );
            }
            Message::HotkeysSaved(revision, result) => self.hotkeys.saved(revision, result),
            Message::KeyReleased(key) => {
                self.held_keys.remove(&key);
            }
            Message::KeyPressed(key, modifiers, ignored) => {
                let repeated = !self.held_keys.insert(key.clone());
                if let Some(palette) = &mut self.palette {
                    use iced::keyboard::{key::Named, Key};
                    let delta = if key == Key::Named(Named::ArrowDown) {
                        1
                    } else if key == Key::Named(Named::ArrowUp) {
                        -1
                    } else {
                        0
                    };
                    if delta != 0 {
                        palette.selected = palette
                            .selected
                            .saturating_add_signed(delta)
                            .min(palette.results.len().saturating_sub(1));
                        return scrollable::scroll_to(
                            scrollable::Id::new("muzon-palette-results"),
                            scrollable::AbsoluteOffset {
                                x: 0.,
                                y: palette.selected.saturating_sub(4) as f32 * 42.,
                            },
                        );
                    }
                }
                if let Some(index) = self.hotkeys.capture {
                    if repeated {
                        return Task::none();
                    }
                    if key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) {
                        self.hotkeys.capture = None;
                        return Task::none();
                    }
                    match self.hotkeys.bind(index, &key, modifiers) {
                        Ok(()) => return self.update(Message::HotkeysSave),
                        Err(error) => self.hotkeys.error = Some(error),
                    }
                    return Task::none();
                }
                if key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Space)
                    && modifiers.is_empty()
                    && self.editor.as_ref().is_some_and(|e| e.tap.is_some())
                {
                    if repeated {
                        return Task::none();
                    }
                    return self.update(Message::EditorTapStamp);
                }
                if let Some(action) = self.hotkeys.resolve(&key, modifiers) {
                    if repeated
                        && matches!(
                            action,
                            super::shortcuts::Shortcut::Toggle
                                | super::shortcuts::Shortcut::Next
                                | super::shortcuts::Shortcut::Previous
                                | super::shortcuts::Shortcut::Palette
                        )
                    {
                        return Task::none();
                    }
                    if ignored
                        || matches!(
                            action,
                            super::shortcuts::Shortcut::Search
                                | super::shortcuts::Shortcut::Palette
                                | super::shortcuts::Shortcut::Escape
                        )
                    {
                        return self.update(Message::Shortcut(action));
                    }
                }
            }
            Message::OpenEditor(id) => {
                if self.editor.is_some() || self.collection_overlay.is_some() {
                    return Task::none();
                }
                let Some(track) = self.library_track(id).cloned() else {
                    return Task::none();
                };
                let Some(client) = self.controller.as_ref().map(Controller::client) else {
                    return Task::none();
                };
                self.editor_generation = self.editor_generation.wrapping_add(1);
                let generation = self.editor_generation;
                self.editor = Some(super::editor::Editor::new(
                    generation,
                    muzon_core::editing::EditorSource {
                        track,
                        lyrics: String::new(),
                        lyrics_in_tag: false,
                    },
                ));
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || client.load_editor(id))
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|r| r)
                    },
                    move |r| Message::EditorLoaded(generation, r),
                );
            }
            Message::EditorLoaded(generation, result) => {
                if let Some(editor) = self.editor.as_mut().filter(|e| e.generation == generation) {
                    match result {
                        Ok(source) => {
                            *editor = super::editor::Editor::new(generation, source);
                            editor.loading = false;
                        }
                        Err(error) => {
                            editor.loading = false;
                            editor.error = Some(error);
                        }
                    }
                }
            }
            Message::EditorField(index, value) => {
                if let Some(editor) = self.editor.as_mut().filter(|e| !e.loading && !e.saving) {
                    if let Some(field) = editor.fields.get_mut(index) {
                        *field = value;
                        editor.tags_dirty = true;
                        editor.error = None;
                    }
                }
            }
            Message::EditorText(action) => {
                if let Some(editor) = self.editor.as_mut().filter(|e| !e.loading && !e.saving) {
                    editor.lyrics_dirty |= action.is_edit();
                    editor.lyrics.perform(action);
                }
            }
            Message::EditorTab(tab) => {
                if let Some(editor) = self.editor.as_mut().filter(|e| !e.loading && !e.saving) {
                    editor.tab = tab;
                }
            }
            Message::EditorStorage(in_tag) => {
                if let Some(editor) = self.editor.as_mut().filter(|e| !e.loading && !e.saving) {
                    editor.in_tag = in_tag;
                    editor.lyrics_dirty = true;
                }
            }
            Message::EditorClose => {
                if let Some(editor) = self.editor.as_mut() {
                    if editor.saving {
                        editor.error = Some("Дождитесь окончания записи".into());
                    } else if editor.dirty() {
                        editor.discard = true;
                    } else {
                        self.editor = None;
                    }
                }
            }
            Message::EditorKeep => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.discard = false;
                }
            }
            Message::EditorDiscard => {
                if self.editor.as_ref().is_some_and(|e| !e.saving) {
                    self.editor = None;
                }
            }
            Message::EditorFetch => {
                if let Some(editor) = self
                    .editor
                    .as_mut()
                    .filter(|e| !e.loading && !e.saving && e.fetch.is_none())
                {
                    let optional = |s: &String| (!s.trim().is_empty()).then(|| s.trim().to_owned());
                    let query = muzon_core::data::lrclib::Query {
                        title: editor.fields[0].trim().to_owned(),
                        artist: optional(&editor.fields[1]),
                        album: optional(&editor.fields[2]),
                        duration_secs: editor.source.track.duration_secs,
                    };
                    let generation = editor.generation;
                    editor.fetch_serial = editor.fetch_serial.wrapping_add(1);
                    let serial = editor.fetch_serial;
                    editor.error = None;
                    editor.suggestion = None;
                    let (task, handle) = Task::perform(
                        async move {
                            muzon_core::data::lrclib::suggest(&query)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        move |r| Message::EditorFetched(generation, serial, r),
                    )
                    .abortable();
                    editor.fetch = Some(handle.abort_on_drop());
                    return task;
                }
            }
            Message::EditorFetched(generation, serial, result) => {
                if let Some(editor) = self.editor.as_mut().filter(|e| {
                    e.generation == generation && e.fetch_serial == serial && e.fetch.is_some()
                }) {
                    editor.fetch.take();
                    match result {
                        Ok(Some(found)) => editor.suggestion = Some(found),
                        Ok(None) => {
                            editor.error =
                                Some("В LRCLIB нет подходящего текста для этой записи".into())
                        }
                        Err(error) => editor.error = Some(error),
                    }
                }
            }
            Message::EditorFetchCancel => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.fetch.take();
                }
            }
            Message::EditorUseSuggestion => {
                if let Some(editor) = self
                    .editor
                    .as_mut()
                    .filter(|e| !e.saving && e.tap.is_none())
                {
                    if let Some(found) = editor.suggestion.take() {
                        editor.lyrics = iced::widget::text_editor::Content::with_text(&found.text);
                        editor.lyrics_dirty = true;
                        editor.error = None;
                    }
                }
            }
            Message::EditorDismissSuggestion => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.suggestion = None;
                }
            }
            Message::EditorTapStart => {
                if let Some(editor) = self
                    .editor
                    .as_mut()
                    .filter(|e| !e.saving && !e.loading && e.tap.is_none())
                {
                    match super::editor::TapDraft::new(&editor.lyrics.text()) {
                        Ok(tap) => editor.tap = Some(tap),
                        Err(error) => editor.error = Some(error),
                    }
                }
            }
            Message::EditorTapStamp => {
                if let Some(editor) = self.editor.as_mut().filter(|e| {
                    !e.saving
                        && self.snapshot.current_path.as_deref()
                            == Some(e.source.track.path.as_str())
                }) {
                    if let Some(tap) = &mut editor.tap {
                        if let Err(error) = tap.stamp(self.snapshot.position_secs) {
                            editor.error = Some(error);
                        }
                    }
                }
            }
            Message::EditorTapUndo => {
                if let Some(tap) = self.editor.as_mut().and_then(|e| e.tap.as_mut()) {
                    tap.stamps.pop();
                }
            }
            Message::EditorTapFinish => {
                if let Some(editor) = self.editor.as_mut() {
                    if let Some(tap) = editor.tap.take() {
                        editor.lyrics = iced::widget::text_editor::Content::with_text(&tap.text());
                        editor.lyrics_dirty = true;
                    }
                }
            }
            Message::EditorTapCancel => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.tap = None;
                }
            }
            Message::EditorTimestamp => {
                if let Some(editor) = self.editor.as_mut().filter(|e| {
                    !e.loading
                        && !e.saving
                        && self.snapshot.current_path.as_deref()
                            == Some(e.source.track.path.as_str())
                }) {
                    editor
                        .lyrics
                        .perform(iced::widget::text_editor::Action::Edit(
                            iced::widget::text_editor::Edit::Paste(Arc::new(
                                super::editor::timestamp(self.snapshot.position_secs),
                            )),
                        ));
                    editor.lyrics_dirty = true;
                }
            }
            Message::EditorSave => {
                if self.editor.as_ref().is_some_and(|e| e.tap.is_some()) {
                    return Task::none();
                }
                let Some(client) = self.controller.as_ref().map(Controller::client) else {
                    return Task::none();
                };
                if let Some(editor) = self.editor.as_mut().filter(|e| !e.loading && !e.saving) {
                    let request = match editor.request() {
                        Ok(request) => request,
                        Err(error) => {
                            editor.error = Some(error);
                            return Task::none();
                        }
                    };
                    let id = editor.id;
                    let generation = editor.generation;
                    let tab = editor.tab;
                    editor.saving = true;
                    editor.error = None;
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || client.save_edit(id, request))
                                .await
                                .map_err(|e| e.to_string())
                                .and_then(|r| r)
                        },
                        move |r| Message::EditorSaved(generation, tab, r),
                    );
                }
            }
            Message::EditorSaved(generation, tab, result) => {
                if let Some(editor) = self.editor.as_mut().filter(|e| e.generation == generation) {
                    editor.saving = false;
                    match result {
                        Ok(()) => {
                            editor.error = None;
                            if tab == super::editor::Tab::Tags {
                                editor.tags_dirty = false;
                                editor.fields[6].clear();
                            } else {
                                editor.lyrics_dirty = false;
                            }
                        }
                        Err(error) => editor.error = Some(error),
                    }
                }
                let _ = self.poll();
            }
            Message::SmartSelected(smart) => {
                self.smart = smart;
                self.filter();
                return self.reset_list_scroll();
            }
            Message::AlbumCovers(epoch, loaded) => {
                self.album_grid.finish(epoch, loaded);
                return crate::real::album_grid::load(self);
            }
            Message::AlbumsBack => {
                self.album_grid.browsing = true;
                return scrollable::scroll_to(
                    scrollable::Id::new("muzon-groups"),
                    scrollable::AbsoluteOffset {
                        x: 0.,
                        y: self.group_scroll,
                    },
                );
            }
            Message::GroupsScrolled(offset) => {
                self.group_scroll = offset;
                return crate::real::album_grid::load(self);
            }
            Message::AlbumSelected(key) => {
                if self.groups.iter().any(|g| g.key == key) {
                    self.selected_group = Some(key);
                    self.album_grid.browsing = false;
                    self.filter();
                    return self.reset_list_scroll();
                }
            }
            Message::GroupSelected(index) => {
                if let Some(group) = self.groups.get(index) {
                    self.album_grid.browsing = false;
                    self.selected_group = Some(group.key.clone());
                }
                self.filter();
                return self.reset_list_scroll();
            }
            Message::SetSleep(choice) => {
                if let Some(core) = &self.controller {
                    core.set_sleep(choice);
                }
            }
            Message::Shortcut(action) => {
                use super::shortcuts::Shortcut;
                if action == Shortcut::Toggle
                    && self.editor.as_ref().is_some_and(|e| e.tap.is_some())
                {
                    return self.update(Message::EditorTapStamp);
                }
                if action == Shortcut::Escape {
                    if self.queue_drag.take().is_some() {
                        return Task::none();
                    }
                    if self.palette.take().is_some() {
                        return Task::none();
                    }
                    if self.editor.is_some() {
                        return self.update(Message::EditorClose);
                    }
                    if self.collection_overlay.is_some() {
                        return self.update(Message::CloseCollectionOverlay);
                    }
                    return self.update(Message::ScreenSelected(Screen::Library));
                }
                if self.collection_overlay.is_some() || self.editor.is_some() {
                    return Task::none();
                }
                if self.palette.is_some() && !matches!(action, Shortcut::Palette | Shortcut::Search)
                {
                    return Task::none();
                }
                match action {
                    Shortcut::Palette => {
                        if self.palette.take().is_none() {
                            self.hotkeys.capture = None;
                            self.palette = Some(super::palette::Palette::new(&self.snapshot));
                            return iced::widget::text_input::focus(
                                iced::widget::text_input::Id::new("muzon-palette-search"),
                            );
                        }
                    }
                    Shortcut::Search => {
                        self.palette = None;
                        if self.screen == Screen::NowPlaying {
                            self.screen = Screen::Library;
                        }
                        return iced::widget::text_input::focus(iced::widget::text_input::Id::new(
                            "muzon-search",
                        ));
                    }
                    Shortcut::Toggle => return self.update(Message::TogglePlay),
                    Shortcut::Next => return self.update(Message::Next),
                    Shortcut::Previous => return self.update(Message::Previous),
                    Shortcut::VolumeUp => {
                        return self.update(Message::VolumeChanged(
                            (self.snapshot.volume + 0.05).min(1.0),
                        ))
                    }
                    Shortcut::VolumeDown => {
                        return self.update(Message::VolumeChanged(
                            (self.snapshot.volume - 0.05).max(0.0),
                        ))
                    }
                    Shortcut::SeekForward | Shortcut::SeekBack => {
                        if let Some(core) = &self.controller {
                            let step = if action == Shortcut::SeekForward {
                                5.0
                            } else {
                                -5.0
                            };
                            core.seek(
                                (self.snapshot.position_secs + step)
                                    .clamp(0.0, self.snapshot.display_duration().max(0.0)),
                            );
                        }
                    }
                    Shortcut::Escape => {}
                }
            }
            Message::WindowOpened(id) => {
                if self.window.geometry.maximized {
                    return iced::window::maximize(id, true);
                }
            }
            Message::WindowResized(id, size) => {
                if self.hidden {
                    return Task::none();
                }
                let old_columns = self.album_grid.columns();
                self.album_grid.viewport = size;
                let reset = if old_columns != self.album_grid.columns() {
                    self.group_scroll = 0.;
                    scrollable::snap_to(
                        scrollable::Id::new("muzon-groups"),
                        scrollable::RelativeOffset::START,
                    )
                } else {
                    Task::none()
                };
                let generation = self.window.next_observation();
                return Task::batch([
                    reset,
                    iced::window::get_maximized(id)
                        .map(move |maximized| Message::WindowObserved(generation, size, maximized)),
                ]);
            }
            Message::WindowObserved(generation, size, maximized) => {
                self.window.observe(generation, size, maximized);
            }
            Message::WindowSaved(revision, result) => {
                self.window.saved(revision);
                if let Err(error) = result {
                    self.ui_error = Some(format!("Не удалось сохранить размер окна: {error}"));
                }
            }
            Message::Poll => {
                if let Some(hub) = &self.services_hub {
                    self.services.poll(hub);
                }
                let wanted = self.screen == Screen::NowPlaying
                    && !self.hidden
                    && self.appearance.prefs.spectrum
                    && self.editor.is_none()
                    && self.palette.is_none();
                if let Some(core) = &self.controller {
                    if wanted != self.spectrum_enabled {
                        core.set_spectrum(wanted);
                        self.spectrum_enabled = wanted;
                    }
                    if wanted {
                        self.spectrum = core.spectrum();
                    } else {
                        self.spectrum = Arc::new(Vec::new());
                    }
                }
                if self.screen == Screen::Smart
                    && self.smart_refresh.elapsed() >= Duration::from_secs(60)
                {
                    self.smart_refresh = std::time::Instant::now();
                    self.filter();
                }
                if let Some(tray) = self.tray.as_mut() {
                    tray.refresh(&self.snapshot);
                }
                if self.hidden && !self.tray.as_ref().is_some_and(|tray| tray.ready()) {
                    return self.show_window(false);
                }
                match self.tray.as_ref().and_then(|tray| tray.action()) {
                    Some(super::tray::Action::Show) => return self.show_window(true),
                    Some(super::tray::Action::Quit) => return self.update(Message::Quit),
                    Some(super::tray::Action::Toggle) => return self.update(Message::TogglePlay),
                    Some(super::tray::Action::Previous) => return self.update(Message::Previous),
                    Some(super::tray::Action::Next) => return self.update(Message::Next),
                    None => {}
                }
                match self.desktop.as_ref().and_then(Desktop::request) {
                    Some(WindowRequest::Quit) => return self.update(Message::Quit),
                    Some(WindowRequest::Raise) => return self.show_window(true),
                    None => {}
                }
                let reset = if self.poll() {
                    self.reset_list_scroll()
                } else {
                    Task::none()
                };
                let reset = Task::batch([
                    reset,
                    crate::real::album_grid::load(self),
                    crate::real::track_covers::load(self),
                ]);
                if let Some((store, revision, geometry)) = self.window.due() {
                    return Task::batch([
                        reset,
                        Task::perform(
                            async move {
                                tokio::task::spawn_blocking(move || store.save(revision, geometry))
                                    .await
                                    .map_err(|e| e.to_string())
                                    .and_then(|r| r)
                            },
                            move |result| Message::WindowSaved(revision, result),
                        ),
                    ]);
                }
                return reset;
            }
            Message::SearchChanged(query) => {
                self.query = query;
                self.group_scroll = 0.0;
                self.filter();
                self.library_scroll = 0.0;
                return Task::batch([
                    scrollable::snap_to(
                        scrollable::Id::new(LIBRARY_LIST),
                        scrollable::RelativeOffset::START,
                    ),
                    scrollable::snap_to(
                        scrollable::Id::new("muzon-groups"),
                        scrollable::RelativeOffset::START,
                    ),
                ]);
            }
            Message::ScreenSelected(screen) => {
                self.hotkeys.capture = None;
                self.queue_drag = None;
                self.screen = screen;
                self.album_grid.browsing = true;
                self.groups = super::groups::build(&self.snapshot.library, screen);
                self.group_scroll = 0.0;
                self.collection_overlay = None;
                if screen == Screen::Playlists && self.selected_playlist.is_none() {
                    self.select_playlist(self.snapshot.playlists.first().map(|p| p.id));
                }
                self.filter();
                self.library_scroll = 0.0;
                self.queue_scroll = 0.0;
                return Task::batch([
                    scrollable::snap_to(
                        scrollable::Id::new("muzon-sleep"),
                        scrollable::RelativeOffset::START,
                    ),
                    scrollable::snap_to(
                        scrollable::Id::new("muzon-groups"),
                        scrollable::RelativeOffset::START,
                    ),
                    scrollable::snap_to(
                        scrollable::Id::new(LIBRARY_LIST),
                        scrollable::RelativeOffset::START,
                    ),
                    scrollable::snap_to(
                        scrollable::Id::new(QUEUE_LIST),
                        scrollable::RelativeOffset::START,
                    ),
                ]);
            }
            Message::ToggleFavorite(id) => {
                self.ui_error = None;
                if !self.pending_favorites.insert(id) {
                    return Task::none();
                }
                if let Some(client) = self.controller.as_ref().map(Controller::client) {
                    return Task::perform(
                        async move {
                            let result = tokio::task::spawn_blocking(move || {
                                client.toggle_favorite(Some(id))
                            })
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|r| r)
                            .map(|_| ());
                            (id, result)
                        },
                        |(id, result)| Message::FavoriteFinished { id, result },
                    );
                }
                self.pending_favorites.remove(&id);
            }
            Message::FavoriteFinished { id, result } => {
                self.pending_favorites.remove(&id);
                if let Err(error) = result {
                    self.ui_error = Some(error);
                }
                if self.poll() {
                    return self.reset_list_scroll();
                }
            }

            Message::NewPlaylistName(name) => self.new_playlist_name = name,
            Message::RenamePlaylistName(name) => self.rename_playlist_name = name,
            Message::PlaylistSelected(id) => {
                self.select_playlist(Some(id));
                return self.reset_list_scroll();
            }
            Message::OpenPlaylistPicker(id) => {
                if !self.playlist_busy {
                    self.picker_scroll = 0.0;
                    self.collection_overlay = Some(CollectionOverlay::AddTrack(id));
                }
            }
            Message::AskDeletePlaylist(id) => {
                if !self.playlist_busy {
                    self.collection_overlay = Some(CollectionOverlay::DeletePlaylist(id));
                }
            }
            Message::CloseCollectionOverlay => {
                if !(self.playlist_busy
                    && matches!(
                        self.collection_overlay,
                        Some(CollectionOverlay::DeletePlaylist(_))
                    ))
                {
                    self.collection_overlay = None;
                }
            }
            Message::CreatePlaylist => {
                let tracks = match self.collection_overlay {
                    Some(CollectionOverlay::AddTrack(id)) => vec![id],
                    _ => Vec::new(),
                };
                return self.submit_playlist(PlaylistEdit::Create {
                    name: self.new_playlist_name.clone(),
                    tracks,
                });
            }
            Message::RenamePlaylist => {
                if let Some(id) = self.selected_playlist {
                    return self.submit_playlist(PlaylistEdit::Rename {
                        id,
                        name: self.rename_playlist_name.clone(),
                    });
                }
            }
            Message::EditPlaylist(edit) => return self.submit_playlist(edit),
            Message::PlaylistFinished(result) => {
                self.playlist_busy = false;
                match result {
                    Ok(created) => {
                        self.collection_overlay = None;
                        self.new_playlist_name.clear();
                        let reset = self.poll();
                        let selected_changed =
                            created.is_some_and(|id| self.selected_playlist != Some(id));
                        self.select_playlist(created.or(self.selected_playlist));
                        if reset || (selected_changed && self.screen == Screen::Playlists) {
                            return self.reset_list_scroll();
                        }
                    }
                    Err(error) => self.ui_error = Some(error),
                }
            }
            Message::PlaylistListScrolled(y) => self.playlist_scroll = y,
            Message::PlaylistPickerScrolled(y) => self.picker_scroll = y,
            Message::PlayCollection | Message::EnqueueCollection => {
                if let Some(core) = &self.controller {
                    let ids: Vec<_> = self
                        .filtered
                        .iter()
                        .filter_map(|&i| self.snapshot.library.get(i).map(|t| t.id))
                        .collect();
                    if matches!(message, Message::PlayCollection) {
                        if let Some(&first) = ids.first() {
                            core.set_queue(ids, first);
                        }
                    } else {
                        core.enqueue(ids);
                    }
                }
            }
            Message::AudioChanged(edit) => self.edit_audio(edit),
            Message::AudioPreset(index) => {
                if let Some((_, gains)) = PRESETS.get(index) {
                    self.edit_audio(
                        muzon_core::domain::playback_settings::PlaybackEdit::Equalizer(*gains),
                    );
                    return self.save_audio();
                }
            }
            Message::ResetAudioEffects => {
                self.edit_audio(muzon_core::domain::playback_settings::PlaybackEdit::Reset);
                return self.save_audio();
            }
            Message::TrackTempoChanged { id, rate } => {
                let current = self.snapshot.current_track().is_some_and(|t| t.id == id);
                if !current || self.audio.preview_tempo(id, rate) {
                    if let Some(core) = &self.controller {
                        core.preview_track_tempo(id, rate);
                    }
                }
            }
            Message::ResetTrackTempo(id) => {
                self.audio.tempo_target = None;
                let _ = self.audio.preview_tempo(id, 1.0);
                if let Some(core) = &self.controller {
                    core.preview_track_tempo(id, 1.0);
                }
                return self.save_audio();
            }
            Message::AudioSave => return self.save_audio(),
            Message::AudioSaved(result) => {
                self.audio.saving = false;
                let _ = self.poll();
                // Store errors live in the authoritative snapshot, so a later
                // successful autosave also clears the visible warning.
                if let Err(error) = result {
                    if self.snapshot.audio_error.is_none() {
                        self.ui_error = Some(error);
                    }
                }
            }
            Message::LibraryScrolled(y) => {
                self.library_scroll = y;
                return crate::real::track_covers::load(self);
            }
            Message::TrackCovers(epoch, loaded) => {
                self.track_covers.finish(epoch, loaded);
                return crate::real::track_covers::load(self);
            }
            Message::PickFolder => {
                if !self.folder_picker_open {
                    self.folder_picker_open = true;
                    return Task::perform(
                        async {
                            rfd::AsyncFileDialog::new()
                                .set_title("Выберите папку с музыкой")
                                .pick_folder()
                                .await
                                .map(|file| file.path().to_path_buf())
                        },
                        Message::FolderPicked,
                    );
                }
            }
            Message::FolderPicked(path) => {
                self.folder_picker_open = false;
                if let Some(path) = path {
                    if path.is_dir() {
                        self.folder_input = path.to_string_lossy().into_owned();
                        self.ui_error = None;
                        if let Some(core) = &self.controller {
                            core.add_folder(path);
                        }
                    } else {
                        self.ui_error = Some("Выбранная папка больше не доступна".into());
                    }
                }
            }
            Message::QueueScrolled(y) => self.queue_scroll = y,
            Message::FolderInputChanged(value) => self.folder_input = value,
            Message::SeekChanged(value) => self.seek_preview = Some(value),
            Message::Quit => return iced::window::get_latest().map(Message::QuitRequested),
            Message::DiscardAudioAndQuit => {
                self.closing = true;
                self.audio_close_failed = false;
                return iced::window::get_latest()
                    .map(|id| Message::ShutdownAudioSaved(id, Ok(())));
            }
            other => {
                if let Some(core) = &self.controller {
                    match other {
                        Message::TrackRowClicked(row) => {
                            if let Some(id) = filtered_track_id(&self.snapshot, &self.filtered, row)
                            {
                                core.set_queue(
                                    self.filtered
                                        .iter()
                                        .filter_map(|&i| self.snapshot.library.get(i).map(|t| t.id))
                                        .collect(),
                                    id,
                                );
                            }
                        }
                        Message::EnqueueRow(row) => {
                            if let Some(id) = filtered_track_id(&self.snapshot, &self.filtered, row)
                            {
                                core.enqueue(vec![id]);
                            }
                        }
                        Message::QueueRowClicked(row) => {
                            if let Some(index) = displayed_queue_index(&self.snapshot, row) {
                                core.play_queue_index(index);
                            }
                        }
                        Message::QueueMove(from, to) => {
                            if let Some(core) = &self.controller {
                                core.move_queue(from, to);
                            }
                        }
                        Message::QueueRemove(row) => {
                            if let Some(index) = displayed_queue_index(&self.snapshot, row) {
                                core.remove_queue_index(index);
                            }
                        }
                        Message::TogglePlay => core.toggle_play(),
                        Message::Next => core.next(),
                        Message::Previous => core.previous(),
                        Message::Stop => core.stop(),
                        Message::SeekCommit => {
                            if let Some(position) = self.seek_preview.take() {
                                core.seek(position);
                            }
                        }
                        Message::VolumeChanged(volume) => core.set_volume(volume),
                        Message::ToggleShuffle => core.toggle_shuffle(),
                        Message::CycleRepeat => core.cycle_repeat(),
                        Message::AddFolder => {
                            let path = std::path::PathBuf::from(self.folder_input.trim());
                            if !self.folder_input.trim().is_empty() && path.is_dir() {
                                self.ui_error = None;
                                core.add_folder(path);
                            } else {
                                self.ui_error = Some("Укажите существующую папку с музыкой".into());
                            }
                        }
                        Message::Rescan => core.rescan(),
                        _ => {}
                    }
                }
            }
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app_fixture() -> RealApp {
        let mut snapshot = Snapshot::empty();
        let mut tracks = vec![track(42), track(99), track(7)];
        tracks[1].is_favorite = true;
        tracks[2].is_favorite = true;
        snapshot.library = Arc::new(tracks);
        snapshot.playlists = Arc::new(vec![muzon_core::domain::playlist::Playlist {
            id: 10,
            name: "Mix".into(),
            track_ids: vec![7, 42, 123],
        }]);
        let mut app = RealApp {
            controller: None,
            desktop: None,
            tray: None,
            hidden: false,
            _profile_lock: File::open("/dev/null").unwrap(),
            window: crate::real::window_state::WindowMemory::new(Path::new(
                "/does-not-exist/muzon-test",
            )),
            audio: AudioUi::new(&snapshot),
            services: crate::real::services::Services::new(None, None),
            services_hub: None,
            hotkeys: crate::real::hotkeys::Hotkeys::new(Path::new("/does-not-exist/muzon-test")),
            appearance: crate::real::appearance::Appearance::new(Path::new(
                "/does-not-exist/muzon-test",
            )),
            spectrum: Arc::new(Vec::new()),
            spectrum_enabled: false,
            editor: None,
            palette: None,
            queue_drag: None,
            editor_generation: 0,
            held_keys: HashSet::new(),
            snapshot: Arc::new(snapshot),
            media: Arc::new(Media::default()),
            cover: None,
            screen: Screen::Library,
            query: String::new(),
            smart: crate::real::smart::Smart::MostPlayed,
            smart_refresh: std::time::Instant::now(),
            album_grid: crate::real::album_grid::Grid::new(std::path::PathBuf::new()),
            groups: Vec::new(),
            group_rows: Vec::new(),
            selected_group: None,
            group_scroll: 0.0,
            filtered: Vec::new(),
            search_index: Vec::new(),
            library_by_id: HashMap::new(),
            selected_playlist: None,
            new_playlist_name: String::new(),
            rename_playlist_name: String::new(),
            collection_overlay: None,
            playlist_busy: false,
            pending_favorites: HashSet::new(),
            playlist_scroll: 0.0,
            picker_scroll: 0.0,
            folder_input: String::new(),
            folder_picker_open: false,
            track_covers: crate::real::track_covers::Covers::new(std::path::PathBuf::new()),
            library_scroll: 0.0,
            queue_scroll: 0.0,
            seek_preview: None,
            closing: false,
            audio_close_failed: false,
            ui_error: None,
        };
        app.rebuild_search();
        app
    }
    #[test]
    fn a_failed_sound_save_cancels_close_and_keeps_the_worker_available() {
        use muzon_core::{
            domain::playback_settings::PlaybackEdit,
            testing::{gst_lock, scratch_dir},
        };
        let _gst = gst_lock();
        let dir = scratch_dir("native-ui-close-audio-save");
        std::fs::create_dir_all(dir.join("config/playback_settings.json")).unwrap();
        let core = Controller::spawn_test(CoreConfig {
            db: Arc::new(Db::open(&dir.join("library.sqlite3")).unwrap()),
            session_store: SessionStore::new(&dir.join("config")),
            playback_settings_store: PlaybackSettingsStore::new(&dir.join("config")),
            cover_dir: dir.join("covers"),
        })
        .unwrap();
        core.preview_playback(PlaybackEdit::Crossfade(3.0));
        let mut app = app_fixture();
        app.controller = Some(core);
        let _ = app.update(Message::WindowCloseRequested(None));
        assert!(app.closing);
        let result = app.controller.as_ref().unwrap().save_audio();
        assert!(result.is_err());
        let _ = app.update(Message::ShutdownAudioSaved(None, result));
        assert!(!app.closing);
        assert!(app.audio_close_failed);
        assert!(app.controller.as_ref().unwrap().is_running());
        assert!(app.snapshot.audio_dirty);
        assert!(app.ui_error.is_some());
        std::fs::remove_dir(dir.join("config/playback_settings.json")).unwrap();
        let _ = app.update(Message::AudioSave);
        let result = app.controller.as_ref().unwrap().save_audio();
        assert!(result.is_ok());
        let _ = app.update(Message::AudioSaved(result));
        assert!(app.ui_error.is_none());
        assert!(!app.snapshot.audio_dirty);
        assert!(!app.audio_close_failed);
    }

    #[test]
    fn favorite_and_playlist_views_preserve_identity_order_and_the_playing_queue() {
        let mut app = app_fixture();
        let queue = app.snapshot.queue.clone();
        let _ = app.update(Message::ScreenSelected(Screen::Favorites));
        assert_eq!(app.filtered, vec![1, 2]);
        let _ = app.update(Message::SearchChanged("99".into()));
        assert_eq!(app.filtered, vec![1]);
        let _ = app.update(Message::ScreenSelected(Screen::Playlists));
        assert!(
            app.filtered.is_empty(),
            "search remains active in the selected playlist"
        );
        let _ = app.update(Message::SearchChanged(String::new()));
        assert_eq!(
            app.filtered,
            vec![2, 0],
            "membership is mapped by id, missing tracks are omitted"
        );
        assert_eq!(filtered_track_id(&app.snapshot, &app.filtered, 0), Some(7));
        assert!(Arc::ptr_eq(&queue, &app.snapshot.queue));
        let _ = app.update(Message::PlaylistSelected(999));
        assert!(app.filtered.is_empty());
    }
    #[test]
    fn collection_screens_and_modal_lists_build_without_scroll_axis_fill() {
        let mut app = app_fixture();
        for screen in [
            Screen::Library,
            Screen::Favorites,
            Screen::Playlists,
            Screen::Audio,
            Screen::Sleep,
            Screen::Shortcuts,
            Screen::Appearance,
            Screen::Cloud,
            Screen::Scrobble,
            Screen::Smart,
            Screen::Albums,
            Screen::Artists,
            Screen::Genres,
        ] {
            let _ = app.update(Message::ScreenSelected(screen));
            let _ = crate::real::view::view(&app);
        }
        for overlay in [
            CollectionOverlay::AddTrack(42),
            CollectionOverlay::DeletePlaylist(10),
        ] {
            app.collection_overlay = Some(overlay);
            let _ = crate::real::view::view(&app);
        }
        let _ = app.update(Message::CloseCollectionOverlay);
        assert!(app.collection_overlay.is_none());
        let _ = app.update(Message::AskDeletePlaylist(10));
        app.playlist_busy = true;
        let _ = app.update(Message::CloseCollectionOverlay);
        assert!(
            app.collection_overlay.is_some(),
            "an in-flight deletion cannot be presented as cancelled"
        );
    }

    fn track(id: i32) -> muzon_core::domain::Track {
        muzon_core::domain::Track {
            id,
            path: format!("/{id}.wav"),
            title: format!("Track {id}"),
            artist: None,
            album: None,
            duration_secs: Some(10.0),
            track_no: None,
            genre: None,
            year: None,
            is_favorite: false,
            tempo: 1.0,
            play_count: 0,
            last_played_at: None,
            added_at: 0.0,
        }
    }
    #[test]
    fn batch_palette_smart_lists_shortcuts_and_online_results_are_isolated() {
        let mut app = app_fixture();
        {
            let snapshot = Arc::make_mut(&mut app.snapshot);
            let library = Arc::make_mut(&mut snapshot.library);
            library[0].play_count = 4;
            library[0].last_played_at = Some(1.);
            library[1].play_count = 0;
            library[2].play_count = 2;
        }
        app.rebuild_search();
        let queue = app.snapshot.queue.clone();
        let _ = app.update(Message::ScreenSelected(Screen::Smart));
        assert_eq!(app.filtered, vec![0, 2]);
        let _ = app.update(Message::SmartSelected(
            crate::real::smart::Smart::NeverPlayed,
        ));
        assert_eq!(app.filtered, vec![1]);
        assert!(Arc::ptr_eq(&queue, &app.snapshot.queue));
        let _ = app.update(Message::Shortcut(crate::real::shortcuts::Shortcut::Palette));
        assert!(app.palette.is_some());
        let _ = app.update(Message::PaletteQuery("таймер сна".into()));
        let _ = app.update(Message::PaletteExecute);
        assert_eq!(app.screen, Screen::Sleep);
        let mut editor = crate::real::editor::Editor::new(
            5,
            muzon_core::editing::EditorSource {
                track: track(42),
                lyrics: "my draft".into(),
                lyrics_in_tag: false,
            },
        );
        editor.loading = false;
        editor.lyrics_dirty = true;
        editor.fetch_serial = 2;
        let (_, handle) = Task::<Message>::future(std::future::pending()).abortable();
        let observer = handle.clone();
        editor.fetch = Some(handle.abort_on_drop());
        app.editor = Some(editor);
        let _ = app.update(Message::EditorFetched(
            5,
            1,
            Ok(Some(muzon_core::data::lrclib::Suggestion {
                text: "stale".into(),
                synced: false,
            })),
        ));
        assert!(app.editor.as_ref().unwrap().suggestion.is_none());
        let _ = app.update(Message::EditorFetched(
            5,
            2,
            Ok(Some(muzon_core::data::lrclib::Suggestion {
                text: "found".into(),
                synced: false,
            })),
        ));
        assert_eq!(
            app.editor.as_ref().unwrap().lyrics.text().trim(),
            "my draft"
        );
        assert!(observer.is_aborted());
        let _ = crate::real::view::view(&app);
        let _ = app.update(Message::EditorUseSuggestion);
        assert_eq!(app.editor.as_ref().unwrap().lyrics.text().trim(), "found");
        let _ = app.update(Message::EditorTapStart);
        assert!(app.editor.as_ref().unwrap().tap.is_some());
        let _ = crate::real::view::view(&app);
        let _ = app.update(Message::EditorTapCancel);
        assert_eq!(app.editor.as_ref().unwrap().lyrics.text().trim(), "found");
    }

    #[test]
    fn editor_protects_drafts_and_ignores_stale_async_answers() {
        let mut app = app_fixture();
        let mut editor = crate::real::editor::Editor::new(
            2,
            muzon_core::editing::EditorSource {
                track: track(42),
                lyrics: "[00:00]Original".into(),
                lyrics_in_tag: false,
            },
        );
        editor.loading = false;
        app.editor = Some(editor);
        let _ = app.update(Message::EditorField(0, "New title".into()));
        let _ = app.update(Message::EditorClose);
        assert!(app.editor.as_ref().unwrap().discard);
        let _ = app.update(Message::QuitRequested(None));
        assert!(!app.closing);
        assert!(app.editor.is_some());
        let _ = app.update(Message::EditorLoaded(1, Err("stale".into())));
        assert_eq!(app.editor.as_ref().unwrap().fields[0], "New title");
        let _ = app.update(Message::EditorKeep);
        assert!(!app.editor.as_ref().unwrap().discard);
        let _ = crate::real::view::view(&app);
        let _ = app.update(Message::EditorTab(crate::real::editor::Tab::Lyrics));
        let _ = crate::real::view::view(&app);
        let editor = app.editor.as_mut().unwrap();
        editor.saving = true;
        let _ = app.update(Message::EditorDiscard);
        assert!(app.editor.is_some());
        let _ = app.update(Message::EditorSaved(
            2,
            crate::real::editor::Tab::Tags,
            Err("disk full".into()),
        ));
        assert!(app.editor.as_ref().unwrap().tags_dirty);
        assert_eq!(app.editor.as_ref().unwrap().fields[0], "New title");
        let _ = app.update(Message::EditorDiscard);
        assert!(app.editor.is_none());
    }

    #[test]
    fn grouping_and_search_preserve_queue_and_resolve_original_tracks() {
        let mut app = app_fixture();
        let queue = app.snapshot.queue.clone();
        let _ = app.update(Message::ScreenSelected(Screen::Albums));
        assert_eq!(app.groups.len(), 1);
        let _ = app.update(Message::SearchChanged("99".into()));
        assert_eq!(app.filtered, vec![1]);
        assert_eq!(filtered_track_id(&app.snapshot, &app.filtered, 0), Some(99));
        assert!(Arc::ptr_eq(&queue, &app.snapshot.queue));
        let _ = app.update(Message::GroupSelected(usize::MAX));
        assert_eq!(app.filtered, vec![1]);
    }

    #[test]
    fn search_rows_resolve_to_library_ids_not_row_numbers() {
        let mut state = Snapshot::empty();
        state.library = Arc::new(vec![track(42), track(99), track(7)]);
        assert_eq!(filtered_track_id(&state, &[2, 0], 0), Some(7));
        assert_eq!(filtered_track_id(&state, &[2, 0], 1), Some(42));
        assert_eq!(filtered_track_id(&state, &[2], 5), None);
        assert_eq!(filtered_track_id(&state, &[99], 0), None);
    }
    #[test]
    fn duplicate_queue_entries_are_addressed_by_occurrence() {
        let mut state = Snapshot::empty();
        state.queue = Arc::new(vec![track(42), track(42), track(7)]);
        state.display = Arc::new(vec![1, 2, 0]);
        assert_eq!(displayed_queue_index(&state, 0), Some(1));
        assert_eq!(displayed_queue_index(&state, 2), Some(0));
        assert_eq!(displayed_queue_index(&state, 3), None);
    }
    #[test]
    fn album_browsing_resolves_stable_keys_and_never_changes_queue() {
        let mut app = app_fixture();
        let queue = app.snapshot.queue.clone();
        let _ = app.update(Message::ScreenSelected(Screen::Albums));
        assert!(app.album_grid.browsing);
        let key = app.groups[0].key.clone();
        let _ = app.update(Message::AlbumSelected(key.clone()));
        assert!(!app.album_grid.browsing);
        assert_eq!(app.selected_group, Some(key.clone()));
        let _ = app.update(Message::AlbumSelected(("missing".into(), "missing".into())));
        assert_eq!(app.selected_group, Some(key));
        let _ = app.update(Message::AlbumsBack);
        assert!(app.album_grid.browsing);
        assert!(Arc::ptr_eq(&queue, &app.snapshot.queue));
        let _ = crate::real::album_grid::view(&app);
    }
    #[test]
    fn folder_cancel_preserves_input_and_invalid_choice_reports_error() {
        let mut app = app_fixture();
        app.folder_input = "/keep".into();
        app.folder_picker_open = true;
        let _ = app.update(Message::FolderPicked(None));
        assert!(!app.folder_picker_open);
        assert_eq!(app.folder_input, "/keep");
        let _ = app.update(Message::FolderPicked(Some(
            "/does-not-exist/muzon-folder".into(),
        )));
        assert!(app.ui_error.is_some());
        assert_eq!(app.folder_input, "/keep");
    }
    #[test]
    fn every_settings_category_can_build_and_keeps_playback_queue() {
        let mut app = app_fixture();
        let queue = app.snapshot.queue.clone();
        for (screen, _) in crate::real::settings::CATEGORIES {
            let _ = app.update(Message::ScreenSelected(screen));
            let _ = crate::real::settings::view(&app);
            assert!(Arc::ptr_eq(&queue, &app.snapshot.queue));
        }
    }
}
