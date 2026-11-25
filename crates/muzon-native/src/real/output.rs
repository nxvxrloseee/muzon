//! Output policy is a request; only the core's negotiated status confirms it.
use super::{card, heading};
use crate::real::{message::Message, state::RealApp, style as st};
use iced::{
    widget::{button, column, row, text, text_input},
    Element, Length,
};
use muzon_core::domain::playback_settings::{validate_output_device, OutputMode, PlaybackEdit};

pub fn view(app: &RealApp) -> Element<'_, Message> {
    let settings = &app.audio.draft;
    let mut modes = row![].spacing(st::space::SM);
    for (mode, label) in [
        (OutputMode::Normal, "Системный выход"),
        (OutputMode::BitPerfect, "Bit-perfect · ALSA hw:"),
    ] {
        modes = modes.push(
            button(text(label).size(st::type_size::BODY))
                .style(if settings.output_mode == mode {
                    st::selected
                } else {
                    st::quiet
                })
                .padding([9, 14])
                .on_press_maybe(
                    (!app.audio.saving
                        && (mode == OutputMode::Normal
                            || settings
                                .output_device
                                .as_deref()
                                .is_some_and(|id| validate_output_device(id).is_ok())))
                    .then_some(Message::AudioChanged(PlaybackEdit::OutputMode(mode))),
                ),
        );
    }
    let mut devices = column![row![
        text("Прямое ALSA-устройство")
            .size(st::type_size::BODY)
            .width(Length::Fill),
        button(if app.audio.devices_refreshing {
            "Обновление…"
        } else {
            "Обновить"
        })
        .style(st::quiet)
        .padding([7, 12])
        .on_press_maybe((!app.audio.devices_refreshing).then_some(Message::RefreshAudioDevices)),
    ],]
    .spacing(10);
    if app.snapshot.output_devices.is_empty() {
        devices = devices.push(
            text(if app.audio.devices_refreshing {
                "Поиск прямых ALSA hw: устройств…"
            } else {
                "Прямые ALSA hw: устройства не найдены. Подключите устройство и обновите список."
            })
            .size(st::type_size::CAPTION)
            .color(st::muted()),
        );
    }
    for device in app.snapshot.output_devices.iter() {
        devices = devices.push(
            button(
                column![
                    text(device.name.clone()).size(st::type_size::BODY),
                    text(device.id.clone())
                        .size(st::type_size::CAPTION)
                        .color(st::muted()),
                ]
                .spacing(4),
            )
            .width(Length::Fill)
            .padding([9, 14])
            .style(if settings.output_device.as_ref() == Some(&device.id) {
                st::selected
            } else {
                st::quiet
            })
            .on_press_maybe((!app.audio.saving).then_some(Message::AudioChanged(
                PlaybackEdit::OutputDevice(Some(device.id.clone())),
            ))),
        );
    }
    if let Some(id) = &settings.output_device {
        if !app
            .snapshot
            .output_devices
            .iter()
            .any(|device| &device.id == id)
        {
            devices = devices.push(text(format!("Устройство не найдено монитором: {id}. Ручной hw: адрес допустим; доступность проверяется при открытии. Автоматическая замена не выполняется."))
                .size(st::type_size::CAPTION).color(st::danger()));
        }
    }
    if let Some(error) = &app.audio.devices_error {
        devices = devices.push(
            text(format!("Не удалось обновить устройства: {error}"))
                .size(st::type_size::CAPTION)
                .color(st::danger()),
        );
    }
    let manual_valid = validate_output_device(app.audio.output_input.trim()).is_ok();
    devices = devices.push(
        row![
            text_input("hw:CARD=USB,DEV=0 или hw:2,0", &app.audio.output_input)
                .on_input_maybe((!app.audio.saving).then_some(Message::AudioDeviceInput))
                .on_submit_maybe(
                    (manual_valid && !app.audio.saving).then_some(Message::AudioDeviceApply)
                )
                .padding(10)
                .width(Length::Fill),
            button("Выбрать адрес")
                .style(st::secondary)
                .padding([9, 14])
                .on_press_maybe(
                    (manual_valid && !app.audio.saving).then_some(Message::AudioDeviceApply)
                ),
        ]
        .spacing(10),
    );
    if !app.audio.output_input.is_empty() && !manual_valid {
        devices = devices.push(text("Нужен прямой ALSA-адрес hw:CARD=имя,DEV=номер или hw:номер,номер. Имена default, plughw: и pulse не подходят.")
            .size(st::type_size::CAPTION).color(st::danger()));
    }
    let status = &app.snapshot.output_status;
    let actual_mode = mode_label(status.mode);
    let format = status
        .negotiated_format
        .as_ref()
        .map(|format| {
            format!(
                "{} · {} Гц · {} кан.",
                format.format, format.rate, format.channels
            )
        })
        .unwrap_or_else(|| "Не согласован — начните воспроизведение".into());
    let mut observed = column![
        text(format!("Текущий выход ядра: {actual_mode}")).size(st::type_size::BODY),
        text(format!("Устройство: {}", status.device.as_deref().unwrap_or("Системное по умолчанию")))
            .size(st::type_size::CAPTION).color(st::muted()),
        text(format!("Согласованный формат: {format}"))
            .size(st::type_size::CAPTION).color(st::muted()),
        text("Формат подтверждён согласованием GStreamer, когда доступен. Побитовое совпадение на физическом выходе не проверено.")
            .size(st::type_size::CAPTION).color(st::muted()),
    ].spacing(7);
    if let Some(error) = &status.error {
        observed = observed.push(
            text(format!("Ошибка выхода: {error}"))
                .size(st::type_size::CAPTION)
                .color(st::danger()),
        );
    }
    if app.audio.output_changing {
        observed = observed.push(
            text(format!(
                "Применяется запрос: {} · {}. Ожидаем окончательный ответ ядра…",
                mode_label(settings.output_mode),
                settings
                    .output_device
                    .as_deref()
                    .unwrap_or("Системное по умолчанию")
            ))
            .size(st::type_size::CAPTION)
            .color(st::muted()),
        );
    }
    if let Some(failed) = &app.audio.failed_output {
        let matches_active = status.mode == failed.mode
            && status.device == failed.device
            && status.error.is_none()
            && app.snapshot.playback.output_mode == failed.mode
            && app.snapshot.playback.output_device == failed.device;
        observed = observed.push(text(format!(
            "Не удалось завершить запрос: {} · {}. {} Ошибка: {}",
            mode_label(failed.mode), failed.device.as_deref().unwrap_or("Системное по умолчанию"),
            if matches_active { "Маршрут применён; сохранение не подтверждено." }
            else { "Запрошенный маршрут не применён. Его сохранение не подтверждено. Для выбора отключённого устройства сначала переключитесь на системный выход." },
            failed.error
        )).size(st::type_size::CAPTION).color(st::danger()));
    }
    card(column![
        heading("Аудиовыход", "Выбор режима не подтверждает согласованный формат"),
        modes,
        devices,
        observed,
        text("ALSA hw: требует эксклюзивного доступа. Закройте другие приложения, использующие устройство; системный микшер и программная громкость обходятся.")
            .size(st::type_size::CAPTION).color(st::muted()),
    ].spacing(14).width(Length::Fill))
}

fn mode_label(mode: OutputMode) -> &'static str {
    match mode {
        OutputMode::Normal => "Системный выход",
        OutputMode::BitPerfect => "Прямой ALSA hw:",
    }
}
