//! One bounded results list; IDs/keys are resolved again when the action runs.
use super::{
    groups,
    message::Message,
    smart::Smart,
    state::{RealApp, Screen},
    style as st,
};
use iced::{
    widget::{button, column, container, row, scrollable, text, text_input},
    Element, Length,
};
use muzon_core::controller::Snapshot;
#[derive(Clone)]
pub enum Action {
    Screen(Screen),
    Track(i32),
    Group(Screen, groups::GroupKey),
    Playlist(i32),
    Toggle,
    Next,
    Previous,
    Rescan,
    PickFolder,
    Quit,
    EditCurrent,
    Smart(Smart),
}
pub struct Entry {
    pub label: String,
    pub section: &'static str,
    pub action: Action,
    search: String,
}
pub struct Palette {
    pub query: String,
    pub entries: Vec<Entry>,
    pub results: Vec<usize>,
    pub selected: usize,
}
impl Palette {
    pub fn new(snapshot: &Snapshot) -> Self {
        let mut entries = Vec::new();
        let mut add = |section, label: String, action| {
            let alias = match &action {
                Action::Screen(screen) => format!("{screen:?}"),
                Action::Smart(kind) => format!("{kind:?}"),
                Action::Toggle => "play pause toggle".into(),
                Action::Next => "next".into(),
                Action::Previous => "previous".into(),
                Action::Rescan => "rescan refresh".into(),
                Action::PickFolder => "folder browse directory".into(),
                Action::Quit => "quit exit".into(),
                Action::EditCurrent => "edit tags lyrics".into(),
                _ => String::new(),
            };
            let search = format!("{label} {alias}").to_lowercase();
            entries.push(Entry {
                label,
                section,
                action,
                search,
            });
        };
        for (label, screen) in [
            ("Библиотека", Screen::Library),
            ("Альбомы", Screen::Albums),
            ("Исполнители", Screen::Artists),
            ("Жанры", Screen::Genres),
            ("Избранное", Screen::Favorites),
            ("Подборки", Screen::Smart),
            ("Плейлисты", Screen::Playlists),
            ("Настройки · Звук", Screen::Audio),
            ("Таймер сна", Screen::Sleep),
            ("Клавиши", Screen::Shortcuts),
            ("Внешний вид", Screen::Appearance),
            ("Облако", Screen::Cloud),
            ("Скробблинг", Screen::Scrobble),
            ("Сейчас играет", Screen::NowPlaying),
            ("Очередь", Screen::Queue),
        ] {
            add("Раздел", label.into(), Action::Screen(screen));
        }
        for (label, action) in [
            ("Пауза / воспроизведение", Action::Toggle),
            ("Следующий трек", Action::Next),
            ("Предыдущий трек", Action::Previous),
            ("Выбрать папку с музыкой", Action::PickFolder),
            ("Обновить библиотеку", Action::Rescan),
            ("Выйти из Muzon", Action::Quit),
            ("Редактировать текущий трек", Action::EditCurrent),
        ] {
            add("Команда", label.into(), action);
        }
        for kind in Smart::ALL {
            add("Подборка", kind.title().into(), Action::Smart(kind));
        }
        for track in snapshot.library.iter() {
            add(
                "Трек",
                format!(
                    "{} — {}",
                    track.title,
                    track.artist.as_deref().unwrap_or("Без исполнителя")
                ),
                Action::Track(track.id),
            );
        }
        for screen in [Screen::Albums, Screen::Artists] {
            for group in groups::build(&snapshot.library, screen) {
                let section = if screen == Screen::Albums {
                    "Альбом"
                } else {
                    "Исполнитель"
                };
                add(
                    section,
                    if group.subtitle.is_empty() {
                        group.title
                    } else {
                        format!("{} · {}", group.title, group.subtitle)
                    },
                    Action::Group(screen, group.key),
                );
            }
        }
        for playlist in snapshot.playlists.iter() {
            add(
                "Плейлист",
                playlist.name.clone(),
                Action::Playlist(playlist.id),
            );
        }
        let mut palette = Self {
            query: String::new(),
            entries,
            results: Vec::new(),
            selected: 0,
        };
        palette.filter();
        palette
    }
    pub fn filter(&mut self) {
        let query = self.query.trim().to_lowercase();
        let mut counts = std::collections::HashMap::new();
        self.results = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| query.is_empty() || entry.search.contains(&query))
            .filter(|(_, entry)| {
                let count = counts.entry(entry.section).or_insert(0);
                *count += 1;
                *count <= 8
            })
            .map(|(i, _)| i)
            .take(64)
            .collect();
        self.selected = 0;
    }
}
pub fn overlay(app: &RealApp) -> Option<Element<'_, Message>> {
    let palette = app.palette.as_ref()?;
    let mut rows = column![].spacing(2);
    for (position, &index) in palette.results.iter().enumerate() {
        let entry = &palette.entries[index];
        rows = rows.push(
            button(
                row![
                    text(entry.section).size(11).color(st::muted()).width(100),
                    text(&entry.label).size(13).width(Length::Fill)
                ]
                .spacing(10),
            )
            .height(40)
            .width(Length::Fill)
            .style(if position == palette.selected {
                st::selected
            } else {
                st::quiet
            })
            .on_press(Message::PaletteRun(position)),
        );
    }
    if palette.results.is_empty() {
        rows = rows.push(text("Ничего не найдено").size(14).color(st::muted()));
    }
    let panel = column![
        row![
            text("Команды и поиск").size(21).width(Length::Fill),
            button("Закрыть")
                .style(st::quiet)
                .on_press(Message::PaletteClose)
        ],
        text_input(
            "Треки, альбомы, исполнители, плейлисты, команды…",
            &palette.query
        )
        .id(iced::widget::text_input::Id::new("muzon-palette-search"))
        .padding(12)
        .style(st::input)
        .on_input(Message::PaletteQuery)
        .on_submit(Message::PaletteExecute),
        scrollable(rows)
            .id(scrollable::Id::new("muzon-palette-results"))
            .height(Length::Fill)
            .style(st::scroll),
        text("Вверх / вниз — выбор, Enter — выполнить, Escape — закрыть")
            .size(11)
            .color(st::muted())
    ]
    .spacing(14)
    .height(Length::Fill);
    Some(
        container(
            container(panel)
                .padding(22)
                .max_width(760)
                .height(Length::Fill)
                .style(st::card),
        )
        .padding(35)
        .center(Length::Fill)
        .style(|_| st::surface(iced::Color::from_rgba(0., 0., 0., 0.78)))
        .into(),
    )
}
