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
#[serde(default)]
pub(super) struct Preferences {
    pub version: u32,
    #[serde(deserialize_with = "section")]
    pub appearance: Appearance,
    #[serde(deserialize_with = "section")]
    pub chat: Chat,
    #[serde(deserialize_with = "section")]
    pub notifications: DeviceNotifications,
    #[serde(deserialize_with = "section")]
    pub updates: twayblade_platform::updates::Preferences,
    #[serde(deserialize_with = "section")]
    pub calls: Calls,
}
/// A section this build cannot read (a value from another build, such as a
/// theme it does not have) falls back to its defaults on its own instead of
/// resetting every preference. Unknown fields are ignored for the same reason:
/// development and release builds share this file.
fn section<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Default,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(T::deserialize(value).unwrap_or_default())
}
/// Call view choices that carry over between calls.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Calls {
    /// Show whoever is talking, once they have spoken for about a second.
    pub follow_speaker: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: VERSION,
            appearance: Default::default(),
            chat: Default::default(),
            notifications: Default::default(),
            updates: Default::default(),
            calls: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct DeviceNotifications {
    pub enabled: bool,
    pub sound: bool,
    pub preview: bool,
    pub badge: bool,
}
impl Default for DeviceNotifications {
    fn default() -> Self {
        // On, so the system asks for permission once an account opens.
        Self {
            enabled: true,
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
    Discord,
    Dracula,
    TokyoNight,
    OneDark,
    Monokai,
    Everforest,
    RosePine,
    Kanagawa,
    Tomorrow,
    Custom,
}
impl Scheme {
    pub const ALL: [Self; 18] = [
        Self::System,
        Self::Light,
        Self::Dark,
        Self::Catppuccin,
        Self::Nord,
        Self::Gruvbox,
        Self::Solarized,
        Self::Discord,
        Self::Dracula,
        Self::TokyoNight,
        Self::OneDark,
        Self::Monokai,
        Self::Everforest,
        Self::RosePine,
        Self::Kanagawa,
        Self::Tomorrow,
        Self::HighContrast,
        Self::Custom,
    ];
    /// Schemes that exist only in a dark palette ignore the Light variant.
    pub fn dark_only(self) -> bool {
        matches!(self, Self::Dracula | Self::Monokai)
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Catppuccin => "catppuccin",
            Self::Nord => "nord",
            Self::Gruvbox => "gruvbox",
            Self::Solarized => "solarized",
            Self::Discord => "discord",
            Self::Dracula => "dracula",
            Self::TokyoNight => "tokyo-night",
            Self::OneDark => "one-dark",
            Self::Monokai => "monokai",
            Self::Everforest => "everforest",
            Self::RosePine => "rose-pine",
            Self::Kanagawa => "kanagawa",
            Self::Tomorrow => "tomorrow",
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
#[serde(default)]
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
    /// A line with the latest message under each conversation in the sidebar.
    pub room_previews: bool,
    /// Back and Forward buttons in the title bar.
    pub navigation_arrows: bool,
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
    /// Row height with a message preview line under the name.
    pub fn preview_row(self) -> f32 {
        match self {
            Self::Compact => 40.,
            Self::Regular => 46.,
            Self::Comfortable => 52.,
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
            room_previews: true,
            navigation_arrows: true,
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
/// When images in conversations download on their own (#174).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AutoDownload {
    Always,
    /// Not on metered connections or in Low Data Mode.
    #[default]
    Unmetered,
    Never,
}
impl AutoDownload {
    pub const ALL: [Self; 3] = [Self::Always, Self::Unmetered, Self::Never];
    /// Whether images may download now, given the connection's metering.
    pub fn allows(self, metered: Option<bool>) -> bool {
        match self {
            Self::Always => true,
            Self::Unmetered => metered != Some(true),
            Self::Never => false,
        }
    }
}
/// How much media may be kept on this device (#174).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MediaCache {
    Off,
    Mb250,
    #[default]
    Mb500,
    Gb1,
    Gb2,
    Gb5,
}
impl MediaCache {
    pub const ALL: [Self; 6] = [
        Self::Off,
        Self::Mb250,
        Self::Mb500,
        Self::Gb1,
        Self::Gb2,
        Self::Gb5,
    ];
    /// The limit in bytes; `None` keeps media in memory only.
    pub fn bytes(self) -> Option<u64> {
        const MB: u64 = 1024 * 1024;
        match self {
            Self::Off => None,
            Self::Mb250 => Some(250 * MB),
            Self::Mb500 => Some(500 * MB),
            Self::Gb1 => Some(1024 * MB),
            Self::Gb2 => Some(2048 * MB),
            Self::Gb5 => Some(5120 * MB),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Chat {
    pub preview_size: twayblade_core::conversation::PreviewSize,
    pub auto_download: AutoDownload,
    pub media_cache: MediaCache,
    pub style: ChatStyle,
    pub density: Density,
    pub avatars: bool,
    pub timestamps: Timestamps,
    /// Preview links under messages; the homeserver fetches the page.
    pub link_previews: bool,
    /// Also in encrypted rooms, which reveals their links to the homeserver.
    pub link_previews_encrypted: bool,
}
impl Default for Chat {
    fn default() -> Self {
        Self {
            preview_size: Default::default(),
            auto_download: Default::default(),
            media_cache: Default::default(),
            style: Default::default(),
            density: Default::default(),
            avatars: true,
            timestamps: Default::default(),
            link_previews: true,
            link_previews_encrypted: false,
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
    fn auto_download_follows_metering_and_cache_limits_are_ordered() {
        assert!(AutoDownload::Always.allows(Some(true)));
        assert!(AutoDownload::Unmetered.allows(Some(false)));
        // Unknown metering counts as unmetered.
        assert!(AutoDownload::Unmetered.allows(None));
        assert!(!AutoDownload::Unmetered.allows(Some(true)));
        assert!(!AutoDownload::Never.allows(Some(false)));
        assert_eq!(MediaCache::Off.bytes(), None);
        let limits: Vec<u64> = MediaCache::ALL.iter().filter_map(|c| c.bytes()).collect();
        assert!(limits.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(MediaCache::default().bytes(), Some(500 * 1024 * 1024));
    }
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
        for invalid in ["corrupt", "{\"version\":999}"] {
            fs::write(&path, invalid).unwrap();
            assert_eq!(Preferences::load(&path).unwrap(), Preferences::default());
            assert_eq!(
                fs::read_to_string(path.with_extension("json.bak")).unwrap(),
                invalid
            );
        }
        fs::write(&path, "{}").unwrap();
        assert_eq!(Preferences::load(&path).unwrap(), Preferences::default());
        // Another build's file: unknown fields and an unreadable section
        // must not reset the sections this build can read.
        fs::remove_file(path.with_extension("json.bak")).unwrap();
        fs::write(
            &path,
            r#"{"version":1,"unknown":true,
                "appearance":{"scheme":"not-a-theme","scale":150},
                "chat":{"style":"irc","later":1},
                "notifications":{"enabled":false,"sound":false}}"#,
        )
        .unwrap();
        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded.appearance, Appearance::default());
        assert_eq!(loaded.chat.style, ChatStyle::Irc);
        assert!(!loaded.notifications.enabled && !loaded.notifications.sound);
        assert!(!path.with_extension("json.bak").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
