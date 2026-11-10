//! Product theme definitions. UI code consumes semantic Kit tokens only.
use gpui_kit::component::Theme;
use gpui_kit::{App, Hsla, px, rgb};

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

/// Product roles GPUI Kit's theme has no slot for. Source hex values stay here.
pub(super) struct Palette {
    pub rail: Hsla,
    pub field: Hsla,
    pub pressed: Hsla,
    pub warning: Hsla,
    pub warning_background: Hsla,
    pub error: Hsla,
    pub on_avatar: Hsla,
}

pub(super) fn palette(cx: &App) -> Palette {
    let dark = gpui_kit::component::ActiveTheme::theme(cx).is_dark();
    let pick =
        |light: u32, dark_value: u32| -> Hsla { rgb(if dark { dark_value } else { light }).into() };
    Palette {
        rail: pick(0xe8e8ec, 0x1f1f21),
        field: pick(0xffffff, 0x2a2a2d),
        pressed: pick(0xe4e4e9, 0x353538),
        warning: pick(0x8a3f00, 0xffc285),
        warning_background: pick(0xfff1e0, 0x3a2a18),
        error: pick(0xc4281c, 0xff8a80),
        on_avatar: rgb(0xffffff).into(),
    }
}

/// Avatar fills with at least 4.5:1 contrast against white initials.
const AVATAR_FILLS: [u32; 6] = [0x5b5bd6, 0x2f7d6d, 0xb5482f, 0x8a5cbf, 0x56606e, 0x6b7a2a];

/// Stable per-identity colour: the same Matrix ID always gets the same fill.
pub(super) fn avatar_color(id: &str) -> Hsla {
    let hash = id.bytes().fold(0u32, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(byte.into())
    });
    rgb(AVATAR_FILLS[hash as usize % AVATAR_FILLS.len()]).into()
}

/// Up to two initials from a display name; one letter for a bare Matrix ID.
pub(super) fn initials(name: &str) -> String {
    let name = name.trim();
    let letters: String = match name.strip_prefix(['@', '#', '!']) {
        Some(id) => id.chars().take(1).flat_map(char::to_uppercase).collect(),
        None => name
            .split_whitespace()
            .take(2)
            .filter_map(|word| word.chars().next())
            .flat_map(char::to_uppercase)
            .collect(),
    };
    if letters.is_empty() {
        "?".into()
    } else {
        letters
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_skip_sigils_and_avatar_colours_are_stable() {
        assert_eq!(initials("Alex Rivera"), "AR");
        assert_eq!(initials("@alex:example.org"), "A");
        assert_eq!(initials("general"), "G");
        assert_eq!(initials(""), "?");
        assert_eq!(avatar_color("@a:x"), avatar_color("@a:x"));
    }
}
