//! Product theme definitions. UI code consumes semantic Kit tokens only.
use gpui_kit::component::Theme;
use gpui_kit::{App, Global, Hsla, Rgba, px, rgb};

#[derive(Clone, Copy, Default, PartialEq)]
struct SystemAccent(Option<[f32; 3]>);
impl Global for SystemAccent {}

pub(super) fn set_system_accent(accent: Option<[f32; 3]>, cx: &mut App) -> bool {
    let accent = accent.filter(|rgb| {
        rgb.iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
    });
    let next = SystemAccent(accent);
    if cx.try_global::<SystemAccent>().copied() == Some(next) {
        return false;
    }
    cx.set_global(next);
    true
}

fn luminance(color: Rgba) -> f32 {
    fn linear(channel: f32) -> f32 {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}
fn foreground(accent: Rgba) -> Hsla {
    let luminance = luminance(accent);
    if (luminance + 0.05) / 0.05 >= 1.05 / (luminance + 0.05) {
        rgb(0x000000).into()
    } else {
        rgb(0xffffff).into()
    }
}
fn contrast(a: Rgba, b: Rgba) -> f32 {
    let (a, b) = (luminance(a) + 0.05, luminance(b) + 0.05);
    a.max(b) / a.min(b)
}

/// The accent hue, darkened (light mode) or lightened (dark mode) until it
/// reaches 4.5:1 against the background, so any system accent stays readable
/// when used as text.
fn readable_accent(accent: Rgba, background: Rgba, dark: bool) -> Rgba {
    let target = if dark { 1.0 } else { 0.0 };
    let mut color = accent;
    for _ in 0..20 {
        if contrast(color, background) >= 4.5 {
            break;
        }
        let step = |channel: f32| channel + (target - channel) * 0.1;
        color = Rgba {
            r: step(color.r),
            g: step(color.g),
            b: step(color.b),
            a: 1.,
        };
    }
    color
}

fn tint(background: Rgba, accent: Rgba, amount: f32) -> Hsla {
    Rgba {
        r: background.r + (accent.r - background.r) * amount,
        g: background.g + (accent.g - background.g) * amount,
        b: background.b + (accent.b - background.b) * amount,
        a: 1.,
    }
    .into()
}

pub(super) fn apply(cx: &mut App) {
    let system = cx
        .try_global::<SystemAccent>()
        .copied()
        .unwrap_or_default()
        .0;
    Theme::update(cx, |theme| {
        let dark = theme.is_dark();
        // macOS system text size; Kit scales controls from this.
        theme.font_size = px(13.);
        theme.radius = px(8.);
        theme.radius_lg = px(14.);
        let colors = &mut theme.colors;
        let accent: Rgba = system
            .map(|[r, g, b]| Rgba { r, g, b, a: 1. })
            .unwrap_or_else(|| rgb(if dark { 0x0070df } else { 0x0064d2 }));
        let primary: Hsla = accent.into();
        let on_primary = foreground(accent);
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
        colors.primary = primary;
        colors.primary_foreground = on_primary;
        colors.primary_hover = primary;
        colors.primary_active = primary;
        colors.button_primary = primary;
        colors.button_primary_foreground = on_primary;
        colors.button_primary_hover = primary;
        colors.button_primary_active = primary;
        colors.ring = primary;
        colors.link = readable_accent(accent, colors.background.into(), dark).into();
        colors.button = colors.secondary;
        colors.button_foreground = colors.foreground;
        colors.accent = tint(
            colors.background.into(),
            accent,
            if dark { 0.22 } else { 0.13 },
        );
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
    fn accent_text_is_readable_for_light_and_dark_accents() {
        let white = Rgba {
            r: 1.,
            g: 1.,
            b: 1.,
            a: 1.,
        };
        let black = Rgba {
            r: 0.11,
            g: 0.11,
            b: 0.12,
            a: 1.,
        };
        let yellow = Rgba {
            r: 1.,
            g: 0.8,
            b: 0.,
            a: 1.,
        };
        let navy = Rgba {
            r: 0.05,
            g: 0.1,
            b: 0.4,
            a: 1.,
        };
        assert!(contrast(readable_accent(yellow, white, false), white) >= 4.5);
        assert!(contrast(readable_accent(navy, black, true), black) >= 4.5);
        // An already readable accent is left unchanged.
        assert!(readable_accent(navy, white, false) == navy);
    }

    #[test]
    fn system_accent_foregrounds_keep_text_contrast_for_light_and_dark_accents() {
        for value in [
            0xff0000, 0xff8000, 0xffd400, 0x00cc66, 0x007aff, 0xaf52de, 0xff2d55, 0x808080,
            0x000000, 0xffffff,
        ] {
            let accent = rgb(value);
            let text: Rgba = foreground(accent).into();
            let a = luminance(accent);
            let b = luminance(text);
            let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
            assert!(ratio >= 4.5, "accent text contrast is below 4.5:1");
        }
    }

    #[test]
    fn initials_skip_sigils_and_avatar_colours_are_stable() {
        assert_eq!(initials("Alex Rivera"), "AR");
        assert_eq!(initials("@alex:example.org"), "A");
        assert_eq!(initials("general"), "G");
        assert_eq!(initials(""), "?");
        assert_eq!(avatar_color("@a:x"), avatar_color("@a:x"));
    }
}
