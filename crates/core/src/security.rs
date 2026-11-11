//! SDK-owned verification, cross-signing and recovery. No recovery secrets in snapshots.
use crate::auth::Account;
use futures_util::StreamExt;
use matrix_sdk::{
    Client,
    encryption::{
        recovery::RecoveryState,
        verification::{
            SasState, SasVerification, Verification, VerificationRequest, VerificationRequestState,
        },
    },
    ruma::{
        OwnedDeviceId,
        events::key::verification::{
            VerificationMethod, request::ToDeviceKeyVerificationRequestEvent,
        },
    },
};
use std::sync::{Arc, Mutex, Weak};
use tokio::{sync::watch, task::JoinHandle};
use zeroize::Zeroizing;

#[derive(Clone, Debug, Default)]
pub struct DeviceRow {
    pub id: String,
    pub name: Option<String>,
    pub verified: bool,
    pub current: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum VerificationPhase {
    #[default]
    Requested,
    Waiting,
    Emojis(Vec<(String, String)>),
    Confirmed,
    Done,
    Cancelled,
}
#[derive(Clone, Debug, Default)]
pub struct VerificationView {
    pub flow: String,
    pub device: String,
    pub incoming: bool,
    pub phase: VerificationPhase,
}
#[derive(Clone, Debug, Default)]
pub struct SecuritySnapshot {
    pub oauth_management: bool,
    pub password_devices: bool,
    pub ready: bool,
    pub failed: bool,
    pub has_identity: bool,
    pub verified: bool,
    pub backup_exists: bool,
    pub recovery_enabled: bool,
    pub devices: Vec<DeviceRow>,
    pub verification: Option<VerificationView>,
}
#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("security_unavailable")]
    Unavailable,
    #[error("security_setup")]
    Setup,
    #[error("security_recovery")]
    Recovery,
    #[error("security_verification")]
    Verification,
    #[error("security_device_auth")]
    DeviceAuthentication,
}
#[derive(Default)]
struct Flow {
    request: Option<VerificationRequest>,
    sas: Option<SasVerification>,
}
type Tasks = Arc<Mutex<Vec<JoinHandle<()>>>>;
struct Lifetime {
    client: Client,
    handler: matrix_sdk::event_handler::EventHandlerHandle,
    tasks: Tasks,
}
impl Drop for Lifetime {
    fn drop(&mut self) {
        self.client.remove_event_handler(self.handler.clone());
        if let Ok(tasks) = self.tasks.lock() {
            for task in tasks.iter() {
                task.abort();
            }
        }
    }
}
#[derive(Clone)]
pub struct Security {
    client: Client,
    flow: Arc<Mutex<Flow>>,
    updates: watch::Sender<SecuritySnapshot>,
    lifetime: Arc<Lifetime>,
}
fn publish(updates: &watch::Sender<SecuritySnapshot>, update: impl FnOnce(&mut SecuritySnapshot)) {
    updates.send_modify(update);
}
fn publish_phase(updates: &watch::Sender<SecuritySnapshot>, id: &str, phase: VerificationPhase) {
    publish(updates, |view| {
        if let Some(flow) = &mut view.verification
            && flow.flow == id
        {
            flow.phase = phase;
        }
    });
}
fn spawn(
    tasks: &Weak<Mutex<Vec<JoinHandle<()>>>>,
    future: impl std::future::Future<Output = ()> + Send + 'static,
) {
    if let Some(tasks) = tasks.upgrade()
        && let Ok(mut tasks) = tasks.lock()
    {
        tasks.retain(|task| !task.is_finished());
        tasks.push(tokio::spawn(future));
    }
}

impl Security {
    pub fn new(account: &Account) -> Self {
        let client = account.client.clone();
        let flow = Arc::new(Mutex::new(Flow::default()));
        let (updates, _) = watch::channel(SecuritySnapshot::default());
        let tasks: Tasks = Arc::new(Mutex::new(Vec::new()));
        let handler = client.add_event_handler({
            let flow = flow.clone();
            let updates = updates.clone();
            let tasks = Arc::downgrade(&tasks);
            move |event: ToDeviceKeyVerificationRequestEvent, client: Client| {
                let flow = flow.clone();
                let updates = updates.clone();
                let tasks = tasks.clone();
                async move {
                    if Some(event.sender.as_ref()) != client.user_id() {
                        return;
                    }
                    if let Some(request) = client
                        .encryption()
                        .get_verification_request(
                            &event.sender,
                            event.content.transaction_id.as_str(),
                        )
                        .await
                    {
                        set_request(client, flow, updates, tasks, request);
                    }
                }
            }
        });
        let lifetime = Arc::new(Lifetime {
            client: client.clone(),
            handler,
            tasks,
        });
        let security = Self {
            client: client.clone(),
            flow,
            updates: updates.clone(),
            lifetime,
        };
        let task_list = Arc::downgrade(&security.lifetime.tasks);
        spawn(&task_list, async move {
            client
                .encryption()
                .wait_for_e2ee_initialization_tasks()
                .await;
            if refresh(&client, &updates).await.is_err() {
                publish(&updates, |view| view.failed = true);
            }
            let encryption = client.encryption();
            let mut verification = encryption.verification_state();
            let recovery = encryption.recovery();
            let mut recovery = Box::pin(recovery.state_stream());
            let backups = encryption.backups();
            let mut backups = Box::pin(backups.state_stream());
            let mut downloaded = false;
            loop {
                tokio::select! { _ = verification.next() => {}, _ = recovery.next() => {}, _ = backups.next() => {} }
                if refresh(&client, &updates).await.is_err() {
                    publish(&updates, |view| view.failed = true);
                }
                let verified = updates.borrow().verified;
                if !downloaded && verified && client.encryption().backups().are_enabled().await {
                    downloaded = Security::download_keys_for_client(&client).await.is_ok();
                }
            }
        });
        security
    }

    pub fn subscribe(&self) -> watch::Receiver<SecuritySnapshot> {
        self.updates.subscribe()
    }
    pub async fn refresh(&self) -> Result<(), SecurityError> {
        refresh(&self.client, &self.updates).await
    }

    pub async fn setup_backup(&self) -> Result<Zeroizing<String>, SecurityError> {
        let user = self.client.user_id().ok_or(SecurityError::Unavailable)?;
        if self
            .client
            .encryption()
            .request_user_identity(user)
            .await
            .map_err(|_| SecurityError::Unavailable)?
            .is_none()
        {
            self.client
                .encryption()
                .bootstrap_cross_signing(None)
                .await
                .map_err(|_| SecurityError::Setup)?;
        }
        let status = self
            .client
            .encryption()
            .cross_signing_status()
            .await
            .ok_or(SecurityError::Setup)?;
        if !status.is_complete() {
            return Err(SecurityError::Setup);
        }
        let key = self
            .client
            .encryption()
            .recovery()
            .enable()
            .wait_for_backups_to_upload()
            .await
            .map_err(|_| SecurityError::Setup)?;
        let key = Zeroizing::new(key);
        // Once generated, deliver the key even if the presentation refresh fails.
        let _ = self.refresh().await;
        Ok(key)
    }

    pub async fn recover(&self, key: Zeroizing<String>) -> Result<(), SecurityError> {
        self.client
            .encryption()
            .recovery()
            .recover(&key)
            .await
            .map_err(|_| SecurityError::Recovery)?;
        drop(key);
        self.download_keys().await?;
        self.refresh().await
    }

    pub async fn reset_recovery(&self) -> Result<Zeroizing<String>, SecurityError> {
        let key = Zeroizing::new(
            self.client
                .encryption()
                .recovery()
                .reset_key()
                .await
                .map_err(|_| SecurityError::Recovery)?,
        );
        let _ = self.refresh().await;
        Ok(key)
    }

    pub async fn download_keys(&self) -> Result<(), SecurityError> {
        let backups = self.client.encryption().backups();
        if backups.are_enabled().await {
            for room in self
                .client
                .rooms()
                .into_iter()
                .filter(|room| room.encryption_state().is_encrypted())
            {
                backups
                    .download_room_keys_for_room(room.room_id())
                    .await
                    .map_err(|_| SecurityError::Recovery)?;
            }
        }
        Ok(())
    }

    pub async fn request_verification(&self, device: String) -> Result<(), SecurityError> {
        let device = OwnedDeviceId::from(device);
        let own = self.client.user_id().ok_or(SecurityError::Unavailable)?;
        let device = self
            .client
            .encryption()
            .get_device(own, &device)
            .await
            .map_err(|_| SecurityError::Unavailable)?
            .ok_or(SecurityError::Verification)?;
        let request = device
            .request_verification_with_methods(vec![VerificationMethod::SasV1])
            .await
            .map_err(|_| SecurityError::Verification)?;
        set_request(
            self.client.clone(),
            self.flow.clone(),
            self.updates.clone(),
            Arc::downgrade(&self.lifetime.tasks),
            request,
        );
        Ok(())
    }

    /// Verify another user's identity (SAS emoji), shown in the same flow UI.
    pub async fn request_user_verification(&self, user: String) -> Result<(), SecurityError> {
        let user = matrix_sdk::ruma::OwnedUserId::try_from(user)
            .map_err(|_| SecurityError::Verification)?;
        let identity = self
            .client
            .encryption()
            .request_user_identity(&user)
            .await
            .map_err(|_| SecurityError::Unavailable)?
            .ok_or(SecurityError::Verification)?;
        let request = identity
            .request_verification_with_methods(vec![VerificationMethod::SasV1])
            .await
            .map_err(|_| SecurityError::Verification)?;
        set_request(
            self.client.clone(),
            self.flow.clone(),
            self.updates.clone(),
            Arc::downgrade(&self.lifetime.tasks),
            request,
        );
        Ok(())
    }

    pub async fn accept_verification(&self) -> Result<(), SecurityError> {
        let request = self
            .flow
            .lock()
            .map_err(|_| SecurityError::Unavailable)?
            .request
            .clone()
            .ok_or(SecurityError::Verification)?;
        request
            .accept_with_methods(vec![VerificationMethod::SasV1])
            .await
            .map_err(|_| SecurityError::Verification)
    }
    pub async fn confirm_verification(&self) -> Result<(), SecurityError> {
        let sas = self
            .flow
            .lock()
            .map_err(|_| SecurityError::Unavailable)?
            .sas
            .clone()
            .ok_or(SecurityError::Verification)?;
        sas.confirm().await.map_err(|_| SecurityError::Verification)
    }
    pub async fn cancel_verification(&self, mismatch: bool) -> Result<(), SecurityError> {
        let (sas, request) = {
            let flow = self.flow.lock().map_err(|_| SecurityError::Unavailable)?;
            (flow.sas.clone(), flow.request.clone())
        };
        let id = request
            .as_ref()
            .map(|request| request.flow_id().to_owned())
            .ok_or(SecurityError::Verification)?;
        if let Some(sas) = sas {
            if mismatch {
                sas.mismatch().await
            } else {
                sas.cancel().await
            }
            .map_err(|_| SecurityError::Verification)?;
        } else if let Some(request) = request {
            request
                .cancel()
                .await
                .map_err(|_| SecurityError::Verification)?;
        }
        publish_phase(&self.updates, &id, VerificationPhase::Cancelled);
        Ok(())
    }
    pub async fn device_management_url(&self, id: String) -> Result<String, SecurityError> {
        use matrix_sdk::ruma::api::client::discovery::get_authorization_server_metadata::v1::{
            AccountManagementActionData, DeviceDeleteData,
        };
        let id = OwnedDeviceId::from(id);
        if self.client.device_id() == Some(&id) {
            return Err(SecurityError::DeviceAuthentication);
        }
        let metadata = self
            .client
            .oauth()
            .server_metadata()
            .await
            .map_err(|_| SecurityError::DeviceAuthentication)?;
        let url = metadata
            .account_management_url_with_action(AccountManagementActionData::DeviceDelete(
                DeviceDeleteData::new(&id),
            ))
            .ok_or(SecurityError::DeviceAuthentication)?;
        if url.scheme() != "https" {
            return Err(SecurityError::DeviceAuthentication);
        }
        Ok(url.to_string())
    }

    pub async fn sign_out_device(
        &self,
        id: String,
        password: Zeroizing<String>,
    ) -> Result<(), SecurityError> {
        use matrix_sdk::ruma::api::client::uiaa::{AuthData, Password, UserIdentifier};
        let id = OwnedDeviceId::from(id);
        if self.client.device_id() == Some(&id) {
            return Err(SecurityError::DeviceAuthentication);
        }
        let user = self
            .client
            .user_id()
            .ok_or(SecurityError::Unavailable)?
            .to_owned();
        let mut auth = Password::new(UserIdentifier::from(user), password.to_string());
        if let Err(error) = self
            .client
            .delete_devices(
                std::slice::from_ref(&id),
                Some(AuthData::Password(auth.clone())),
            )
            .await
        {
            auth.session = error
                .as_uiaa_response()
                .and_then(|info| info.session.clone());
            if auth.session.is_none() {
                return Err(SecurityError::DeviceAuthentication);
            }
            self.client
                .delete_devices(&[id], Some(AuthData::Password(auth)))
                .await
                .map_err(|_| SecurityError::DeviceAuthentication)?;
        }
        self.refresh().await
    }
}

async fn refresh(
    client: &Client,
    updates: &watch::Sender<SecuritySnapshot>,
) -> Result<(), SecurityError> {
    let user = client.user_id().ok_or(SecurityError::Unavailable)?;
    let identity = client
        .encryption()
        .request_user_identity(user)
        .await
        .map_err(|_| SecurityError::Unavailable)?;
    let own = client
        .encryption()
        .get_own_device()
        .await
        .map_err(|_| SecurityError::Unavailable)?;
    let backup_exists = client
        .encryption()
        .backups()
        .fetch_exists_on_server()
        .await
        .map_err(|_| SecurityError::Unavailable)?;
    let devices = client
        .devices()
        .await
        .map_err(|_| SecurityError::Unavailable)?;
    let mut rows = Vec::new();
    for device in devices.devices {
        let crypto = client
            .encryption()
            .get_device(user, &device.device_id)
            .await
            .map_err(|_| SecurityError::Unavailable)?;
        rows.push(DeviceRow {
            current: client.device_id() == Some(&device.device_id),
            id: device.device_id.to_string(),
            name: device.display_name,
            verified: crypto.is_some_and(|device| device.is_verified()),
        });
    }
    rows.sort_by(|a, b| b.current.cmp(&a.current).then(a.id.cmp(&b.id)));
    let oauth = matches!(client.session(), Some(matrix_sdk::AuthSession::OAuth(_)));
    let oauth_management = if oauth {
        client
            .oauth()
            .server_metadata()
            .await
            .ok()
            .is_some_and(|metadata| {
                metadata
                    .account_management_uri
                    .as_ref()
                    .is_some_and(|uri| uri.scheme() == "https")
            })
    } else {
        false
    };
    publish(updates, |view| {
        view.oauth_management = oauth_management;
        view.password_devices = !oauth;
        view.ready = true;
        view.failed = false;
        view.has_identity = identity.is_some();
        // Not `is_verified()`: the SDK marks its own device locally trusted, so
        // that is true even for a session no other client trusts. Verified means
        // signed by the account's cross-signing identity, as Element X reports it.
        view.verified = own.is_some_and(|device| device.is_cross_signed_by_owner());
        view.backup_exists = backup_exists;
        view.recovery_enabled = client.encryption().recovery().state() == RecoveryState::Enabled;
        view.devices = rows;
    });
    Ok(())
}

fn set_request(
    client: Client,
    flow: Arc<Mutex<Flow>>,
    updates: watch::Sender<SecuritySnapshot>,
    tasks: Weak<Mutex<Vec<JoinHandle<()>>>>,
    request: VerificationRequest,
) {
    let Ok(mut active) = flow.lock() else {
        return;
    };
    let busy = active
        .sas
        .as_ref()
        .map(|sas| !sas.is_done() && !sas.is_cancelled())
        .unwrap_or_else(|| {
            active
                .request
                .as_ref()
                .is_some_and(|old| !old.is_done() && !old.is_cancelled())
        });
    if busy {
        return;
    }
    active.request = Some(request.clone());
    active.sas = None;
    drop(active);
    publish(&updates, |view| {
        view.verification = Some(VerificationView {
            flow: request.flow_id().to_owned(),
            incoming: !request.we_started(),
            device: request.other_user_id().to_string(),
            phase: if request.we_started() {
                VerificationPhase::Waiting
            } else {
                VerificationPhase::Requested
            },
        })
    });
    let list = tasks.clone();
    spawn(&tasks, async move {
        let mut changes = Box::pin(request.changes());
        let mut state = request.state();
        loop {
            match state {
                VerificationRequestState::Ready { .. } if request.we_started() => {
                    let _ = request.start_sas().await;
                }
                VerificationRequestState::Transitioned {
                    verification: Verification::SasV1(sas),
                } => {
                    if let Ok(mut active) = flow.lock() {
                        active.sas = Some(sas.clone());
                    }
                    let _ = sas.accept().await;
                    watch_sas(client, updates, sas, request.flow_id().to_owned()).await;
                    return;
                }
                VerificationRequestState::Done => {
                    publish_phase(&updates, request.flow_id(), VerificationPhase::Done);
                    return;
                }
                VerificationRequestState::Cancelled(_) => {
                    publish_phase(&updates, request.flow_id(), VerificationPhase::Cancelled);
                    return;
                }
                _ => {}
            }
            let Some(next) = changes.next().await else {
                break;
            };
            state = next;
        }
        drop(list);
    });
}
async fn watch_sas(
    client: Client,
    updates: watch::Sender<SecuritySnapshot>,
    sas: SasVerification,
    id: String,
) {
    let mut changes = Box::pin(sas.changes());
    let mut state = sas.state();
    loop {
        let phase = match state {
            SasState::KeysExchanged { .. } => sas
                .emoji()
                .map(|emoji| {
                    VerificationPhase::Emojis(
                        emoji
                            .into_iter()
                            .map(|emoji| (emoji.symbol.into(), emoji.description.into()))
                            .collect(),
                    )
                })
                .unwrap_or(VerificationPhase::Waiting),
            SasState::Confirmed => VerificationPhase::Confirmed,
            SasState::Done { .. } => VerificationPhase::Done,
            SasState::Cancelled(_) => VerificationPhase::Cancelled,
            _ => VerificationPhase::Waiting,
        };
        let done = phase == VerificationPhase::Done;
        let cancelled = phase == VerificationPhase::Cancelled;
        publish_phase(&updates, &id, phase);
        if done {
            let _ = Security::download_keys_for_client(&client).await;
            let _ = refresh(&client, &updates).await;
            return;
        }
        if cancelled {
            return;
        }
        let Some(next) = changes.next().await else {
            return;
        };
        state = next;
    }
}
impl Security {
    async fn download_keys_for_client(client: &Client) -> Result<(), SecurityError> {
        let backups = client.encryption().backups();
        if backups.are_enabled().await {
            for room in client
                .rooms()
                .into_iter()
                .filter(|room| room.encryption_state().is_encrypted())
            {
                backups
                    .download_room_keys_for_room(room.room_id())
                    .await
                    .map_err(|_| SecurityError::Recovery)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod integration;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_finished_verification_cannot_overwrite_a_new_request() {
        let (updates, _) = watch::channel(SecuritySnapshot {
            verification: Some(VerificationView {
                flow: "new".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        publish_phase(&updates, "old", VerificationPhase::Cancelled);
        assert_eq!(
            updates.borrow().verification.as_ref().unwrap().phase,
            VerificationPhase::Requested
        );
        publish_phase(&updates, "new", VerificationPhase::Waiting);
        assert_eq!(
            updates.borrow().verification.as_ref().unwrap().phase,
            VerificationPhase::Waiting
        );
    }
}
