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
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: VERSION,
            appearance: Default::default(),
            chat: Default::default(),
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
}
impl Scheme {
    pub const ALL: [Self; 7] = [
        Self::System,
        Self::Light,
        Self::Dark,
        Self::HighContrast,
        Self::Solarized,
        Self::Gruvbox,
        Self::Nord,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::HighContrast => "high-contrast",
            Self::Solarized => "solarized",
            Self::Gruvbox => "gruvbox",
            Self::Nord => "nord",
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
    pub style: ChatStyle,
    pub density: Density,
    pub avatars: bool,
    pub timestamps: Timestamps,
}
impl Default for Chat {
    fn default() -> Self {
        Self {
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
            directories::ProjectDirs::from("", "MatrixClient", "MatrixClient")?
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
