//! SDK-owned OAuth authorization code/PKCE, with one local browser callback.
use super::{
    Account, AuthContext, AuthError, SavedSession, client, install_session_callbacks,
    loopback::Loopback, remove_store,
};
use matrix_sdk::{
    Client,
    authentication::oauth::{
        CsrfToken,
        registration::{ApplicationType, ClientMetadata, Localized, OAuthGrantType},
    },
    ruma::serde::Raw,
};
use tokio::sync::watch;
use url::Url;
use uuid::Uuid;

// No Debug: the authorization URL and eventual callback are sensitive.
pub struct OAuthLogin {
    client: Client,
    context: AuthContext,
    record: SavedSession,
    listener: Loopback,
    state: CsrfToken,
    url: Url,
    recovery: Option<Account>,
}

impl OAuthLogin {
    pub async fn begin(homeserver: String) -> Result<Self, AuthError> {
        Self::begin_in(AuthContext::application()?, homeserver).await
    }

    pub async fn reauthenticate(account: Account) -> Result<Self, AuthError> {
        Self::begin_attempt(
            account.recovery_context()?,
            account.homeserver(),
            Some(account),
        )
        .await
    }

    async fn begin_in(context: AuthContext, homeserver: String) -> Result<Self, AuthError> {
        Self::begin_attempt(context, homeserver, None).await
    }

    async fn begin_attempt(
        context: AuthContext,
        homeserver: String,
        recovery: Option<Account>,
    ) -> Result<Self, AuthError> {
        let homeserver = super::validate_homeserver(&homeserver).map_err(|_| AuthError::OAuth)?;
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
        let listener = Loopback::bind().await?;
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
        if let Some(saved) = &record.oauth {
            client
                .oauth()
                .restore_registered_client(super::ClientId::new(saved.client_id.clone()));
        }
        let device_id = recovery
            .as_ref()
            .and_then(|account| account.client.device_id().map(ToOwned::to_owned));
        let mut metadata = ClientMetadata::new(
            ApplicationType::Native,
            vec![OAuthGrantType::AuthorizationCode {
                redirect_uris: vec![listener.redirect.clone()],
            }],
            Localized::new(homeserver, []),
        );
        metadata.client_name = Some(Localized::new("Twayblade desktop".to_owned(), []));
        let metadata = Raw::new(&metadata).map_err(|_| AuthError::OAuth)?;
        let result = client
            .oauth()
            .login(
                listener.redirect.clone(),
                device_id,
                Some(metadata.into()),
                None,
            )
            .build()
            .await;
        match result {
            Ok(data) => Ok(Self {
                client,
                context,
                record,
                listener,
                state: data.state,
                url: data.url,
                recovery,
            }),
            Err(_) => {
                drop(client);
                if recovery.is_none() {
                    let _ = context.session_store.delete().await;
                    let _ = remove_store(&path).await;
                }
                Err(AuthError::OAuth)
            }
        }
    }

    /// Pass only to the OS browser, never to diagnostics or persistent UI state.
    pub fn browser_url(&self) -> &str {
        self.url.as_str()
    }

    pub async fn finish(mut self, cancel: watch::Receiver<bool>) -> Result<Account, AuthError> {
        let path = self.context.store_path(self.record.store_id);
        let oauth = self.client.oauth();
        let result = async {
            let callback = self.listener.receive(self.state.secret(), cancel).await?;
            oauth
                .finish_login(callback.into())
                .await
                .map_err(|_| AuthError::OAuth)?;
            self.record
                .update(self.client.session().ok_or(AuthError::OAuth)?)?;
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
            oauth.abort_login(&self.state).await;
            if self.client.session().is_some() {
                let _ = self.client.logout().await;
            }
            drop(oauth);
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
    use matrix_sdk::reqwest::Certificate;
    use std::{
        io::Write,
        path::{Path, PathBuf},
        process::{Command, Stdio},
    };
    use twayblade_platform::SessionStore;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/local/oauth")
    }
    async fn context(scope: Uuid) -> AuthContext {
        AuthContext {
            session_store: SessionStore::new(&format!("twayblade.integration.oauth.{scope}")),
            data_dir: root().join(format!("lifecycle-{scope}")),
            root_certificate: Some(
                Certificate::from_pem(
                    &tokio::fs::read(root().join("ca.crt"))
                        .await
                        .expect("fixture CA"),
                )
                .expect("CA format"),
            ),
        }
    }

    #[test]
    #[ignore = "requires isolated MAS + Synapse and unlocked macOS Keychain"]
    fn oauth_keychain_lifecycle_across_processes() {
        let scope = Uuid::new_v4();
        for phase in ["login", "restore-refresh", "restore-logout"] {
            let status = Command::new(std::env::current_exe().expect("test executable"))
                .args([
                    "--exact",
                    "auth::oauth::tests::oauth_lifecycle_phase",
                    "--ignored",
                    "--nocapture",
                ])
                .env("MATRIX_OAUTH_SCOPE", scope.to_string())
                .env("MATRIX_OAUTH_PHASE", phase)
                .status()
                .expect("lifecycle subprocess");
            assert!(status.success(), "OAuth lifecycle phase failed");
        }
    }

    #[tokio::test]
    #[ignore = "internal isolated OAuth lifecycle subprocess"]
    async fn oauth_lifecycle_phase() {
        let Ok(scope) = std::env::var("MATRIX_OAUTH_SCOPE") else {
            return;
        };
        let scope = Uuid::parse_str(&scope).expect("test scope");
        let context = context(scope).await;
        if std::env::var("MATRIX_OAUTH_PHASE").as_deref() == Ok("login") {
            let pending = OAuthLogin::begin_in(context, "https://127.0.0.1:19448".into())
                .await
                .expect("OAuth discovery and registration");
            assert!(
                pending
                    .url
                    .query_pairs()
                    .any(|(key, value)| key == "code_challenge_method" && value == "S256")
            );
            let input = zeroize::Zeroizing::new(serde_json::to_vec(&serde_json::json!({
                "url": pending.browser_url(), "username": format!("oauth{}", &scope.simple().to_string()[..16]), "password": Uuid::new_v4().to_string()
            })).expect("browser input"));
            let (cancel, receiver) = watch::channel(false);
            let browser = tokio::task::spawn_blocking(move || {
                let script = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/integration/oauth-browser.py");
                let mut child = Command::new("python3")
                    .arg(script)
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
                let status = child.wait().expect("fixture browser exit");
                if !status.success() {
                    cancel.send_replace(true);
                }
                // Keep the cancellation sender alive through callback delivery.
                (status, cancel)
            });
            let account = pending
                .finish(receiver)
                .await
                .expect("OAuth callback and token exchange");
            assert!(browser.await.expect("browser task").0.success());
            assert!(account.user_id().ends_with(":oauth.twayblade.test"));
            let device = account.client.device_id().expect("device").to_string();
            tokio::fs::create_dir_all(&account.store_path)
                .await
                .expect("test directory");
            tokio::fs::write(root().join(format!("{scope}.device")), device)
                .await
                .expect("device evidence");
        } else {
            let account = super::super::restore_in(&context)
                .await
                .expect("restore OAuth session")
                .expect("saved session");
            assert_eq!(
                account.client.device_id().expect("device").as_str(),
                tokio::fs::read_to_string(root().join(format!("{scope}.device")))
                    .await
                    .expect("device evidence")
            );
            if std::env::var("MATRIX_OAUTH_PHASE").as_deref() == Ok("restore-logout") {
                account
                    .client
                    .refresh_access_token()
                    .await
                    .expect("rotated refresh token survived relaunch");
                account.logout().await.expect("OAuth logout");
                assert!(
                    context
                        .session_store
                        .load()
                        .await
                        .expect("Keychain load")
                        .is_none()
                );
                assert!(!account.store_path.exists());
                return;
            }
            let before = account.client.access_token().expect("access token");
            tokio::time::sleep(std::time::Duration::from_secs(6)).await;
            account
                .client
                .whoami()
                .await
                .expect("expired token automatically refreshed");
            assert!(
                before != account.client.access_token().expect("rotated token"),
                "access token rotated"
            );
            let value = context
                .session_store
                .load()
                .await
                .expect("Keychain load")
                .expect("session");
            let saved: SavedSession = serde_json::from_str(&value).expect("saved record");
            assert!(saved.session.is_none());
            let saved = saved.oauth.expect("OAuth record");
            assert!(!saved.client_id.is_empty());
            assert!(
                saved.user.tokens.access_token == account.client.access_token().expect("token"),
                "rotated tokens persisted synchronously"
            );
            account
                .client
                .whoami()
                .await
                .expect("refreshed session accepted by Synapse");
        }
    }
    #[tokio::test]
    #[ignore = "requires isolated MAS + Synapse and unlocked macOS Keychain"]
    async fn oauth_cancel_and_wrong_state_discard_checkpoint() {
        use tokio::io::AsyncWriteExt;
        for wrong_state in [false, true] {
            let scope = Uuid::new_v4();
            let pending =
                OAuthLogin::begin_in(context(scope).await, "https://127.0.0.1:19448".into())
                    .await
                    .expect("OAuth registration");
            let path = pending.context.store_path(pending.record.store_id);
            let redirect = pending.listener.redirect.clone();
            let (cancel, receiver) = watch::channel(false);
            let waiting = tokio::spawn(pending.finish(receiver));
            if wrong_state {
                let address = format!("127.0.0.1:{}", redirect.port().expect("callback port"));
                let mut socket = tokio::net::TcpStream::connect(&address)
                    .await
                    .expect("callback listener");
                socket
                    .write_all(
                        format!(
                            "GET /?code=sample&state=wrong HTTP/1.1\r\nHost: {address}\r\n\r\n"
                        )
                        .as_bytes(),
                    )
                    .await
                    .expect("wrong-state callback");
            } else {
                cancel.send_replace(true);
            }
            let error = waiting
                .await
                .expect("callback task")
                .expect_err("invalid authorization accepted");
            assert!(matches!(
                (wrong_state, error),
                (false, AuthError::Cancelled) | (true, AuthError::Callback)
            ));
            assert!(
                context(scope)
                    .await
                    .session_store
                    .load()
                    .await
                    .expect("Keychain load")
                    .is_none()
            );
            assert!(!path.exists());
            assert!(
                tokio::net::TcpStream::connect(format!(
                    "127.0.0.1:{}",
                    redirect.port().expect("port")
                ))
                .await
                .is_err()
            );
        }
    }
}
