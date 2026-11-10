//! Product theme definitions. UI code consumes semantic Kit tokens only.
use gpui_kit::component::Theme;
use gpui_kit::{App, px, rgb};

pub(super) fn apply(cx: &mut App) {
    Theme::update(cx, |theme| {
        let dark = theme.is_dark();
        theme.font_size = px(14.);
        theme.radius = px(8.);
        theme.radius_lg = px(14.);
        let colors = &mut theme.colors;
        let blue = rgb(if dark { 0x0070df } else { 0x0064d2 }).into();
        let white = rgb(0xffffff).into();
        colors.background = rgb(if dark { 0x1c1c1e } else { 0xffffff }).into();
        colors.sidebar = rgb(if dark { 0x252527 } else { 0xf3f3f5 }).into();
        colors.foreground = rgb(if dark { 0xf2f2f7 } else { 0x1c1c1e }).into();
        colors.sidebar_foreground = colors.foreground;
        colors.secondary_foreground = colors.foreground;
        colors.muted_foreground = rgb(if dark { 0xa0a0a7 } else { 0x66666e }).into();
        colors.secondary = rgb(if dark { 0x353538 } else { 0xebebef }).into();
        colors.muted = colors.secondary;
        colors.border = rgb(if dark { 0x3a3a3e } else { 0xdcdce0 }).into();
        colors.sidebar_border = colors.border;
        colors.primary = blue;
        colors.primary_foreground = white;
        colors.primary_hover = blue;
        colors.primary_active = blue;
        colors.button_primary = blue;
        colors.button_primary_foreground = white;
        colors.button_primary_hover = blue;
        colors.button_primary_active = blue;
        colors.ring = blue;
        colors.button = colors.secondary;
        colors.button_foreground = colors.foreground;
        colors.accent = rgb(if dark { 0x2c4058 } else { 0xdceaff }).into();
        colors.accent_foreground = colors.foreground;
        colors.sidebar_accent = colors.accent;
        colors.sidebar_accent_foreground = colors.foreground;
        colors.list_active = colors.accent;
        colors.list_active_border = colors.border;
        colors.list = colors.sidebar;
    });
}
