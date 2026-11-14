//! Visual roles match the original React application's src/index.css.
use iced::{
    color,
    widget::{button, container, slider, text_input},
    Border, Color, Theme,
};
use std::sync::atomic::{AtomicU32, Ordering};
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
    role(7)
}
pub fn muted() -> Color {
    role(8)
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
fn on_accent() -> Color {
    let color = accent();
    if color.r * 0.2126 + color.g * 0.7152 + color.b * 0.0722 > 0.6 {
        color!(0x17171c)
    } else {
        color!(0xffffff)
    }
}
pub fn theme() -> Theme {
    Theme::custom(
        "Muzon".into(),
        iced::theme::Palette {
            background: background(),
            text: foreground(),
            primary: accent(),
            success: color!(0x6ad8ad),
            danger: color!(0xef4444),
        },
    )
}
pub fn surface(color: Color) -> container::Style {
    container::Style {
        background: Some(color.into()),
        text_color: Some(foreground()),
        ..Default::default()
    }
}
pub fn card(_: &Theme) -> container::Style {
    container::Style {
        border: Border {
            radius: 12.0.into(),
            ..Default::default()
        },
        ..surface(card_color())
    }
}
pub fn quiet(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: hovered.then_some(hover().into()),
        text_color: if hovered { foreground() } else { muted() },
        border: Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
pub fn selected(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(card_color().into()),
        text_color: foreground(),
        ..quiet(theme, status)
    }
}
pub fn track(active: bool, status: button::Status) -> button::Style {
    button::Style {
        background: if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            Some(hover().into())
        } else if active {
            Some(
                Color {
                    a: 0.18,
                    ..accent()
                }
                .into(),
            )
        } else {
            None
        },
        text_color: if active { accent() } else { foreground() },
        border: Border {
            radius: 7.0.into(),
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
            radius: 8.0.into(),
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
            radius: 8.0.into(),
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
            width: 3.0,
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

pub fn ambient(_: &Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Gradient(
            iced::gradient::Linear::new(iced::Degrees(135.0))
                .add_stop(0.0, color!(0x29203f))
                .add_stop(0.6, color!(0x14121e))
                .add_stop(1.0, background())
                .into(),
        )),
        ..surface(background())
    }
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
            radius: 8.0.into(),
        },
        icon: muted(),
        placeholder: muted(),
        value: foreground(),
        selection: accent(),
    }
}
