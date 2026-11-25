//! Views render immutable core snapshots. Only visible list rows become widgets.
use super::{
    collections,
    icons::{icon, Icon},
    style as st,
};
use super::{
    message::Message,
    state::{RealApp, Screen, LIBRARY_LIST, QUEUE_LIST},
};
use crate::ui::{current_lyric_line, mmss, visible_window, ROW_HEIGHT};
use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, image, row, scrollable, slider, text, text_input, vertical_space,
};
use iced::{Alignment, Element, Length};
use muzon_queue::RepeatMode;

fn nav<'a>(app: &'a RealApp, screen: Screen, label: &str, glyph: Icon) -> Element<'a, Message> {
    let active =
        app.screen == screen || screen == Screen::Audio && super::settings::contains(app.screen);
    button(
        row![
            icon(
                glyph,
                18.0,
                if active {
                    st::foreground()
                } else {
                    st::muted()
                }
            ),
            text(label.to_owned()).size(14)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding([11, 12])
    .width(Length::Fill)
    .style(if active { st::selected } else { st::quiet })
    .on_press(Message::ScreenSelected(screen))
    .into()
}
pub fn view(app: &RealApp) -> Element<'_, Message> {
    let sidebar = container(
        column![
            row![
                icon(Icon::Library, 22.0, st::accent()),
                text("Muzon").size(22)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            vertical_space().height(12),
            text_input("Поиск…", &app.query)
                .id(iced::widget::text_input::Id::new("muzon-search"))
                .on_input(Message::SearchChanged)
                .padding(11)
                .size(13)
                .style(st::input),
            vertical_space().height(6),
            button(text("Команды · Ctrl+K").size(12))
                .style(st::quiet)
                .padding([6, 10])
                .width(Length::Fill)
                .on_press(Message::Shortcut(super::shortcuts::Shortcut::Palette)),
            text("МЕДИАТЕКА").size(10).color(st::muted()),
            st::scrollable(
                column![
                    nav(app, Screen::Library, "Библиотека", Icon::Library),
                    nav(app, Screen::Albums, "Альбомы", Icon::Music),
                    nav(app, Screen::Artists, "Исполнители", Icon::Music),
                    nav(app, Screen::Genres, "Жанры", Icon::Library),
                    nav(app, Screen::Favorites, "Избранное", Icon::Heart),
                    nav(app, Screen::Smart, "Подборки", Icon::Queue),
                    nav(app, Screen::Playlists, "Плейлисты", Icon::Queue),
                    nav(app, Screen::NowPlaying, "Сейчас играет", Icon::Music),
                    nav(
                        app,
                        Screen::Queue,
                        &format!("Очередь · {}", app.snapshot.queue.len()),
                        Icon::Queue
                    )
                ]
                .spacing(4)
            )
            .height(Length::Fill)
            .style(st::scroll),
            nav(app, Screen::Audio, "Настройки", Icon::Effects),
            text(if app.snapshot.sleep.end_of_track {
                "Сон · после трека".to_owned()
            } else if let Some(seconds) = app.snapshot.sleep.remaining_secs {
                format!("Сон · {}:{:02}", seconds / 60, seconds % 60)
            } else {
                "Вся ваша музыка. Рядом.".to_owned()
            })
            .size(12)
            .color(st::muted()),
            button(
                text(if app.closing {
                    "Сохранение…"
                } else {
                    "Выйти"
                })
                .size(12)
            )
            .on_press(Message::Quit)
            .style(st::quiet)
            .padding([8, 12]),
        ]
        .spacing(12)
        .padding(20)
        .width(240)
        .height(Length::Fill),
    )
    .style(|_| st::surface(st::sidebar()));
    let content = if super::settings::contains(app.screen) {
        super::settings::view(app)
    } else {
        match app.screen {
            Screen::Library => library(app),
            Screen::Albums | Screen::Artists | Screen::Genres => super::groups::view(app),
            Screen::Favorites => collections::favorites(app),
            Screen::Smart => smart(app),
            Screen::Playlists => collections::playlists(app),
            Screen::Audio => super::audio_settings::view(app),
            Screen::Sleep => super::sleep::view(app),
            Screen::Shortcuts => super::hotkeys::view(app),
            Screen::Appearance => super::appearance::view(app),
            Screen::Cloud => super::services::cloud(app),
            Screen::Scrobble => super::services::scrobble(app),
            Screen::NowPlaying => now_playing(app),
            Screen::Queue => queue(app),
        }
    };
    let main: Element<'_, Message> = if app.screen == Screen::NowPlaying {
        container(
            column![
                button(text("Свернуть").size(13))
                    .padding([9, 16])
                    .style(st::secondary)
                    .on_press(Message::ScreenSelected(Screen::Library)),
                content
            ]
            .spacing(24),
        )
        .padding(28)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(st::ambient)
        .into()
    } else {
        row![
            sidebar,
            container(content)
                .padding(28)
                .width(Length::Fill)
                .height(Length::Fill)
        ]
        .height(Length::Fill)
        .into()
    };
    let mut body = column![main].height(Length::Fill);
    if let Some(error) = app
        .ui_error
        .as_ref()
        .or(app.snapshot.error.as_ref())
        .or(app.snapshot.audio_error.as_ref())
    {
        let mut notice = row![text(error)
            .size(12)
            .color(iced::color!(0xff9696))
            .width(Length::Fill)]
        .spacing(10)
        .align_y(Alignment::Center);
        if app.audio_close_failed && !app.closing {
            notice = notice
                .push(
                    button(text("Повторить").size(12))
                        .style(st::secondary)
                        .padding([7, 10])
                        .on_press(Message::Quit),
                )
                .push(
                    button(text("Закрыть всё равно").size(12))
                        .style(st::quiet)
                        .padding([7, 10])
                        .on_press(Message::DiscardAudioAndQuit),
                );
        }
        body = body.push(container(notice).padding([6, 18]));
    }
    let base: Element<'_, Message> = container(
        body.push(player_bar(app))
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .style(|_| st::surface(st::background()))
    .into();
    if let Some(overlay) = super::editor::overlay(app) {
        iced::widget::stack![base, iced::widget::opaque(overlay)].into()
    } else if let Some(overlay) = super::palette::overlay(app) {
        iced::widget::stack![base, iced::widget::opaque(overlay)].into()
    } else if let Some(overlay) = collections::overlay(app) {
        iced::widget::stack![base, iced::widget::opaque(overlay)].into()
    } else {
        base
    }
}
fn library(app: &RealApp) -> Element<'_, Message> {
    let header = row![
        text("Библиотека").size(30),
        text(format!("Треков: {}", app.filtered.len()))
            .size(12)
            .color(st::muted())
    ]
    .spacing(16)
    .align_y(Alignment::Center);
    let folder = row![
        text_input("Путь к папке с музыкой", &app.folder_input)
            .on_input(Message::FolderInputChanged)
            .on_submit(Message::AddFolder)
            .padding(10)
            .size(13)
            .style(st::input),
        button(
            text(if app.folder_picker_open {
                "Выбор папки…"
            } else {
                "Выбрать папку…"
            })
            .size(13)
        )
        .padding([10, 14])
        .style(st::primary)
        .on_press_maybe((!app.folder_picker_open).then_some(Message::PickFolder)),
        button(text("Добавить путь").size(13))
            .padding([10, 14])
            .style(st::secondary)
            .on_press_maybe((!app.folder_input.trim().is_empty()).then_some(Message::AddFolder)),
        button(
            text(if app.snapshot.scanning {
                "Сканирование…"
            } else {
                "Обновить"
            })
            .size(13)
        )
        .padding([10, 14])
        .style(st::secondary)
        .on_press_maybe((!app.snapshot.scanning).then_some(Message::Rescan))
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let mut content = column![header, folder].spacing(20);
    if let Some(message) = &app.snapshot.message {
        content = content.push(text(message).size(12).color(st::muted()));
    }
    if app.filtered.is_empty() {
        return content
            .push(
                container(text("Добавьте папку с музыкой или измените поиск."))
                    .center(Length::Fill),
            )
            .height(Length::Fill)
            .into();
    }
    let headings: Element<'_, Message> = if app.album_grid.viewport.width < 1040. {
        container(text("НАЗВАНИЕ / ИСПОЛНИТЕЛЬ").size(10).color(st::muted()))
            .padding([0, 68])
            .into()
    } else {
        row![
            text("  #").width(28).size(11).color(st::muted()),
            container(text("")).width(32.),
            text("НАЗВАНИЕ")
                .width(Length::FillPortion(3))
                .size(10)
                .color(st::muted()),
            text("ИСПОЛНИТЕЛЬ")
                .width(Length::FillPortion(2))
                .size(10)
                .color(st::muted()),
            text("АЛЬБОМ")
                .width(Length::FillPortion(2))
                .size(10)
                .color(st::muted()),
            text("ВРЕМЯ").width(163).size(10).color(st::muted())
        ]
        .spacing(8)
        .into()
    };
    content
        .push(container(headings).padding(iced::Padding {
            right: st::SCROLL_GUTTER,
            ..Default::default()
        }))
        .push(library_rows(app))
        .height(Length::Fill)
        .into()
}
pub(super) fn library_rows(app: &RealApp) -> Element<'_, Message> {
    let total = app.filtered.len();
    let window = visible_window(app.library_scroll, total);
    let mut rows =
        column![vertical_space().height(window.start as f32 * ROW_HEIGHT)].width(Length::Fill);
    for visible in window.clone() {
        let track = &app.snapshot.library[app.filtered[visible]];
        let current = app.snapshot.current_path.as_deref() == Some(track.path.as_str());
        let label = if app.screen == Screen::Playlists || app.album_grid.viewport.width < 1040. {
            row![
                text(if current { ">" } else { "" })
                    .size(11)
                    .width(15)
                    .color(st::accent()),
                app.track_covers.view(&track.path),
                column![
                    text(&track.title)
                        .size(13)
                        .wrapping(iced::widget::text::Wrapping::None),
                    text(track.artist.as_deref().unwrap_or("—"))
                        .size(11)
                        .color(st::muted())
                ]
                .spacing(2)
                .width(Length::Fill),
                text(mmss(track.duration_secs.unwrap_or(0.0)))
                    .size(11)
                    .color(st::muted())
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        } else {
            row![
                text(if current {
                    ">".to_owned()
                } else {
                    (visible + 1).to_string()
                })
                .size(11)
                .color(if current { st::accent() } else { st::muted() })
                .width(20),
                app.track_covers.view(&track.path),
                text(&track.title).width(Length::FillPortion(3)).size(14),
                text(track.artist.as_deref().unwrap_or("—"))
                    .width(Length::FillPortion(2))
                    .size(12)
                    .color(st::muted()),
                text(track.album.as_deref().unwrap_or("—"))
                    .width(Length::FillPortion(2))
                    .size(12)
                    .color(st::muted()),
                text(mmss(track.duration_secs.unwrap_or(0.0)))
                    .width(55)
                    .size(12)
                    .color(st::muted())
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        };
        let play = button(
            container(label)
                .clip(true)
                .height(ROW_HEIGHT)
                .align_y(Vertical::Center)
                .padding([0, 8]),
        )
        .width(Length::Fill)
        .padding(0)
        .style(move |_, status| st::track(current, status))
        .on_press(Message::TrackRowClicked(visible));
        let mut actions = row![
            play,
            collections::favorite_button(app, track),
            collections::icon_action(
                app,
                Icon::Edit,
                "Редактор тегов / LRC",
                Some(Message::OpenEditor(track.id))
            ),
            collections::icon_action(
                app,
                Icon::Queue,
                "В плейлист",
                (!app.playlist_busy).then_some(Message::OpenPlaylistPicker(track.id))
            ),
            collections::icon_action(
                app,
                Icon::Plus,
                "Добавить в очередь",
                Some(Message::EnqueueRow(visible))
            )
        ]
        .spacing(4);
        if app.screen == Screen::Playlists {
            if let Some(id) = app.selected_playlist {
                actions = actions
                    .push(collections::icon_action(
                        app,
                        Icon::Up,
                        "Выше в плейлисте",
                        (!app.playlist_busy).then_some(Message::EditPlaylist(
                            muzon_core::controller::PlaylistEdit::Move {
                                id,
                                track: track.id,
                                offset: -1,
                            },
                        )),
                    ))
                    .push(collections::icon_action(
                        app,
                        Icon::Down,
                        "Ниже в плейлисте",
                        (!app.playlist_busy).then_some(Message::EditPlaylist(
                            muzon_core::controller::PlaylistEdit::Move {
                                id,
                                track: track.id,
                                offset: 1,
                            },
                        )),
                    ))
                    .push(collections::icon_action(
                        app,
                        Icon::Close,
                        "Убрать из плейлиста",
                        (!app.playlist_busy).then_some(Message::EditPlaylist(
                            muzon_core::controller::PlaylistEdit::Remove {
                                id,
                                track: track.id,
                            },
                        )),
                    ));
            }
        }
        rows = rows.push(actions);
    }
    rows = rows.push(vertical_space().height((total - window.end) as f32 * ROW_HEIGHT));
    st::scrollable(rows)
        .id(scrollable::Id::new(LIBRARY_LIST))
        .on_scroll(|v| Message::LibraryScrolled(v.absolute_offset().y))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
fn queue(app: &RealApp) -> Element<'_, Message> {
    let controls = row![
        text("Очередь").size(30),
        button(if app.snapshot.shuffle {
            "Перемешивание: вкл."
        } else {
            "Перемешивание: выкл."
        })
        .style(if app.snapshot.shuffle {
            st::selected
        } else {
            st::quiet
        })
        .padding([9, 12])
        .on_press(Message::ToggleShuffle),
        button(match app.snapshot.repeat {
            RepeatMode::Off => "Без повтора",
            RepeatMode::All => "Повтор очереди",
            RepeatMode::One => "Повтор трека",
        })
        .style(st::secondary)
        .padding([9, 12])
        .on_press(Message::CycleRepeat)
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let total = app.snapshot.display.len();
    let window = visible_window(app.queue_scroll, total);
    let mut rows =
        column![vertical_space().height(window.start as f32 * ROW_HEIGHT)].width(Length::Fill);
    for visible in window.clone() {
        let index = app.snapshot.display[visible];
        let track = &app.snapshot.queue[index];
        let label = format!(
            "{}{} — {}",
            if app.snapshot.cursor == Some(index) {
                ">  "
            } else {
                ""
            },
            track.title,
            track.artist.as_deref().unwrap_or("—")
        );
        rows = rows.push(
            iced::widget::mouse_area(
                row![
                    iced::widget::mouse_area(
                        container(icon(Icon::Queue, 16., st::muted()))
                            .width(24)
                            .height(ROW_HEIGHT)
                            .align_y(Vertical::Center)
                    )
                    .on_press(Message::QueueDragStart(visible))
                    .interaction(iced::mouse::Interaction::Grab),
                    button(
                        container(text(label).size(14))
                            .height(ROW_HEIGHT)
                            .align_y(Vertical::Center)
                    )
                    .padding(0)
                    .width(Length::Fill)
                    .style(move |_, status| st::track(
                        app.snapshot.cursor == Some(index)
                            || app.queue_drag.as_ref().is_some_and(|d| d.to == visible),
                        status
                    ))
                    .on_press(Message::QueueRowClicked(visible)),
                    button("Выше")
                        .style(st::quiet)
                        .padding([4, 6])
                        .height(ROW_HEIGHT)
                        .on_press_maybe(
                            (visible > 0)
                                .then_some(Message::QueueMove(visible, visible.saturating_sub(1)))
                        ),
                    button("Ниже")
                        .style(st::quiet)
                        .padding([4, 6])
                        .height(ROW_HEIGHT)
                        .on_press_maybe(
                            (visible + 1 < total)
                                .then_some(Message::QueueMove(visible, visible + 1))
                        ),
                    button("×")
                        .style(st::quiet)
                        .width(32)
                        .height(ROW_HEIGHT)
                        .on_press(Message::QueueRemove(visible))
                ]
                .spacing(6),
            )
            .on_enter(Message::QueueDragHover(visible)),
        );
    }
    rows = rows.push(vertical_space().height((total - window.end) as f32 * ROW_HEIGHT));
    column![
        controls,
        st::scrollable(rows)
            .id(scrollable::Id::new(QUEUE_LIST))
            .on_scroll(|v| Message::QueueScrolled(v.absolute_offset().y))
            .height(Length::Fill)
            .width(Length::Fill)
    ]
    .spacing(15)
    .height(Length::Fill)
    .into()
}
fn now_playing(app: &RealApp) -> Element<'_, Message> {
    let Some(track) = app.snapshot.current_track() else {
        return container(text("Выберите трек в библиотеке").size(22))
            .center(Length::Fill)
            .into();
    };
    let media_matches = app.media.track_path.as_deref() == Some(track.path.as_str());
    let cover: Element<'_, Message> = if media_matches {
        if let Some(handle) = &app.cover {
            image(handle.clone())
                .width(300)
                .height(300)
                .content_fit(iced::ContentFit::Contain)
                .into()
        } else {
            container(icon(Icon::Music, 72.0, st::muted()))
                .center(300)
                .style(st::card)
                .into()
        }
    } else {
        container(icon(Icon::Music, 72.0, st::muted()))
            .center(300)
            .style(st::card)
            .into()
    };
    let mut details = column![
        cover,
        row![
            text(&track.title).size(24).width(Length::Fill),
            collections::favorite_button(app, track)
        ]
        .align_y(Alignment::Center),
        text(track.artist.as_deref().unwrap_or("Неизвестный исполнитель"))
            .size(18)
            .color(st::muted()),
        text(track.album.as_deref().unwrap_or(""))
            .size(13)
            .color(st::muted()),
        button(
            container(icon(
                if app.snapshot.is_playing {
                    Icon::Pause
                } else {
                    Icon::Play
                },
                22.0,
                st::foreground()
            ))
            .center(24)
        )
        .padding(12)
        .style(st::primary)
        .on_press(Message::TogglePlay)
    ]
    .spacing(14)
    .width(Length::Fill);
    if app.appearance.prefs.spectrum {
        let mut bars = row![].spacing(2).height(64).align_y(Alignment::End);
        for index in 0..48 {
            let value = if app.snapshot.is_playing {
                app.spectrum.get(index).copied().unwrap_or(0.).clamp(0., 1.)
            } else {
                0.
            };
            bars = bars.push(
                container(vertical_space().height((value * 60.).max(2.)))
                    .width(Length::FillPortion(1))
                    .style(|_| st::surface(st::accent())),
            );
        }
        details = details.push(bars);
    }
    let details = st::scrollable(container(details).padding(24).width(350).style(st::card))
        .width(350)
        .height(Length::Fill)
        .style(st::scroll);
    let mut lines = column![text("ТЕКСТ ПЕСНИ").size(11).color(st::muted())]
        .spacing(14)
        .width(Length::Fill);
    if media_matches && !app.media.lyrics.is_empty() {
        let times: Vec<f64> = app.media.lyrics.iter().map(|line| line.time_secs).collect();
        let active = current_lyric_line(&times, app.snapshot.position_secs);
        // Bound the displayed lyric window too; keep the active line in view.
        let start = active.unwrap_or(0).saturating_sub(2);
        for (index, line) in app.media.lyrics.iter().enumerate().skip(start).take(12) {
            lines = lines.push(
                text(&line.text)
                    .size(if Some(index) == active { 21 } else { 17 })
                    .color(if Some(index) == active {
                        st::foreground()
                    } else {
                        st::muted()
                    }),
            );
        }
    } else {
        lines = lines.push(
            text(if media_matches && app.media.loaded {
                "Локальный текст не найден"
            } else {
                "Загрузка обложки и текста…"
            })
            .size(14),
        );
    }
    row![
        details,
        container(st::scrollable(lines).height(Length::Fill))
            .padding(28)
            .height(Length::Fill)
            .width(Length::Fill)
            .style(st::card)
    ]
    .spacing(24)
    .align_y(Alignment::Center)
    .height(Length::Fill)
    .into()
}
fn control<'a>(
    app: &RealApp,
    glyph: Icon,
    message: Message,
    filled: bool,
    label: &'a str,
) -> Element<'a, Message> {
    let button = button(container(icon(glyph, 17.0, st::foreground())).center(18))
        .padding(10)
        .style(move |theme, status| {
            let mut style = if filled {
                st::secondary(theme, status)
            } else {
                st::quiet(theme, status)
            };
            style.border.radius = 24.0.into();
            style
        })
        .on_press(message);
    if app.collection_overlay.is_some() {
        button.into()
    } else {
        iced::widget::tooltip(
            button,
            text(label).size(12),
            iced::widget::tooltip::Position::Top,
        )
        .into()
    }
}

fn player_bar(app: &RealApp) -> Element<'_, Message> {
    let duration = app.snapshot.display_duration().max(0.1);
    let position = app
        .seek_preview
        .unwrap_or(app.snapshot.position_secs)
        .clamp(0.0, duration);
    let track = app.snapshot.current_track();
    let title = track
        .map(|t| t.title.as_str())
        .unwrap_or("Ничего не играет");
    let art: Element<'_, Message> = if let Some(handle) = &app.cover {
        image(handle.clone())
            .width(46)
            .height(46)
            .content_fit(iced::ContentFit::Cover)
            .into()
    } else {
        container(icon(Icon::Music, 20.0, st::muted()))
            .center(46)
            .style(st::card)
            .into()
    };
    let info = button(
        row![
            art,
            container(
                column![
                    text(title)
                        .size(13)
                        .wrapping(iced::widget::text::Wrapping::None),
                    text(
                        track
                            .map(|t| t.artist.as_deref().unwrap_or("Без исполнителя"))
                            .unwrap_or("Выберите музыку")
                    )
                    .size(11)
                    .wrapping(iced::widget::text::Wrapping::None)
                    .color(st::muted())
                ]
                .spacing(5)
            )
            .width(Length::Fill)
            .clip(true)
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .width(200)
    .padding(4)
    .style(st::quiet)
    .on_press(Message::ScreenSelected(Screen::NowPlaying));
    let transport = row![
        control(
            app,
            Icon::Previous,
            Message::Previous,
            false,
            "Предыдущий трек"
        ),
        control(
            app,
            if app.snapshot.is_playing {
                Icon::Pause
            } else {
                Icon::Play
            },
            Message::TogglePlay,
            true,
            if app.snapshot.is_playing {
                "Пауза"
            } else {
                "Воспроизвести"
            }
        ),
        control(app, Icon::Next, Message::Next, false, "Следующий трек"),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    container(
        row![
            info,
            container(
                track
                    .map(|track| collections::favorite_button(app, track))
                    .unwrap_or_else(|| container(text("")).width(30).into())
            )
            .width(30),
            transport,
            text(mmss(position)).size(11).color(st::muted()),
            slider(0.0..=duration, position, Message::SeekChanged)
                .step(0.1)
                .on_release(Message::SeekCommit)
                .style(st::progress)
                .width(Length::Fill),
            text(mmss(duration.round())).size(11).color(st::muted()),
            icon(Icon::Volume, 18.0, st::muted()),
            slider(0.0..=1.0, app.snapshot.volume, Message::VolumeChanged)
                .step(0.01)
                .style(st::progress)
                .width(76),
            control(app, Icon::Stop, Message::Stop, false, "Остановить"),
            control(
                app,
                Icon::Effects,
                Message::ScreenSelected(Screen::Audio),
                false,
                "Настройки звука"
            ),
            control(
                app,
                Icon::Queue,
                Message::ScreenSelected(Screen::Queue),
                false,
                "Очередь"
            ),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([12, 16])
    .height(80)
    .width(Length::Fill)
    .style(|_| st::surface(st::player()))
    .into()
}

fn smart(app: &RealApp) -> Element<'_, Message> {
    let mut tabs = column![].spacing(6);
    for chunk in super::smart::Smart::ALL.chunks(3) {
        let mut line = row![].spacing(6);
        for &kind in chunk {
            line = line.push(
                button(text(kind.title()).size(11))
                    .style(if app.smart == kind {
                        st::primary
                    } else {
                        st::secondary
                    })
                    .padding([8, 6])
                    .on_press(Message::SmartSelected(kind)),
            );
        }
        tabs = tabs.push(line);
    }
    column![
        text("Умные подборки").size(28),
        tabs,
        text(app.smart.description()).size(12).color(st::muted()),
        row![
            button("Слушать")
                .style(st::primary)
                .padding([8, 14])
                .on_press_maybe((!app.filtered.is_empty()).then_some(Message::PlayCollection)),
            button("В очередь")
                .style(st::secondary)
                .padding([8, 14])
                .on_press_maybe((!app.filtered.is_empty()).then_some(Message::EnqueueCollection)),
            text(format!("Треков: {}", app.filtered.len())).size(12)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        library_rows(app)
    ]
    .spacing(16)
    .height(Length::Fill)
    .into()
}
