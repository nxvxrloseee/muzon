//! Sound controls keep a short-lived drag preview; the core owns actual values
//! and debounced persistence, including edits pending when the window closes.
#[path = "output.rs"]
mod output;

use super::{message::Message, state::RealApp, style as st};
use iced::{
    widget::{button, column, container, keyed_column, row, slider, text, vertical_slider},
    Alignment, Element, Length,
};
use muzon_core::{
    controller::Snapshot,
    data::audio::pipeline::{MAX_TEMPO, MIN_TEMPO},
    domain::playback_settings::{
        OutputMode, PlaybackEdit, PlaybackSettings, ReplayGainMode, EQ_FREQUENCIES_HZ, EQ_MAX_DB,
        EQ_MIN_DB, MAX_CROSSFADE_SECS, MAX_PREAMP_DB, MIN_PREAMP_DB,
    },
};
use std::time::{Duration, Instant};
pub const PRESETS: [(&str, [f64; 10]); 4] = [
    ("Плоский", [0.; 10]),
    ("Бас", [6., 5., 4., 2., 0., 0., 0., 0., 0., 0.]),
    ("Вокал", [-2., -1., 0., 2., 4., 4., 2., 0., -1., -2.]),
    ("Рок", [4., 3., 1., 0., -1., 0., 2., 3., 4., 4.]),
];
pub struct FailedOutput {
    pub mode: OutputMode,
    pub device: Option<String>,
    pub error: String,
}

pub struct AudioUi {
    pub draft: PlaybackSettings,
    last_edit: Option<Instant>,
    tempo_preview: Option<(i32, f64, Instant)>,
    pub tempo_target: Option<i32>,
    pub saving: bool,
    pub output_changing: bool,
    pub failed_output: Option<FailedOutput>,
    pub output_input: String,
    pub devices_refreshing: bool,
    pub devices_error: Option<String>,
    pub devices_refreshed: Option<Instant>,
}
impl AudioUi {
    pub fn new(s: &Snapshot) -> Self {
        Self {
            draft: (*s.playback).clone(),
            last_edit: None,
            tempo_preview: None,
            tempo_target: None,
            saving: false,
            output_changing: false,
            failed_output: None,
            output_input: s.playback.output_device.clone().unwrap_or_default(),
            devices_refreshing: false,
            devices_error: None,
            devices_refreshed: None,
        }
    }
    pub fn finish_output(&mut self, s: &Snapshot, error: Option<String>) {
        self.failed_output = error.map(|error| FailedOutput {
            mode: self.draft.output_mode,
            device: self.draft.output_device.clone(),
            error,
        });
        self.settled(s);
    }
    fn settled(&mut self, s: &Snapshot) {
        self.draft = (*s.playback).clone();
        self.last_edit = None;
        self.saving = false;
        self.output_changing = false;
    }
    pub fn sync(&mut self, s: &Snapshot) {
        if !self.saving
            && self
                .last_edit
                .is_none_or(|at| at.elapsed() >= Duration::from_millis(600))
        {
            self.draft = (*s.playback).clone();
            self.last_edit = None;
        }
        if self
            .tempo_preview
            .is_some_and(|(_, _, at)| at.elapsed() >= Duration::from_millis(600))
        {
            self.tempo_preview = None;
            self.tempo_target = None;
        }
    }
    pub fn preview(&mut self, edit: &PlaybackEdit) -> Result<(), String> {
        self.draft = edit.apply(&self.draft)?;
        self.last_edit = Some(Instant::now());
        Ok(())
    }
    pub fn preview_tempo(&mut self, id: i32, rate: f64) -> bool {
        if self.tempo_target.is_some_and(|target| target != id) {
            return false;
        }
        self.tempo_target = Some(id);
        self.tempo_preview = Some((id, rate, Instant::now()));
        true
    }
    pub fn track_changed(&mut self) {
        self.tempo_target = None;
        self.tempo_preview = None;
    }
    pub fn tempo(&self, s: &Snapshot, id: i32) -> f64 {
        if let Some((target, rate, _)) = self.tempo_preview {
            if target == id {
                return rate;
            }
        }
        if s.current_track().is_some_and(|t| t.id == id) && s.current_path.is_some() {
            s.tempo
        } else {
            s.tempo_overrides
                .get(&id)
                .copied()
                .or_else(|| s.library.iter().find(|t| t.id == id).map(|t| t.tempo))
                .unwrap_or(1.0)
        }
    }
}
fn card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(22)
        .width(Length::Fill)
        .style(st::card)
        .into()
}
fn heading(title: &str, detail: &str) -> Element<'static, Message> {
    column![
        text(title.to_owned()).size(st::type_size::TITLE),
        text(detail.to_owned())
            .size(st::type_size::CAPTION)
            .color(st::muted())
    ]
    .spacing(7)
    .into()
}
/// No interactive controls are constructed for bypassed effects. This also keeps
/// keyboard navigation from editing a control whose DSP is inactive.
fn locked_effects(settings: &PlaybackSettings) -> Element<'static, Message> {
    let gains = settings
        .eq_gains
        .iter()
        .map(|gain| format!("{gain:+.1}"))
        .collect::<Vec<_>>()
        .join(" · ");
    let replay = match settings.replay_gain.mode {
        ReplayGainMode::Off => "Выключено",
        ReplayGainMode::Track => "Треки",
        ReplayGainMode::Album => "Альбомы",
    };
    let muted = |label: &str| {
        text(label.to_owned())
            .size(st::type_size::CAPTION)
            .color(st::muted())
    };
    column![
        card(heading(
            "Эффекты недоступны в bit-perfect",
            "Эквалайзер, ReplayGain, кроссфейд, скорость и программная громкость обходятся. Настройки эффектов сохранены для обычного выхода. Громкость регулируйте на устройстве.",
        )),
        card(column![
            heading("Эквалайзер · обход", "10 полос · сохранённые значения, дБ"),
            muted(&gains),
            button("Пресеты недоступны").style(st::quiet),
        ].spacing(14)),
        card(column![
            heading("Переходы между треками · обход", "Кроссфейд отключён в bit-perfect"),
            muted(&format!("Сохранено: {:.1} с", settings.crossfade_secs)),
        ].spacing(14)),
        card(heading("Скорость воспроизведения · 1×", "Изменение скорости недоступно в bit-perfect")),
        card(column![
            heading("Выравнивание громкости · обход", "ReplayGain не применяется в bit-perfect"),
            muted(&format!("Сохранено: {replay} · предусиление {:+.1} дБ", settings.replay_gain.preamp_db)),
        ].spacing(14)),
    ]
    .spacing(20)
    .into()
}

fn effects(app: &RealApp) -> Element<'_, Message> {
    let settings = &app.audio.draft;
    let mut presets = row![].spacing(st::space::SM);
    for (i, (label, gains)) in PRESETS.iter().enumerate() {
        presets = presets.push(
            button(text(*label).size(st::type_size::CAPTION))
                .padding([8, 12])
                .style(if settings.eq_gains == *gains {
                    st::selected
                } else {
                    st::quiet
                })
                .on_press(Message::AudioPreset(i)),
        );
    }
    let mut bands = row![].spacing(st::space::SM);
    for (i, (&gain, &hz)) in settings
        .eq_gains
        .iter()
        .zip(EQ_FREQUENCIES_HZ.iter())
        .enumerate()
    {
        let frequency = if hz >= 1000 {
            format!("{:.1}к", hz as f64 / 1000.)
        } else {
            hz.to_string()
        };
        bands = bands.push(
            column![
                text(format!("{gain:+.1}"))
                    .size(st::type_size::CAPTION)
                    .color(st::muted()),
                vertical_slider(EQ_MIN_DB..=EQ_MAX_DB, gain, move |gain| {
                    Message::AudioChanged(PlaybackEdit::EqBand { index: i, gain })
                })
                .step(0.5)
                .on_release(Message::AudioSave)
                .style(st::progress)
                .height(145)
                .width(24),
                text(frequency)
                    .size(st::type_size::CAPTION)
                    .color(st::muted())
            ]
            .spacing(10)
            .align_x(Alignment::Center)
            .width(Length::Fill),
        );
    }
    let eq = card(
        column![
            heading("Эквалайзер", "10 полос · от −24 до +12 дБ"),
            presets,
            bands
        ]
        .spacing(18),
    );
    let crossfade = card(
        column![
            heading(
                "Переходы между треками",
                "Плавное смешивание конца и начала соседних треков"
            ),
            row![
                text("Кроссфейд")
                    .size(st::type_size::BODY)
                    .width(Length::Fill),
                text(if settings.crossfade_secs == 0. {
                    "Без пауз".into()
                } else {
                    format!("{:.1} с", settings.crossfade_secs)
                })
                .size(st::type_size::BODY)
                .color(st::muted())
            ],
            slider(0.0..=MAX_CROSSFADE_SECS, settings.crossfade_secs, |v| {
                Message::AudioChanged(PlaybackEdit::Crossfade(v))
            })
            .step(0.5)
            .on_release(Message::AudioSave)
            .style(st::progress),
        ]
        .spacing(15),
    );
    let speed: Element<'_, Message> = if let Some(track) = app.snapshot.current_track() {
        let id = track.id;
        let rate = app.audio.tempo(&app.snapshot, id);
        let card = card(
            column![
                heading(
                    "Скорость воспроизведения",
                    &format!("Для трека «{}»", track.title)
                ),
                row![
                    text(format!("{rate:.2}×"))
                        .size(st::type_size::EMPHASIS)
                        .width(Length::Fill),
                    button(text("Сбросить до 1×").size(st::type_size::CAPTION))
                        .style(st::quiet)
                        .padding([7, 12])
                        .on_press(Message::ResetTrackTempo(id))
                ]
                .align_y(Alignment::Center),
                slider(MIN_TEMPO..=MAX_TEMPO, rate, move |rate| {
                    Message::TrackTempoChanged { id, rate }
                })
                .step(0.05)
                .on_release(Message::AudioSave)
                .style(st::progress),
                text("Сохраняется отдельно для каждого трека. Высота тона не меняется.")
                    .size(st::type_size::CAPTION)
                    .color(st::muted())
            ]
            .spacing(14),
        );
        keyed_column([(id, card)]).width(Length::Fill).into()
    } else {
        card(heading(
            "Скорость воспроизведения",
            "Выберите трек, чтобы настроить его скорость",
        ))
    };
    let mut modes = row![].spacing(st::space::SM);
    for (mode, label) in [
        (ReplayGainMode::Off, "Выключено"),
        (ReplayGainMode::Track, "Треки"),
        (ReplayGainMode::Album, "Альбомы"),
    ] {
        modes = modes.push(
            button(text(label).size(st::type_size::BODY))
                .padding([9, 14])
                .style(if settings.replay_gain.mode == mode {
                    st::selected
                } else {
                    st::quiet
                })
                .on_press(Message::AudioChanged(PlaybackEdit::ReplayGainMode(mode))),
        );
    }
    let mut replay = column![
        heading("Выравнивание громкости", "ReplayGain"),
        modes,
        text("Используются теги ReplayGain. Файлы без этих тегов звучат без изменений.")
            .size(st::type_size::CAPTION)
            .color(st::muted())
    ]
    .spacing(14);
    if settings.replay_gain.mode != ReplayGainMode::Off {
        replay = replay
            .push(row![
                text("Предусиление")
                    .size(st::type_size::BODY)
                    .width(Length::Fill),
                text(format!("{:+.1} дБ", settings.replay_gain.preamp_db))
                    .size(st::type_size::BODY)
                    .color(st::muted())
            ])
            .push(
                slider(
                    MIN_PREAMP_DB..=MAX_PREAMP_DB,
                    settings.replay_gain.preamp_db,
                    |v| Message::AudioChanged(PlaybackEdit::Preamp(v)),
                )
                .step(0.5)
                .on_release(Message::AudioSave)
                .style(st::progress),
            );
    }
    column![eq, crossfade, speed, card(replay)]
        .spacing(20)
        .into()
}

pub fn view(app: &RealApp) -> Element<'_, Message> {
    let settings = &app.audio.draft;
    let dsp_locked = settings.output_mode == OutputMode::BitPerfect
        || app.snapshot.playback.output_mode == OutputMode::BitPerfect
        || app.snapshot.output_status.mode == OutputMode::BitPerfect;
    let status = if app.snapshot.audio_error.is_some() {
        "Есть несохранённые изменения"
    } else if app.snapshot.audio_dirty || app.audio.saving {
        "Сохранение…"
    } else {
        "Изменения сохраняются автоматически"
    };
    let mut content = column![
        row![
            text("Звук")
                .size(st::type_size::DISPLAY)
                .width(Length::Fill),
            button(text("Сбросить эффекты").size(st::type_size::CAPTION))
                .style(st::secondary)
                .padding([9, 14])
                .on_press_maybe(
                    (!dsp_locked && !app.audio.output_changing)
                        .then_some(Message::ResetAudioEffects)
                )
        ]
        .align_y(Alignment::Center),
        text(status).size(st::type_size::CAPTION).color(st::muted()),
        output::view(app),
        if dsp_locked {
            locked_effects(settings)
        } else if app.audio.output_changing {
            card(heading(
                "Применение аудиовыхода…",
                "Настройки эффектов станут доступны после ответа ядра",
            ))
        } else {
            effects(app)
        }
    ]
    .spacing(20)
    .width(Length::Fill);
    if let Some(error) = &app.snapshot.audio_error {
        content = content
            .push(
                text(format!(
                    "Не удалось сохранить или применить настройки: {error}"
                ))
                .size(st::type_size::CAPTION)
                .color(st::danger()),
            )
            .push(
                button("Повторить сохранение")
                    .style(st::secondary)
                    .padding([9, 14])
                    .on_press(Message::AudioSave),
            );
    }
    st::scrollable(content)
        .style(st::scroll)
        .height(Length::Fill)
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_preview_stays_pending_until_ack_and_reverts_to_authoritative_route() {
        let snapshot = Snapshot::empty();
        let mut ui = AudioUi::new(&snapshot);
        ui.preview(&PlaybackEdit::OutputDevice(Some(
            "hw:CARD=USB,DEV=0".into(),
        )))
        .unwrap();
        ui.preview(&PlaybackEdit::OutputMode(OutputMode::BitPerfect))
            .unwrap();
        ui.saving = true;
        ui.output_changing = true;
        ui.last_edit = Some(Instant::now() - Duration::from_secs(1));
        ui.sync(&snapshot);
        assert_eq!(ui.draft.output_mode, OutputMode::BitPerfect);
        assert_eq!(
            snapshot.output_status.mode,
            OutputMode::Normal,
            "a requested route is not a confirmed core output"
        );
        ui.finish_output(&snapshot, Some("Device disconnected".into()));
        assert_eq!(ui.draft.output_mode, OutputMode::Normal);
        assert_eq!(ui.draft.output_device, None);
        assert!(!ui.saving);
        assert!(!ui.output_changing);
        ui.sync(&snapshot);
        let failed = ui.failed_output.as_ref().unwrap();
        assert_eq!(failed.mode, OutputMode::BitPerfect);
        assert_eq!(failed.device.as_deref(), Some("hw:CARD=USB,DEV=0"));
        assert_eq!(failed.error, "Device disconnected");
        ui.finish_output(&snapshot, None);
        assert!(ui.failed_output.is_none());
    }

    #[test]
    fn a_drag_preview_does_not_jump_back_to_an_older_core_snapshot() {
        let s = Snapshot::empty();
        let mut ui = AudioUi::new(&s);
        ui.preview(&PlaybackEdit::EqBand {
            index: 0,
            gain: 6.0,
        })
        .unwrap();
        ui.sync(&s);
        assert_eq!(ui.draft.eq_gains[0], 6.0);
        assert!(ui.preview_tempo(1, 1.25));
        assert!(!ui.preview_tempo(2, 0.8));
        ui.track_changed();
        assert!(ui.preview_tempo(2, 0.8));
        ui.last_edit = Some(Instant::now() - Duration::from_secs(1));
        ui.sync(&s);
        assert_eq!(
            ui.draft.eq_gains[0], 0.0,
            "idle previews follow authoritative state again"
        );
    }
}
