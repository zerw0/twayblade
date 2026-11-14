//! Frontend-only preferences. Never put account, credential or conversation data here.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const VERSION: u32 = 1;
const MAX_BYTES: u64 = 65536;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Preferences {
    pub version: u32,
    pub appearance: Appearance,
    pub chat: Chat,
    pub notifications: DeviceNotifications,
    pub updates: twayblade_platform::updates::Preferences,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: VERSION,
            appearance: Default::default(),
            chat: Default::default(),
            notifications: Default::default(),
            updates: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct DeviceNotifications {
    pub enabled: bool,
    pub sound: bool,
    pub preview: bool,
    pub badge: bool,
}
impl Default for DeviceNotifications {
    fn default() -> Self {
        Self {
            enabled: false,
            sound: true,
            preview: true,
            badge: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Scheme {
    #[default]
    System,
    Light,
    Dark,
    HighContrast,
    Solarized,
    Gruvbox,
    Nord,
    Catppuccin,
    Custom,
}
impl Scheme {
    pub const ALL: [Self; 9] = [
        Self::System,
        Self::Light,
        Self::Dark,
        Self::Catppuccin,
        Self::Nord,
        Self::Gruvbox,
        Self::Solarized,
        Self::HighContrast,
        Self::Custom,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Catppuccin => "catppuccin",
            Self::Nord => "nord",
            Self::Gruvbox => "gruvbox",
            Self::Solarized => "solarized",
            Self::HighContrast => "high-contrast",
            Self::Custom => "custom",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct CustomTheme {
    pub background: u32,
    pub sidebar: u32,
    pub text: u32,
    pub muted: u32,
    pub secondary: u32,
    pub border: u32,
}
impl CustomTheme {
    /// Editable colours, in Settings order: (id, label).
    pub const FIELDS: [(&'static str, &'static str); 6] = [
        ("bg", "Background"),
        ("sidebar", "Sidebar"),
        ("text", "Text"),
        ("muted", "Secondary text"),
        ("secondary", "Secondary surface"),
        ("border", "Border"),
    ];
    /// Same order as `FIELDS` and the theme's surface array.
    pub fn colors(self) -> [u32; 6] {
        [
            self.background,
            self.sidebar,
            self.text,
            self.muted,
            self.secondary,
            self.border,
        ]
    }
    pub fn from_colors([background, sidebar, text, muted, secondary, border]: [u32; 6]) -> Self {
        Self {
            background,
            sidebar,
            text,
            muted,
            secondary,
            border,
        }
    }
    pub fn with(self, field: usize, value: u32) -> Self {
        let mut colors = self.colors();
        colors[field] = value & 0xff_ffff;
        Self::from_colors(colors)
    }
}
impl Default for CustomTheme {
    fn default() -> Self {
        Self {
            background: 0x1e1e2e,
            sidebar: 0x181825,
            text: 0xcdd6f4,
            muted: 0xa6adc8,
            secondary: 0x313244,
            border: 0x45475a,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Variant {
    #[default]
    System,
    Light,
    Dark,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Appearance {
    pub scheme: Scheme,
    pub variant: Variant,
    pub accent: Option<[u8; 3]>,
    pub scale: u16,
    pub interface_font: Option<String>,
    pub message_font: Option<String>,
    pub message_size: u16,
    pub code_font: Option<String>,
    /// Spacing of sidebar rows, section headers and menus.
    pub sidebar_density: SidebarDensity,
    pub custom_theme: Option<CustomTheme>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SidebarDensity {
    Compact,
    #[default]
    Regular,
    Comfortable,
}
impl SidebarDensity {
    /// Row height in points for sidebar and panel lists.
    pub fn row(self) -> f32 {
        match self {
            Self::Compact => 26.,
            Self::Regular => 30.,
            Self::Comfortable => 34.,
        }
    }
    /// Extra space above each sidebar section.
    pub fn section_gap(self) -> f32 {
        match self {
            Self::Compact => 8.,
            Self::Regular => 14.,
            Self::Comfortable => 18.,
        }
    }
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            scheme: Default::default(),
            variant: Default::default(),
            accent: None,
            scale: 100,
            interface_font: None,
            message_font: None,
            message_size: 13,
            code_font: None,
            sidebar_density: SidebarDensity::Regular,
            custom_theme: None,
        }
    }
}
impl Appearance {
    fn validate(&mut self) {
        self.scale = (self.scale.clamp(85, 150) / 5) * 5;
        self.message_size = self.message_size.clamp(10, 28);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ChatStyle {
    #[default]
    Bubbles,
    Irc,
    Minimal,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Density {
    Compact,
    #[default]
    Regular,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Timestamps {
    #[default]
    OnHover,
    Always,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Chat {
    pub preview_size: twayblade_core::conversation::PreviewSize,
    pub style: ChatStyle,
    pub density: Density,
    pub avatars: bool,
    pub timestamps: Timestamps,
}
impl Default for Chat {
    fn default() -> Self {
        Self {
            preview_size: Default::default(),
            style: Default::default(),
            density: Default::default(),
            avatars: true,
            timestamps: Default::default(),
        }
    }
}
impl Preferences {
    pub fn path() -> Option<PathBuf> {
        Some(
            directories::ProjectDirs::from("", "Twayblade", "Twayblade")?
                .config_dir()
                .join("preferences.json"),
        )
    }
    pub fn load(path: &Path) -> io::Result<Self> {
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 <= MAX_BYTES
            && let Ok(mut value) = serde_json::from_slice::<Self>(&bytes)
            && value.version == VERSION
        {
            value.appearance.validate();
            return Ok(value);
        }
        // Preserve the original bytes, including an unknown future schema. If the
        // backup cannot be made, do not overwrite the unrecognised original.
        fs::rename(path, path.with_extension("json.bak"))?;
        Ok(Self::default())
    }
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("Missing config directory"))?;
        fs::create_dir_all(parent)?;
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let temporary = parent.join(format!(
            ".preferences-{}-{}.new",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&serde_json::to_vec_pretty(self)?)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, path)
        })();
        let _ = fs::remove_file(&temporary);
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_atomic_round_trip_and_invalid_backup() {
        let dir = std::env::temp_dir().join(format!("matrix-preferences-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("preferences.json");
        assert_eq!(Preferences::load(&path).unwrap(), Preferences::default());
        let mut custom = Preferences::default();
        custom.appearance.scheme = Scheme::Nord;
        custom.appearance.scale = 150;
        custom.appearance.message_font = Some("Saved family".into());
        custom.appearance.accent = Some([17, 55, 99]);
        custom.chat.style = ChatStyle::Irc;
        custom.notifications.enabled = true;
        custom.notifications.preview = false;
        custom.notifications.sound = false;
        custom.notifications.badge = false;
        custom.updates.enabled = false;
        custom.updates.channel = twayblade_platform::updates::Channel::Prerelease;
        custom.save(&path).unwrap();
        assert_eq!(Preferences::load(&path).unwrap(), custom);
        for invalid in ["corrupt", "{\"version\":999}", "{\"unknown\":true}"] {
            fs::write(&path, invalid).unwrap();
            assert_eq!(Preferences::load(&path).unwrap(), Preferences::default());
            assert_eq!(
                fs::read_to_string(path.with_extension("json.bak")).unwrap(),
                invalid
            );
        }
        fs::write(&path, "{}").unwrap();
        assert_eq!(Preferences::load(&path).unwrap(), Preferences::default());
        fs::remove_dir_all(dir).unwrap();
    }
}
