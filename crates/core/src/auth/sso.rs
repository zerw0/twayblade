//! Legacy SSO through the SDK and the shared, state-bound loopback receiver.
use super::{
    Account, AuthContext, AuthError, SavedSession, client, install_session_callbacks,
    loopback::{CallbackKind, Loopback},
    remove_store,
};
use matrix_sdk::Client;
use tokio::sync::watch;
use url::Url;
use uuid::Uuid;

pub struct SsoLogin {
    client: Client,
    context: AuthContext,
    record: SavedSession,
    listener: Loopback,
    state: String,
    url: Url,
    recovery: Option<Account>,
}

impl SsoLogin {
    pub async fn begin(homeserver: String, provider: Option<String>) -> Result<Self, AuthError> {
        Self::begin_in(AuthContext::application()?, homeserver, provider).await
    }

    pub async fn reauthenticate(
        account: Account,
        provider: Option<String>,
    ) -> Result<Self, AuthError> {
        Self::begin_attempt(
            account.recovery_context()?,
            account.homeserver(),
            provider,
            Some(account),
        )
        .await
    }

    async fn begin_in(
        context: AuthContext,
        homeserver: String,
        provider: Option<String>,
    ) -> Result<Self, AuthError> {
        Self::begin_attempt(context, homeserver, provider, None).await
    }

    async fn begin_attempt(
        context: AuthContext,
        homeserver: String,
        provider: Option<String>,
        recovery: Option<Account>,
    ) -> Result<Self, AuthError> {
        let homeserver = super::validate_homeserver(&homeserver).map_err(|_| AuthError::Login)?;
        if recovery.is_none()
            && let Some(value) = context
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
            remove_store(&context.store_path(previous.store_id)).await?;
        }
        let mut listener = Loopback::bind().await?;
        listener.kind = CallbackKind::Sso;
        let state = Uuid::new_v4().to_string();
        let mut redirect = listener.redirect.clone();
        redirect.query_pairs_mut().append_pair("state", &state);
        let record = if let Some(account) = &recovery {
            account.recovery_record().await?
        } else {
            SavedSession {
                homeserver: homeserver.to_string(),
                generation: Uuid::new_v4(),
                store_id: Uuid::new_v4(),
                passphrase: Uuid::new_v4().to_string(),
                session: None,
                oauth: None,
            }
        };
        if recovery.is_none() {
            context.save(&record).await?;
        }
        let (client, path) = client(&context, &record).await?;
        let url = client
            .matrix_auth()
            .get_sso_login_url(redirect.as_str(), provider.as_deref())
            .await
            .ok()
            .and_then(|value| Url::parse(&value).ok());
        match url {
            Some(url) => Ok(Self {
                client,
                context,
                record,
                listener,
                state,
                url,
                recovery,
            }),
            None => {
                drop(client);
                if recovery.is_none() {
                    let _ = context.session_store.delete().await;
                    let _ = remove_store(&path).await;
                }
                Err(AuthError::Login)
            }
        }
    }

    pub fn browser_url(&self) -> &str {
        self.url.as_str()
    }

    pub async fn finish(mut self, cancel: watch::Receiver<bool>) -> Result<Account, AuthError> {
        let path = self.context.store_path(self.record.store_id);
        let result = async {
            let callback = self.listener.receive(&self.state, cancel).await?;
            let mut login = self
                .client
                .matrix_auth()
                .login_with_sso_callback(callback.into())
                .map_err(|_| AuthError::Callback)?;
            if let Some(account) = &self.recovery {
                login = login.device_id(
                    account
                        .client
                        .device_id()
                        .ok_or(AuthError::Restore)?
                        .as_str(),
                );
            }
            login
                .initial_device_display_name("Matrix Client desktop")
                .await
                .map_err(|_| AuthError::Login)?;
            self.record
                .update(self.client.session().ok_or(AuthError::Login)?)?;
            if let Some(account) = &self.recovery {
                if self.client.user_id() != account.client.user_id()
                    || self.client.device_id() != account.client.device_id()
                {
                    return Err(AuthError::Restore);
                }
                account.persist_reauthentication(&mut self.record)?;
            } else {
                self.context.save(&self.record).await?;
            }
            let gate = install_session_callbacks(
                &self.client,
                self.context.session_store.clone(),
                self.record.store_id,
            )?;
            Ok(Account::new(
                self.client.clone(),
                path.clone(),
                self.context.session_store.clone(),
                gate,
            )
            .with_certificate(self.context.root_certificate.clone()))
        }
        .await;
        if result.is_err() {
            if self.client.session().is_some() {
                let _ = self.client.logout().await;
            }
            drop(self.client);
            if self.recovery.is_none() {
                let _ = self.context.session_store.delete().await;
                let _ = remove_store(&path).await;
            }
        }
        result
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use matrix_client_platform::SessionStore;
    use matrix_sdk::{
        reqwest::Certificate, ruma::api::client::session::get_login_types::v3::LoginType,
    };
    use std::{
        io::Write,
        path::Path,
        process::{Command, Stdio},
    };

    #[tokio::test]
    #[ignore = "requires isolated legacy Synapse + MAS OIDC provider and unlocked Keychain"]
    async fn sso_local_oidc_token_exchange_and_restore() {
        let scope = Uuid::new_v4();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/sso");
        let context = AuthContext {
            session_store: SessionStore::new(&format!("matrix-client.integration.sso.{scope}")),
            data_dir: root.join(format!("lifecycle-{scope}")),
            root_certificate: Some(
                Certificate::from_pem(
                    &tokio::fs::read(root.join("ca.crt")).await.expect("test CA"),
                )
                .expect("CA format"),
            ),
        };
        let pending = SsoLogin::begin_in(
            context,
            "https://127.0.0.1:20448".into(),
            Some("oidc-fixture".into()),
        )
        .await
        .expect("SSO login URL");
        let flows = pending
            .client
            .matrix_auth()
            .get_login_types()
            .await
            .expect("SSO discovery");
        assert!(flows.flows.iter().any(|flow| matches!(flow, LoginType::Sso(sso) if sso.identity_providers.iter().any(|provider| provider.id == "oidc-fixture" && provider.name == "Local OIDC"))));
        let input = zeroize::Zeroizing::new(serde_json::to_vec(&serde_json::json!({
            "url": pending.browser_url(), "sso": true, "username": format!("sso{}", &scope.simple().to_string()[..16]), "password": Uuid::new_v4().to_string()
        })).expect("browser input"));
        let (cancel, receiver) = watch::channel(false);
        let browser = tokio::task::spawn_blocking(move || {
            let mut child = Command::new("python3")
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../tests/integration/oauth-browser.py"),
                )
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .expect("fixture browser");
            child
                .stdin
                .take()
                .expect("browser stdin")
                .write_all(&input)
                .expect("browser input");
            let status = child.wait().expect("browser exit");
            if !status.success() {
                cancel.send_replace(true);
            }
            (status, cancel)
        });
        let account = pending
            .finish(receiver)
            .await
            .expect("SSO callback/loginToken exchange");
        assert!(browser.await.expect("browser task").0.success());
        assert!(account.user_id().ends_with(":sso.matrixclient.test"));
        account.client.whoami().await.expect("SSO session accepted");
        let store = account.session_store.clone();
        let path = account.store_path.clone();
        let device = account.client.device_id().expect("device").to_string();
        drop(account);
        let context = AuthContext {
            session_store: store,
            data_dir: root.join(format!("lifecycle-{scope}")),
            root_certificate: Some(
                Certificate::from_pem(
                    &tokio::fs::read(root.join("ca.crt")).await.expect("test CA"),
                )
                .expect("CA format"),
            ),
        };
        let restored = super::super::restore_in(&context)
            .await
            .expect("SSO restore")
            .expect("saved session");
        assert_eq!(
            restored.client.device_id().expect("device").as_str(),
            device
        );
        restored.logout().await.expect("SSO logout");
        assert!(
            context
                .session_store
                .load()
                .await
                .expect("Keychain load")
                .is_none()
        );
        assert!(!path.exists());
    }
}
