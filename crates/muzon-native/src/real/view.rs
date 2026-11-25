//! Views render immutable core snapshots. Only visible list rows become widgets.
use super::{
    collections, focus,
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
    button, column, container, hover, image, pick_list, row, scrollable, slider, text, text_input,
    tooltip, vertical_space,
};
use iced::{Alignment, Element, Length};
use muzon_queue::RepeatMode;

pub(super) fn ellipsis(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_owned();
    }
    let prefix: String = value.chars().take(limit.saturating_sub(1)).collect();
    format!("{prefix}…")
}
fn short_text<'a>(value: &'a str, limit: usize, size: u16) -> Element<'a, Message> {
    tooltip(
        text(ellipsis(value, limit))
            .size(size)
            .wrapping(iced::widget::text::Wrapping::None),
        text(value).size(st::type_size::BODY),
        tooltip::Position::Top,
    )
    .style(st::card)
    .into()
}
fn row_number(index: usize, current: bool) -> Element<'static, Message> {
    hover(
        container(
            text(if current {
                "▶".into()
            } else {
                (index + 1).to_string()
            })
            .size(st::type_size::CAPTION)
            .color(if current { st::accent() } else { st::muted() }),
        )
        .width(24)
        .height(24)
        .align_y(Vertical::Center),
        container(icon(Icon::Play, 16., st::foreground()))
            .center(24)
            .style(|_| st::surface(st::hover())),
    )
}
fn format_badge<'a>(app: &'a RealApp, path: &'a str) -> Element<'a, Message> {
    let label = app
        .track_covers
        .format(path)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            std::path::Path::new(path)
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("—")
                .to_uppercase()
        });
    let compact = label
        .replace(" кГц / ", "/")
        .replace(" бит", "")
        .replace(" кГц", "k");
    tooltip(
        container(
            text(ellipsis(&compact, 11))
                .wrapping(iced::widget::text::Wrapping::None)
                .size(st::type_size::CAPTION)
                .color(st::muted()),
        )
        .width(76)
        .clip(true),
        text(label).size(st::type_size::BODY),
        tooltip::Position::Top,
    )
    .style(st::card)
    .into()
}
fn focused_icon_action(
    app: &RealApp,
    glyph: Icon,
    label: &'static str,
    message: Option<Message>,
) -> Element<'static, Message> {
    let child = collections::icon_action(app, glyph, label, message.clone());
    focus::action(child, if focus_enabled(app) { message } else { None })
}
fn focus_enabled(app: &RealApp) -> bool {
    app.editor.is_none()
        && app.palette.is_none()
        && app.collection_overlay.is_none()
        && app.hotkeys.capture.is_none()
        && !app.closing
}
fn nav<'a>(app: &'a RealApp, screen: Screen, label: &str, glyph: Icon) -> Element<'a, Message> {
    let active =
        app.screen == screen || screen == Screen::Audio && super::settings::contains(app.screen);
    let button = button(
        row![
            container(text(""))
                .width(3)
                .height(24)
                .style(move |_| st::surface(if active {
                    st::accent()
                } else {
                    iced::Color::TRANSPARENT
                })),
            icon(
                glyph,
                18.0,
                if active {
                    st::foreground()
                } else {
                    st::muted()
                }
            ),
            text(label.to_owned()).size(st::type_size::BODY)
        ]
        .spacing(st::space::MD)
        .align_y(Alignment::Center),
    )
    .padding([6, 8])
    .width(Length::Fill)
    .style(if active { st::selected } else { st::quiet })
    .on_press(Message::ScreenSelected(screen));
    focus::action(
        button,
        focus_enabled(app).then_some(Message::ScreenSelected(screen)),
    )
}
pub fn view(app: &RealApp) -> Element<'_, Message> {
    let sidebar = container(
        column![
            row![
                icon(Icon::Library, 22., st::accent()),
                text("Muzon").size(st::type_size::TITLE)
            ]
            .spacing(st::space::SM)
            .align_y(Alignment::Center),
            container(
                row![
                    text_input("Поиск…", &app.query)
                        .id(iced::widget::text_input::Id::new("muzon-search"))
                        .on_input(Message::SearchChanged)
                        .padding(8)
                        .size(st::type_size::BODY)
                        .style(st::input),
                    button(text("Ctrl+K").size(st::type_size::CAPTION))
                        .padding([8, 4])
                        .style(st::quiet)
                        .on_press(Message::Shortcut(super::shortcuts::Shortcut::Palette))
                ]
                .align_y(Alignment::Center)
            )
            .style(st::card),
            st::scrollable(
                column![
                    text("Медиатека")
                        .size(st::type_size::CAPTION)
                        .color(st::muted()),
                    nav(app, Screen::Library, "Библиотека", Icon::Library),
                    nav(app, Screen::Albums, "Альбомы", Icon::Album),
                    nav(app, Screen::Artists, "Исполнители", Icon::Artist),
                    nav(app, Screen::Genres, "Жанры", Icon::Genre),
                    vertical_space().height(st::space::SM),
                    text("Коллекции")
                        .size(st::type_size::CAPTION)
                        .color(st::muted()),
                    nav(app, Screen::Favorites, "Избранное", Icon::Heart),
                    nav(app, Screen::Smart, "Подборки", Icon::Smart),
                    nav(app, Screen::Playlists, "Плейлисты", Icon::Playlist),
                ]
                .spacing(st::space::XS)
            )
            .height(Length::Fill),
            text(if app.snapshot.sleep.end_of_track {
                "Сон · после трека".to_owned()
            } else if let Some(seconds) = app.snapshot.sleep.remaining_secs {
                format!("Сон · {}:{:02}", seconds / 60, seconds % 60)
            } else {
                String::new()
            })
            .size(st::type_size::CAPTION)
            .color(st::muted()),
            nav(app, Screen::Audio, "Настройки", Icon::Effects),
            pick_list(["Команды…", "Выйти"], None::<&str>, |action| {
                if action == "Выйти" {
                    Message::Quit
                } else {
                    Message::Shortcut(super::shortcuts::Shortcut::Palette)
                }
            })
            .placeholder(if app.closing {
                "Сохранение…"
            } else {
                "Меню"
            })
            .text_size(st::type_size::BODY)
            .style(st::picker)
            .menu_style(st::menu)
            .width(Length::Fill),
        ]
        .spacing(st::space::MD)
        .padding(st::space::LG)
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
                button(text("Свернуть").size(st::type_size::BODY))
                    .padding([9, 16])
                    .style(st::secondary)
                    .on_press(Message::ScreenSelected(Screen::Library)),
                content
            ]
            .spacing(st::space::XL),
        )
        .padding(28)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| {
            st::art_background(if app.media.track_path == app.snapshot.current_path {
                app.media.dominant_color
            } else {
                None
            })
        })
        .into()
    } else {
        row![
            sidebar,
            container(content)
                .padding(if app.album_grid.viewport.width < 1040. {
                    16
                } else {
                    28
                })
                .width(Length::Fill)
                .height(Length::Fill)
        ]
        .height(Length::Fill)
        .into()
    };
    let main = if app.screen == Screen::NowPlaying
        && app.media.track_path.as_deref() == app.snapshot.current_path.as_deref()
    {
        if let Some(handle) = &app.cover {
            iced::widget::stack![
                image(handle.clone())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .content_fit(iced::ContentFit::Cover),
                container(main)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| st::surface(iced::Color {
                        a: 0.94,
                        ..st::background()
                    }))
            ]
            .into()
        } else {
            main
        }
    } else {
        main
    };
    let mut body = column![main].height(Length::Fill);
    if let Some(error) = app
        .ui_error
        .as_ref()
        .or(app.snapshot.error.as_ref())
        .or(app.snapshot.audio_error.as_ref())
    {
        let mut notice = row![text(error)
            .size(st::type_size::CAPTION)
            .color(st::danger())
            .width(Length::Fill)]
        .spacing(10)
        .align_y(Alignment::Center);
        if app.audio_close_failed && !app.closing {
            notice = notice
                .push(
                    button(text("Повторить").size(st::type_size::CAPTION))
                        .style(st::secondary)
                        .padding([7, 10])
                        .on_press(Message::Quit),
                )
                .push(
                    button(text("Закрыть всё равно").size(st::type_size::CAPTION))
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
        text("Библиотека").size(st::type_size::DISPLAY),
        text(format!("Треков: {}", app.filtered.len()))
            .size(st::type_size::CAPTION)
            .color(st::muted())
    ]
    .spacing(st::space::LG)
    .align_y(Alignment::Center);
    let folder = column![
        text_input("Путь к папке с музыкой", &app.folder_input)
            .on_input(Message::FolderInputChanged)
            .on_submit(Message::AddFolder)
            .padding(10)
            .size(st::type_size::BODY)
            .style(st::input),
        row![
            button(
                text(if app.folder_picker_open {
                    "Выбор папки…"
                } else {
                    "Выбрать папку…"
                })
                .size(st::type_size::BODY)
            )
            .padding([10, 14])
            .style(st::primary)
            .on_press_maybe((!app.folder_picker_open).then_some(Message::PickFolder)),
            button(text("Добавить путь").size(st::type_size::BODY))
                .padding([10, 14])
                .style(st::secondary)
                .on_press_maybe(
                    (!app.folder_input.trim().is_empty()).then_some(Message::AddFolder)
                ),
            button(
                text(if app.snapshot.scanning {
                    "Сканирование…"
                } else {
                    "Обновить"
                })
                .size(st::type_size::BODY)
            )
            .padding([10, 14])
            .style(st::secondary)
            .on_press_maybe((!app.snapshot.scanning).then_some(Message::Rescan))
        ]
        .spacing(st::space::SM)
    ]
    .spacing(st::space::SM);
    let mut content = column![header, folder].spacing(20);
    if let Some(message) = &app.snapshot.message {
        content = content.push(
            text(message)
                .size(st::type_size::CAPTION)
                .color(st::muted()),
        );
    }
    if app.filtered.is_empty() {
        return content
            .push(
                container(
                    column![
                        text("Добавьте папку с музыкой или измените поиск."),
                        button("Выбрать папку…")
                            .style(st::primary)
                            .padding(st::space::SM)
                            .on_press_maybe(
                                (!app.folder_picker_open).then_some(Message::PickFolder)
                            )
                    ]
                    .spacing(st::space::MD),
                )
                .center(Length::Fill),
            )
            .height(Length::Fill)
            .into();
    }
    let headings: Element<'_, Message> = if app.album_grid.viewport.width < 1040. {
        container(
            text("НАЗВАНИЕ / ИСПОЛНИТЕЛЬ")
                .size(st::type_size::CAPTION)
                .color(st::muted()),
        )
        .padding([0, 68])
        .into()
    } else {
        row![
            text("  #")
                .width(28)
                .size(st::type_size::CAPTION)
                .color(st::muted()),
            container(text("")).width(32.),
            text("НАЗВАНИЕ")
                .width(Length::FillPortion(3))
                .size(st::type_size::CAPTION)
                .color(st::muted()),
            text("ИСПОЛНИТЕЛЬ")
                .width(Length::FillPortion(2))
                .size(st::type_size::CAPTION)
                .color(st::muted()),
            text("АЛЬБОМ")
                .width(Length::FillPortion(2))
                .size(st::type_size::CAPTION)
                .color(st::muted()),
            text("ФОРМАТ / ВРЕМЯ")
                .width(247)
                .size(st::type_size::CAPTION)
                .color(st::muted())
        ]
        .spacing(st::space::SM)
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
                row_number(visible, current),
                app.track_covers.view(&track.path),
                column![
                    short_text(&track.title, 28, st::type_size::BODY),
                    short_text(
                        track.artist.as_deref().unwrap_or("—"),
                        28,
                        st::type_size::CAPTION
                    )
                ]
                .spacing(2)
                .width(Length::Fill),
                format_badge(app, &track.path),
                text(mmss(track.duration_secs.unwrap_or(0.0)))
                    .size(st::type_size::CAPTION)
                    .color(st::muted())
            ]
            .spacing(st::space::SM)
            .align_y(Alignment::Center)
        } else {
            row![
                row_number(visible, current),
                app.track_covers.view(&track.path),
                container(short_text(&track.title, 32, st::type_size::BODY))
                    .width(Length::FillPortion(3))
                    .clip(true),
                container(short_text(
                    track.artist.as_deref().unwrap_or("—"),
                    22,
                    st::type_size::CAPTION
                ))
                .width(Length::FillPortion(2))
                .clip(true),
                container(short_text(
                    track.album.as_deref().unwrap_or("—"),
                    22,
                    st::type_size::CAPTION
                ))
                .width(Length::FillPortion(2))
                .clip(true),
                format_badge(app, &track.path),
                text(mmss(track.duration_secs.unwrap_or(0.0)))
                    .width(55)
                    .align_x(iced::alignment::Horizontal::Right)
                    .size(st::type_size::CAPTION)
                    .color(st::muted())
            ]
            .spacing(st::space::SM)
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
            focused_icon_action(
                app,
                Icon::Edit,
                "Редактор тегов / LRC",
                Some(Message::OpenEditor(track.id))
            ),
            focused_icon_action(
                app,
                Icon::Queue,
                "В плейлист",
                (!app.playlist_busy).then_some(Message::OpenPlaylistPicker(track.id))
            ),
            focused_icon_action(
                app,
                Icon::Plus,
                "Добавить в очередь",
                Some(Message::EnqueueRow(visible))
            )
        ]
        .spacing(st::space::XS);
        if app.screen == Screen::Playlists {
            if let Some(id) = app.selected_playlist {
                actions = actions
                    .push(focused_icon_action(
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
                    .push(focused_icon_action(
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
                    .push(focused_icon_action(
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
    let controls = column![
        text("Очередь").size(st::type_size::TITLE),
        row![
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
        .spacing(st::space::SM)
    ]
    .spacing(10);
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
                        container(
                            tooltip(
                                text(ellipsis(
                                    &label,
                                    if app.album_grid.viewport.width < 1040. {
                                        32
                                    } else {
                                        64
                                    }
                                ))
                                .size(st::type_size::BODY)
                                .wrapping(iced::widget::text::Wrapping::None),
                                text(label).size(st::type_size::BODY),
                                tooltip::Position::Top
                            )
                            .style(st::card)
                        )
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
        return container(text("Выберите трек в библиотеке").size(st::type_size::TITLE))
            .center(Length::Fill)
            .into();
    };
    let art_size = ((app.album_grid.viewport.width - 112.) * 0.40)
        .min(app.album_grid.viewport.height - 300.)
        .clamp(96., 400.);
    let media_matches = app.media.track_path.as_deref() == Some(track.path.as_str());
    let cover: Element<'_, Message> = if media_matches {
        if let Some(handle) = &app.cover {
            image(handle.clone())
                .width(art_size)
                .height(art_size)
                .content_fit(iced::ContentFit::Contain)
                .into()
        } else {
            container(icon(Icon::Music, 72.0, st::muted()))
                .center(art_size)
                .style(st::card)
                .into()
        }
    } else {
        container(icon(Icon::Music, 72.0, st::muted()))
            .center(art_size)
            .style(st::card)
            .into()
    };
    let mut details = column![
        cover,
        row![
            container(short_text(&track.title, 26, st::type_size::TITLE))
                .width(Length::Fill)
                .clip(true),
            collections::favorite_button(app, track)
        ]
        .align_y(Alignment::Center),
        text(track.artist.as_deref().unwrap_or("Неизвестный исполнитель"))
            .size(st::type_size::EMPHASIS)
            .color(st::muted()),
        text(track.album.as_deref().unwrap_or(""))
            .size(st::type_size::BODY)
            .color(st::muted()),
        focus::action(
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
            .padding(st::space::MD)
            .style(st::primary)
            .on_press(Message::TogglePlay),
            focus_enabled(app).then_some(Message::TogglePlay)
        )
    ]
    .spacing(14)
    .width(Length::Fill);
    let output = &app.snapshot.output_status;
    if output.mode == muzon_core::domain::playback_settings::OutputMode::BitPerfect {
        let label = if let Some(format) = &output.negotiated_format {
            format!(
                "ALSA · {} Гц · {} · {} кан.",
                format.rate, format.format, format.channels
            )
        } else {
            "ALSA · формат выхода не получен".to_owned()
        };
        let status = output
            .error
            .as_deref()
            .unwrap_or(if output.bit_perfect_verified {
                "Bit-perfect подтверждён"
            } else {
                "Bit-perfect не подтверждён"
            });
        details = details
            .push(text(label).size(st::type_size::CAPTION).color(st::muted()))
            .push(
                tooltip(
                    text(status)
                        .size(st::type_size::CAPTION)
                        .color(if output.error.is_some() {
                            st::danger()
                        } else {
                            st::muted()
                        }),
                    text(output.device.as_deref().unwrap_or("Устройство не выбрано")),
                    tooltip::Position::Top,
                )
                .style(st::card),
            );
    }
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
    let details = st::scrollable(
        container(details)
            .padding(st::space::LG)
            .width(art_size + 32.)
            .style(|theme| {
                let mut style = st::card(theme);
                style.border.radius = st::radius::PANEL.into();
                style
            }),
    )
    .width(art_size + 32.)
    .height(Length::Fill)
    .style(st::scroll);
    let mut lines = column![text("Текст песни")
        .size(st::type_size::CAPTION)
        .color(st::muted())]
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
                    .size(if Some(index) == active {
                        st::type_size::TITLE
                    } else {
                        st::type_size::EMPHASIS
                    })
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
            .size(st::type_size::BODY),
        );
    }
    let right: Element<'_, Message> =
        if media_matches && app.media.loaded && app.media.lyrics.is_empty() {
            let mut preview = column![text("Далее в очереди").size(st::type_size::EMPHASIS)]
                .spacing(st::space::SM);
            for (visible, &index) in app.snapshot.display.iter().enumerate().take(12) {
                let queued = &app.snapshot.queue[index];
                preview = preview.push(
                    button(
                        container(short_text(&queued.title, 30, st::type_size::BODY))
                            .width(Length::Fill)
                            .clip(true),
                    )
                    .style(st::quiet)
                    .padding(st::space::SM)
                    .width(Length::Fill)
                    .on_press(Message::QueueRowClicked(visible)),
                );
            }
            preview = preview.push(
                button("Вся очередь")
                    .style(st::secondary)
                    .padding(st::space::SM)
                    .on_press(Message::ScreenSelected(Screen::Queue)),
            );
            st::scrollable(preview).height(Length::Fill).into()
        } else {
            st::scrollable(lines).height(Length::Fill).into()
        };
    row![
        details,
        container(right)
            .padding(st::space::LG)
            .height(Length::Fill)
            .width(Length::Fill)
            .style(st::card)
    ]
    .spacing(st::space::XL)
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
        .on_press(message.clone());
    let button = focus::action(button, focus_enabled(app).then_some(message));
    if app.collection_overlay.is_some() {
        button
    } else {
        iced::widget::tooltip(
            button,
            text(label).size(st::type_size::CAPTION),
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
    let art: Element<'_, Message> = if let Some(handle) = app
        .cover
        .as_ref()
        .filter(|_| app.media.track_path.as_deref() == app.snapshot.current_path.as_deref())
    {
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
                    short_text(title, 24, st::type_size::BODY),
                    text(
                        track
                            .map(|t| t.artist.as_deref().unwrap_or("Без исполнителя"))
                            .unwrap_or("Выберите музыку")
                    )
                    .size(st::type_size::CAPTION)
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
    .padding(st::space::XS)
    .style(st::quiet)
    .on_press(Message::ScreenSelected(Screen::NowPlaying));
    let info = focus::action(
        info,
        focus_enabled(app).then_some(Message::ScreenSelected(Screen::NowPlaying)),
    );
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
    .spacing(st::space::XS)
    .align_y(Alignment::Center);
    container(
        column![
            row![
                info,
                container(
                    track
                        .map(|track| collections::favorite_button(app, track))
                        .unwrap_or_else(|| container(text("")).width(30).into())
                )
                .width(30),
                container(transport)
                    .width(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Center),
                icon(Icon::Volume, 18., st::muted()),
                slider(0.0..=1.0, app.snapshot.volume, Message::VolumeChanged)
                    .step(0.01)
                    .style(st::progress)
                    .width(76),
                control(
                    app,
                    Icon::Queue,
                    Message::ScreenSelected(Screen::Queue),
                    app.screen == Screen::Queue,
                    "Очередь"
                ),
                control(app, Icon::Stop, Message::Stop, false, "Остановить"),
            ]
            .spacing(st::space::SM)
            .align_y(Alignment::Center),
            row![
                text(mmss(position))
                    .size(st::type_size::CAPTION)
                    .color(st::muted())
                    .width(44),
                slider(0.0..=duration, position, Message::SeekChanged)
                    .step(0.1)
                    .on_release(Message::SeekCommit)
                    .style(st::progress)
                    .width(Length::Fill),
                text(mmss(duration.round()))
                    .size(st::type_size::CAPTION)
                    .color(st::muted())
                    .width(44),
            ]
            .spacing(st::space::SM)
            .align_y(Alignment::Center)
        ]
        .spacing(st::space::XS),
    )
    .padding([8, 16])
    .height(104)
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
                button(text(kind.title()).size(st::type_size::CAPTION))
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
        text("Умные подборки").size(st::type_size::DISPLAY),
        tabs,
        text(app.smart.description())
            .size(st::type_size::CAPTION)
            .color(st::muted()),
        row![
            button("Слушать")
                .style(st::primary)
                .padding([8, 14])
                .on_press_maybe((!app.filtered.is_empty()).then_some(Message::PlayCollection)),
            button("В очередь")
                .style(st::secondary)
                .padding([8, 14])
                .on_press_maybe((!app.filtered.is_empty()).then_some(Message::EnqueueCollection)),
            text(format!("Треков: {}", app.filtered.len())).size(st::type_size::CAPTION)
        ]
        .spacing(st::space::MD)
        .align_y(Alignment::Center),
        library_rows(app)
    ]
    .spacing(st::space::LG)
    .height(Length::Fill)
    .into()
}
