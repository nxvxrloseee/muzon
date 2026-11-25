//! Library browsing, collections, artwork and folder selection.
use super::*;

/// Filtering names never become queue positions: resolve a click to a stable id.
pub fn filtered_track_id(snapshot: &Snapshot, filtered: &[usize], row: usize) -> Option<i32> {
    filtered
        .get(row)
        .and_then(|&i| snapshot.library.get(i))
        .map(|t| t.id)
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
    pub(super) fn rebuild_search(&mut self) {
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
        self.groups = crate::real::groups::build(&self.snapshot.library, self.screen);
        self.filter();
    }

    pub(super) fn filter(&mut self) {
        let query = self.query.trim().to_lowercase();
        if crate::real::groups::is_grouped(self.screen) {
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
            self.filtered = crate::real::smart::select(&self.snapshot.library, self.smart, now)
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

    pub(super) fn reset_list_scroll(&mut self) -> Task<Message> {
        self.library_scroll = 0.0;
        scrollable::snap_to(
            scrollable::Id::new(LIBRARY_LIST),
            scrollable::RelativeOffset::START,
        )
    }

    pub(super) fn submit_playlist(&mut self, edit: PlaylistEdit) -> Task<Message> {
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

    pub(super) fn select_playlist(&mut self, id: Option<i32>) {
        self.selected_playlist = id;
        self.rename_playlist_name = id
            .and_then(|id| self.snapshot.playlists.iter().find(|p| p.id == id))
            .map(|p| p.name.clone())
            .unwrap_or_default();
        self.filter();
    }

    pub(super) fn update_library(&mut self, message: Message) -> Task<Message> {
        match message {
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
            Message::PlayAlbum(key) => {
                if let (Some(core), Some(group)) =
                    (&self.controller, self.groups.iter().find(|g| g.key == key))
                {
                    let ids: Vec<_> = group
                        .indices
                        .iter()
                        .filter_map(|&i| self.snapshot.library.get(i).map(|t| t.id))
                        .collect();
                    if let Some(&first) = ids.first() {
                        core.set_queue(ids, first);
                    }
                }
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
                self.groups = crate::real::groups::build(&self.snapshot.library, screen);
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
            Message::FolderInputChanged(value) => self.folder_input = value,
            Message::AddFolder => {
                if let Some(core) = &self.controller {
                    let path = std::path::PathBuf::from(self.folder_input.trim());
                    if !self.folder_input.trim().is_empty() && path.is_dir() {
                        self.ui_error = None;
                        core.add_folder(path);
                    } else {
                        self.ui_error = Some("Укажите существующую папку с музыкой".into());
                    }
                }
            }
            Message::Rescan => {
                if let Some(core) = &self.controller {
                    core.rescan();
                }
            }
            _ => unreachable!("message routed to wrong state subsystem"),
        }
        Task::none()
    }
}
