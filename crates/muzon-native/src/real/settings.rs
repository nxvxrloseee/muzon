//! Settings have one sidebar entry and a compact category navigation of their own.
use super::{
    message::Message,
    state::{RealApp, Screen},
    style as st,
};
use iced::{
    widget::{button, column, row, text},
    Element, Length,
};
pub const CATEGORIES: [(Screen, &str); 6] = [
    (Screen::Audio, "Звук"),
    (Screen::Appearance, "Внешний вид"),
    (Screen::Shortcuts, "Клавиши"),
    (Screen::Sleep, "Таймер сна"),
    (Screen::Cloud, "Облако"),
    (Screen::Scrobble, "Скробблинг"),
];
pub fn contains(screen: Screen) -> bool {
    CATEGORIES.iter().any(|(s, _)| *s == screen)
}
pub fn view(app: &RealApp) -> Element<'_, Message> {
    let mut categories = column![].spacing(st::space::SM);
    for chunk in CATEGORIES.chunks(3) {
        let mut line = row![].spacing(st::space::SM).width(Length::Fill);
        for &(screen, label) in chunk {
            line = line.push(
                button(text(label).size(st::type_size::BODY))
                    .padding([9, 10])
                    .width(Length::FillPortion(1))
                    .style(if app.screen == screen {
                        st::primary
                    } else {
                        st::secondary
                    })
                    .on_press(Message::ScreenSelected(screen)),
            );
        }
        categories = categories.push(line);
    }
    let content = match app.screen {
        Screen::Appearance => super::appearance::view(app),
        Screen::Shortcuts => super::hotkeys::view(app),
        Screen::Sleep => super::sleep::view(app),
        Screen::Cloud => super::services::cloud(app),
        Screen::Scrobble => super::services::scrobble(app),
        _ => super::audio_settings::view(app),
    };
    column![
        text("Настройки").size(st::type_size::DISPLAY),
        categories,
        content
    ]
    .spacing(st::space::LG)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_settings_belong_to_one_navigation_group() {
        for (screen, _) in CATEGORIES {
            assert!(contains(screen));
        }
        assert!(!contains(Screen::Library));
        assert!(!contains(Screen::Albums));
    }
}
