//! Password sessions backed by encrypted SDK stores and OS secret storage.

use directories::ProjectDirs;
use matrix_client_platform as secrets;
use matrix_sdk::{Client, authentication::matrix::MatrixSession};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::validate_homeserver;

// No Debug: this contains access credentials and the store passphrase.
#[derive(Serialize, Deserialize)]
struct SavedSession {
    homeserver: String,
    store_id: Uuid,
    passphrase: String,
    session: Option<MatrixSession>,
}

#[derive(Clone)]
pub struct Account {
    pub(crate) client: Client,
    pub(crate) selected_room: tokio::sync::watch::Sender<Option<matrix_sdk::ruma::OwnedRoomId>>,
    instance_id: Uuid,
    remotely_revoked: Arc<AtomicBool>,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Account { credentials: [REDACTED] }")
    }
}

impl Account {
    pub fn instance_id(&self) -> u128 {
        self.instance_id.as_u128()
    }

    fn new(client: Client) -> Self {
        Self {
            client,
            selected_room: tokio::sync::watch::channel(None).0,
            instance_id: Uuid::new_v4(),
            remotely_revoked: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn user_id(&self) -> String {
        self.client
            .user_id()
            .map(ToString::to_string)
            .unwrap_or_default()
    }

    /// Revoke the remote session before discarding the local credentials.
    pub async fn logout(&self) -> Result<(), AuthError> {
        if !self.remotely_revoked.load(Ordering::SeqCst) {
            self.client
                .matrix_auth()
                .logout()
                .await
                .map_err(|_| AuthError::Logout)?;
            self.remotely_revoked.store(true, Ordering::SeqCst);
        }
        secrets::delete_session()
            .await
            .map_err(|_| AuthError::Storage)
    }
}

impl std::hash::Hash for Account {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.instance_id.hash(state);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error(
        "Secure session storage could not be opened. Unlock your OS credential store and retry."
    )]
    Storage,
    #[error("The saved session could not be restored. It has been retained for recovery.")]
    Restore,
    #[error("Sign-in failed. Check your credentials, homeserver and connection.")]
    Login,
    #[error(
        "Sign-out could not be completed. The session has been retained; retry when connected."
    )]
    Logout,
    #[error("A saved session already exists. Restart to restore it before signing in again.")]
    ExistingSession,
}

async fn save(record: &SavedSession) -> Result<(), AuthError> {
    let value = Zeroizing::new(serde_json::to_string(record).map_err(|_| AuthError::Storage)?);
    secrets::save_session(value)
        .await
        .map_err(|_| AuthError::Storage)
}

async fn client(record: &SavedSession) -> Result<Client, AuthError> {
    validate_homeserver(&record.homeserver).map_err(|_| AuthError::Restore)?;
    let dirs = ProjectDirs::from("", "MatrixClient", "MatrixClient").ok_or(AuthError::Storage)?;
    let path = dirs
        .data_local_dir()
        .join("sessions")
        .join(record.store_id.to_string());
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|_| AuthError::Storage)?;
    Client::builder()
        .homeserver_url(&record.homeserver)
        .sqlite_store(path, Some(&record.passphrase))
        .build()
        .await
        .map_err(|_| AuthError::Storage)
}

pub async fn restore() -> Result<Option<Account>, AuthError> {
    let Some(value) = secrets::load_session()
        .await
        .map_err(|_| AuthError::Storage)?
    else {
        return Ok(None);
    };
    let mut record: SavedSession = serde_json::from_str(&value).map_err(|_| AuthError::Restore)?;
    let Some(session) = record.session.take() else {
        // A pre-login checkpoint contains no account credentials.
        return Ok(None);
    };
    let client = client(&record).await?;
    client
        .matrix_auth()
        .restore_session(session, Default::default())
        .await
        .map_err(|_| AuthError::Restore)?;
    Ok(Some(Account::new(client)))
}

pub async fn login_password(
    homeserver: String,
    username: String,
    password: Zeroizing<String>,
) -> Result<Account, AuthError> {
    let homeserver = validate_homeserver(&homeserver).map_err(|_| AuthError::Login)?;
    if username.trim().is_empty() || password.is_empty() {
        return Err(AuthError::Login);
    }
    if let Some(value) = secrets::load_session()
        .await
        .map_err(|_| AuthError::Storage)?
    {
        let previous: SavedSession =
            serde_json::from_str(&value).map_err(|_| AuthError::Restore)?;
        if previous.session.is_some() {
            return Err(AuthError::ExistingSession);
        }
    }
    let mut record = SavedSession {
        homeserver: homeserver.to_string(),
        store_id: Uuid::new_v4(),
        passphrase: Uuid::new_v4().to_string(),
        session: None,
    };
    // Prove secure storage works before transmitting the password or creating a device.
    save(&record).await?;
    let client = client(&record).await?;
    client
        .matrix_auth()
        .login_username(username.trim(), &password)
        .initial_device_display_name("Matrix Client desktop")
        .await
        .map_err(|_| AuthError::Login)?;
    record.session = Some(client.matrix_auth().session().ok_or(AuthError::Login)?);
    if let Err(error) = save(&record).await {
        // Best effort revocation if secure persistence failed after server login.
        let _ = client.matrix_auth().logout().await;
        return Err(error);
    }
    Ok(Account::new(client))
}

#[cfg(test)]
mod integration;
