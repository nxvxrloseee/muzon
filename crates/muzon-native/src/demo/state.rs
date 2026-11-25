//! Application state and the update logic.
//!
//! The queue is a `muzon-queue` [`QueueShape`] whose elements are library
//! indices; every navigation decision (next/previous/repeat/shuffle) is asked
//! of that crate and merely *executed* here — nothing about the order is
//! recomputed in the UI. Search is a separate cache of library indices and
//! structurally cannot touch the queue.

use iced::widget::scrollable;
use iced::{Subscription, Task};
use muzon_queue::{
    display_order_indices, identity_indices, next_action, previous_action, Navigation, QueueShape,
    RepeatMode,
};

use crate::demo::data::{self, Track};
use crate::demo::message::Message;

/// `scrollable::Id` of the library list, so search can scroll it back to the
/// top and keep the windowed list consistent with the widget's real offset.
pub const LIBRARY_LIST: &str = "muzon-native/library";
/// `scrollable::Id` of the queue list. See [`LIBRARY_LIST`].
pub const QUEUE_LIST: &str = "muzon-native/queue";

/// Which main screen the content pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// The filterable track list.
    Library,
    /// Cover placeholder, progress and lyrics for the playing track.
    NowPlaying,
    /// The queue in `muzon-queue`'s display order, with the mode toggles.
    Queue,
}

/// The whole prototype state: data, queue, playback simulation and which
/// screen is open.
pub struct App {
    pub screen: Screen,
    /// The synthetic library; indices everywhere else point into this.
    pub library: Vec<Track>,
    /// Current search text.
    pub query: String,
    /// Library indices matching [`App::query`], in library order. Recomputed
    /// only when the query changes — a pure cache, never consulted by the
    /// queue.
    pub filtered: Vec<usize>,
    /// The playing queue: elements are library indices. Lives in
    /// `muzon-queue`'s shape wholesale, permutation and cursor included.
    pub queue: QueueShape<usize>,
    /// Whether the *saved* permutation (not a new one) is the active order.
    pub shuffle: bool,
    pub repeat: RepeatMode,
    /// Simulated playback: `true` between Play and Stop. Only drives the
    /// timer subscription and the progress bar — there is no audio.
    pub playing: bool,
    /// Simulated playback position in whole seconds.
    pub position_secs: u32,
    /// Last known pixel offset of the library list (drives the windowed rows).
    pub library_scroll: f32,
    /// Last known pixel offset of the queue list.
    pub queue_scroll: f32,
}

impl App {
    /// The prototype with the full 5000-track synthetic library.
    #[must_use]
    pub fn new() -> Self {
        Self::with_library(data::generate_library())
    }

    /// A prototype over an arbitrary library — the test seam.
    #[must_use]
    pub fn with_library(library: Vec<Track>) -> Self {
        let queue = QueueShape::new((0..library.len()).collect());
        let filtered = (0..library.len()).collect();
        Self {
            screen: Screen::Library,
            library,
            query: String::new(),
            filtered,
            // The whole library queued, sequential order, nothing playing.
            queue,
            shuffle: false,
            repeat: RepeatMode::Off,
            playing: false,
            position_secs: 0,
            library_scroll: 0.0,
            queue_scroll: 0.0,
        }
    }

    /// The track under the queue cursor, if anything is playing.
    ///
    /// The cursor is a position in the queue, and a queue element is a
    /// library index — the two numberings coincide only on the identity
    /// queue, never after `play_from_filtered_row`.
    #[must_use]
    pub fn current_library_index(&self) -> Option<usize> {
        let queue_index = self.queue.cursor?;
        self.queue.queue.get(queue_index).copied()
    }

    /// The track under the queue cursor, if anything is playing.
    #[must_use]
    pub fn current_track(&self) -> Option<&Track> {
        let library_index = self.current_library_index()?;
        self.library.get(library_index)
    }

    /// Duration of the playing track, or 0 with nothing playing.
    #[must_use]
    pub fn duration_secs(&self) -> u32 {
        self.current_track().map_or(0, |track| track.duration_secs)
    }

    /// The queue view's order — `muzon-queue`'s [`display_order_indices`]:
    /// the active play order rotated so the playing element is first.
    #[must_use]
    pub fn queue_display(&self) -> Vec<usize> {
        display_order_indices(&self.queue, self.shuffle)
    }

    /// The library index behind a row of the filtered list. Rows and library
    /// indices are deliberately distinct: filtering must not renumber the
    /// queue.
    #[must_use]
    pub fn filtered_row_to_library_index(&self, row: usize) -> Option<usize> {
        self.filtered.get(row).copied()
    }

    /// Re-filter the library for the current query: a case-insensitive
    /// substring match over title, artist and album. Touches only
    /// [`App::query`] and [`App::filtered`] — the queue is never involved.
    pub fn refresh_filter(&mut self) {
        let query = self.query.trim().to_lowercase();
        self.filtered = self
            .library
            .iter()
            .enumerate()
            .filter(|(_, track)| {
                query.is_empty()
                    || track.title.to_lowercase().contains(&query)
                    || track.artist.to_lowercase().contains(&query)
                    || track.album.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect();
    }

    /// Move the cursor within the existing queue and start "playing".
    pub fn play_queue_index(&mut self, index: usize) {
        if index >= self.queue.queue.len() {
            return;
        }
        self.queue.cursor = Some(index);
        self.position_secs = 0;
        self.playing = true;
    }

    /// Replace the queue with the currently filtered list — the view the user
    /// is playing from, like `setQueue(visible, track)` in the real app — and
    /// start the clicked row. The cursor is an index into that list, which is
    /// exactly the clicked row.
    pub fn play_from_filtered_row(&mut self, row: usize) {
        if row >= self.filtered.len() {
            return;
        }
        let elements = self.filtered.clone();
        let shuffle_order = if self.shuffle {
            data::shuffle_permutation(elements.len())
        } else {
            identity_indices(elements.len())
        };
        self.queue = QueueShape {
            queue: elements,
            shuffle_order,
            cursor: Some(row),
        };
        self.position_secs = 0;
        self.playing = true;
    }

    /// Execute a navigation decision from `muzon-queue`. The crate decided;
    /// this only applies it. [`Navigation::Stop`] pauses while keeping the
    /// cursor — the queue position is never lost.
    pub fn execute(&mut self, navigation: Navigation) {
        match navigation {
            Navigation::Play(index) => self.play_queue_index(index),
            Navigation::Stop => {
                self.playing = false;
                self.position_secs = 0;
            }
            Navigation::Noop => {}
        }
    }

    /// Skip forward, as decided by [`next_action`].
    pub fn advance(&mut self) {
        let navigation = next_action(&self.queue, self.shuffle, self.repeat);
        self.execute(navigation);
    }

    /// Skip backward, as decided by [`previous_action`].
    pub fn rewind(&mut self) {
        let navigation = previous_action(&self.queue, self.shuffle);
        self.execute(navigation);
    }

    /// One timer step: only moves while "playing", and hands the end of a
    /// track to `muzon-queue`'s navigation (repeat included).
    pub fn tick(&mut self) {
        if !self.playing {
            return;
        }
        self.position_secs += 1;
        if self.position_secs >= self.duration_secs().max(1) {
            self.advance();
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// The iced `update`: maps a [`Message`] onto [`App`].
pub fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            app.tick();
            Task::none()
        }
        Message::SearchChanged(query) => {
            app.query = query;
            app.refresh_filter();
            // Reset the scroll state with the widget: the windowed list and
            // the scrollable must agree, and results should start at the top.
            app.library_scroll = 0.0;
            scrollable::scroll_to(
                scrollable::Id::new(LIBRARY_LIST),
                scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
            )
        }
        Message::TrackRowClicked(row) => {
            app.play_from_filtered_row(row);
            Task::none()
        }
        Message::QueueRowClicked(display_position) => {
            let display = app.queue_display();
            if let Some(&queue_index) = display.get(display_position) {
                app.play_queue_index(queue_index);
            }
            Task::none()
        }
        Message::TogglePlay => {
            if app.queue.cursor.is_some() {
                app.playing = !app.playing;
            }
            Task::none()
        }
        Message::Next => {
            app.advance();
            Task::none()
        }
        Message::Previous => {
            app.rewind();
            Task::none()
        }
        Message::ToggleShuffle => {
            app.shuffle = !app.shuffle;
            Task::none()
        }
        Message::CycleRepeat => {
            app.repeat = match app.repeat {
                RepeatMode::Off => RepeatMode::All,
                RepeatMode::All => RepeatMode::One,
                RepeatMode::One => RepeatMode::Off,
            };
            Task::none()
        }
        Message::ScreenSelected(screen) => {
            app.screen = screen;
            // Fresh screen, fresh scrollables: keep the windowed lists in
            // step with the remounted widgets.
            app.library_scroll = 0.0;
            app.queue_scroll = 0.0;
            Task::none()
        }
        Message::LibraryScrolled(offset) => {
            app.library_scroll = offset;
            Task::none()
        }
        Message::QueueScrolled(offset) => {
            app.queue_scroll = offset;
            Task::none()
        }
    }
}

/// The one-second playback timer — subscribed *only* while "playing", so no
/// ticks (and no redraw pressure) exist when paused.
pub fn subscription(app: &App) -> Subscription<Message> {
    if app.playing {
        iced::time::every(std::time::Duration::from_secs(1)).map(|_| Message::Tick)
    } else {
        Subscription::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::message::Message;

    /// `update` hands its `Task` to the iced runtime; tests just drop it.
    fn send(app: &mut App, message: Message) {
        let _ = update(app, message);
    }

    fn track(id: u64, title: &str, artist: &str, album: &str) -> Track {
        Track {
            id,
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            duration_secs: 180,
        }
    }

    /// 0: "Alpha Song" / "Beta Band"; 1: "Delta Song" / "Epsilon Band";
    /// 2: "Alpha Two" / "Eta Band".
    fn sample_app() -> App {
        App::with_library(vec![
            track(1, "Alpha Song", "Beta Band", "Gamma LP"),
            track(2, "Delta Song", "Epsilon Band", "Zeta LP"),
            track(3, "Alpha Two", "Eta Band", "Theta LP"),
        ])
    }

    // --- filtered row -> source track -------------------------------------

    #[test]
    fn filtered_rows_map_back_to_source_tracks() {
        let mut app = sample_app();
        send(&mut app, Message::SearchChanged("alpha".into()));

        assert_eq!(app.filtered, vec![0, 2]);
        assert_eq!(app.filtered_row_to_library_index(0), Some(0));
        assert_eq!(app.filtered_row_to_library_index(1), Some(2));
        // Past the end of the visible rows: no source track.
        assert_eq!(app.filtered_row_to_library_index(2), None);

        let library_index = app.filtered_row_to_library_index(1).expect("row exists");
        assert_eq!(app.library[library_index].title, "Alpha Two");
        assert_eq!(app.library[library_index].id, 3);
    }

    #[test]
    fn clicking_a_filtered_row_plays_that_source_track() {
        let mut app = sample_app();
        send(&mut app, Message::SearchChanged("alpha".into()));
        send(&mut app, Message::TrackRowClicked(1)); // second visible row

        assert!(app.playing);
        assert_eq!(app.position_secs, 0);
        let current = app.current_track().expect("something is playing");
        assert_eq!(current.title, "Alpha Two");
        // The queue became the filtered list the user played from, with the
        // cursor on the clicked row:
        assert_eq!(app.queue.queue, vec![0, 2]);
        assert_eq!(app.queue.cursor, Some(1));
        // The cursor is a position in the queue; the playing *element* is
        // library index 2 — the two numberings must not be confused:
        assert_eq!(app.current_library_index(), Some(2));
    }

    #[test]
    fn searching_never_mutates_the_queue() {
        let mut app = sample_app();
        send(&mut app, Message::TrackRowClicked(0)); // queue = whole library
        let before = app.queue.clone();

        send(&mut app, Message::SearchChanged("alpha".into()));
        assert_eq!(app.queue, before);

        // Even a search with no matches, while playing:
        send(&mut app, Message::SearchChanged("zzz".into()));
        assert!(app.filtered.is_empty());
        assert_eq!(app.queue, before);
        assert!(app.playing);
        assert_eq!(app.queue.cursor, Some(0));
    }

    // --- Navigation execution, cursor never lost ---------------------------

    #[test]
    fn repeat_off_at_the_end_stops_without_losing_the_cursor() {
        let mut app = sample_app();
        app.play_queue_index(2);
        app.position_secs = 120;

        send(&mut app, Message::Next);
        assert!(!app.playing);
        assert_eq!(app.queue.cursor, Some(2), "Stop must keep the cursor");
        assert_eq!(app.position_secs, 0);
    }

    #[test]
    fn repeat_all_wraps_around_to_the_first_element() {
        let mut app = sample_app();
        app.play_queue_index(2);
        app.repeat = RepeatMode::All;

        send(&mut app, Message::Next);
        assert_eq!(app.queue.cursor, Some(0));
        assert!(app.playing);
    }

    #[test]
    fn repeat_one_plays_the_same_element_again() {
        let mut app = sample_app();
        app.play_queue_index(1);
        app.repeat = RepeatMode::One;
        app.position_secs = 42;

        send(&mut app, Message::Next);
        assert_eq!(app.queue.cursor, Some(1));
        assert!(app.playing);
        assert_eq!(app.position_secs, 0);
    }

    #[test]
    fn previous_on_the_first_element_stays_on_it() {
        let mut app = sample_app();
        app.play_queue_index(0);

        send(&mut app, Message::Previous);
        assert_eq!(app.queue.cursor, Some(0));
        assert!(app.playing);
    }

    #[test]
    fn next_with_nothing_playing_starts_the_first_element_of_the_active_order() {
        let mut app = sample_app();
        send(&mut app, Message::Next);
        assert_eq!(
            app.queue.cursor,
            Some(0),
            "sequential order starts at the head"
        );

        let mut shuffled = sample_app();
        shuffled.queue.shuffle_order = vec![2, 0, 1];
        shuffled.shuffle = true;
        send(&mut shuffled, Message::Next);
        assert_eq!(
            shuffled.queue.cursor,
            Some(2),
            "shuffled order starts at its head"
        );
    }

    #[test]
    fn an_empty_queue_makes_next_and_previous_a_noop() {
        let mut app = App::with_library(vec![]);

        send(&mut app, Message::Next);
        send(&mut app, Message::Previous);
        assert_eq!(app.queue.cursor, None);
        assert!(!app.playing);
    }

    #[test]
    fn shuffle_next_follows_the_saved_permutation_and_stop_keeps_the_cursor() {
        let mut app = sample_app();
        app.queue.shuffle_order = vec![2, 0, 1];
        app.shuffle = true;
        app.play_queue_index(0);

        // Active order is [2, 0, 1]; element 0 sits at position 1 → next is 1.
        send(&mut app, Message::Next);
        assert_eq!(app.queue.cursor, Some(1));
        assert!(app.playing);

        // Element 1 is last in the active order: repeat off stops, cursor stays.
        send(&mut app, Message::Next);
        assert_eq!(app.queue.cursor, Some(1));
        assert!(!app.playing);
    }

    #[test]
    fn repeat_modes_cycle_off_all_one() {
        let mut app = sample_app();
        assert_eq!(app.repeat, RepeatMode::Off);
        send(&mut app, Message::CycleRepeat);
        assert_eq!(app.repeat, RepeatMode::All);
        send(&mut app, Message::CycleRepeat);
        assert_eq!(app.repeat, RepeatMode::One);
        send(&mut app, Message::CycleRepeat);
        assert_eq!(app.repeat, RepeatMode::Off);
    }

    // --- queue view clicks and the timer -----------------------------------

    #[test]
    fn queue_screen_click_maps_display_position_to_the_element() {
        let mut app = sample_app();
        app.queue.shuffle_order = vec![2, 0, 1];
        app.shuffle = true;
        app.play_queue_index(1); // element 1 sits at position 2 of the order

        // The display order is rotated so the playing element is first.
        assert_eq!(app.queue_display(), vec![1, 2, 0]);

        send(&mut app, Message::QueueRowClicked(2)); // clicked element "0"
        assert_eq!(app.queue.cursor, Some(0));
        assert_eq!(app.position_secs, 0);
        assert!(app.playing);
    }

    #[test]
    fn the_timer_moves_the_position_and_hands_track_ends_to_navigation() {
        let mut app = sample_app();
        app.play_queue_index(0);
        app.library[0].duration_secs = 2;

        send(&mut app, Message::Tick);
        assert_eq!(app.position_secs, 1);
        assert!(app.playing);

        // Two seconds in, the track ends: repeat off advances to the queue's
        // second element.
        send(&mut app, Message::Tick);
        assert_eq!(app.queue.cursor, Some(1));
        assert_eq!(app.position_secs, 0);
        assert!(app.playing);
    }

    #[test]
    fn a_paused_app_ignores_ticks() {
        let mut app = sample_app();
        app.play_queue_index(0);
        app.playing = false;

        send(&mut app, Message::Tick);
        assert_eq!(app.position_secs, 0);
        assert_eq!(app.queue.cursor, Some(0));
    }
}
