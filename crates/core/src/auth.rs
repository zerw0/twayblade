//! Password and OAuth sessions backed by encrypted SDK stores and OS secret storage.

use directories::ProjectDirs;
use matrix_sdk::{
    AuthSession, Client,
    authentication::{
        matrix::MatrixSession,
        oauth::{ClientId, OAuthSession, UserSession},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use twayblade_platform as secrets;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::validate_homeserver;
mod loopback;
mod oauth;
mod recovery;
mod sso;
pub use oauth::OAuthLogin;
pub use recovery::SessionState;
pub use sso::SsoLogin;

// No Debug: this contains access credentials and the store passphrase.
#[derive(Serialize, Deserialize)]
struct SavedSession {
    homeserver: String,
    #[serde(default)]
    generation: Uuid,
    store_id: Uuid,
    passphrase: String,
    session: Option<MatrixSession>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    oauth: Option<SavedOAuth>,
}

// Keep the existing password record readable without a credential migration.
#[derive(Serialize, Deserialize)]
struct SavedOAuth {
    client_id: String,
    user: UserSession,
}

impl SavedSession {
    fn update(&mut self, session: AuthSession) -> Result<(), AuthError> {
        match session {
            AuthSession::Matrix(session) => {
                self.session = Some(session);
                self.oauth = None;
            }
            AuthSession::OAuth(session) => {
                self.oauth = Some(SavedOAuth {
                    client_id: session.client_id.to_string(),
                    user: session.user,
                });
                self.session = None;
            }
            _ => return Err(AuthError::Storage),
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Account {
    pub(crate) client: Client,
    pub(crate) thread_read_updates: tokio::sync::broadcast::Sender<(String, Option<String>)>,
    pub(crate) notification_service:
        tokio::sync::watch::Sender<Option<Arc<matrix_sdk_ui::sync_service::SyncService>>>,
    pub(crate) selected_room: tokio::sync::watch::Sender<Option<matrix_sdk::ruma::OwnedRoomId>>,
    pub(crate) sync_requests:
        Arc<Mutex<std::collections::HashMap<matrix_sdk::ruma::OwnedRoomId, usize>>>,
    pub(crate) sync_request_updates: tokio::sync::watch::Sender<Vec<matrix_sdk::ruma::OwnedRoomId>>,
    instance_id: Uuid,
    remotely_revoked: Arc<AtomicBool>,
    store_path: PathBuf,
    session_store: secrets::SessionStore,
    persistence_lock: Arc<Mutex<()>>,
    root_certificate: Option<matrix_sdk::reqwest::Certificate>,
    session_state: tokio::sync::watch::Receiver<SessionState>,
    _session_monitor: Arc<recovery::SessionMonitor>,
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

    pub(crate) fn new(
        client: Client,
        store_path: PathBuf,
        session_store: secrets::SessionStore,
        persistence_lock: Arc<Mutex<()>>,
    ) -> Self {
        let (session_state, monitor) = recovery::monitor(&client);
        Self {
            client,
            thread_read_updates: tokio::sync::broadcast::channel(32).0,
            notification_service: tokio::sync::watch::channel(None).0,
            selected_room: tokio::sync::watch::channel(None).0,
            sync_requests: Arc::new(Mutex::new(std::collections::HashMap::new())),
            sync_request_updates: tokio::sync::watch::channel(Vec::new()).0,
            instance_id: Uuid::new_v4(),
            remotely_revoked: Arc::new(AtomicBool::new(false)),
            store_path,
            session_store,
            persistence_lock,
            root_certificate: None,
            session_state,
            _session_monitor: monitor,
        }
    }

    fn with_certificate(mut self, certificate: Option<matrix_sdk::reqwest::Certificate>) -> Self {
        self.root_certificate = certificate;
        self
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
            self.client.logout().await.map_err(|_| AuthError::Logout)?;
            self.remotely_revoked.store(true, Ordering::SeqCst);
        }
        {
            let _guard = self
                .persistence_lock
                .lock()
                .map_err(|_| AuthError::Storage)?;
            self.session_store
                .delete_sync()
                .map_err(|_| AuthError::Storage)?;
        }
        // The revoked device's keys and cached history have no further use.
        remove_store(&self.store_path).await
    }
}

impl std::hash::Hash for Account {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.instance_id.hash(state);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("auth_oauth")]
    OAuth,
    #[error("auth_callback")]
    Callback,
    #[error("auth_cancelled")]
    Cancelled,
    #[error("auth_timeout")]
    Timeout,
    #[error("auth_storage")]
    Storage,
    #[error("auth_restore")]
    Restore,
    #[error("auth_login")]
    Login,
    #[error("auth_logout")]
    Logout,
    #[error("auth_existing_session")]
    ExistingSession,
    #[error("auth_already_running")]
    AlreadyRunning,
}

/// Held for the process lifetime: one store and credential entry per user means
/// two connected instances would corrupt each other. Released by the OS on exit.
pub struct InstanceLock(#[allow(dead_code)] std::fs::File);

/// One-time move of local data from the pre-rename "MatrixClient" folders to
/// "Twayblade" (sessions/encrypted stores, preferences). Skipped when the new
/// folders already exist or the old app still holds its instance lock.
pub fn migrate_legacy_storage() {
    let (Some(old), Some(new)) = (
        ProjectDirs::from("", "MatrixClient", "MatrixClient"),
        ProjectDirs::from("", "Twayblade", "Twayblade"),
    ) else {
        return;
    };
    if let Ok(lock) = std::fs::File::open(old.data_local_dir().join("instance.lock"))
        && lock.try_lock().is_err()
    {
        return;
    }
    let pairs = [
        (old.data_local_dir(), new.data_local_dir()),
        (old.data_dir(), new.data_dir()),
        (old.config_dir(), new.config_dir()),
    ];
    for (from, to) in pairs {
        if from.exists() && !to.exists() {
            if let Some(parent) = to.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::rename(from, to);
        }
    }
}

pub fn lock_instance() -> Result<InstanceLock, AuthError> {
    let dirs = ProjectDirs::from("", "Twayblade", "Twayblade").ok_or(AuthError::Storage)?;
    let dir = dirs.data_local_dir();
    std::fs::create_dir_all(dir).map_err(|_| AuthError::Storage)?;
    let file = std::fs::File::create(dir.join("instance.lock")).map_err(|_| AuthError::Storage)?;
    match file.try_lock() {
        Ok(()) => Ok(InstanceLock(file)),
        Err(std::fs::TryLockError::WouldBlock) => Err(AuthError::AlreadyRunning),
        Err(_) => Err(AuthError::Storage),
    }
}

// Internal scope keeps test credentials/stores separate from the user's account.
// Only tests can supply a custom certificate or data directory.
struct AuthContext {
    session_store: secrets::SessionStore,
    data_dir: PathBuf,
    root_certificate: Option<matrix_sdk::reqwest::Certificate>,
}

impl AuthContext {
    fn application() -> Result<Self, AuthError> {
        let dirs = ProjectDirs::from("", "Twayblade", "Twayblade").ok_or(AuthError::Storage)?;
        Ok(Self {
            session_store: secrets::SessionStore::default(),
            data_dir: dirs.data_local_dir().to_owned(),
            root_certificate: None,
        })
    }

    fn store_path(&self, store_id: Uuid) -> PathBuf {
        self.data_dir.join("sessions").join(store_id.to_string())
    }

    async fn save(&self, record: &SavedSession) -> Result<(), AuthError> {
        let value = Zeroizing::new(serde_json::to_string(record).map_err(|_| AuthError::Storage)?);
        self.session_store
            .save(value)
            .await
            .map_err(|_| AuthError::Storage)
    }
}

async fn remove_store(path: &std::path::Path) -> Result<(), AuthError> {
    match tokio::fs::remove_dir_all(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(AuthError::Storage),
    }
}

async fn client(
    context: &AuthContext,
    record: &SavedSession,
) -> Result<(Client, PathBuf), AuthError> {
    validate_homeserver(&record.homeserver).map_err(|_| AuthError::Restore)?;
    let path = context.store_path(record.store_id);
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|_| AuthError::Storage)?;
    let mut builder = Client::builder()
        .with_threading_support(matrix_sdk::ThreadingSupport::Enabled {
            with_subscriptions: false,
        })
        .homeserver_url(&record.homeserver)
        .handle_refresh_tokens()
        .sqlite_store(&path, Some(&record.passphrase));
    if let Some(certificate) = &context.root_certificate {
        builder = builder.add_root_certificates(vec![certificate.clone()]);
    }
    let client = builder.build().await.map_err(|_| AuthError::Storage)?;
    Ok((client, path))
}

pub async fn restore() -> Result<Option<Account>, AuthError> {
    restore_in(&AuthContext::application()?).await
}

async fn restore_in(context: &AuthContext) -> Result<Option<Account>, AuthError> {
    let Some(value) = context
        .session_store
        .load()
        .await
        .map_err(|_| AuthError::Storage)?
    else {
        return Ok(None);
    };
    let mut record: SavedSession = serde_json::from_str(&value).map_err(|_| AuthError::Restore)?;
    let session = match (record.session.take(), record.oauth.take()) {
        (Some(session), None) => AuthSession::Matrix(session),
        (None, Some(session)) => AuthSession::OAuth(Box::new(OAuthSession {
            client_id: ClientId::new(session.client_id),
            user: session.user,
        })),
        (None, None) => return Ok(None),
        _ => return Err(AuthError::Restore),
    };
    let (client, path) = client(context, &record).await?;
    client
        .restore_session(session)
        .await
        .map_err(|_| AuthError::Restore)?;
    let persistence_lock =
        install_session_callbacks(&client, context.session_store.clone(), record.store_id)?;
    client.send_queue().enable_upload_progress(true);
    client.send_queue().set_enabled(true).await;
    Ok(Some(
        Account::new(
            client,
            path,
            context.session_store.clone(),
            persistence_lock,
        )
        .with_certificate(context.root_certificate.clone()),
    ))
}

pub async fn login_password(
    homeserver: String,
    username: String,
    password: Zeroizing<String>,
) -> Result<Account, AuthError> {
    login_password_in(&AuthContext::application()?, homeserver, username, password).await
}

async fn login_password_in(
    context: &AuthContext,
    homeserver: String,
    username: String,
    password: Zeroizing<String>,
) -> Result<Account, AuthError> {
    let homeserver = validate_homeserver(&homeserver).map_err(|_| AuthError::Login)?;
    if username.trim().is_empty() || password.is_empty() {
        return Err(AuthError::Login);
    }
    if let Some(value) = context
        .session_store
        .load()
        .await
        .map_err(|_| AuthError::Storage)?
    {
        let previous: SavedSession =
            serde_json::from_str(&value).map_err(|_| AuthError::Restore)?;
        if previous.session.is_some() || previous.oauth.is_some() {
            return Err(AuthError::ExistingSession);
        }
        // An interrupted earlier sign-in left an empty, never-logged-in store.
        remove_store(&context.store_path(previous.store_id)).await?;
    }
    let mut record = SavedSession {
        homeserver: homeserver.to_string(),
        generation: Uuid::new_v4(),
        store_id: Uuid::new_v4(),
        passphrase: Uuid::new_v4().to_string(),
        session: None,
        oauth: None,
    };
    // Prove secure storage works before transmitting the password or creating a device.
    context.save(&record).await?;
    let (client, path) = client(context, &record).await?;
    let login = client
        .matrix_auth()
        .login_username(username.trim(), &password)
        .initial_device_display_name("Twayblade desktop")
        .await;
    let session = login.ok().and_then(|_| client.matrix_auth().session());
    let Some(session) = session else {
        // No device exists; discard the checkpoint and its empty store.
        drop(client);
        let _ = context.session_store.delete().await;
        let _ = remove_store(&path).await;
        return Err(AuthError::Login);
    };
    record.session = Some(session);
    if let Err(error) = context.save(&record).await {
        // Best effort revocation if secure persistence failed after server login.
        let _ = client.matrix_auth().logout().await;
        return Err(error);
    }
    let persistence_lock =
        install_session_callbacks(&client, context.session_store.clone(), record.store_id)?;
    Ok(Account::new(
        client,
        path,
        context.session_store.clone(),
        persistence_lock,
    )
    .with_certificate(context.root_certificate.clone()))
}

/// Serialize full-record replacement with logout. A refresh from a stale
/// client cannot recreate a deleted record or overwrite a different account.
fn install_session_callbacks(
    client: &Client,
    store: secrets::SessionStore,
    store_id: Uuid,
) -> Result<Arc<Mutex<()>>, AuthError> {
    let value = store
        .load_sync()
        .map_err(|_| AuthError::Storage)?
        .ok_or(AuthError::Storage)?;
    let record: SavedSession = serde_json::from_str(&value).map_err(|_| AuthError::Storage)?;
    let generation = record.generation;
    let gate = Arc::new(Mutex::new(()));
    let reload_store = store.clone();
    let reload_gate = gate.clone();
    let save_gate = gate.clone();
    client
        .set_session_callbacks(
            Box::new(move |_| {
                let _guard = reload_gate.lock().map_err(|_| AuthError::Storage)?;
                let value = reload_store.load_sync()?.ok_or(AuthError::Storage)?;
                let record: SavedSession = serde_json::from_str(&value)?;
                if record.store_id != store_id || record.generation != generation {
                    return Err(AuthError::Storage.into());
                }
                match (record.session, record.oauth) {
                    (Some(session), None) => Ok(session.tokens),
                    (None, Some(session)) => Ok(session.user.tokens),
                    _ => Err(AuthError::Storage.into()),
                }
            }),
            Box::new(move |client| {
                let _guard = save_gate.lock().map_err(|_| AuthError::Storage)?;
                let value = store.load_sync()?.ok_or(AuthError::Storage)?;
                let mut record: SavedSession = serde_json::from_str(&value)?;
                if record.store_id != store_id || record.generation != generation {
                    return Err(AuthError::Storage.into());
                }
                record.update(client.session().ok_or(AuthError::Storage)?)?;
                let value = Zeroizing::new(serde_json::to_string(&record)?);
                store.save_sync(&value)?;
                Ok(())
            }),
        )
        .map_err(|_| AuthError::Storage)?;
    Ok(gate)
}

#[cfg(test)]
mod integration;
