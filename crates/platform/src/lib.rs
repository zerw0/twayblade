//! OS credentials, notifications and appearance. No plaintext secret fallback.

use zeroize::Zeroizing;
#[cfg(target_os = "linux")]
mod accent;
mod motion;
pub mod notifications;
pub use motion::{reduce_motion, reduce_motion_changes};

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
compile_error!("No credential backend is configured for this operating system");

const SERVICE: &str = if cfg!(debug_assertions) {
    "matrix-client.development"
} else {
    "matrix-client"
};
const ACCOUNT: &str = "active-session-v1";

#[derive(Debug, thiserror::Error)]
#[error(
    "Secure credential storage is unavailable or locked. Unlock your OS credential store and retry."
)]
pub struct SecretStoreError;

/// A named OS credential slot. Cloning this handle never reads credentials.
#[derive(Clone)]
pub struct SessionStore {
    service: String,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(SERVICE)
    }
}

impl SessionStore {
    pub fn new(service: &str) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self) -> Result<keyring::Entry, SecretStoreError> {
        keyring::Entry::new(&self.service, ACCOUNT).map_err(|_| SecretStoreError)
    }

    /// Synchronous SDK session callbacks must finish persisting rotated tokens
    /// before returning. Each write replaces the complete OS credential record.
    pub fn load_sync(&self) -> Result<Option<Zeroizing<String>>, SecretStoreError> {
        match self.entry()?.get_password() {
            Ok(value) => Ok(Some(Zeroizing::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(SecretStoreError),
        }
    }

    pub fn save_sync(&self, value: &str) -> Result<(), SecretStoreError> {
        self.entry()?
            .set_password(value)
            .map_err(|_| SecretStoreError)
    }

    pub fn delete_sync(&self) -> Result<(), SecretStoreError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(SecretStoreError),
        }
    }

    pub async fn load(&self) -> Result<Option<Zeroizing<String>>, SecretStoreError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.load_sync())
            .await
            .map_err(|_| SecretStoreError)?
    }

    pub async fn save(&self, value: Zeroizing<String>) -> Result<(), SecretStoreError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.save_sync(&value))
            .await
            .map_err(|_| SecretStoreError)?
    }

    pub async fn delete(&self) -> Result<(), SecretStoreError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.delete_sync())
            .await
            .map_err(|_| SecretStoreError)?
    }
}

/// Read the OS accent without changing preferences. Call on a background worker;
/// Linux uses the local desktop portal and may be unavailable on bare desktops.
pub fn system_accent() -> Option<[f32; 3]> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSColor, NSColorSpace};
        let color =
            NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
        Some([
            color.redComponent() as f32,
            color.greenComponent() as f32,
            color.blueComponent() as f32,
        ])
    }
    #[cfg(target_os = "windows")]
    {
        use windows::UI::ViewManagement::{UIColorType, UISettings};
        let color = UISettings::new()
            .ok()?
            .GetColorValue(UIColorType::Accent)
            .ok()?;
        Some([
            f32::from(color.R) / 255.,
            f32::from(color.G) / 255.,
            f32::from(color.B) / 255.,
        ])
    }
    #[cfg(target_os = "linux")]
    {
        accent::read()
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn native_system_accent_is_an_srgb_color() {
        let accent = super::system_accent().expect("AppKit control accent");
        assert!(
            accent
                .into_iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        );
    }
}
