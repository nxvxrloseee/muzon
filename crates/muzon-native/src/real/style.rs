//! Visual roles match the original React application's src/index.css.
use iced::{
    color,
    widget::{button, container, slider, text_input},
    Border, Color, Theme,
};
use std::sync::atomic::{AtomicU32, Ordering};
/// Shared UI type scale, in logical pixels.
pub mod type_size {
    pub const CAPTION: u16 = 12;
    pub const BODY: u16 = 14;
    pub const EMPHASIS: u16 = 16;
    pub const TITLE: u16 = 24;
    pub const DISPLAY: u16 = 32;
}
/// Layout spacing; row height and virtualization geometry remain independent.
pub mod space {
    pub const XS: u16 = 4;
    pub const SM: u16 = 8;
    pub const MD: u16 = 12;
    pub const LG: u16 = 16;
    pub const XL: u16 = 24;
}
pub mod radius {
    pub const CONTROL: f32 = 8.;
    pub const CARD: f32 = 12.;
    pub const PANEL: f32 = 16.;
}
pub const FONT: iced::Font = iced::Font::with_name("Noto Sans");
pub const FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Regular.ttf");

pub const DARK: [u32; 12] = [
    0x0b0b0f, 0x111116, 0x15151b, 0x1a1a21, 0x232330, 0xff5c8a, 0x7c5cff, 0xf2f2f5, 0x9a9aa5,
    0x26262f, 0x2a2a33, 0x7c5cff,
];
pub const LIGHT: [u32; 12] = [
    0xf5f5f7, 0xffffff, 0xffffff, 0xffffff, 0xededf2, 0xc43e91, 0x6a4cff, 0x17171c, 0x5c5c66,
    0xe2e2e8, 0xe2e2e8, 0x6a4cff,
];
static COLORS: [AtomicU32; 12] = [
    AtomicU32::new(DARK[0]),
    AtomicU32::new(DARK[1]),
    AtomicU32::new(DARK[2]),
    AtomicU32::new(DARK[3]),
    AtomicU32::new(DARK[4]),
    AtomicU32::new(DARK[5]),
    AtomicU32::new(DARK[6]),
    AtomicU32::new(DARK[7]),
    AtomicU32::new(DARK[8]),
    AtomicU32::new(DARK[9]),
    AtomicU32::new(DARK[10]),
    AtomicU32::new(DARK[11]),
];
static OPACITY: AtomicU32 = AtomicU32::new(1f32.to_bits());
pub fn configure(colors: [u32; 12], opacity: f32) {
    for (target, value) in COLORS.iter().zip(colors) {
        target.store(value, Ordering::Relaxed);
    }
    OPACITY.store(opacity.to_bits(), Ordering::Relaxed);
}
fn role(index: usize) -> Color {
    let rgb = COLORS[index].load(Ordering::Relaxed);
    let alpha = if index < 5 {
        f32::from_bits(OPACITY.load(Ordering::Relaxed))
    } else {
        1.
    };
    Color::from_rgba(
        ((rgb >> 16) & 255) as f32 / 255.,
        ((rgb >> 8) & 255) as f32 / 255.,
        (rgb & 255) as f32 / 255.,
        alpha,
    )
}
pub fn background() -> Color {
    role(0)
}
pub fn sidebar() -> Color {
    role(1)
}
pub fn player() -> Color {
    role(2)
}
pub fn card_color() -> Color {
    role(3)
}
pub fn hover() -> Color {
    role(4)
}
pub fn heart() -> Color {
    role(5)
}
pub fn accent() -> Color {
    role(6)
}
pub fn foreground() -> Color {
    readable_on(role(7), background())
}
pub fn muted() -> Color {
    readable_on(role(8), background())
}
pub fn divider() -> Color {
    role(9)
}
pub fn progress_track() -> Color {
    role(10)
}
pub fn progress_fill() -> Color {
    role(11)
}
/// WCAG 2.x relative luminance, evaluated against an opaque surface.
fn luminance(color: Color) -> f32 {
    fn linear(v: f32) -> f32 {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}
pub fn contrast_ratio(a: Color, b: Color) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
/// Keep a theme's ink when readable; otherwise move toward black or white.
/// This affects rendering only, never persisted JSON or the 12 palette roles.
pub fn readable_on(ink: Color, surface: Color) -> Color {
    let ink = Color { a: 1., ..ink };
    if contrast_ratio(ink, surface) >= 4.5 {
        return ink;
    }
    let target = if contrast_ratio(Color::BLACK, surface) >= contrast_ratio(Color::WHITE, surface) {
        Color::BLACK
    } else {
        Color::WHITE
    };
    for step in 1..=100 {
        let t = step as f32 / 100.;
        let adjusted = Color::from_rgb(
            ink.r + (target.r - ink.r) * t,
            ink.g + (target.g - ink.g) * t,
            ink.b + (target.b - ink.b) * t,
        );
        if contrast_ratio(adjusted, surface) >= 4.5 {
            return adjusted;
        }
    }
    target
}
/// Error ink is independent of the favorite-heart role.
pub fn danger() -> Color {
    readable_on(color!(0xd43b45), background())
}
pub fn on_accent() -> Color {
    readable_on(foreground(), accent())
}
pub fn theme() -> Theme {
    Theme::custom(
        "Muzon".into(),
        iced::theme::Palette {
            background: background(),
            text: foreground(),
            primary: accent(),
            success: color!(0x6ad8ad),
            danger: danger(),
        },
    )
}
pub fn surface(color: Color) -> container::Style {
    container::Style {
        background: Some(color.into()),
        text_color: Some(readable_on(role(7), color)),
        ..Default::default()
    }
}
/// Elevated surface derives from existing roles, keeping 12-color JSON compatible.
pub fn surface_elevated() -> Color {
    let base = card_color();
    let text = role(7);
    Color {
        r: base.r * 0.96 + text.r * 0.04,
        g: base.g * 0.96 + text.g * 0.04,
        b: base.b * 0.96 + text.b * 0.04,
        ..base
    }
}
pub fn accent_soft() -> Color {
    Color {
        a: 0.15,
        ..accent()
    }
}
pub fn card(_: &Theme) -> container::Style {
    container::Style {
        border: Border {
            radius: radius::CARD.into(),
            ..Default::default()
        },
        ..surface(surface_elevated())
    }
}
pub fn quiet(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: hovered.then_some(hover().into()),
        text_color: if hovered { foreground() } else { muted() },
        border: Border {
            radius: radius::CONTROL.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
pub fn selected(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(accent_soft().into()),
        text_color: foreground(),
        ..quiet(theme, status)
    }
}
pub fn track(active: bool, status: button::Status) -> button::Style {
    button::Style {
        background: if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            Some(hover().into())
        } else if active {
            Some(accent_soft().into())
        } else {
            None
        },
        text_color: if active { accent() } else { foreground() },
        border: Border {
            radius: radius::CONTROL.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
pub fn primary(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            if status == button::Status::Disabled {
                card_color()
            } else if status == button::Status::Hovered {
                Color {
                    a: 0.85,
                    ..accent()
                }
            } else {
                accent()
            }
            .into(),
        ),
        text_color: if status == button::Status::Disabled {
            muted()
        } else {
            on_accent()
        },
        border: Border {
            radius: radius::CONTROL.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
pub fn secondary(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            if status == button::Status::Hovered {
                hover()
            } else {
                card_color()
            }
            .into(),
        ),
        ..selected(theme, status)
    }
}
pub fn input(_: &Theme, status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: card_color().into(),
        border: Border {
            color: if matches!(status, text_input::Status::Focused) {
                accent()
            } else {
                divider()
            },
            width: 1.0,
            radius: radius::CONTROL.into(),
        },
        icon: muted(),
        placeholder: muted(),
        value: foreground(),
        selection: accent(),
    }
}
pub fn progress(_: &Theme, status: slider::Status) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (progress_fill().into(), progress_track().into()),
            width: if status == slider::Status::Active {
                3.0
            } else {
                6.0
            },
            border: Border {
                radius: 2.0.into(),
                ..Default::default()
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle {
                radius: if status == slider::Status::Active {
                    3.0
                } else {
                    5.0
                },
            },
            background: foreground().into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}

/// Dedicated rail space prevents the thumb from covering trailing row actions.
pub const SCROLL_GUTTER: f32 = 16.;
pub fn scrollable<'a, Message: 'a>(
    content: impl Into<iced::Element<'a, Message>>,
) -> iced::widget::Scrollable<'a, Message> {
    use iced::widget::scrollable::{Direction, Scrollbar};
    iced::widget::scrollable(content)
        .direction(Direction::Vertical(
            Scrollbar::new()
                .width(6)
                .scroller_width(6)
                .margin(2)
                .spacing(6),
        ))
        .style(scroll)
}

pub fn scroll(
    _: &Theme,
    status: iced::widget::scrollable::Status,
) -> iced::widget::scrollable::Style {
    use iced::widget::scrollable::{Rail, Scroller, Status, Style};
    let rail = Rail {
        background: None,
        border: Border::default(),
        scroller: Scroller {
            color: if matches!(status, Status::Dragged { .. }) {
                muted()
            } else {
                divider()
            },
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
        },
    };
    Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
    }
}

/// Multiline lyrics use the same visual roles as the metadata fields.
pub fn editor(
    _: &Theme,
    status: iced::widget::text_editor::Status,
) -> iced::widget::text_editor::Style {
    iced::widget::text_editor::Style {
        background: background().into(),
        border: Border {
            color: if status == iced::widget::text_editor::Status::Focused {
                accent()
            } else {
                divider()
            },
            width: 1.0,
            radius: radius::CONTROL.into(),
        },
        icon: muted(),
        placeholder: muted(),
        value: foreground(),
        selection: accent(),
    }
}

/// Subtle art tint keeps the palette's base lightness and readable text.
pub fn art_background(rgb: Option<[u8; 3]>) -> container::Style {
    let base = background();
    let rgb = rgb.unwrap_or([
        (base.r * 255.) as u8,
        (base.g * 255.) as u8,
        (base.b * 255.) as u8,
    ]);
    let tint = Color::from_rgb(
        base.r * 0.85 + rgb[0] as f32 / 255. * 0.15,
        base.g * 0.85 + rgb[1] as f32 / 255. * 0.15,
        base.b * 0.85 + rgb[2] as f32 / 255. * 0.15,
    );
    container::Style {
        background: Some(iced::Background::Gradient(
            iced::gradient::Linear::new(iced::Degrees(135.))
                .add_stop(0., tint)
                .add_stop(1., base)
                .into(),
        )),
        ..surface(base)
    }
}

pub fn picker(
    _: &Theme,
    status: iced::widget::pick_list::Status,
) -> iced::widget::pick_list::Style {
    iced::widget::pick_list::Style {
        text_color: foreground(),
        placeholder_color: muted(),
        handle_color: foreground(),
        background: card_color().into(),
        border: Border {
            color: if status == iced::widget::pick_list::Status::Opened {
                accent()
            } else {
                divider()
            },
            width: 1.,
            radius: radius::CONTROL.into(),
        },
    }
}
pub fn menu(_: &Theme) -> iced::widget::overlay::menu::Style {
    iced::widget::overlay::menu::Style {
        background: card_color().into(),
        border: Border {
            color: divider(),
            width: 1.,
            radius: radius::CONTROL.into(),
        },
        text_color: foreground(),
        selected_text_color: foreground(),
        selected_background: hover().into(),
    }
}

#[cfg(test)]
mod contrast_tests {
    use super::*;
    fn rgb(value: u32) -> Color {
        Color::from_rgb8((value >> 16) as u8, (value >> 8) as u8, value as u8)
    }
    #[test]
    fn built_in_text_and_danger_meet_wcag_on_opaque_surfaces() {
        for palette in [DARK, LIGHT] {
            for surface in palette.iter().take(5).copied().map(rgb) {
                for ink in [rgb(palette[7]), rgb(palette[8]), color!(0xd43b45)] {
                    assert!(contrast_ratio(readable_on(ink, surface), surface) >= 4.5);
                }
            }
        }
    }
    #[test]
    fn custom_low_contrast_ink_is_corrected_without_changing_readable_ink() {
        for surface in [Color::BLACK, Color::WHITE, rgb(0x777777), rgb(0x123456)] {
            assert!(contrast_ratio(readable_on(surface, surface), surface) >= 4.5);
        }
        assert_eq!(readable_on(Color::WHITE, Color::BLACK), Color::WHITE);
        assert!((contrast_ratio(Color::WHITE, Color::BLACK) - 21.).abs() < 0.001);
    }
}
