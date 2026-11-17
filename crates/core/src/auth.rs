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
mod accounts;
mod loopback;
mod oauth;
mod recovery;
mod sso;
pub use accounts::SavedAccount;
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
    fn new(homeserver: String) -> Self {
        Self {
            homeserver,
            generation: Uuid::new_v4(),
            store_id: Uuid::new_v4(),
            passphrase: Uuid::new_v4().to_string(),
            session: None,
            oauth: None,
        }
    }

    fn user_id(&self) -> Option<String> {
        match (&self.session, &self.oauth) {
            (Some(session), None) => Some(session.meta.user_id.to_string()),
            (None, Some(oauth)) => Some(oauth.user.meta.user_id.to_string()),
            _ => None,
        }
    }

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
    pub(crate) call_slot: Arc<tokio::sync::Semaphore>,
    /// Live timelines of recently opened rooms, reused on return (#173).
    pub(crate) recent_timelines: crate::conversation::RecentTimelines,
    /// Media may be kept in the encrypted store (#174); off until set.
    pub(crate) media_cache: Arc<std::sync::atomic::AtomicBool>,
    instance_id: Uuid,
    remotely_revoked: Arc<AtomicBool>,
    store_id: Uuid,
    store_path: PathBuf,
    data_dir: PathBuf,
    /// The service holding every account's slot (for recovery contexts).
    base_store: secrets::SessionStore,
    /// This account's own credential slot.
    session_store: secrets::SessionStore,
    persistence_lock: Arc<Mutex<()>>,
    pub(crate) root_certificate: Option<matrix_sdk::reqwest::Certificate>,
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

    /// Test fixtures: an account outside the saved-account index.
    #[cfg(test)]
    pub(crate) fn new(
        client: Client,
        store_path: PathBuf,
        session_store: secrets::SessionStore,
        persistence_lock: Arc<Mutex<()>>,
    ) -> Self {
        let data_dir = store_path
            .parent()
            .and_then(|path| path.parent())
            .map(ToOwned::to_owned)
            .unwrap_or_default();
        let mut account = Self::open(
            client,
            &AuthContext {
                session_store: session_store.clone(),
                data_dir,
                root_certificate: None,
            },
            Uuid::new_v4(),
            persistence_lock,
        );
        account.store_path = store_path;
        account.session_store = session_store;
        account
    }

    #[cfg(test)]
    pub(crate) fn with_certificate(
        mut self,
        certificate: Option<matrix_sdk::reqwest::Certificate>,
    ) -> Self {
        self.root_certificate = certificate;
        self
    }

    fn open(
        client: Client,
        context: &AuthContext,
        store_id: Uuid,
        persistence_lock: Arc<Mutex<()>>,
    ) -> Self {
        let (session_state, monitor) = recovery::monitor(&client);
        let store_path = context.store_path(store_id);
        let session_store = context.slot(store_id);
        Self {
            client,
            thread_read_updates: tokio::sync::broadcast::channel(32).0,
            notification_service: tokio::sync::watch::channel(None).0,
            selected_room: tokio::sync::watch::channel(None).0,
            sync_requests: Arc::new(Mutex::new(std::collections::HashMap::new())),
            sync_request_updates: tokio::sync::watch::channel(Vec::new()).0,
            call_slot: Arc::new(tokio::sync::Semaphore::new(1)),
            recent_timelines: Default::default(),
            media_cache: Default::default(),
            instance_id: Uuid::new_v4(),
            remotely_revoked: Arc::new(AtomicBool::new(false)),
            store_id,
            store_path,
            data_dir: context.data_dir.clone(),
            base_store: context.session_store.clone(),
            session_store,
            persistence_lock,
            root_certificate: context.root_certificate.clone(),
            session_state,
            _session_monitor: monitor,
        }
    }

    /// The saved account this session belongs to (see [`saved_accounts`]).
    pub fn id(&self) -> String {
        self.store_id.to_string()
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
        self.forget().await
    }

    /// Drops this account from the saved list and deletes its local store.
    /// The revoked device's keys and cached history have no further use.
    async fn forget(&self) -> Result<(), AuthError> {
        let store_id = self.store_id;
        accounts::update(&self.data_dir, |index| {
            index.remove(store_id);
            Ok(())
        })?;
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
    /// The Matrix user signing in is already saved in Twayblade.
    #[error("auth_duplicate_account")]
    DuplicateAccount,
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

    fn slot(&self, store_id: Uuid) -> secrets::SessionStore {
        self.session_store.slot(&store_id.to_string())
    }

    async fn save(&self, record: &SavedSession) -> Result<(), AuthError> {
        let value = Zeroizing::new(serde_json::to_string(record).map_err(|_| AuthError::Storage)?);
        self.slot(record.store_id)
            .save(value)
            .await
            .map_err(|_| AuthError::Storage)
    }

    /// Starts adding an account: a pending index entry and a credential
    /// checkpoint, proving secure storage works before any password or browser
    /// step. Cleaned up by [`Self::abandon`], or at the next launch.
    async fn begin_new(&self, homeserver: String) -> Result<SavedSession, AuthError> {
        let record = SavedSession::new(homeserver);
        let entry = accounts::Entry {
            store_id: record.store_id,
            user_id: String::new(),
            homeserver: record.homeserver.clone(),
            pending: true,
        };
        accounts::update(&self.data_dir, |index| {
            index.accounts.push(entry);
            Ok(())
        })?;
        if let Err(error) = self.save(&record).await {
            self.abandon(&record).await;
            return Err(error);
        }
        Ok(record)
    }

    /// Best-effort removal of an unfinished sign-in's slot, store and entry.
    async fn abandon(&self, record: &SavedSession) {
        let _ = self.slot(record.store_id).delete().await;
        let _ = remove_store(&self.store_path(record.store_id)).await;
        let _ = accounts::update(&self.data_dir, |index| {
            index.remove(record.store_id);
            Ok(())
        });
    }

    /// Lists the signed-in account and makes it active. Rejects a user that
    /// is already saved, before anything is connected.
    fn complete(&self, record: &SavedSession) -> Result<(), AuthError> {
        let user_id = record.user_id().ok_or(AuthError::Storage)?;
        accounts::update(&self.data_dir, |index| {
            index.complete(record.store_id, &user_id)
        })
    }

    /// Runs once at launch, before the first restore: moves the original
    /// single-session slot into its own per-account slot and removes sign-ins
    /// a crash interrupted.
    async fn prepare(&self) -> Result<(), AuthError> {
        if let Some(value) = self
            .session_store
            .load()
            .await
            .map_err(|_| AuthError::Storage)?
        {
            let record: SavedSession =
                serde_json::from_str(&value).map_err(|_| AuthError::Restore)?;
            if let Some(user_id) = record.user_id() {
                // Write the new slot and the index before removing the old
                // slot, so an interruption only repeats this step.
                self.slot(record.store_id)
                    .save(value)
                    .await
                    .map_err(|_| AuthError::Storage)?;
                accounts::update(&self.data_dir, |index| {
                    if !index
                        .accounts
                        .iter()
                        .any(|entry| entry.store_id == record.store_id)
                    {
                        index.accounts.insert(
                            0,
                            accounts::Entry {
                                store_id: record.store_id,
                                user_id,
                                homeserver: record.homeserver.clone(),
                                pending: false,
                            },
                        );
                    }
                    index.active.get_or_insert(record.store_id);
                    Ok(())
                })?;
            } else {
                // An interrupted sign-in left an empty, never-logged-in store.
                remove_store(&self.store_path(record.store_id)).await?;
            }
            self.session_store
                .delete()
                .await
                .map_err(|_| AuthError::Storage)?;
        }
        let pending: Vec<Uuid> = accounts::read(&self.data_dir)?
            .map(|index| {
                index
                    .accounts
                    .iter()
                    .filter(|entry| entry.pending)
                    .map(|entry| entry.store_id)
                    .collect()
            })
            .unwrap_or_default();
        for store_id in pending {
            self.slot(store_id)
                .delete()
                .await
                .map_err(|_| AuthError::Storage)?;
            remove_store(&self.store_path(store_id)).await?;
            accounts::update(&self.data_dir, |index| {
                index.remove(store_id);
                Ok(())
            })?;
        }
        Ok(())
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
        .request_config(crate::request_policy::interactive())
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

/// Opens the account used last, at launch. `None` when no account is saved.
pub async fn restore() -> Result<Option<Account>, AuthError> {
    restore_in(&AuthContext::application()?).await
}

/// Opens another saved account and makes it the one used at launch.
pub async fn restore_account(id: &str) -> Result<Account, AuthError> {
    let context = AuthContext::application()?;
    let store_id = Uuid::parse_str(id).map_err(|_| AuthError::Restore)?;
    restore_entry(&context, store_id)
        .await?
        .ok_or(AuthError::Restore)
}

/// Every signed-in account, in the order they were added. No credentials.
pub fn saved_accounts() -> Result<Vec<SavedAccount>, AuthError> {
    saved_accounts_in(&AuthContext::application()?)
}

fn saved_accounts_in(context: &AuthContext) -> Result<Vec<SavedAccount>, AuthError> {
    Ok(accounts::read(&context.data_dir)?
        .map(|index| index.saved())
        .unwrap_or_default())
}

async fn restore_in(context: &AuthContext) -> Result<Option<Account>, AuthError> {
    context.prepare().await?;
    // An entry whose credential vanished (for example a cleared keychain) is
    // dropped, and the next saved account is tried.
    loop {
        let Some(store_id) = accounts::read(&context.data_dir)?.and_then(|index| index.launch())
        else {
            return Ok(None);
        };
        if let Some(account) = restore_entry(context, store_id).await? {
            return Ok(Some(account));
        }
    }
}

/// `None` (after removing the entry) when the account's credential is gone.
async fn restore_entry(
    context: &AuthContext,
    store_id: Uuid,
) -> Result<Option<Account>, AuthError> {
    let listed = accounts::read(&context.data_dir)?
        .is_some_and(|index| index.ready().any(|entry| entry.store_id == store_id));
    if !listed {
        return Err(AuthError::Restore);
    }
    let slot = context.slot(store_id);
    let record = match slot.load().await.map_err(|_| AuthError::Storage)? {
        Some(value) => {
            let record: SavedSession =
                serde_json::from_str(&value).map_err(|_| AuthError::Restore)?;
            (record.store_id == store_id && record.user_id().is_some()).then_some(record)
        }
        None => None,
    };
    let Some(mut record) = record else {
        let _ = slot.delete().await;
        remove_store(&context.store_path(store_id)).await?;
        accounts::update(&context.data_dir, |index| {
            index.remove(store_id);
            Ok(())
        })?;
        return Ok(None);
    };
    let session = match (record.session.take(), record.oauth.take()) {
        (Some(session), None) => AuthSession::Matrix(session),
        (None, Some(session)) => AuthSession::OAuth(Box::new(OAuthSession {
            client_id: ClientId::new(session.client_id),
            user: session.user,
        })),
        _ => return Err(AuthError::Restore),
    };
    let (client, _) = client(context, &record).await?;
    client
        .restore_session(session)
        .await
        .map_err(|_| AuthError::Restore)?;
    let persistence_lock = install_session_callbacks(&client, slot, store_id)?;
    client.send_queue().enable_upload_progress(true);
    client.send_queue().set_enabled(true).await;
    accounts::update(&context.data_dir, |index| {
        index.active = Some(store_id);
        Ok(())
    })?;
    Ok(Some(Account::open(
        client,
        context,
        store_id,
        persistence_lock,
    )))
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
    // Prove secure storage works before transmitting the password or creating a device.
    let mut record = context.begin_new(homeserver.to_string()).await?;
    let (client, _) = match client(context, &record).await {
        Ok(client) => client,
        Err(error) => {
            context.abandon(&record).await;
            return Err(error);
        }
    };
    let login = client
        .matrix_auth()
        .login_username(username.trim(), &password)
        .initial_device_display_name("Twayblade desktop")
        .await;
    let session = login.ok().and_then(|_| client.matrix_auth().session());
    let Some(session) = session else {
        // No device exists; discard the checkpoint and its empty store.
        drop(client);
        context.abandon(&record).await;
        return Err(AuthError::Login);
    };
    record.session = Some(session);
    let saved = match context.save(&record).await {
        Ok(()) => context.complete(&record),
        Err(error) => Err(error),
    };
    if let Err(error) = saved {
        // Revoke the new device if it cannot be kept (storage failed, or this
        // user is already signed in here).
        let _ = client.matrix_auth().logout().await;
        drop(client);
        context.abandon(&record).await;
        return Err(error);
    }
    let persistence_lock =
        install_session_callbacks(&client, context.slot(record.store_id), record.store_id)?;
    Ok(Account::open(
        client,
        context,
        record.store_id,
        persistence_lock,
    ))
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

// Only the macOS Keychain integration tests read credentials back.
#[cfg(all(test, target_os = "macos"))]
impl AuthContext {
    /// Test view of the saved credential: the first listed account's slot
    /// (including a pending sign-in), else the original single-session slot.
    async fn saved_credential(
        &self,
    ) -> Result<Option<Zeroizing<String>>, secrets::SecretStoreError> {
        let first = accounts::read(&self.data_dir)
            .ok()
            .flatten()
            .and_then(|index| index.accounts.first().map(|entry| entry.store_id));
        match first {
            Some(store_id) => self.slot(store_id).load().await,
            None => self.session_store.load().await,
        }
    }
}

#[cfg(test)]
mod account_storage {
    use super::*;
    use matrix_sdk::{
        SessionMeta, SessionTokens,
        ruma::{device_id, user_id},
    };

    fn record(user: &matrix_sdk::ruma::UserId) -> SavedSession {
        let mut record = SavedSession::new("https://matrix.example.org/".into());
        record.session = Some(MatrixSession {
            meta: SessionMeta {
                user_id: user.to_owned(),
                device_id: device_id!("TESTDEVICE").to_owned(),
            },
            tokens: SessionTokens {
                access_token: "not-a-real-token".into(),
                refresh_token: None,
            },
        });
        record
    }

    /// Touches the real OS credential store under an isolated service name;
    /// every item it writes is removed again.
    #[tokio::test]
    #[ignore = "requires an unlocked OS credential store"]
    async fn single_session_slot_migrates_and_interrupted_sign_ins_are_removed() {
        let scope = Uuid::new_v4();
        let context = AuthContext {
            session_store: secrets::SessionStore::new(&format!(
                "twayblade.integration.accounts.{scope}"
            )),
            data_dir: std::env::temp_dir().join(format!("twayblade-accounts-{scope}")),
            root_certificate: None,
        };
        // An account saved by an earlier version, in the single original slot.
        let legacy = record(user_id!("@alice:example.org"));
        let value = serde_json::to_string(&legacy).expect("record");
        context
            .session_store
            .save(Zeroizing::new(value.clone()))
            .await
            .expect("legacy slot");
        tokio::fs::create_dir_all(context.store_path(legacy.store_id))
            .await
            .expect("legacy store");
        // A sign-in that a crash interrupted: pending entry, slot and store.
        let interrupted = context
            .begin_new("https://other.example.org/".into())
            .await
            .expect("pending sign-in");
        tokio::fs::create_dir_all(context.store_path(interrupted.store_id))
            .await
            .expect("pending store");

        context.prepare().await.expect("prepare");
        assert!(
            context.session_store.load().await.expect("load").is_none(),
            "the original slot is emptied"
        );
        let moved = context
            .slot(legacy.store_id)
            .load()
            .await
            .expect("load")
            .expect("account slot");
        assert_eq!(moved.as_str(), value, "the record moves unchanged");
        assert!(context.store_path(legacy.store_id).exists(), "history kept");
        assert!(
            context
                .slot(interrupted.store_id)
                .load()
                .await
                .expect("load")
                .is_none()
        );
        assert!(!context.store_path(interrupted.store_id).exists());
        let index = accounts::read(&context.data_dir)
            .expect("index")
            .expect("written");
        assert_eq!(index.launch(), Some(legacy.store_id));
        assert_eq!(
            saved_accounts_in(&context).expect("list"),
            [SavedAccount {
                id: legacy.store_id.to_string(),
                user_id: "@alice:example.org".into(),
                homeserver: "https://matrix.example.org/".into(),
            }]
        );
        // Running again is a no-op.
        context.prepare().await.expect("second prepare");
        assert_eq!(saved_accounts_in(&context).expect("list").len(), 1);

        // Adding the same user again is refused without touching the saved one.
        let mut again = context
            .begin_new("https://matrix.example.org/".into())
            .await
            .expect("second sign-in");
        again.session = record(user_id!("@alice:example.org")).session;
        assert!(matches!(
            context.complete(&again),
            Err(AuthError::DuplicateAccount)
        ));
        context.abandon(&again).await;
        assert!(
            context
                .slot(again.store_id)
                .load()
                .await
                .expect("load")
                .is_none()
        );
        assert_eq!(saved_accounts_in(&context).expect("list").len(), 1);

        let _ = context.slot(legacy.store_id).delete().await;
        let _ = tokio::fs::remove_dir_all(&context.data_dir).await;
    }
}

#[cfg(test)]
mod integration;

#[cfg(test)]
mod room_open;
