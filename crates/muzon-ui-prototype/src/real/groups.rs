//! Cached group membership uses library indices, never cloned Track objects.
use super::{
    message::Message,
    state::{RealApp, Screen},
    style as st,
    view::library_rows,
};
use crate::ui::{visible_window, ROW_HEIGHT};
use iced::{
    widget::{button, column, container, row, scrollable, text, vertical_space},
    Alignment, Element, Length,
};
use muzon_core::domain::Track;
use std::collections::BTreeMap;

pub type GroupKey = (String, String);
pub struct Group {
    pub key: GroupKey,
    pub title: String,
    pub subtitle: String,
    pub indices: Vec<usize>,
    search: String,
    year: Option<i32>,
}
pub fn is_grouped(screen: Screen) -> bool {
    matches!(screen, Screen::Albums | Screen::Artists | Screen::Genres)
}
pub fn build(library: &[Track], screen: Screen) -> Vec<Group> {
    let mut groups: BTreeMap<GroupKey, Group> = BTreeMap::new();
    for (index, track) in library.iter().enumerate() {
        let artist = track.artist.as_deref().unwrap_or("Неизвестный исполнитель");
        let album = track.album.as_deref().unwrap_or("Неизвестный альбом");
        let genre = track.genre.as_deref().unwrap_or("Без жанра");
        let (key, title, subtitle) = match screen {
            Screen::Albums => ((artist.to_owned(), album.to_owned()), album, artist),
            Screen::Artists => ((artist.to_owned(), String::new()), artist, ""),
            Screen::Genres => ((genre.to_owned(), String::new()), genre, ""),
            _ => return Vec::new(),
        };
        let group = groups.entry(key.clone()).or_insert_with(|| Group {
            key,
            title: title.to_owned(),
            subtitle: subtitle.to_owned(),
            indices: Vec::new(),
            search: format!("{title}\n{subtitle}").to_lowercase(),
            year: None,
        });
        group.indices.push(index);
        if let Some(year) = track.year {
            group.year = Some(group.year.map_or(year, |old| old.min(year)));
        }
    }
    let mut groups: Vec<Group> = groups.into_values().collect();
    for group in &mut groups {
        match screen {
            Screen::Albums => group
                .indices
                .sort_by_key(|&i| library[i].track_no.unwrap_or(0)),
            Screen::Artists => group.indices.sort_by(|&a, &b| {
                let a = &library[a];
                let b = &library[b];
                a.year
                    .unwrap_or(i32::MAX)
                    .cmp(&b.year.unwrap_or(i32::MAX))
                    .then_with(|| a.album.cmp(&b.album))
                    .then_with(|| a.track_no.unwrap_or(0).cmp(&b.track_no.unwrap_or(0)))
            }),
            _ => {}
        }
    }
    groups.sort_by(|a, b| {
        if screen == Screen::Albums {
            a.subtitle
                .to_lowercase()
                .cmp(&b.subtitle.to_lowercase())
                .then_with(|| a.year.unwrap_or(i32::MAX).cmp(&b.year.unwrap_or(i32::MAX)))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        } else {
            (screen == Screen::Genres && a.title == "Без жанра")
                .cmp(&(screen == Screen::Genres && b.title == "Без жанра"))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        }
    });
    groups
}
impl Group {
    pub fn matches(&self, query: &str, search: &[String]) -> bool {
        query.is_empty()
            || self.search.contains(query)
            || self
                .indices
                .iter()
                .any(|&i| search.get(i).is_some_and(|s| s.contains(query)))
    }
    pub fn tracks(&self, query: &str, search: &[String]) -> Vec<usize> {
        let group_matches = query.is_empty() || self.search.contains(query);
        self.indices
            .iter()
            .copied()
            .filter(|&i| group_matches || search.get(i).is_some_and(|s| s.contains(query)))
            .collect()
    }
}
pub fn view(app: &RealApp) -> Element<'_, Message> {
    if app.screen == Screen::Albums && app.album_grid.browsing {
        return super::album_grid::view(app);
    }
    let title = match app.screen {
        Screen::Albums => "Альбомы",
        Screen::Artists => "Исполнители",
        _ => "Жанры",
    };
    let window = visible_window(app.group_scroll, app.group_rows.len());
    let mut list =
        column![vertical_space().height(window.start as f32 * ROW_HEIGHT)].width(Length::Fill);
    for visible in window.clone() {
        let index = app.group_rows[visible];
        let group = &app.groups[index];
        let selected = app.selected_group.as_ref() == Some(&group.key);
        list = list.push(
            button(
                container(
                    row![
                        text(&group.title).size(13).width(Length::Fill),
                        text(group.indices.len()).size(11).color(st::muted())
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .height(ROW_HEIGHT)
                .align_y(iced::alignment::Vertical::Center)
                .clip(true)
                .padding([0, 8]),
            )
            .width(Length::Fill)
            .padding(0)
            .style(if selected { st::selected } else { st::quiet })
            .on_press(Message::GroupSelected(index)),
        );
    }
    list =
        list.push(vertical_space().height((app.group_rows.len() - window.end) as f32 * ROW_HEIGHT));
    let groups = scrollable(list)
        .id(scrollable::Id::new("muzon-groups"))
        .height(Length::Fill)
        .on_scroll(|v| Message::GroupsScrolled(v.absolute_offset().y));
    let selected = app
        .selected_group
        .as_ref()
        .and_then(|key| app.groups.iter().find(|g| &g.key == key));
    let heading = selected
        .map(|g| g.title.as_str())
        .unwrap_or("Нет результатов");
    let subtitle = selected.map(|g| g.subtitle.as_str()).unwrap_or("");
    let detail = column![
        text(heading).size(24),
        text(subtitle).size(12).color(st::muted()),
        row![
            button("Слушать")
                .style(st::primary)
                .padding([8, 12])
                .on_press_maybe((!app.filtered.is_empty()).then_some(Message::PlayCollection)),
            button("В очередь")
                .style(st::secondary)
                .padding([8, 12])
                .on_press_maybe((!app.filtered.is_empty()).then_some(Message::EnqueueCollection)),
            text(format!("Треков: {}", app.filtered.len())).size(12)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        library_rows(app)
    ]
    .spacing(12)
    .height(Length::Fill)
    .width(Length::Fill);
    if app.screen == Screen::Albums {
        return column![
            button("← Все альбомы")
                .style(st::quiet)
                .padding([8, 12])
                .on_press(Message::AlbumsBack),
            detail
        ]
        .spacing(16)
        .height(Length::Fill)
        .into();
    }
    column![
        text(title).size(28),
        row![container(groups).width(200), detail]
            .spacing(20)
            .height(Length::Fill)
    ]
    .spacing(18)
    .height(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track(id: i32, artist: &str, album: &str, number: i32) -> Track {
        Track {
            id,
            path: format!("/{id}"),
            title: format!("Song {id}"),
            artist: Some(artist.into()),
            album: Some(album.into()),
            duration_secs: Some(60.0),
            track_no: Some(number),
            genre: None,
            year: None,
            is_favorite: false,
            tempo: 1.0,
            play_count: 0,
            last_played_at: None,
            added_at: 0.0,
        }
    }
    #[test]
    fn album_keys_do_not_collide_and_tracks_follow_tag_numbers() {
        let library = vec![
            track(1, "A B", "C", 2),
            track(2, "A", "B C", 1),
            track(3, "A B", "C", 1),
        ];
        let groups = build(&library, Screen::Albums);
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups.iter().find(|g| g.subtitle == "A B").unwrap().indices,
            vec![2, 0]
        );
    }
    #[test]
    fn group_and_track_search_keep_library_indices() {
        let library = vec![
            track(1, "Artist", "Album", 1),
            track(2, "Artist", "Album", 2),
        ];
        let groups = build(&library, Screen::Albums);
        let search = vec!["first song".into(), "second song".into()];
        assert_eq!(groups[0].tracks("album", &search), vec![0, 1]);
        assert_eq!(groups[0].tracks("second", &search), vec![1]);
        assert!(!groups[0].matches("missing", &search));
    }
    #[test]
    fn unknown_genre_is_last_and_known_years_lead_discography() {
        let mut library = vec![
            track(1, "Artist", "Z", 1),
            track(2, "Artist", "A", 2),
            track(3, "Artist", "A", 1),
        ];
        library[0].genre = Some("Rock".into());
        library[1].year = Some(1999);
        library[2].year = Some(1999);
        let genres = build(&library, Screen::Genres);
        assert_eq!(genres.last().unwrap().title, "Без жанра");
        let artists = build(&library, Screen::Artists);
        assert_eq!(artists[0].indices, vec![2, 1, 0]);
    }
}
