//! Matrix integration. UI code must not own protocol state or credentials.
#![recursion_limit = "256"]

pub mod auth;
pub mod conversation;
pub mod profile;
pub mod rooms;
pub mod spaces;

use matrix_sdk::Client;
use std::time::Duration;
use url::Url;

#[derive(Debug, Clone)]
pub struct LoginDiscovery {
    pub homeserver: String,
    pub legacy_flows: Option<Vec<String>>,
    pub oauth_metadata_available: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("Enter a valid HTTPS homeserver URL, for example https://matrix.org")]
    InvalidHomeserver,
    #[error("Could not connect to this homeserver. Check its URL and network connection.")]
    Connection,
}

/// Require HTTPS and reject credentials, fragments and query parameters.
pub fn validate_homeserver(input: &str) -> Result<Url, DiscoveryError> {
    let url = Url::parse(input.trim()).map_err(|_| DiscoveryError::InvalidHomeserver)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(DiscoveryError::InvalidHomeserver);
    }
    Ok(url)
}

/// Anonymous discovery only: no credentials are collected or stored.
pub async fn discover_login(input: &str) -> Result<LoginDiscovery, DiscoveryError> {
    let homeserver = validate_homeserver(input)?;
    tokio::time::timeout(Duration::from_secs(30), discover(homeserver))
        .await
        .map_err(|_| DiscoveryError::Connection)?
}

async fn discover(homeserver: Url) -> Result<LoginDiscovery, DiscoveryError> {
    let client = Client::builder()
        .homeserver_url(homeserver.as_str())
        .build()
        .await
        .map_err(|_| DiscoveryError::Connection)?;
    let matrix_auth = client.matrix_auth();
    let oauth = client.oauth();
    let (legacy, metadata) = tokio::join!(matrix_auth.get_login_types(), oauth.server_metadata());
    if legacy.is_err() && metadata.is_err() {
        return Err(DiscoveryError::Connection);
    }
    let legacy_flows = legacy.ok().map(|response| {
        response
            .flows
            .iter()
            .map(|flow| flow.login_type().to_owned())
            .collect()
    });
    // Failure means unconfirmed, not proof that OAuth is unsupported.
    let oauth_metadata_available = metadata.is_ok();
    Ok(LoginDiscovery {
        homeserver: homeserver.to_string(),
        legacy_flows,
        oauth_metadata_available,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homeserver_rejects_insecure_or_credential_bearing_urls() {
        for input in [
            "http://matrix.org",
            "https://user:password@matrix.org",
            "https://matrix.org?token=secret",
            "https://matrix.org/#secret",
            "https://matrix.org/path",
            "matrix.org",
        ] {
            assert!(validate_homeserver(input).is_err());
        }
        assert_eq!(
            validate_homeserver(" https://matrix.org ")
                .unwrap()
                .as_str(),
            "https://matrix.org/"
        );
        assert!(validate_homeserver("https://matrix.example.org:8448").is_ok());
    }
}
