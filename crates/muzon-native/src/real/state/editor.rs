//! Tag and lyrics editor, including generation-checked async results.
use super::*;

impl RealApp {
    pub(super) fn update_editor(&mut self, message: Message) -> Task<Message> {
        match message {
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
                self.editor = Some(crate::real::editor::Editor::new(
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
                            *editor = crate::real::editor::Editor::new(generation, source);
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
                    match crate::real::editor::TapDraft::new(&editor.lyrics.text()) {
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
                                crate::real::editor::timestamp(self.snapshot.position_secs),
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
                            if tab == crate::real::editor::Tab::Tags {
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
            _ => unreachable!("message routed to wrong state subsystem"),
        }
        Task::none()
    }
}
