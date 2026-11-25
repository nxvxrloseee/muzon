//! Keyboard shortcuts, command palette and focus state.
use super::*;

impl RealApp {
    pub(super) fn update_input(&mut self, message: Message) -> Task<Message> {
        match message {
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
                    use crate::real::palette::Action;
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
                if key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab)
                    && !modifiers.control()
                    && !modifiers.alt()
                    && !modifiers.logo()
                {
                    return if modifiers.shift() {
                        iced::widget::focus_previous()
                    } else {
                        iced::widget::focus_next()
                    };
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
                            crate::real::shortcuts::Shortcut::Toggle
                                | crate::real::shortcuts::Shortcut::Next
                                | crate::real::shortcuts::Shortcut::Previous
                                | crate::real::shortcuts::Shortcut::Palette
                        )
                    {
                        return Task::none();
                    }
                    if ignored
                        || matches!(
                            action,
                            crate::real::shortcuts::Shortcut::Search
                                | crate::real::shortcuts::Shortcut::Palette
                                | crate::real::shortcuts::Shortcut::Escape
                        )
                    {
                        return self.update(Message::Shortcut(action));
                    }
                }
            }
            Message::Shortcut(action) => {
                use crate::real::shortcuts::Shortcut;
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
                            self.palette = Some(crate::real::palette::Palette::new(&self.snapshot));
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
            _ => unreachable!("message routed to wrong state subsystem"),
        }
        Task::none()
    }
}
