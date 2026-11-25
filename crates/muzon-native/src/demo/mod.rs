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
        .window(iced::window::Settings {
            size: iced::Size::new(1180., 760.),
            #[cfg(target_os = "linux")]
            platform_specific: iced::window::settings::PlatformSpecific {
                application_id: "muzon-native".into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .centered()
        .resizable(true)
        .run_with(|| (state::App::new(), iced::Task::none()))
}
