//! Paginated attachment metadata. Sources and cursors are opaque and never logged.
use crate::{auth::Account, conversation::ImageError};
use matrix_sdk::{
    media::{MediaFormat, MediaRequestParameters},
    room::MessagesOptions,
    ruma::{
        RoomId,
        events::{
            AnySyncMessageLikeEvent, AnySyncTimelineEvent,
            room::{MediaSource, message::MessageType},
        },
    },
};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum MediaKind {
    #[default]
    Images,
    Files,
}
#[derive(Clone)]
pub struct Token {
    account: u128,
    room: String,
    kind: MediaKind,
    position: String,
}
#[derive(Clone)]
pub struct Source(MediaSource);
#[derive(Clone)]
pub struct MediaItem {
    pub event_id: String,
    pub sender: String,
    pub sender_name: String,
    pub timestamp_ms: i64,
    pub filename: String,
    pub mimetype: Option<String>,
    pub size: Option<u64>,
    pub thumbnail_source: Option<Source>,
    pub source: Source,
    pub width: Option<u64>,
    pub height: Option<u64>,
}
pub struct MediaPage {
    pub items: Vec<MediaItem>,
    pub next: Option<Token>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MediaError {
    #[error("media_invalid_request")]
    InvalidRequest,
    #[error("media_unavailable")]
    Unavailable,
    #[error("media_pagination")]
    Pagination,
}
fn validate_chunk_size(requested: usize, actual: usize) -> Result<(), MediaError> {
    if actual > requested {
        Err(MediaError::Pagination)
    } else {
        Ok(())
    }
}
fn page_limit(limit: usize) -> Result<usize, MediaError> {
    if (1..=100).contains(&limit) {
        Ok(limit)
    } else {
        Err(MediaError::InvalidRequest)
    }
}
fn project(event: AnySyncTimelineEvent, kind: MediaKind) -> Option<MediaItem> {
    let AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(event)) = event
    else {
        return None;
    };
    let event = event.as_original()?;
    let (filename, source, mimetype, size, thumbnail, width, height) =
        match (&event.content.msgtype, kind) {
            (MessageType::Image(m), MediaKind::Images) => {
                let i = m.info.as_deref();
                (
                    m.filename(),
                    m.source.clone(),
                    i.and_then(|i| i.mimetype.clone()),
                    i.and_then(|i| i.size),
                    i.and_then(|i| i.thumbnail_source.clone()),
                    i.and_then(|i| i.width),
                    i.and_then(|i| i.height),
                )
            }
            (MessageType::File(m), MediaKind::Files) => {
                let i = m.info.as_deref();
                (
                    m.filename(),
                    m.source.clone(),
                    i.and_then(|i| i.mimetype.clone()),
                    i.and_then(|i| i.size),
                    i.and_then(|i| i.thumbnail_source.clone()),
                    None,
                    None,
                )
            }
            (MessageType::Video(m), MediaKind::Files) => {
                let i = m.info.as_deref();
                (
                    m.filename(),
                    m.source.clone(),
                    i.and_then(|i| i.mimetype.clone()),
                    i.and_then(|i| i.size),
                    i.and_then(|i| i.thumbnail_source.clone()),
                    i.and_then(|i| i.width),
                    i.and_then(|i| i.height),
                )
            }
            (MessageType::Audio(m), MediaKind::Files) => {
                let i = m.info.as_deref();
                (
                    m.filename(),
                    m.source.clone(),
                    i.and_then(|i| i.mimetype.clone()),
                    i.and_then(|i| i.size),
                    None,
                    None,
                    None,
                )
            }
            _ => return None,
        };
    Some(MediaItem {
        event_id: event.event_id.to_string(),
        sender: event.sender.to_string(),
        sender_name: event.sender.to_string(),
        timestamp_ms: u64::from(event.origin_server_ts.0) as i64,
        filename: filename.to_owned(),
        mimetype,
        size: size.map(u64::from),
        thumbnail_source: thumbnail.map(Source),
        source: Source(source),
        width: width.map(u64::from),
        height: height.map(u64::from),
    })
}
impl Account {
    /// Newest first; sparse pages may be empty with a continuation. At most four
    /// SDK requests per page, at most `limit` events per request, no hidden buffer.
    pub async fn room_media(
        &self,
        room_id: &str,
        kind: MediaKind,
        from: Option<Token>,
        limit: usize,
    ) -> Result<MediaPage, MediaError> {
        let limit = page_limit(limit)?;
        if from
            .as_ref()
            .is_some_and(|t| t.account != self.instance_id() || t.room != room_id || t.kind != kind)
        {
            return Err(MediaError::InvalidRequest);
        }
        let id = RoomId::parse(room_id).map_err(|_| MediaError::InvalidRequest)?;
        let room = self.client.get_room(&id).ok_or(MediaError::Unavailable)?;
        let encrypted = room
            .latest_encryption_state()
            .await
            .map_err(|_| MediaError::Unavailable)?
            .is_encrypted();
        let mut position = from.map(|t| t.position);
        let mut items = Vec::new();
        for _ in 0..4 {
            let options = |filtered| {
                let mut options = MessagesOptions::backward();
                options.from = position.clone();
                options.limit = ((limit - items.len()) as u32).into();
                if filtered {
                    options.filter.types = Some(vec!["m.room.message".into()]);
                    // contains_url misses encrypted `file.url` sources forwarded
                    // into plain rooms; use the safe event-type filter instead.
                }
                options
            };
            // Encrypted event envelopes cannot be filtered by their media content.
            let page = match room.messages(options(!encrypted)).await {
                Ok(page) => page,
                Err(_) if !encrypted => room
                    .messages(options(false))
                    .await
                    .map_err(|_| MediaError::Pagination)?,
                Err(_) => return Err(MediaError::Pagination),
            };
            validate_chunk_size(limit - items.len(), page.chunk.len())?;
            for event in page.chunk {
                if let Ok(event) = event.raw().deserialize()
                    && let Some(mut item) = project(event, kind)
                {
                    if items
                        .iter()
                        .any(|i: &MediaItem| i.event_id == item.event_id)
                    {
                        continue;
                    }
                    if let Ok(user) = matrix_sdk::ruma::UserId::parse(&item.sender)
                        && let Ok(Some(member)) = room.get_member_no_sync(&user).await
                    {
                        item.sender_name = member.name().to_owned();
                    }
                    items.push(item);
                }
            }
            let end = page.end.filter(|end| Some(end) != position.as_ref());
            position = end;
            if position.is_none() || items.len() >= limit {
                break;
            }
        }
        Ok(MediaPage {
            items,
            next: position.map(|position| Token {
                account: self.instance_id(),
                room: room_id.to_owned(),
                kind,
                position,
            }),
        })
    }
    /// SDK decryption with its media disk cache explicitly disabled.
    pub async fn media_bytes(&self, source: &Source) -> Result<Vec<u8>, MediaError> {
        self.client
            .media()
            .get_media_content(
                &MediaRequestParameters {
                    source: source.0.clone(),
                    format: MediaFormat::File,
                },
                false,
            )
            .await
            .map_err(|_| MediaError::Unavailable)
    }
    pub async fn gallery_image(
        &self,
        item: &MediaItem,
        mut cancel: tokio::sync::watch::Receiver<bool>,
    ) -> Result<crate::conversation::ViewerImage, ImageError> {
        if *cancel.borrow() {
            return Err(ImageError::Cancelled);
        }
        if item.size.is_some_and(|s| s > 64 * 1024 * 1024) {
            return Err(ImageError::Limit);
        }
        let bytes = tokio::select! {
            _ = cancel.changed() => return Err(ImageError::Cancelled),
            result = tokio::time::timeout(std::time::Duration::from_secs(30), self.media_bytes(&item.source)) => result.map_err(|_| ImageError::Download)?.map_err(|_| ImageError::Download)?,
        };
        crate::conversation::decode_viewer_bytes(bytes, item.filename.clone(), cancel).await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_server_pages_are_rejected_before_projection() {
        assert_eq!(validate_chunk_size(1, 2), Err(MediaError::Pagination));
        assert_eq!(validate_chunk_size(50, 50), Ok(()));
        assert_eq!(validate_chunk_size(50, 0), Ok(()));
    }
    #[test]
    fn page_bounds_are_explicit() {
        for limit in [0, 101, usize::MAX] {
            assert_eq!(page_limit(limit), Err(MediaError::InvalidRequest));
        }
        for limit in [1, 50, 100] {
            assert_eq!(page_limit(limit), Ok(limit));
        }
    }
    #[test]
    fn media_projection_preserves_event_identity_and_encrypted_sources() {
        let value = serde_json::json!({"type":"m.room.message", "event_id":"$image", "sender":"@a:example.org", "origin_server_ts":123, "content":{"msgtype":"m.image", "body":"image.png", "file":{"url":"mxc://example.org/image", "key":{"kty":"oct","key_ops":["encrypt","decrypt"],"alg":"A256CTR","k":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","ext":true},"iv":"AAAAAAAAAAAAAAAAAAAAAA","hashes":{"sha256":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"},"v":"v2"}}});
        let event = serde_json::from_value(value.clone()).unwrap();
        let item = project(event, MediaKind::Images).unwrap();
        assert_eq!(item.event_id, "$image");
        assert!(matches!(item.source.0, MediaSource::Encrypted(_)));
        assert!(project(serde_json::from_value(value).unwrap(), MediaKind::Files).is_none());
    }
}
