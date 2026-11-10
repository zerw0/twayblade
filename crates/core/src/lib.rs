//! Matrix integration. UI code must not own protocol state or credentials.
#![recursion_limit = "256"]

pub mod auth;
pub mod conversation;
pub mod profile;
pub mod room_state;
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
    #[error("discovery_invalid_homeserver")]
    InvalidHomeserver,
    #[error("discovery_insecure_homeserver")]
    InsecureHomeserver,
    #[error("discovery_connection")]
    Connection,
}

/// Validate a resolved homeserver base URL: HTTPS only, no credentials,
/// query or fragment. Path prefixes are allowed by the client-server spec.
pub fn validate_homeserver(input: &str) -> Result<Url, DiscoveryError> {
    let url = Url::parse(input.trim()).map_err(|_| DiscoveryError::InvalidHomeserver)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(DiscoveryError::InvalidHomeserver);
    }
    Ok(url)
}

/// Accept what users actually type: a server name (`matrix.org`) or an HTTPS
/// URL. Server names are resolved through `.well-known/matrix/client`.
fn server_input(input: &str) -> Result<String, DiscoveryError> {
    let input = input.trim().trim_end_matches('/');
    if input.contains("://") {
        return validate_homeserver(input).map(String::from);
    }
    // `Url` gives us IDNA/port validation for the bare server name.
    let url =
        Url::parse(&format!("https://{input}")).map_err(|_| DiscoveryError::InvalidHomeserver)?;
    if input.is_empty() || url.path() != "/" || validate_homeserver(url.as_str()).is_err() {
        return Err(DiscoveryError::InvalidHomeserver);
    }
    Ok(input.to_owned())
}

/// Anonymous discovery only: no credentials are collected or stored.
pub async fn discover_login(input: &str) -> Result<LoginDiscovery, DiscoveryError> {
    let server = server_input(input)?;
    tokio::time::timeout(Duration::from_secs(30), discover(server))
        .await
        .map_err(|_| DiscoveryError::Connection)?
}

async fn discover(server: String) -> Result<LoginDiscovery, DiscoveryError> {
    let client = Client::builder()
        .server_name_or_homeserver_url(&server)
        .build()
        .await
        .map_err(|_| DiscoveryError::Connection)?;
    // `.well-known` may point anywhere; never accept a plain-HTTP delegation.
    let homeserver = validate_homeserver(client.homeserver().as_str())
        .map_err(|_| DiscoveryError::InsecureHomeserver)?;
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
    fn homeserver_url_rejects_insecure_or_credential_bearing_urls() {
        for input in [
            "http://matrix.org",
            "https://user:password@matrix.org",
            "https://matrix.org?token=secret",
            "https://matrix.org/#secret",
            "matrix.org",
        ] {
            assert!(validate_homeserver(input).is_err(), "{input}");
        }
        assert_eq!(
            validate_homeserver(" https://matrix.org ")
                .unwrap()
                .as_str(),
            "https://matrix.org/"
        );
        assert!(validate_homeserver("https://matrix.example.org:8448").is_ok());
        assert!(validate_homeserver("https://example.org/matrix/").is_ok());
    }

    #[test]
    fn server_input_accepts_server_names_and_https_urls_only() {
        assert_eq!(server_input(" matrix.org ").unwrap(), "matrix.org");
        assert_eq!(
            server_input("example.org:8448").unwrap(),
            "example.org:8448"
        );
        assert_eq!(
            server_input("https://matrix-client.matrix.org/").unwrap(),
            "https://matrix-client.matrix.org/"
        );
        for input in [
            "",
            "http://matrix.org",
            "matrix.org/path",
            "user@matrix.org",
            "matrix.org?x=1",
            "matrix.org#x",
        ] {
            assert!(server_input(input).is_err(), "{input}");
        }
    }
}
