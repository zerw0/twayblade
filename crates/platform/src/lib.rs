//! OS secret storage. There is deliberately no file or mock fallback.

use zeroize::Zeroizing;

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

    pub async fn load(&self) -> Result<Option<Zeroizing<String>>, SecretStoreError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || match store.entry()?.get_password() {
            Ok(value) => Ok(Some(Zeroizing::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(SecretStoreError),
        })
        .await
        .map_err(|_| SecretStoreError)?
    }

    pub async fn save(&self, value: Zeroizing<String>) -> Result<(), SecretStoreError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || {
            store
                .entry()?
                .set_password(value.as_str())
                .map_err(|_| SecretStoreError)
        })
        .await
        .map_err(|_| SecretStoreError)?
    }

    pub async fn delete(&self) -> Result<(), SecretStoreError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || match store.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(SecretStoreError),
        })
        .await
        .map_err(|_| SecretStoreError)?
    }
}
