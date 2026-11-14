//! The real player: `muzon-core`'s controller behind an Iced frontend.
//!
//! One invariant shapes everything here: **the UI does not own playback.** The
//! worker in `muzon-core` owns the queue, decides what plays next and arms the
//! gapless/crossfade hand-off. This module keeps only view state (which screen,
//! the search text, scroll offsets) plus a cache of the latest snapshot and the
//! decoded cover. Every action is a command to that worker.

mod album_grid;
mod appearance;
mod audio_settings;
mod collections;
mod desktop;
mod editor;
mod groups;
mod hotkeys;
mod icons;
pub mod message;
mod palette;
mod preferences;
mod services;
mod settings;
mod shortcuts;
mod sleep;
mod smart;
pub mod state;
mod style;
mod track_covers;
mod tray;
pub mod view;
mod window_state;

pub use crate::real::state::RealApp;

/// Runs the real player. The controller is already alive by the time the window
/// is built: a missing GStreamer fails on the terminal, not as an empty window.
pub fn run(app: RealApp) -> iced::Result {
    let size = app.window.geometry.size();
    let transparent = app.appearance.running_transparent;
    app.appearance.apply();
    iced::application("Muzon", RealApp::update, view::view)
        .subscription(RealApp::subscription)
        .theme(|_| style::theme())
        .style(|app, _| iced::application::Appearance {
            background_color: if app.appearance.running_transparent {
                iced::Color::TRANSPARENT
            } else {
                style::background()
            },
            text_color: style::foreground(),
        })
        .window(iced::window::Settings {
            size,
            transparent,
            min_size: Some(iced::Size::new(800.0, 480.0)),
            ..Default::default()
        })
        .centered()
        .resizable(true)
        .exit_on_close_request(false)
        .run_with(move || (app, iced::Task::done(message::Message::AppearanceRefresh)))
}
