//! Product theme definitions. UI code consumes semantic Kit tokens only.
use crate::preferences::{Appearance, Scheme, Variant};
use gpui_kit::component::{Theme, ThemeMode};
#[derive(Clone)]
struct MessageFont(gpui_kit::SharedString);
impl Global for MessageFont {}
pub(super) fn message_font(cx: &App) -> gpui_kit::SharedString {
    cx.try_global::<MessageFont>()
        .map(|font| font.0.clone())
        .unwrap_or_else(|| {
            gpui_kit::component::ActiveTheme::theme(cx)
                .font_family
                .clone()
        })
}
impl Global for Appearance {}
pub(super) fn set_appearance(value: Appearance, cx: &mut App) {
    cx.set_global(value);
    apply(cx);
}
use gpui_kit::{App, Global, Hsla, Pixels, Rgba, rgb};
thread_local! { static UI_SCALE: std::cell::Cell<f32> = const { std::cell::Cell::new(1.) }; }
/// All frontend pixel dimensions share this scale. OS window bounds remain OS pixels.
pub(super) fn scaled_px(value: f32) -> Pixels {
    gpui_kit::px(value * UI_SCALE.with(|scale| scale.get()))
}
use scaled_px as px;
pub(super) fn font_or_default(
    saved: Option<&str>,
    installed: &[String],
    fallback: &str,
) -> gpui_kit::SharedString {
    saved
        .filter(|name| installed.iter().any(|family| family == name))
        .unwrap_or(fallback)
        .to_owned()
        .into()
}

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
#[cfg(test)]
fn readable_accent(accent: Rgba, background: Rgba, dark: bool) -> Rgba {
    readable_accent_to(accent, background, dark, 4.5)
}
fn readable_accent_to(accent: Rgba, background: Rgba, dark: bool, minimum: f32) -> Rgba {
    let target = if dark { 1.0 } else { 0.0 };
    let mut color = accent;
    for _ in 0..100 {
        if contrast(color, background) >= minimum {
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

/// Curated surfaces and contrast-safe text; colour literals stay in this module.
pub(crate) fn surfaces(scheme: Scheme, dark: bool) -> [u32; 6] {
    match (scheme, dark) {
        (Scheme::HighContrast, false) => {
            [0xffffff, 0xf5f5f5, 0x000000, 0x333333, 0xe8e8e8, 0x555555]
        }
        (Scheme::HighContrast, true) => {
            [0x000000, 0x111111, 0xffffff, 0xcccccc, 0x222222, 0xaaaaaa]
        }
        (Scheme::Solarized, false) => [0xfdf6e3, 0xeee8d5, 0x073642, 0x52676d, 0xe8e1cd, 0x93a1a1],
        (Scheme::Solarized, true) => [0x002b36, 0x073642, 0xeee8d5, 0x9baead, 0x10404a, 0x586e75],
        (Scheme::Gruvbox, false) => [0xfbf1c7, 0xf2e5bc, 0x282828, 0x665c54, 0xebdbb2, 0xa89984],
        (Scheme::Gruvbox, true) => [0x282828, 0x32302f, 0xfbf1c7, 0xbdae93, 0x3c3836, 0x7c6f64],
        (Scheme::Nord, false) => [0xeceff4, 0xe5e9f0, 0x2e3440, 0x4c566a, 0xd8dee9, 0x929bab],
        (Scheme::Nord, true) => [0x2e3440, 0x343b49, 0xeceff4, 0xb8c5d7, 0x3b4252, 0x616e88],
        // Catppuccin Latte and Mocha.
        (Scheme::Catppuccin, false) => [0xeff1f5, 0xe6e9ef, 0x4c4f69, 0x585b73, 0xdce0e8, 0xccd0da],
        (Scheme::Catppuccin, true) => [0x1e1e2e, 0x181825, 0xcdd6f4, 0xa6adc8, 0x313244, 0x45475a],
        (Scheme::Custom, false) => [0xeff1f5, 0xe6e9ef, 0x4c4f69, 0x585b73, 0xdce0e8, 0xccd0da],
        (Scheme::Custom, true) => [0x1e1e2e, 0x181825, 0xcdd6f4, 0xa6adc8, 0x313244, 0x45475a],
        (_, false) => [0xffffff, 0xf3f3f5, 0x1c1c1e, 0x66666e, 0xebebef, 0xdcdce0],
        (_, true) => [0x1c1c1e, 0x252527, 0xf2f2f7, 0xa0a0a7, 0x353538, 0x3a3a3e],
    }
}
/// A custom theme as applied: text and secondary text are corrected when they
/// fall below 4.5:1 against any surface they are drawn on.
/// The border colour, moved toward the text colour until it reaches 3:1
/// against the background (WCAG 1.4.11 for control boundaries).
fn field_outline(border: Rgba, background: Rgba, text: Rgba) -> Rgba {
    (0..=20)
        .map(|step| mix(border, text, step as f32 / 20.))
        .find(|candidate| contrast(*candidate, background) >= 3.)
        .unwrap_or(text)
}
pub(crate) struct CustomPalette {
    pub colors: [u32; 6],
    pub adjusted: bool,
    pub dark: bool,
}
fn hex(color: Rgba) -> u32 {
    let byte = |c: f32| (c.clamp(0., 1.) * 255.).round() as u32;
    (byte(color.r) << 16) | (byte(color.g) << 8) | byte(color.b)
}
fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.,
    }
}
pub(crate) fn custom_palette(custom: crate::preferences::CustomTheme) -> CustomPalette {
    const MINIMUM: f32 = 4.5;
    let [background, sidebar, text, muted, secondary, border] = custom.colors();
    let surfaces = [background, sidebar, secondary].map(rgb);
    let worst = |color: Rgba| {
        surfaces
            .iter()
            .map(|surface| contrast(color, *surface))
            .fold(f32::MAX, f32::min)
    };
    // Black or white, whichever reads better on every surface.
    let ink = [rgb(0x000000), rgb(0xffffff)]
        .into_iter()
        .max_by(|a, b| worst(*a).total_cmp(&worst(*b)))
        .unwrap_or(rgb(0xffffff));
    // Move a colour toward the ink until it is readable, keeping its hue
    // as long as possible.
    let readable = |color: Rgba| {
        (0..=20)
            .map(|step| mix(color, ink, step as f32 / 20.))
            .find(|candidate| worst(*candidate) >= MINIMUM)
            .unwrap_or(ink)
    };
    let fixed_text = hex(readable(rgb(text)));
    let fixed_muted = hex(readable(rgb(muted)));
    CustomPalette {
        adjusted: fixed_text != text || fixed_muted != muted,
        colors: [
            background,
            sidebar,
            fixed_text,
            fixed_muted,
            secondary,
            border,
        ],
        dark: luminance(rgb(background)) < 0.18,
    }
}
/// Background, sidebar and text colours for a scheme's preview tile in Settings.
pub(super) fn scheme_preview(
    scheme: Scheme,
    dark: bool,
    custom: Option<crate::preferences::CustomTheme>,
) -> [Hsla; 3] {
    let [background, sidebar, text, ..] = if scheme == Scheme::Custom
        && let Some(custom) = custom
    {
        custom_palette(custom).colors
    } else {
        surfaces(scheme, dark)
    };
    [
        rgb(background).into(),
        rgb(sidebar).into(),
        rgb(text).into(),
    ]
}
pub(super) const SWATCHES: [(&str, u32); 6] = [
    ("Blue", 0x0064d2),
    ("Purple", 0x8851b8),
    ("Pink", 0xc13c79),
    ("Red", 0xc23b31),
    ("Orange", 0xd87716),
    ("Green", 0x287d55),
];
pub(super) fn swatch(value: u32) -> Hsla {
    rgb(value).into()
}
pub(super) fn accent_bytes(color: Hsla) -> [u8; 3] {
    let value: Rgba = color.into();
    [value.r, value.g, value.b].map(|v| (v.clamp(0., 1.) * 255.).round() as u8)
}
fn effective_accent(accent: Rgba, high_contrast: bool) -> Rgba {
    if !high_contrast {
        return accent;
    }
    let mut value = accent;
    let target: Rgba = foreground(accent).into();
    // Move the accent away from its chosen foreground until it meets 7:1.
    for _ in 0..100 {
        if contrast(value, target) >= 7. {
            break;
        }
        let step = |v: f32, t: f32| v + ((1. - t) - v) * 0.1;
        value = Rgba {
            r: step(value.r, target.r),
            g: step(value.g, target.g),
            b: step(value.b, target.b),
            a: 1.,
        };
    }
    value
}
pub(super) fn apply(cx: &mut App) {
    let system = cx
        .try_global::<SystemAccent>()
        .copied()
        .unwrap_or_default()
        .0;
    let preferences = cx.try_global::<Appearance>().cloned().unwrap_or_default();
    let mode = match preferences.scheme {
        Scheme::Light => Some(ThemeMode::Light),
        Scheme::Dark => Some(ThemeMode::Dark),
        Scheme::System => None,
        // A custom theme's own background decides light or dark, so controls
        // drawn from the system mode match it.
        Scheme::Custom => Some(
            if custom_palette(preferences.custom_theme.unwrap_or_default()).dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
        ),
        _ => match preferences.variant {
            Variant::System => None,
            Variant::Light => Some(ThemeMode::Light),
            Variant::Dark => Some(ThemeMode::Dark),
        },
    };
    UI_SCALE.with(|scale| scale.set(f32::from(preferences.scale.clamp(85, 150)) / 100.));
    let installed = cx.text_system().all_font_names();
    let defaults = Theme::default();
    let interface_font = font_or_default(
        preferences.interface_font.as_deref(),
        &installed,
        &defaults.font_family,
    );
    let code_font = font_or_default(
        preferences.code_font.as_deref(),
        &installed,
        &defaults.mono_font_family,
    );
    cx.set_global(MessageFont(font_or_default(
        preferences.message_font.as_deref(),
        &installed,
        &interface_font,
    )));
    match mode {
        Some(mode) => Theme::change(mode, None, cx),
        // Returning to System after a forced Light/Dark must re-read the OS
        // appearance; otherwise the last forced mode sticks until the OS changes.
        None => Theme::sync_system_appearance(None, cx),
    }
    Theme::update(cx, |theme| {
        let dark = theme.is_dark();
        let [background, sidebar, text, muted, secondary, border] =
            if preferences.scheme == Scheme::Custom {
                custom_palette(preferences.custom_theme.unwrap_or_default()).colors
            } else {
                surfaces(preferences.scheme, dark)
            };
        // macOS system text size; Kit scales controls from this.
        theme.font_size = px(13.);
        theme.font_family = interface_font;
        theme.mono_font_family = code_font;
        theme.radius = px(8.);
        theme.radius_lg = px(14.);
        let colors = &mut theme.colors;
        let custom = preferences
            .accent
            .map(|value| value.map(|v| f32::from(v) / 255.));
        let accent: Rgba = custom
            .or(system)
            .map(|[r, g, b]| Rgba { r, g, b, a: 1. })
            .unwrap_or_else(|| {
                rgb(match (preferences.scheme, dark) {
                    (Scheme::Catppuccin, true) => 0xcba6f7,
                    (Scheme::Catppuccin, false) => 0x8839ef,
                    _ if dark => 0x0070df,
                    _ => 0x0064d2,
                })
            });
        let accent = effective_accent(accent, preferences.scheme == Scheme::HighContrast);
        let primary: Hsla = accent.into();
        let on_primary = foreground(accent);
        colors.background = rgb(background).into();
        colors.sidebar = rgb(sidebar).into();
        colors.foreground = rgb(text).into();
        colors.sidebar_foreground = colors.foreground;
        colors.secondary_foreground = colors.foreground;
        colors.muted_foreground = rgb(muted).into();
        colors.secondary = rgb(secondary).into();
        colors.muted = colors.secondary;
        colors.border = rgb(border).into();
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
        colors.link = readable_accent_to(
            accent,
            colors.background.into(),
            dark,
            if preferences.scheme == Scheme::HighContrast {
                7.
            } else {
                4.5
            },
        )
        .into();
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
        colors.list_hover = colors.secondary;
        colors.list_head = colors.sidebar;
        colors.list_even = colors.background;
        colors.popover = colors.background;
        colors.popover_foreground = colors.foreground;
        // Kit draws text-field outlines with `input`. Keep them at the 3:1
        // non-text contrast minimum against the window background, so fields
        // stay findable in every scheme, including custom themes.
        colors.input = field_outline(rgb(border), rgb(background), rgb(text)).into();
        colors.selection = colors.accent;
        colors.sidebar_primary = colors.primary;
        colors.sidebar_primary_foreground = colors.primary_foreground;
    });
}

/// Product roles GPUI Kit's theme has no slot for. Source hex values stay here.
pub(super) struct Palette {
    /// Window title bar, a step off the window background (lighter in dark mode).
    pub chrome: Hsla,
    /// Space rail, a step darker than the sidebar.
    pub rail: Hsla,
    /// Selected sidebar row: a tint of the accent rather than a solid fill.
    pub selection: Hsla,
    /// Selected segment in a segmented control (a raised pill, as in macOS).
    pub pill: Hsla,
    /// Connection status dot.
    pub online: Hsla,
    /// Favourite star (a non-text icon: 3:1 is enough).
    pub star: Hsla,
    pub field: Hsla,
    pub warning: Hsla,
    pub warning_background: Hsla,
    pub error: Hsla,
    pub on_avatar: Hsla,
}

pub(super) fn palette(cx: &App) -> Palette {
    let dark = gpui_kit::component::ActiveTheme::theme(cx).is_dark();
    let pick =
        |light: u32, dark_value: u32| -> Hsla { rgb(if dark { dark_value } else { light }).into() };
    let theme = gpui_kit::component::ActiveTheme::theme(cx);
    let shift = |color: Hsla, amount: f32| Hsla {
        l: (color.l + amount).clamp(0., 1.),
        ..color
    };
    Palette {
        chrome: shift(theme.sidebar, if dark { 0.03 } else { -0.03 }),
        rail: shift(theme.sidebar, if dark { -0.04 } else { -0.05 }),
        selection: theme.primary.opacity(if dark { 0.18 } else { 0.12 }),
        pill: pick(0xffffff, 0x56565c),
        online: pick(0x248a3d, 0x30d158),
        star: pick(0xb88700, 0xffd60a),
        field: gpui_kit::component::ActiveTheme::theme(cx).background,
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

/// IRC names retain their identity hue with readable contrast on the transcript.
pub(super) fn avatar_text_color(id: &str, cx: &App) -> Hsla {
    use gpui_kit::component::ActiveTheme;
    readable_accent_to(
        avatar_color(id).into(),
        cx.theme().secondary.into(),
        cx.theme().is_dark(),
        4.5,
    )
    .into()
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

    fn palette_contrast(scheme: Scheme, dark: bool) {
        let [background, sidebar, text, muted, secondary, border] = surfaces(scheme, dark);
        let minimum = if scheme == Scheme::HighContrast {
            7.
        } else {
            4.5
        };
        for bg in [background, sidebar, secondary] {
            assert!(
                contrast(rgb(text), rgb(bg)) >= minimum,
                "body {scheme:?} dark={dark}"
            );
            assert!(
                contrast(rgb(muted), rgb(bg)) >= minimum,
                "muted {scheme:?} dark={dark} bg={bg:x}"
            );
        }
        assert!(
            contrast(
                field_outline(rgb(border), rgb(background), rgb(text)),
                rgb(background)
            ) >= 3.,
            "field outline {scheme:?} dark={dark}"
        );
        for (_, value) in SWATCHES {
            let accent = effective_accent(rgb(value), scheme == Scheme::HighContrast);
            assert!(contrast(accent, foreground(accent).into()) >= minimum);
            assert!(
                contrast(
                    readable_accent_to(accent, rgb(background), dark, minimum),
                    rgb(background)
                ) >= minimum
            );
        }
    }
    macro_rules! palette_test {
        ($name:ident, $scheme:ident, $dark:expr) => {
            #[test]
            fn $name() {
                palette_contrast(Scheme::$scheme, $dark);
            }
        };
    }
    palette_test!(system_light, System, false);
    palette_test!(system_dark, System, true);
    palette_test!(light, Light, false);
    palette_test!(dark, Dark, true);
    palette_test!(high_contrast_light, HighContrast, false);
    palette_test!(high_contrast_dark, HighContrast, true);
    palette_test!(solarized_light, Solarized, false);
    palette_test!(solarized_dark, Solarized, true);
    palette_test!(gruvbox_light, Gruvbox, false);
    palette_test!(gruvbox_dark, Gruvbox, true);
    palette_test!(nord_light, Nord, false);
    palette_test!(nord_dark, Nord, true);
    palette_test!(catppuccin_light, Catppuccin, false);
    palette_test!(catppuccin_dark, Catppuccin, true);
    palette_test!(custom_light, Custom, false);
    palette_test!(custom_dark, Custom, true);
    #[test]
    fn custom_palettes_keep_readable_text_and_follow_their_background() {
        use crate::preferences::CustomTheme;
        let check = |custom: CustomTheme| {
            let palette = custom_palette(custom);
            let [background, sidebar, text, muted, secondary, _] = palette.colors;
            for surface in [background, sidebar, secondary] {
                assert!(contrast(rgb(text), rgb(surface)) >= 4.5, "{custom:?}");
                assert!(contrast(rgb(muted), rgb(surface)) >= 4.5, "{custom:?}");
            }
            palette
        };
        // The default (Catppuccin Mocha) is readable as chosen, and dark.
        let default = check(CustomTheme::default());
        assert!(!default.adjusted && default.dark);
        assert_eq!(default.colors, CustomTheme::default().colors());
        // Grey on grey and text equal to the background get corrected.
        for (field, value) in [(2, 0x1e1e2e), (3, 0x313244), (2, 0x777777)] {
            assert!(check(CustomTheme::default().with(field, value)).adjusted);
        }
        // A light background makes the theme light; dark text stays as chosen.
        let light = check(CustomTheme::from_colors([
            0xfafafa, 0xf0f0f0, 0x202020, 0x555555, 0xe6e6e6, 0xcccccc,
        ]));
        assert!(!light.dark && !light.adjusted);
        // Colours are clamped to 24 bits.
        assert_eq!(
            CustomTheme::default().with(0, 0xff12_3456).background,
            0x12_3456
        );
    }
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
