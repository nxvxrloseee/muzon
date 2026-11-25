//! Account and cloud actions with cooperative cancellation.
use super::*;

impl RealApp {
    pub(super) fn update_services(&mut self, message: Message) -> Task<Message> {
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
            _ => unreachable!("message routed to wrong state subsystem"),
        }
        Task::none()
    }
}
