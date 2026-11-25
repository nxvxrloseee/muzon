//! UI drafts for explicit account/storage actions. No credentials are serialized here.
use super::{message::Message, state::RealApp, style as st};
use iced::{
    widget::{button, checkbox, column, container, progress_bar, row, text, text_input},
    Element, Length, Task,
};
use muzon_services::{
    data::s3::sync::{SyncOutcome, SyncProgress},
    domain::{
        scrobble::{ScrobbleStatus, ServiceStatus},
        S3Config,
    },
    hub::{CancellationToken, Hub},
    Secret,
};
use std::sync::Arc;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    SaveConfig,
    SaveKeys,
    ForgetKeys,
    Check,
    Sync,
    ConnectListenBrainz,
    DisconnectListenBrainz,
    BeginLastFm,
    FinishLastFm,
    DisconnectLastFm,
    Retry,
}
#[derive(Debug, Clone)]
pub enum Reply {
    Done(String),
    AuthUrl(Secret),
    Synced(SyncOutcome),
    Cancelled,
}
pub struct Services {
    pub available: bool,
    pub config: S3Config,
    pub access: Secret,
    pub secret: Secret,
    pub token: Secret,
    pub lastfm_key: Secret,
    pub lastfm_secret: Secret,
    pub status: ScrobbleStatus,
    pub progress: Option<SyncProgress>,
    pub error: Option<String>,
    pub message: Option<String>,
    pub auth_url: Option<Secret>,
    pub busy: Option<Action>,
    pub generation: u64,
    pub cancel: Option<CancellationToken>,
    pub pending_quit: bool,
    pub pending_discard: bool,
}
impl Services {
    pub fn new(hub: Option<&Hub>, error: Option<String>) -> Self {
        Self {
            available: hub.is_some(),
            config: hub.map(Hub::config).unwrap_or_default(),
            access: Secret(String::new()),
            secret: Secret(String::new()),
            token: Secret(String::new()),
            lastfm_key: Secret(String::new()),
            lastfm_secret: Secret(String::new()),
            status: hub.map(Hub::status).unwrap_or_default(),
            progress: None,
            error,
            message: None,
            auth_url: None,
            busy: None,
            generation: 0,
            cancel: None,
            pending_quit: false,
            pending_discard: false,
        }
    }
    pub fn poll(&mut self, hub: &Hub) {
        self.status = hub.status();
        self.progress = hub.progress();
        if self.error.is_none() {
            self.error = hub.error();
        }
    }
    pub fn start(&mut self, hub: Arc<Hub>, action: Action) -> Task<Message> {
        if self.busy.is_some() {
            return Task::none();
        }
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.busy = Some(action);
        self.error = None;
        self.message = None;
        let cfg = self.config.clone();
        let access = self.access.clone();
        let secret = self.secret.clone();
        let token = self.token.clone();
        let lastfm_key = self.lastfm_key.clone();
        let lastfm_secret = self.lastfm_secret.clone();
        let cancel = CancellationToken::new();
        if action == Action::Sync {
            self.cancel = Some(cancel.clone());
        }
        Task::perform(
            async move {
                let result = async {
                    match action {
                        Action::SaveConfig => {
                            let hub = hub.clone();
                            tokio::task::spawn_blocking(move || hub.configure(cfg)).await??;
                            Ok(Reply::Done("Настройки хранилища сохранены".into()))
                        }
                        Action::SaveKeys => {
                            let hub = hub.clone();
                            tokio::task::spawn_blocking(move || {
                                hub.configure(cfg)?;
                                hub.save_keys(&access.0, &secret.0)
                            })
                            .await??;
                            Ok(Reply::Done("Ключи сохранены в связке этого профиля".into()))
                        }
                        Action::ForgetKeys => {
                            let hub = hub.clone();
                            tokio::task::spawn_blocking(move || hub.forget_keys()).await??;
                            Ok(Reply::Done("Ключи хранилища удалены".into()))
                        }
                        Action::Check => {
                            let copy = hub.clone();
                            tokio::task::spawn_blocking(move || copy.configure(cfg)).await??;
                            hub.check().await?;
                            Ok(Reply::Done("Соединение с хранилищем работает".into()))
                        }
                        Action::Sync => {
                            let copy = hub.clone();
                            tokio::task::spawn_blocking(move || copy.configure(cfg)).await??;
                            match hub.sync(cancel.clone()).await {
                                Ok(outcome) => Ok(Reply::Synced(outcome)),
                                Err(_) if cancel.is_cancelled() => Ok(Reply::Cancelled),
                                Err(error) => Err(error),
                            }
                        }
                        Action::ConnectListenBrainz => {
                            hub.scrobbler.connect_listenbrainz(&token.0).await?;
                            hub.retry();
                            Ok(Reply::Done("ListenBrainz подключён".into()))
                        }
                        Action::DisconnectListenBrainz => {
                            let hub = hub.clone();
                            tokio::task::spawn_blocking(move || {
                                hub.scrobbler.disconnect_listenbrainz()
                            })
                            .await??;
                            Ok(Reply::Done(
                                "ListenBrainz отключён, его очередь очищена".into(),
                            ))
                        }
                        Action::BeginLastFm => {
                            let url = hub
                                .scrobbler
                                .lastfm_begin_auth(&lastfm_key.0, &lastfm_secret.0)
                                .await?;
                            Ok(Reply::AuthUrl(Secret(url)))
                        }
                        Action::FinishLastFm => {
                            hub.scrobbler.lastfm_finish_auth().await?;
                            hub.retry();
                            Ok(Reply::Done("Last.fm подключён".into()))
                        }
                        Action::DisconnectLastFm => {
                            let hub = hub.clone();
                            tokio::task::spawn_blocking(move || hub.scrobbler.disconnect_lastfm())
                                .await??;
                            Ok(Reply::Done("Last.fm отключён, его очередь очищена".into()))
                        }
                        Action::Retry => {
                            hub.retry();
                            Ok(Reply::Done("Повторная отправка запрошена".into()))
                        }
                    }
                }
                .await
                .map_err(|e: anyhow::Error| e.to_string());
                (generation, action, result)
            },
            |(generation, action, result)| Message::ServiceFinished(generation, action, result),
        )
    }
    pub fn finished(
        &mut self,
        generation: u64,
        action: Action,
        result: Result<Reply, String>,
    ) -> bool {
        if self.generation != generation || self.busy.is_none() {
            return false;
        }
        self.busy = None;
        self.cancel = None;
        match result {
            Ok(Reply::Done(message)) => {
                self.message = Some(message);
                match action {
                    Action::SaveKeys => {
                        self.access = Secret(String::new());
                        self.secret = Secret(String::new());
                    }
                    Action::ConnectListenBrainz => self.token = Secret(String::new()),
                    Action::FinishLastFm => {
                        self.auth_url = None;
                        self.lastfm_key = Secret(String::new());
                        self.lastfm_secret = Secret(String::new());
                    }
                    _ => {}
                }
            }
            Ok(Reply::AuthUrl(url)) => {
                self.auth_url = Some(url);
                self.lastfm_secret = Secret(String::new());
                self.message=Some("Откройте страницу Last.fm, подтвердите доступ, затем нажмите «Я подтвердил доступ»".into());
            }
            Ok(Reply::Synced(outcome)) => {
                self.message = Some(format!(
                    "Загружено: {}, скачано: {}, без изменений: {}",
                    outcome.uploaded, outcome.downloaded, outcome.up_to_date
                ));
                if !outcome.failures.is_empty() {
                    self.error = Some(
                        outcome
                            .failures
                            .iter()
                            .take(10)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                }
            }
            Ok(Reply::Cancelled) => {
                self.message =
                    Some("Синхронизация отменена. Уже завершённые копирования сохранены".into())
            }
            Err(error) => self.error = Some(error),
        }
        true
    }
}
fn notice<'a>(services: &'a Services) -> iced::widget::Column<'a, Message> {
    let mut lines = column![].spacing(10);
    if let Some(action) = services.busy {
        lines = lines.push(
            text(if action == Action::Sync {
                "Синхронизация…"
            } else {
                "Выполняется операция…"
            })
            .size(13)
            .color(st::accent()),
        );
    }
    if let Some(message) = &services.message {
        lines = lines.push(
            text(message)
                .size(13)
                .width(Length::Fill)
                .color(st::muted()),
        );
    }
    if let Some(error) = &services.error {
        lines = lines.push(
            text(error)
                .size(12)
                .width(Length::Fill)
                .color(iced::color!(0xff9696)),
        );
    }
    lines
}
fn field<'a>(
    label: &'static str,
    value: &'a str,
    index: usize,
    enabled: bool,
) -> Element<'a, Message> {
    column![
        text(label)
            .size(12)
            .width(Length::Fill)
            .shaping(iced::widget::text::Shaping::Advanced)
            .color(st::muted()),
        text_input(label, value)
            .padding(10)
            .style(st::input)
            .on_input_maybe(enabled.then_some(move |value| Message::ServiceField(index, value)))
    ]
    .spacing(5)
    .into()
}
fn secret<'a>(
    label: &'static str,
    value: &'a Secret,
    index: usize,
    enabled: bool,
) -> Element<'a, Message> {
    column![
        text(label)
            .size(12)
            .width(Length::Fill)
            .shaping(iced::widget::text::Shaping::Advanced)
            .color(st::muted()),
        text_input(label, &value.0)
            .padding(10)
            .secure(true)
            .style(st::input)
            .on_input_maybe(
                enabled.then_some(move |value| Message::ServiceSecret(index, Secret(value)))
            )
    ]
    .spacing(5)
    .into()
}
fn action(label: &'static str, action: Action, enabled: bool) -> Element<'static, Message> {
    button(text(label).size(13))
        .padding([9, 13])
        .style(st::secondary)
        .on_press_maybe(enabled.then_some(Message::ServiceAction(action)))
        .into()
}
pub fn cloud(app: &RealApp) -> Element<'_, Message> {
    let s = &app.services;
    let enabled = s.available && s.busy.is_none();
    let mut content=column![text("Облачная библиотека · S3").size(28),text("Синхронизация запускается вручную.\nЛокальные файлы не заменяются и не удаляются.\nФайлы другого размера заменяют их облачные копии.").size(13).width(Length::Fill).shaping(iced::widget::text::Shaping::Advanced).color(st::muted()),
        field("Endpoint (пусто — AWS)",&s.config.endpoint,0,enabled),
        row![container(field("Регион",&s.config.region,1,enabled)).width(Length::Fill),container(field("Бакет",&s.config.bucket,2,enabled)).width(Length::Fill)].spacing(12),
        field("Префикс",&s.config.prefix,3,enabled),checkbox("Path-style адресация (MinIO и совместимые сервисы)",s.config.path_style).on_toggle(Message::ServicePathStyle),
        row![action("Сохранить настройки",Action::SaveConfig,enabled),action("Проверить сохранённые ключи",Action::Check,enabled&&s.config.is_configured())].spacing(12),
        text("Ключи изолированы от установленной версии Muzon.\nПоля не показывают ранее сохранённые значения.").size(12).width(Length::Fill).shaping(iced::widget::text::Shaping::Advanced).color(st::muted()),
        row![container(secret("Access key",&s.access,0,enabled)).width(Length::Fill),container(secret("Secret key",&s.secret,1,enabled)).width(Length::Fill)].spacing(12),
        row![action("Сохранить ключи",Action::SaveKeys,enabled&&!s.access.0.trim().is_empty()&&!s.secret.0.trim().is_empty()),action("Забыть ключи",Action::ForgetKeys,enabled)].spacing(12),
        text("Перед проверкой и синхронизацией сохраните ключи.\nОни хранятся в системной связке, а не в JSON.").size(12).width(Length::Fill).shaping(iced::widget::text::Shaping::Advanced).color(st::muted()),
        action("Синхронизировать",Action::Sync,enabled&&s.config.is_configured()),notice(s)].spacing(16);
    if s.busy == Some(Action::Sync) {
        if let Some(progress) = &s.progress {
            let phase = match progress.phase.as_str() {
                "upload" => "Загрузка в облако",
                "download" => "Скачивание",
                "scan" => "Сравнение файлов",
                "library" => "Обновление библиотеки",
                _ => "Завершение",
            };
            content = content
                .push(
                    text(format!(
                        "{phase}: {} / {} · {}",
                        progress.done, progress.total, progress.current
                    ))
                    .size(13),
                )
                .push(progress_bar(
                    0_f32..=progress.total.max(1) as f32,
                    progress.done.min(progress.total) as f32,
                ));
        }
        content = content.push(
            button("Отменить синхронизацию")
                .style(st::quiet)
                .padding(9)
                .on_press(Message::ServiceCancel),
        );
    }
    st::scrollable(content.width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(st::scroll)
        .into()
}
fn status<'a>(label: &'static str, status: &'a ServiceStatus) -> Element<'a, Message> {
    let mut content = column![
        text(label).size(21),
        text(status.user.as_deref().unwrap_or("Не подключён")).size(14),
        text(format!("В очереди: {}", status.pending))
            .size(12)
            .color(st::muted())
    ]
    .spacing(6);
    if let Some(error) = &status.last_error {
        content = content.push(
            text(error)
                .size(12)
                .width(Length::Fill)
                .color(iced::color!(0xff9696)),
        );
    }
    content.into()
}
pub fn scrobble(app: &RealApp) -> Element<'_, Message> {
    let s = &app.services;
    let enabled = s.available && s.busy.is_none();
    let mut lastfm = column![
        status("Last.fm", &s.status.lastfm),
        secret(
            "API key приложения Last.fm",
            &s.lastfm_key,
            3,
            enabled && s.status.lastfm.user.is_none()
        ),
        secret(
            "Shared secret приложения",
            &s.lastfm_secret,
            4,
            enabled && s.status.lastfm.user.is_none()
        ),
        row![
            action(
                "Начать авторизацию",
                Action::BeginLastFm,
                enabled
                    && s.status.lastfm.user.is_none()
                    && !s.lastfm_key.0.is_empty()
                    && !s.lastfm_secret.0.is_empty()
            ),
            action(
                "Отключить и очистить очередь",
                Action::DisconnectLastFm,
                enabled && s.status.lastfm.user.is_some()
            )
        ]
        .spacing(12)
    ]
    .spacing(12);
    if s.auth_url.is_some() {
        lastfm = lastfm
            .push(
                row![
                    button("Открыть страницу Last.fm")
                        .padding(9)
                        .style(st::primary)
                        .on_press(Message::ServiceOpenAuth),
                    button("Скопировать ссылку")
                        .padding(9)
                        .style(st::quiet)
                        .on_press(Message::ServiceCopyAuth)
                ]
                .spacing(10),
            )
            .push(action("Я подтвердил доступ", Action::FinishLastFm, enabled));
    }
    let content=column![text("Скробблинг").size(28),text("Засчитываем половину трека или 4 минуты.\nТреки до 30 секунд не учитываются.\nСервисы получают прослушивания и текущий трек.\nОфлайн-очередь сохраняется на диске.").size(13).width(Length::Fill).shaping(iced::widget::text::Shaping::Advanced).color(st::muted()),
        status("ListenBrainz",&s.status.listenbrainz),secret("Токен из профиля ListenBrainz",&s.token,2,enabled&&s.status.listenbrainz.user.is_none()),
        row![action("Подключить ListenBrainz",Action::ConnectListenBrainz,enabled&&s.status.listenbrainz.user.is_none()&&!s.token.0.trim().is_empty()),action("Отключить и очистить очередь",Action::DisconnectListenBrainz,enabled&&s.status.listenbrainz.user.is_some())].spacing(12),
        lastfm,action("Повторить отправку",Action::Retry,enabled),notice(s),text("Для хранения ключей нужен secret-tool (libsecret).\nСекреты изолированы по профилям и не записываются в JSON.").size(12).width(Length::Fill).shaping(iced::widget::text::Shaping::Advanced).color(st::muted())].spacing(20);
    st::scrollable(content.width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(st::scroll)
        .into()
}
