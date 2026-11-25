//! Startup, polling, window lifecycle and ordered shutdown.
use super::*;

impl RealApp {
    pub fn open(profile: &Path) -> anyhow::Result<Option<Self>> {
        let Some(lock) = crate::real::desktop::instance::claim(profile)? else {
            return Ok(None);
        };
        let (tx, requests) = std::sync::mpsc::sync_channel(16);
        let instance = crate::real::desktop::instance::InstanceServer::start(profile, tx.clone())?;
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
            tray: crate::real::tray::Tray::new().unwrap_or_else(|e| {
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

    pub(super) fn request_current_media(&self) {
        if let (Some(core), Some(path)) = (&self.controller, &self.snapshot.current_path) {
            core.request_media(path.clone());
        }
    }

    pub(super) fn poll(&mut self) -> bool {
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
            let mut palette = crate::real::palette::Palette::new(&self.snapshot);
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

    pub(super) fn show_window(&mut self, focus: bool) -> Task<Message> {
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
        // Cancellation is cooperative: keep processing completions, but do not
        // accept new drafts or work while an explicit quit waits for the service.
        if self.services.pending_quit
            && !matches!(
                message,
                Message::ServiceFinished(..)
                    | Message::ServiceCancel
                    | Message::Poll
                    | Message::WindowFocusLost
                    | Message::KeyReleased(..)
                    | Message::WindowObserved(..)
                    | Message::WindowSaved(..)
                    | Message::AppearanceSaved(..)
                    | Message::AppearanceThemeLoaded(..)
                    | Message::HotkeysSaved(..)
                    | Message::AudioSaved(..)
                    | Message::PlaylistFinished(..)
                    | Message::FavoriteFinished { .. }
                    | Message::EditorLoaded(..)
                    | Message::EditorSaved(..)
                    | Message::EditorFetched(..)
                    | Message::AlbumCovers(..)
                    | Message::TrackCovers(..)
                    | Message::ServiceBrowser(..)
            )
        {
            return Task::none();
        }
        if self.editor.is_some()
            && matches!(
                message,
                Message::WindowCloseRequested(_)
                    | Message::QuitRequested(_)
                    | Message::DiscardAudioAndQuit
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
            let skip_audio = std::mem::take(&mut self.services.pending_discard);
            let settings = CloseSettings {
                audio: self
                    .controller
                    .as_ref()
                    .filter(|core| !skip_audio && core.is_running())
                    .map(Controller::client),
                hotkeys: self.hotkeys.flush(),
                appearance: self.appearance.flush(),
            };
            return Task::perform(
                async move {
                    let result = tokio::task::spawn_blocking(move || settings.save())
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
            if let Some(desktop) = &self.desktop {
                desktop.request_shutdown();
            }
            let tray_worker = self
                .tray
                .as_mut()
                .and_then(crate::real::tray::Tray::stop_worker);
            self.tray.take();
            let window_flush = self.window.flush();
            let desktop = self.desktop.take();
            let services_hub = self.services_hub.take();
            if let Some(mut core) = self.controller.take() {
                return Task::perform(
                    async move {
                        let result = tokio::task::spawn_blocking(move || {
                            // Core shutdown records final progress/listens and flushes the
                            // session before services drain the durable scrobble outbox.
                            // Integration joins must never delay the audio stop.
                            core.shutdown();
                            let (store, revision, geometry) = window_flush;
                            if let Err(error) = store.save(revision, geometry) {
                                eprintln!("Muzon window settings: {error}");
                            }
                            if let Some(hub) = services_hub {
                                hub.shutdown();
                            }
                            drop(desktop);
                            if let Some(worker) = tray_worker {
                                let _ = worker.join();
                            }
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
        self.dispatch(message)
    }

    pub(super) fn update_lifecycle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowFocusLost => {
                self.queue_drag = None;
                self.hotkeys.capture = None;
                self.held_keys.clear();
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
                    Some(crate::real::tray::Action::Show) => return self.show_window(true),
                    Some(crate::real::tray::Action::Quit) => return self.update(Message::Quit),
                    Some(crate::real::tray::Action::Toggle) => {
                        return self.update(Message::TogglePlay)
                    }
                    Some(crate::real::tray::Action::Previous) => {
                        return self.update(Message::Previous)
                    }
                    Some(crate::real::tray::Action::Next) => return self.update(Message::Next),
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
            Message::Quit => return iced::window::get_latest().map(Message::QuitRequested),
            Message::DiscardAudioAndQuit => {
                self.services.pending_discard = true;
                return iced::window::get_latest().map(Message::QuitRequested);
            }
            _ => unreachable!("message routed to wrong state subsystem"),
        }
        Task::none()
    }
}

// Shared by normal and audio-discard exits. Always attempt unrelated saves,
// including when the audio save fails and the user must retry or discard it.
struct CloseSettings {
    audio: Option<muzon_core::controller::ControllerClient>,
    hotkeys: (crate::real::hotkeys::Store, u64, [String; 8]),
    appearance: (crate::real::preferences::Store, u64, serde_json::Value),
}
impl CloseSettings {
    fn save(self) -> Result<(), String> {
        let audio = self
            .audio
            .map(|client| client.save_audio())
            .unwrap_or(Ok(()));
        let (store, revision, values) = self.hotkeys;
        let hotkeys = store.save(revision, &values);
        let (store, revision, value) = self.appearance;
        let appearance = store.save(revision, &value);
        audio.and(hotkeys).and(appearance)
    }
}

#[cfg(test)]
#[path = "shutdown_tests.rs"]
mod shutdown_tests;
