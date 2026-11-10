//! Revocation projection and same-device recovery. The SDK owns token semantics.
use super::{
    Account, AuthContext, AuthError, SavedSession, client, install_session_callbacks, remove_store,
};
use matrix_sdk::{Client, SessionChange};
use std::sync::Arc;
use tokio::sync::watch;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionState {
    #[default]
    Active,
    SoftLoggedOut,
    HardLoggedOut,
}

pub(super) struct SessionMonitor(tokio::task::JoinHandle<()>);
impl Drop for SessionMonitor {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(super) fn monitor(client: &Client) -> (watch::Receiver<SessionState>, Arc<SessionMonitor>) {
    let mut changes = client.subscribe_to_session_changes();
    let (sender, receiver) = watch::channel(SessionState::Active);
    let task = tokio::spawn(async move {
        loop {
            match changes.recv().await {
                Ok(SessionChange::UnknownToken(error)) => {
                    sender.send_replace(if error.soft_logout {
                        SessionState::SoftLoggedOut
                    } else {
                        SessionState::HardLoggedOut
                    });
                    return;
                }
                Ok(SessionChange::TokensRefreshed)
                | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            }
        }
    });
    (receiver, Arc::new(SessionMonitor(task)))
}

impl Account {
    pub fn session_state(&self) -> watch::Receiver<SessionState> {
        self.session_state.clone()
    }

    pub(super) fn recovery_context(&self) -> Result<AuthContext, AuthError> {
        Ok(AuthContext {
            session_store: self.session_store.clone(),
            data_dir: self
                .store_path
                .parent()
                .and_then(|path| path.parent())
                .ok_or(AuthError::Storage)?
                .to_owned(),
            root_certificate: self.root_certificate.clone(),
        })
    }

    pub(super) async fn recovery_record(&self) -> Result<SavedSession, AuthError> {
        if *self.session_state.borrow() != SessionState::SoftLoggedOut {
            return Err(AuthError::Restore);
        }
        let value = self
            .session_store
            .load()
            .await
            .map_err(|_| AuthError::Storage)?
            .ok_or(AuthError::Restore)?;
        let record: SavedSession = serde_json::from_str(&value).map_err(|_| AuthError::Restore)?;
        if self.recovery_context()?.store_path(record.store_id) != self.store_path {
            return Err(AuthError::Restore);
        }
        Ok(record)
    }

    pub(super) fn persist_reauthentication(
        &self,
        record: &mut SavedSession,
    ) -> Result<(), AuthError> {
        let _guard = self
            .persistence_lock
            .lock()
            .map_err(|_| AuthError::Storage)?;
        record.generation = uuid::Uuid::new_v4();
        let value = Zeroizing::new(serde_json::to_string(record).map_err(|_| AuthError::Storage)?);
        self.session_store
            .save_sync(&value)
            .map_err(|_| AuthError::Storage)
    }

    pub async fn reauthenticate_password(
        &self,
        password: Zeroizing<String>,
    ) -> Result<Account, AuthError> {
        let context = self.recovery_context()?;
        let mut record = self.recovery_record().await?;
        let old = record.session.as_ref().ok_or(AuthError::Login)?;
        let meta = old.meta.clone();
        let (client, path) = client(&context, &record).await?;
        client
            .matrix_auth()
            .login_username(meta.user_id.as_str(), &password)
            .device_id(meta.device_id.as_str())
            .request_refresh_token()
            .await
            .map_err(|_| AuthError::Login)?;
        if client.user_id() != Some(&meta.user_id) || client.device_id() != Some(&meta.device_id) {
            return Err(AuthError::Restore);
        }
        record.update(client.session().ok_or(AuthError::Login)?)?;
        self.persist_reauthentication(&mut record)?;
        let gate =
            install_session_callbacks(&client, context.session_store.clone(), record.store_id)?;
        Ok(Account::new(client, path, context.session_store, gate)
            .with_certificate(context.root_certificate))
    }

    /// Called only after a hard-logout confirmation in the frontend.
    pub async fn discard_revoked_session(&self) -> Result<(), AuthError> {
        if *self.session_state.borrow() != SessionState::HardLoggedOut {
            return Err(AuthError::Logout);
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
        remove_store(&self.store_path).await
    }

    pub fn homeserver(&self) -> String {
        self.client.homeserver().to_string()
    }
    pub fn uses_oauth(&self) -> bool {
        matches!(
            self.client.session(),
            Some(matrix_sdk::AuthSession::OAuth(_))
        )
    }
}
