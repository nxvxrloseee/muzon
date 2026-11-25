//! Collection screens and overlays share the same windowed track list.
use super::{
    icons::{icon, Icon},
    message::Message,
    state::{CollectionOverlay, RealApp},
    style as st,
    view::library_rows,
};
use crate::ui::{visible_window, ROW_HEIGHT};
use iced::{
    widget::{button, column, container, row, text, text_input, tooltip, vertical_space},
    Alignment, Element, Length,
};
use muzon_core::{controller::PlaylistEdit, domain::Track};

pub fn icon_action(
    app: &RealApp,
    glyph: Icon,
    label: &'static str,
    message: Option<Message>,
) -> Element<'static, Message> {
    let button = button(container(icon(glyph, 16.0, st::muted())).center(16))
        .width(30)
        .height(ROW_HEIGHT)
        .padding([0, 7])
        .style(st::quiet)
        .on_press_maybe(message);
    if app.collection_overlay.is_some() || app.editor.is_some() {
        button.into()
    } else {
        tooltip(
            button,
            text(label).size(st::type_size::CAPTION),
            tooltip::Position::Top,
        )
        .into()
    }
}

pub fn favorite_button<'a>(app: &RealApp, track: &Track) -> Element<'a, Message> {
    let label = if track.is_favorite {
        "Убрать из избранного"
    } else {
        "В избранное"
    };
    let button = button(
        container(icon(
            if track.is_favorite {
                Icon::HeartFilled
            } else {
                Icon::Heart
            },
            16.0,
            if track.is_favorite {
                st::heart()
            } else {
                st::muted()
            },
        ))
        .center(16),
    )
    .width(30)
    .height(ROW_HEIGHT)
    .padding([0, 7])
    .style(st::quiet)
    .on_press_maybe(
        (!app.pending_favorites.contains(&track.id)).then_some(Message::ToggleFavorite(track.id)),
    );
    if app.collection_overlay.is_some() || app.editor.is_some() {
        button.into()
    } else {
        tooltip(
            button,
            text(label).size(st::type_size::CAPTION),
            tooltip::Position::Top,
        )
        .into()
    }
}

fn collection_controls(app: &RealApp) -> Element<'_, Message> {
    row![
        button(text("Слушать всё").size(st::type_size::BODY))
            .style(st::primary)
            .padding([9, 14])
            .on_press_maybe((!app.filtered.is_empty()).then_some(Message::PlayCollection)),
        button(text("В очередь").size(st::type_size::BODY))
            .style(st::secondary)
            .padding([9, 14])
            .on_press_maybe((!app.filtered.is_empty()).then_some(Message::EnqueueCollection)),
        text(format!("Треков: {}", app.filtered.len()))
            .size(st::type_size::CAPTION)
            .color(st::muted())
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}
pub fn favorites(app: &RealApp) -> Element<'_, Message> {
    let mut content = column![
        text("Избранное").size(st::type_size::DISPLAY),
        collection_controls(app)
    ]
    .spacing(20)
    .height(Length::Fill);
    if app.filtered.is_empty() {
        content = content.push(
            container(
                column![
                    icon(Icon::Heart, 48.0, st::heart()),
                    text(if app.query.trim().is_empty() {
                        "Отмечайте любимые треки сердечком"
                    } else {
                        "По вашему запросу ничего не найдено"
                    })
                    .size(st::type_size::EMPHASIS)
                    .color(st::muted())
                ]
                .spacing(18)
                .align_x(Alignment::Center),
            )
            .center(Length::Fill),
        );
    } else {
        content = content.push(library_rows(app));
    }
    content.into()
}
fn create_form(app: &RealApp) -> Element<'_, Message> {
    row![
        text_input("Название нового плейлиста", &app.new_playlist_name)
            .size(st::type_size::BODY)
            .padding(10)
            .style(st::input)
            .on_input(Message::NewPlaylistName)
            .on_submit(Message::CreatePlaylist),
        button(
            text(if app.playlist_busy {
                "Сохранение…"
            } else if matches!(app.collection_overlay, Some(CollectionOverlay::AddTrack(_))) {
                "Создать и добавить"
            } else {
                "Создать"
            })
            .size(st::type_size::BODY)
        )
        .style(st::primary)
        .padding([10, 14])
        .on_press_maybe(
            (!app.playlist_busy && !app.new_playlist_name.trim().is_empty())
                .then_some(Message::CreatePlaylist)
        )
    ]
    .spacing(st::space::SM)
    .align_y(Alignment::Center)
    .into()
}
pub fn playlists(app: &RealApp) -> Element<'_, Message> {
    let total = app.snapshot.playlists.len();
    let window = visible_window(app.playlist_scroll, total);
    let mut list = column![vertical_space().height(window.start as f32 * ROW_HEIGHT)].width(190);
    for playlist in app
        .snapshot
        .playlists
        .iter()
        .skip(window.start)
        .take(window.len())
    {
        let active = app.selected_playlist == Some(playlist.id);
        list = list.push(
            button(
                container(
                    row![
                        text(&playlist.name)
                            .size(st::type_size::BODY)
                            .width(Length::Fill)
                            .wrapping(iced::widget::text::Wrapping::None),
                        text(playlist.track_ids.len().to_string())
                            .size(st::type_size::CAPTION)
                            .color(st::muted())
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .height(ROW_HEIGHT)
                .align_y(Alignment::Center)
                .clip(true)
                .padding([0, 10]),
            )
            .width(Length::Fill)
            .padding(0)
            .style(if active { st::selected } else { st::quiet })
            .on_press(Message::PlaylistSelected(playlist.id)),
        );
    }
    list = list.push(vertical_space().height((total - window.end) as f32 * ROW_HEIGHT));
    let detail: Element<'_, Message> = if let Some(playlist) = app
        .selected_playlist
        .and_then(|id| app.snapshot.playlists.iter().find(|p| p.id == id))
    {
        let mut detail = column![
            row![
                text(&playlist.name)
                    .size(st::type_size::TITLE)
                    .width(Length::Fill),
                button(text("Удалить").size(st::type_size::CAPTION))
                    .style(st::quiet)
                    .on_press_maybe(
                        (!app.playlist_busy).then_some(Message::AskDeletePlaylist(playlist.id))
                    )
            ]
            .spacing(st::space::SM)
            .align_y(Alignment::Center),
            row![
                text_input("Новое название", &app.rename_playlist_name)
                    .style(st::input)
                    .padding(9)
                    .size(st::type_size::BODY)
                    .on_input(Message::RenamePlaylistName)
                    .on_submit(Message::RenamePlaylist),
                button(text("Переименовать").size(st::type_size::CAPTION))
                    .style(st::secondary)
                    .padding([9, 12])
                    .on_press_maybe(
                        (!app.playlist_busy && !app.rename_playlist_name.trim().is_empty())
                            .then_some(Message::RenamePlaylist)
                    )
            ]
            .spacing(st::space::SM),
            collection_controls(app)
        ]
        .spacing(14)
        .height(Length::Fill);
        if app.filtered.is_empty() {
            detail = detail.push(
                container(
                    text(if app.query.trim().is_empty() {
                        "Добавляйте треки через кнопку «В плейлист» в библиотеке"
                    } else {
                        "По вашему запросу ничего не найдено"
                    })
                    .size(st::type_size::BODY)
                    .color(st::muted()),
                )
                .center(Length::Fill),
            );
        } else {
            detail = detail.push(library_rows(app));
        }
        detail.into()
    } else {
        container(
            text(if total == 0 {
                "Создайте первый плейлист"
            } else {
                "Выберите плейлист слева"
            })
            .size(st::type_size::EMPHASIS)
            .color(st::muted()),
        )
        .center(Length::Fill)
        .into()
    };
    column![
        text("Плейлисты").size(st::type_size::DISPLAY),
        create_form(app),
        row![
            st::scrollable(list)
                .height(Length::Fill)
                .width(190)
                .on_scroll(|v| Message::PlaylistListScrolled(v.absolute_offset().y)),
            container(detail).width(Length::Fill).height(Length::Fill)
        ]
        .spacing(20)
        .height(Length::Fill)
    ]
    .spacing(20)
    .height(Length::Fill)
    .into()
}
pub fn overlay(app: &RealApp) -> Option<Element<'_, Message>> {
    let overlay = app.collection_overlay?;
    let panel: Element<'_, Message> = match overlay {
        CollectionOverlay::AddTrack(id) => {
            let mut choices = column![].width(Length::Fill);
            let total = app.snapshot.playlists.len();
            let window = visible_window(app.picker_scroll, total);
            choices = choices.push(vertical_space().height(window.start as f32 * ROW_HEIGHT));
            for playlist in app
                .snapshot
                .playlists
                .iter()
                .skip(window.start)
                .take(window.len())
            {
                let included = playlist.track_ids.contains(&id);
                choices = choices.push(
                    button(
                        row![
                            text(&playlist.name)
                                .width(Length::Fill)
                                .size(st::type_size::BODY),
                            text(if included {
                                "Уже добавлен"
                            } else {
                                "Добавить"
                            })
                            .size(st::type_size::CAPTION)
                            .color(st::muted())
                        ]
                        .spacing(st::space::MD),
                    )
                    .padding([0, 12])
                    .height(ROW_HEIGHT)
                    .width(Length::Fill)
                    .style(st::secondary)
                    .on_press_maybe(
                        (!app.playlist_busy && !included).then_some(Message::EditPlaylist(
                            PlaylistEdit::Add {
                                id: playlist.id,
                                tracks: vec![id],
                            },
                        )),
                    ),
                );
            }
            let title = app
                .library_track(id)
                .map(|t| t.title.as_str())
                .unwrap_or("Трек больше не доступен");
            column![
                text("Добавить в плейлист").size(st::type_size::TITLE),
                text(title).size(st::type_size::BODY).color(st::muted()),
                st::scrollable(
                    choices.push(vertical_space().height((total - window.end) as f32 * ROW_HEIGHT))
                )
                .height(Length::Fixed(220.0))
                .on_scroll(|v| Message::PlaylistPickerScrolled(v.absolute_offset().y)),
                text("Или создайте новый")
                    .size(st::type_size::CAPTION)
                    .color(st::muted()),
                create_form(app),
                button("Закрыть")
                    .style(st::quiet)
                    .on_press(Message::CloseCollectionOverlay)
            ]
            .spacing(st::space::LG)
            .into()
        }
        CollectionOverlay::DeletePlaylist(id) => {
            let name = app
                .snapshot
                .playlists
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.name.as_str())
                .unwrap_or("Плейлист");
            column![
                text("Удалить плейлист?").size(st::type_size::TITLE),
                text(name).size(st::type_size::EMPHASIS),
                text("Музыкальные файлы и текущая очередь сохранятся.")
                    .size(st::type_size::BODY)
                    .color(st::muted()),
                row![
                    button("Отмена")
                        .style(st::secondary)
                        .padding([10, 14])
                        .on_press_maybe(
                            (!app.playlist_busy).then_some(Message::CloseCollectionOverlay)
                        ),
                    button("Удалить")
                        .style(st::primary)
                        .padding([10, 14])
                        .on_press_maybe(
                            (!app.playlist_busy)
                                .then_some(Message::EditPlaylist(PlaylistEdit::Delete { id }))
                        )
                ]
                .spacing(10)
            ]
            .spacing(18)
            .into()
        }
    };
    Some(
        container(
            container(panel)
                .padding(st::space::XL)
                .width(460)
                .style(st::card),
        )
        .center(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Color::from_rgba(0., 0., 0., 0.7).into()),
            ..Default::default()
        })
        .into(),
    )
}
