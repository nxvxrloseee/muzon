//! Audio and appearance drafts and persistence.
use super::*;

impl RealApp {
    pub(super) fn save_audio(&mut self) -> Task<Message> {
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

    pub(super) fn edit_audio(&mut self, edit: muzon_core::domain::playback_settings::PlaybackEdit) {
        match self.audio.preview(&edit) {
            Ok(()) => {
                if let Some(core) = &self.controller {
                    core.preview_playback(edit);
                }
            }
            Err(error) => self.ui_error = Some(error),
        }
    }

    pub(super) fn update_settings(&mut self, message: Message) -> Task<Message> {
        match message {
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
            Message::SetSleep(choice) => {
                if let Some(core) = &self.controller {
                    core.set_sleep(choice);
                }
            }
            Message::AudioDeviceInput(value) => self.audio.output_input = value,
            Message::AudioDeviceApply => {
                return self.update_settings(Message::AudioChanged(
                    muzon_core::domain::playback_settings::PlaybackEdit::OutputDevice(Some(
                        self.audio.output_input.trim().to_owned(),
                    )),
                ));
            }
            Message::RefreshAudioDevices => {
                if self.audio.devices_refreshing {
                    return Task::none();
                }
                let Some(client) = self.controller.as_ref().map(Controller::client) else {
                    return Task::none();
                };
                self.audio.devices_refreshing = true;
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            client.refresh_output_devices();
                            let snapshot = wait_for_audio_ack(&client)?;
                            match &snapshot.audio_error {
                                Some(error) if error.starts_with("Audio devices:") => {
                                    Err(error.clone())
                                }
                                _ => Ok(()),
                            }
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|result| result)
                    },
                    Message::AudioDevicesLoaded,
                );
            }
            Message::AudioDevicesLoaded(result) => {
                self.audio.devices_refreshing = false;
                self.audio.devices_refreshed = Some(std::time::Instant::now());
                self.audio.devices_error = result.err();
                let _ = self.poll();
            }
            Message::AudioChanged(edit) => {
                let routing = matches!(
                    edit,
                    muzon_core::domain::playback_settings::PlaybackEdit::OutputMode(_)
                        | muzon_core::domain::playback_settings::PlaybackEdit::OutputDevice(_)
                );
                if routing {
                    if self.audio.saving {
                        return Task::none();
                    }
                    let Some(client) = self.controller.as_ref().map(Controller::client) else {
                        return Task::none();
                    };
                    if let Err(error) = self.audio.preview(&edit) {
                        self.ui_error = Some(error);
                        return Task::none();
                    }
                    let expected = self.audio.draft.clone();
                    self.audio.output_changing = true;
                    self.audio.saving = true;
                    self.ui_error = None;
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                client.preview_playback(edit);
                                let snapshot = wait_for_audio_ack(&client)?;
                                if let Some(error) = &snapshot.audio_error {
                                    return Err(error.clone());
                                }
                                if snapshot.playback.output_mode != expected.output_mode
                                    || snapshot.playback.output_device != expected.output_device
                                {
                                    return Err("Аудиовыход не применён ядром".to_owned());
                                }
                                match client.save_audio() {
                                    Err(error) if ack_timed_out(&error) => {
                                        // The save remains queued; await its completion rather
                                        // than reporting a failure or submitting it twice.
                                        let snapshot = wait_for_audio_ack(&client)?;
                                        if let Some(error) = &snapshot.audio_error {
                                            Err(error.clone())
                                        } else if snapshot.audio_dirty {
                                            Err("Сохранение аудиовыхода не подтверждено".into())
                                        } else {
                                            Ok(())
                                        }
                                    }
                                    result => result,
                                }
                            })
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|result| result)
                        },
                        Message::AudioSaved,
                    );
                }
                self.edit_audio(edit);
            }
            Message::AudioPreset(index) => {
                if let Some((_, gains)) = PRESETS.get(index) {
                    self.edit_audio(
                        muzon_core::domain::playback_settings::PlaybackEdit::Equalizer(*gains),
                    );
                    return self.save_audio();
                }
            }
            Message::ResetAudioEffects => {
                use muzon_core::domain::playback_settings::{PlaybackEdit, ReplayGainMode};
                // Reset DSP only: a selected output route must survive this button.
                for edit in [
                    PlaybackEdit::Equalizer([0.; 10]),
                    PlaybackEdit::Crossfade(0.),
                    PlaybackEdit::ReplayGainMode(ReplayGainMode::Off),
                    PlaybackEdit::Preamp(0.),
                ] {
                    self.edit_audio(edit);
                }
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
                if self.audio.output_changing {
                    self.audio
                        .finish_output(&self.snapshot, result.as_ref().err().cloned());
                }
                // Store errors live in the authoritative snapshot, so a later
                // successful autosave also clears the visible warning.
                if let Err(error) = result {
                    if self.snapshot.audio_error.is_none() {
                        self.ui_error = Some(error);
                    }
                }
            }
            _ => unreachable!("message routed to wrong state subsystem"),
        }
        Task::none()
    }
}

// A timed-out barrier does not cancel the command preceding it. Keep the route
// controls frozen and await another ordered acknowledgment; never resubmit the
// actual output edit. A stopped worker still returns its disconnection error.
fn wait_for_audio_ack(
    client: &muzon_core::controller::ControllerClient,
) -> Result<std::sync::Arc<muzon_core::controller::Snapshot>, String> {
    loop {
        match client.barrier() {
            Err(error) if ack_timed_out(&error) => continue,
            result => return result,
        }
    }
}

fn ack_timed_out(error: &str) -> bool {
    error == std::sync::mpsc::RecvTimeoutError::Timeout.to_string()
}
