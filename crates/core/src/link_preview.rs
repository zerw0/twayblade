//! Link previews through the homeserver's URL preview endpoint. The
//! homeserver fetches the page, so the client never contacts the linked site;
//! the frontend decides when asking is acceptable (the URL is revealed to the
//! homeserver, which matters in encrypted rooms).
use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use crate::auth::Account;

/// Bounded so scrolling through a link-heavy room cannot flood the homeserver.
static FETCHES: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(3)));
const TITLE_CHARS: usize = 200;
const DESCRIPTION_CHARS: usize = 400;
/// Image thumbnail edge in physical pixels (shown at half that).
const IMAGE_SIZE: u32 = 160;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkPreview {
    pub url: String,
    pub site_name: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    /// The page's `og:image`, fitted into a small PNG.
    pub image_png: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LinkPreviewError {
    #[error("link_preview_invalid_url")]
    InvalidUrl,
    #[error("link_preview_unavailable")]
    Unavailable,
    /// The page has nothing worth showing (no title or description).
    #[error("link_preview_empty")]
    Empty,
}

impl Account {
    /// Asks the homeserver for an Open Graph preview of `url` (http/https only).
    pub async fn link_preview(&self, url: String) -> Result<LinkPreview, LinkPreviewError> {
        let parsed = url::Url::parse(&url).map_err(|_| LinkPreviewError::InvalidUrl)?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err(LinkPreviewError::InvalidUrl);
        }
        let _permit = FETCHES
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| LinkPreviewError::Unavailable)?;
        let raw = tokio::time::timeout(
            Duration::from_secs(20),
            self.client.media().get_media_preview(&url, None),
        )
        .await
        .map_err(|_| LinkPreviewError::Unavailable)?
        .map_err(|_| LinkPreviewError::Unavailable)?
        .ok_or(LinkPreviewError::Empty)?;
        let mut preview = parse_open_graph(&url, raw.get())?;
        if let Some(mxc) = image_uri(raw.get()) {
            preview.image_png = self.sticker_thumbnail(mxc, IMAGE_SIZE).await.ok();
        }
        Ok(preview)
    }
}

fn og_text(json: &serde_json::Value, key: &str, limit: usize) -> Option<String> {
    let text = json.get(key)?.as_str()?;
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    Some(match text.char_indices().nth(limit) {
        Some((end, _)) => format!("{}…", text[..end].trim_end()),
        None => text,
    })
}

fn parse_open_graph(url: &str, raw: &str) -> Result<LinkPreview, LinkPreviewError> {
    let json: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| LinkPreviewError::Unavailable)?;
    let preview = LinkPreview {
        url: url.to_owned(),
        site_name: og_text(&json, "og:site_name", TITLE_CHARS),
        title: og_text(&json, "og:title", TITLE_CHARS),
        description: og_text(&json, "og:description", DESCRIPTION_CHARS),
        image_png: None,
    };
    if preview.title.is_none() && preview.description.is_none() {
        return Err(LinkPreviewError::Empty);
    }
    Ok(preview)
}

/// The preview image, only as an `mxc://` URI the homeserver already stored.
fn image_uri(raw: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(raw).ok()?;
    let image = json.get("og:image")?.as_str()?;
    image.starts_with("mxc://").then(|| image.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_graph_fields_are_trimmed_and_bounded() {
        let long = "word ".repeat(200);
        let raw = serde_json::json!({
            "og:title": "  The   Title \n",
            "og:description": long,
            "og:site_name": "Example",
            "og:image": "mxc://example.org/abc",
            "matrix:image:size": 1234,
        })
        .to_string();
        let preview = parse_open_graph("https://example.org/", &raw).unwrap();
        assert_eq!(preview.title.as_deref(), Some("The Title"));
        assert_eq!(preview.site_name.as_deref(), Some("Example"));
        let description = preview.description.unwrap();
        assert!(description.ends_with('…'));
        assert!(description.chars().count() <= DESCRIPTION_CHARS + 1);
        assert_eq!(image_uri(&raw).as_deref(), Some("mxc://example.org/abc"));
    }

    #[test]
    fn pages_without_text_and_web_images_are_not_shown() {
        let raw = serde_json::json!({ "og:image": "https://example.org/a.png" }).to_string();
        assert_eq!(
            parse_open_graph("https://example.org/", &raw),
            Err(LinkPreviewError::Empty)
        );
        assert_eq!(image_uri(&raw), None);
    }
}
