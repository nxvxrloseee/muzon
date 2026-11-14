//! Widget building: pure functions from [`App`] to elements.
//!
//! The two long lists (library, queue) are *windowed*: only a fixed-size
//! window of rows around the scroll position is built into widgets, while
//! top/bottom spacers keep the scrollable's content at its full height. See
//! `visible_window` and the README for the approach and its limits.

use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, progress_bar, row, scrollable, text, text_input, vertical_space,
};
use iced::{Alignment, Element, Length};
use muzon_queue::RepeatMode;

use crate::demo::data::{self, Track};
use crate::demo::message::Message;
use crate::demo::state::{App, Screen, LIBRARY_LIST, QUEUE_LIST};
use crate::ui::{
    mmss, visible_window, ALBUM_WIDTH, ARTIST_WIDTH, MARKER_WIDTH, POSITION_WIDTH, ROW_HEIGHT,
    TIME_WIDTH, TITLE_WIDTH,
};

/// The whole window: sidebar + screen + bottom player bar.
pub fn view(app: &App) -> Element<'_, Message> {
    column![
        row![
            sidebar(app),
            container(match app.screen {
                Screen::Library => library_screen(app),
                Screen::NowPlaying => now_playing_screen(app),
                Screen::Queue => queue_screen(app),
            })
            .width(Length::Fill)
            .height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::Fill),
        player_bar(app),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

// --- Sidebar ---------------------------------------------------------------

fn sidebar(app: &App) -> Element<'_, Message> {
    column![
        text("Muzon").size(20),
        text_input("Search", &app.query)
            .on_input(Message::SearchChanged)
            .size(13),
        nav_button("Library", Screen::Library, app.screen == Screen::Library),
        nav_button(
            "Now Playing",
            Screen::NowPlaying,
            app.screen == Screen::NowPlaying
        ),
        nav_button("Queue", Screen::Queue, app.screen == Screen::Queue),
    ]
    .spacing(8)
    .padding(12)
    .width(Length::Fixed(230.0))
    .height(Length::Fill)
    .into()
}

fn nav_button(label: &str, screen: Screen, active: bool) -> Element<'static, Message> {
    let label = if active {
        format!("»  {label}")
    } else {
        format!("    {label}")
    };
    button(text(label).width(Length::Fill))
        .width(Length::Fill)
        .padding(6)
        .on_press(Message::ScreenSelected(screen))
        .into()
}

// --- Library screen ----------------------------------------------------------

fn library_screen(app: &App) -> Element<'_, Message> {
    let header = row![
        text("").width(Length::Fixed(MARKER_WIDTH)),
        text("Title").width(Length::Fixed(TITLE_WIDTH)),
        text("Artist").width(Length::Fixed(ARTIST_WIDTH)),
        text("Album").width(Length::Fixed(ALBUM_WIDTH)),
        text("Time").width(Length::Fixed(TIME_WIDTH)),
    ]
    .spacing(8)
    .padding([0, 8]);

    let list: Element<'_, Message> = if app.filtered.is_empty() {
        container(text("Nothing found").size(14))
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .into()
    } else {
        library_list(app)
    };

    column![
        text(format!("{} tracks", app.filtered.len())).size(12),
        header,
        list,
    ]
    .spacing(4)
    .padding([8, 12])
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// The windowed library list: spacers + only the visible window of rows.
fn library_list(app: &App) -> Element<'_, Message> {
    let total = app.filtered.len();
    let window = visible_window(app.library_scroll, total);

    let mut content = column![].width(Length::Fill);
    content =
        content.push(vertical_space().height(Length::Fixed(window.start as f32 * ROW_HEIGHT)));
    for row_index in window.clone() {
        let library_index = app.filtered[row_index];
        content = content.push(track_row(
            row_index,
            &app.library[library_index],
            app.current_library_index() == Some(library_index),
        ));
    }
    content = content
        .push(vertical_space().height(Length::Fixed((total - window.end) as f32 * ROW_HEIGHT)));

    scrollable(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .on_scroll(|viewport| Message::LibraryScrolled(viewport.absolute_offset().y))
        .id(scrollable::Id::new(LIBRARY_LIST))
        .into()
}

fn track_row(row_index: usize, track: &Track, current: bool) -> Element<'_, Message> {
    button(
        container(
            row![
                text(if current { "»" } else { "" }).width(Length::Fixed(MARKER_WIDTH)),
                text(&track.title).width(Length::Fixed(TITLE_WIDTH)),
                text(&track.artist).width(Length::Fixed(ARTIST_WIDTH)),
                text(&track.album).width(Length::Fixed(ALBUM_WIDTH)),
                text(mmss(track.duration_secs as f64)).width(Length::Fixed(TIME_WIDTH)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fixed(ROW_HEIGHT))
        .align_y(Vertical::Center)
        .padding([0, 8]),
    )
    .width(Length::Fill)
    .padding(0)
    .on_press(Message::TrackRowClicked(row_index))
    .into()
}

// --- Now Playing screen ------------------------------------------------------

fn now_playing_screen(app: &App) -> Element<'_, Message> {
    let Some(track) = app.current_track() else {
        return container(text("Nothing playing").size(16))
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .into();
    };

    let duration = (app.duration_secs() as f32).max(1.0);
    let position = (app.position_secs as f32).min(duration);

    let mut lyrics = column![].spacing(4);
    for line in data::lyrics_for(track) {
        lyrics = lyrics.push(text(line).size(14));
    }

    column![
        container(text("No cover art").size(13))
            .center(260)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgb8(
                    0x2e, 0x2e, 0x3d,
                ))),
                ..Default::default()
            }),
        text(&track.title).size(26),
        text(&track.artist).size(15),
        text(&track.album).size(13),
        row![
            text(mmss(app.position_secs as f64)).size(11),
            progress_bar(0.0..=duration, position).width(Length::Fill),
            text(mmss(app.duration_secs() as f64)).size(11),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill),
        row![
            button("Prev").on_press(Message::Previous).padding(8),
            button(if app.playing { "Pause" } else { "Play" })
                .on_press(Message::TogglePlay)
                .padding(8),
            button("Next").on_press(Message::Next).padding(8),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        text("Lyrics (synthetic placeholder)").size(12),
        lyrics,
    ]
    .spacing(12)
    .padding(24)
    .width(Length::Fill)
    .align_x(Alignment::Center)
    .into()
}

// --- Queue screen ------------------------------------------------------------

fn queue_screen(app: &App) -> Element<'_, Message> {
    let list: Element<'_, Message> = if app.queue.queue.is_empty() {
        container(text("The queue is empty").size(14))
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .into()
    } else {
        queue_list(app)
    };

    column![
        row![
            text(format!("Queue — {} tracks", app.queue.queue.len())).size(16),
            button(if app.shuffle {
                "Shuffle: on"
            } else {
                "Shuffle: off"
            })
            .on_press(Message::ToggleShuffle)
            .padding(6),
            button(match app.repeat {
                RepeatMode::Off => "Repeat: off",
                RepeatMode::All => "Repeat: all",
                RepeatMode::One => "Repeat: one",
            })
            .on_press(Message::CycleRepeat)
            .padding(6),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        list,
    ]
    .spacing(4)
    .padding([8, 12])
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// The windowed queue view, in `muzon-queue`'s display order (playing element
/// first). Clicks report the display position; the update maps it back to the
/// queue element through the same crate output.
fn queue_list(app: &App) -> Element<'_, Message> {
    let display = app.queue_display();
    let total = display.len();
    let window = visible_window(app.queue_scroll, total);

    let mut content = column![].width(Length::Fill);
    content =
        content.push(vertical_space().height(Length::Fixed(window.start as f32 * ROW_HEIGHT)));
    for display_position in window.clone() {
        let queue_index = display[display_position];
        content = content.push(queue_row(
            display_position,
            &app.library[queue_index],
            app.current_library_index() == Some(queue_index),
        ));
    }
    content = content
        .push(vertical_space().height(Length::Fixed((total - window.end) as f32 * ROW_HEIGHT)));

    scrollable(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .on_scroll(|viewport| Message::QueueScrolled(viewport.absolute_offset().y))
        .id(scrollable::Id::new(QUEUE_LIST))
        .into()
}

fn queue_row(display_position: usize, track: &Track, current: bool) -> Element<'_, Message> {
    button(
        container(
            row![
                text(display_position + 1).width(Length::Fixed(POSITION_WIDTH)),
                text(if current { "»" } else { "" }).width(Length::Fixed(MARKER_WIDTH)),
                text(&track.title).width(Length::Fixed(TITLE_WIDTH)),
                text(&track.artist).width(Length::Fixed(ARTIST_WIDTH)),
                text(mmss(track.duration_secs as f64)).width(Length::Fixed(TIME_WIDTH)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fixed(ROW_HEIGHT))
        .align_y(Vertical::Center)
        .padding([0, 8]),
    )
    .width(Length::Fill)
    .padding(0)
    .on_press(Message::QueueRowClicked(display_position))
    .into()
}

// --- Player bar --------------------------------------------------------------

fn player_bar(app: &App) -> Element<'_, Message> {
    let (title, artist) = match app.current_track() {
        Some(track) => (track.title.as_str(), track.artist.as_str()),
        None => ("Nothing playing", ""),
    };
    let duration = (app.duration_secs() as f32).max(1.0);
    let position = (app.position_secs as f32).min(duration);

    row![
        column![text(title).size(13), text(artist).size(11),].width(Length::Fixed(200.0)),
        button("Prev").on_press(Message::Previous).padding(6),
        button(if app.playing { "Pause" } else { "Play" })
            .on_press(Message::TogglePlay)
            .padding(6),
        button("Next").on_press(Message::Next).padding(6),
        text(mmss(app.position_secs as f64)).size(10),
        progress_bar(0.0..=duration, position).width(Length::Fill),
        text(mmss(app.duration_secs() as f64)).size(10),
    ]
    .spacing(10)
    .padding([8, 12])
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::demo::data;
    use crate::demo::state::App;

    /// Regression: the windowed lists live inside a `scrollable`, whose
    /// constructor asserts that the content does not hint a `Fill` height.
    /// A row hinting `Fill` (via `center_y(Length::Fill)`, which *sets* the
    /// container's height, letting it leak through `Button::new`'s default
    /// sizing and `Column::push`'s `enclose`) used to panic right here, at
    /// construction time, before any renderer was involved.
    #[test]
    fn windowed_lists_can_be_built_inside_a_scrollable() {
        let mut app = App::with_library(data::generate_tracks(100));
        app.queue.cursor = Some(1);
        app.playing = true;

        let _: Element<'_, Message> = library_list(&app);
        let _: Element<'_, Message> = queue_list(&app);
    }
}
