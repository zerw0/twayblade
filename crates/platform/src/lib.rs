//! OS secret storage. There is deliberately no file or mock fallback.

use zeroize::Zeroizing;

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
compile_error!("No credential backend is configured for this operating system");

const SERVICE: &str = "matrix-client.development";
const ACCOUNT: &str = "active-session-v1";

#[derive(Debug, thiserror::Error)]
#[error(
    "Secure credential storage is unavailable or locked. Unlock your OS credential store and retry."
)]
pub struct SecretStoreError;

fn entry() -> Result<keyring::Entry, SecretStoreError> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|_| SecretStoreError)
}

pub async fn load_session() -> Result<Option<Zeroizing<String>>, SecretStoreError> {
    tokio::task::spawn_blocking(|| match entry()?.get_password() {
        Ok(value) => Ok(Some(Zeroizing::new(value))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(SecretStoreError),
    })
    .await
    .map_err(|_| SecretStoreError)?
}

pub async fn save_session(value: Zeroizing<String>) -> Result<(), SecretStoreError> {
    tokio::task::spawn_blocking(move || {
        entry()?
            .set_password(value.as_str())
            .map_err(|_| SecretStoreError)
    })
    .await
    .map_err(|_| SecretStoreError)?
}

pub async fn delete_session() -> Result<(), SecretStoreError> {
    tokio::task::spawn_blocking(|| match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(SecretStoreError),
    })
    .await
    .map_err(|_| SecretStoreError)?
}
