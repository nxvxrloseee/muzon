//! The synthetic frontend: 5000 generated tracks, a simulated playhead, no
//! audio and no backend. It is what `--demo` runs, and it is the only mode
//! that does not need a profile directory.

pub mod data;
pub mod message;
pub mod state;
pub mod view;

/// Runs the demo application. No GStreamer, no database, no files.
pub fn run() -> iced::Result {
    iced::application("Muzon UI prototype", state::update, view::view)
        .subscription(state::subscription)
        .theme(|_| iced::theme::Theme::Dark)
        .window_size((1180.0, 760.0))
        .centered()
        .resizable(true)
        .run_with(|| (state::App::new(), iced::Task::none()))
}
