//! macOS-first visual language, shared by the portable native UI.

use iced::widget::{button, container, text_input};
use iced::{Border, Color, Font, Theme, font, theme};

pub const SYSTEM: Font = if cfg!(target_os = "macos") {
    Font::with_name("System Font")
} else {
    Font::DEFAULT
};

pub const SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..SYSTEM
};
pub const BOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..SYSTEM
};

pub fn sidebar(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(
            if theme.palette().background.r < 0.5 {
                rgb(0x242426)
            } else {
                rgb(0xebebef)
            }
            .into(),
        ),
        ..Default::default()
    }
}

pub fn room_button(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let mut style = button::text(theme, status);
    style.text_color = theme.palette().text;
    style.border.radius = 7.0.into();
    if selected {
        style.background = Some(
            Color {
                a: 0.22,
                ..theme.palette().primary
            }
            .into(),
        );
    }
    style
}

pub fn theme(mode: theme::Mode) -> Theme {
    let dark = mode == theme::Mode::Dark;
    Theme::custom(
        String::from("Matrix Desktop"),
        theme::Palette {
            background: if dark { rgb(0x19191b) } else { rgb(0xffffff) },
            text: if dark { rgb(0xf5f5f7) } else { rgb(0x1d1d1f) },
            primary: if dark { rgb(0x0a84ff) } else { rgb(0x007aff) },
            success: rgb(0x248a3d),
            warning: rgb(0xb25000),
            danger: rgb(0xd70015),
        },
    )
}

fn rgb(value: u32) -> Color {
    Color::from_rgb8((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

pub fn muted(theme: &Theme) -> Color {
    Color {
        a: 0.60,
        ..theme.palette().text
    }
}

pub fn panel(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.extended_palette().background.weak.color.into()),
        border: Border {
            color: Color {
                a: 0.12,
                ..theme.palette().text
            },
            width: 1.0,
            radius: 12.0.into(),
        },
        ..Default::default()
    }
}

pub fn primary(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = button::primary(theme, status);
    style.border.radius = 7.0.into();
    style
}

pub fn input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(theme, status);
    style.border.radius = 9.0.into();
    style.border.width = if matches!(status, text_input::Status::Focused { .. }) {
        1.0
    } else {
        0.0
    };
    style.background = if theme.palette().background.r < 0.5 {
        rgb(0x303033)
    } else {
        rgb(0xe8e8ed)
    }
    .into();
    style
}

pub fn toolbar(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(
            if theme.palette().background.r < 0.5 {
                rgb(0x202022)
            } else {
                rgb(0xf7f7f9)
            }
            .into(),
        ),
        ..Default::default()
    }
}

pub fn conversation_surface(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.palette().background.into()),
        border: Border {
            radius: 12.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn composer_input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = input(theme, status);
    style.background = Color::TRANSPARENT.into();
    style.border.width = 0.0;
    style
}

pub fn icon_button(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = button::text(theme, status);
    style.border.radius = 8.0.into();
    if matches!(status, button::Status::Hovered | button::Status::Pressed) {
        style.background = Some(
            Color {
                a: 0.08,
                ..theme.palette().text
            }
            .into(),
        );
    }
    style
}

pub fn icon(name: &str, size: f32, color: Color) -> iced::widget::Svg<'static> {
    let paths = match name {
        "home" => "<path d='m3 10 9-7 9 7v10a1 1 0 0 1-1 1h-5v-7H9v7H4a1 1 0 0 1-1-1Z'/>",
        "search" => "<circle cx='10.5' cy='10.5' r='6.5'/><path d='m16 16 5 5'/>",
        "account" => "<circle cx='12' cy='8' r='4'/><path d='M4 21v-2a8 8 0 0 1 16 0v2'/>",
        "send" => "<path d='m12 4-7 7m7-7 7 7M12 4v16'/>",
        _ => "<path d='M4 8h16M3 16h16M10 3 7 21M17 3l-3 18'/>",
    };
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='rgb({},{},{})' stroke-width='1.7' stroke-linecap='round' stroke-linejoin='round'>{paths}</svg>",
        (color.r * 255.0) as u8,
        (color.g * 255.0) as u8,
        (color.b * 255.0) as u8
    );
    iced::widget::svg(iced::widget::svg::Handle::from_memory(svg.into_bytes()))
        .width(size)
        .height(size)
}
