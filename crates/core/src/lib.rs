//! Matrix integration. UI code must not own protocol state or credentials.
#![recursion_limit = "256"]

pub mod auth;
pub mod avatars;
pub mod conversation;
pub mod media;
pub mod notifications;
pub mod presence;
pub mod profile;
pub mod room_state;
pub mod rooms;
pub mod search;
pub mod security;
pub mod space_actions;
pub mod spaces;

use matrix_sdk::Client;
use std::time::Duration;
use url::Url;

#[derive(Debug, Clone)]
pub struct LoginDiscovery {
    pub homeserver: String,
    pub legacy_flows: Option<Vec<String>>,
    pub oauth_metadata_available: bool,
    pub sso: bool,
    pub identity_providers: Vec<IdentityProvider>,
}

#[derive(Debug, Clone)]
pub struct IdentityProvider {
    pub id: String,
    pub name: String,
    /// Bounded, decoded and re-encoded PNG; never a remote UI image URL.
    pub icon_png: Option<Vec<u8>>,
}

#[allow(deprecated)] // Login icons are fetched before an access token exists.
async fn provider_icon(client: &Client, uri: &matrix_sdk::ruma::MxcUri) -> Option<Vec<u8>> {
    use matrix_sdk::ruma::api::client::media::get_content_thumbnail::v3::Request;
    let mut request = Request::from_url(uri, 32u32.into(), 32u32.into()).ok()?;
    request.allow_remote = true;
    let bytes = tokio::time::timeout(Duration::from_secs(3), client.send(request))
        .await
        .ok()?
        .ok()?
        .file;
    bounded_provider_icon(bytes)
}

fn bounded_provider_icon(bytes: Vec<u8>) -> Option<Vec<u8>> {
    if bytes.len() > 512 * 1024 {
        return None;
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(256);
    limits.max_image_height = Some(256);
    limits.max_alloc = Some(4 * 1024 * 1024);
    reader.limits(limits);
    let icon = reader.decode().ok()?.thumbnail(32, 32);
    let mut output = std::io::Cursor::new(Vec::new());
    icon.write_to(&mut output, image::ImageFormat::Png).ok()?;
    Some(output.into_inner())
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
    let mut sso = false;
    let mut identity_providers = Vec::new();
    if let Ok(response) = &legacy {
        for flow in &response.flows {
            if let matrix_sdk::ruma::api::client::session::get_login_types::v3::LoginType::Sso(
                flow,
            ) = flow
            {
                sso = true;
                for provider in flow.identity_providers.iter().take(16) {
                    let icon_png = match &provider.icon {
                        Some(uri) => provider_icon(&client, uri).await,
                        None => None,
                    };
                    identity_providers.push(IdentityProvider {
                        id: provider.id.clone(),
                        name: provider.name.clone(),
                        icon_png,
                    });
                }
            }
        }
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
        sso,
        identity_providers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_icons_reject_bombs_and_decode_only_bounded_images() {
        fn png(width: u32, height: u32) -> Vec<u8> {
            let image = image::DynamicImage::new_rgba8(width, height);
            let mut output = std::io::Cursor::new(Vec::new());
            image
                .write_to(&mut output, image::ImageFormat::Png)
                .unwrap();
            output.into_inner()
        }
        assert!(bounded_provider_icon(vec![0; 512 * 1024 + 1]).is_none());
        assert!(bounded_provider_icon(b"invalid image".to_vec()).is_none());
        assert!(bounded_provider_icon(png(512, 512)).is_none());
        let safe = bounded_provider_icon(png(64, 64)).unwrap();
        let safe = image::load_from_memory(&safe).unwrap();
        assert_eq!((safe.width(), safe.height()), (32, 32));
    }

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
