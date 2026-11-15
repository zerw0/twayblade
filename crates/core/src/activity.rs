//! Data for the notifications popover (#99): the latest message of the rooms it
//! lists, and marking those rooms read. Never format previews for logging.
use crate::auth::Account;
use crate::rooms::RoomError;
use matrix_sdk::ruma::{OwnedEventId, RoomId, events::room::message::MessageType};
use matrix_sdk_ui::timeline::{
    LatestEventValue, MsgLikeKind, RoomExt, TimelineDetails, TimelineItemContent,
};

/// What the latest message is; the frontend words the non-text kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewBody {
    Text(String),
    Image,
    Video,
    Audio,
    File,
    Location,
    Sticker,
    Poll,
    Encrypted,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomPreview {
    pub room: String,
    pub sender: String,
    pub sender_name: String,
    pub own: bool,
    pub body: PreviewBody,
    pub timestamp_ms: u64,
}

/// Longest preview text kept; the popover shows one or two lines.
const PREVIEW_CHARS: usize = 160;

fn text(body: &str) -> PreviewBody {
    // One line: the first non-empty line, trimmed.
    let line = body
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    PreviewBody::Text(line.chars().take(PREVIEW_CHARS).collect())
}

/// Maps an SDK timeline item to a preview, or None for content the popover
/// does not summarise (state changes, call events and the like).
pub(crate) fn preview_body(content: &TimelineItemContent) -> Option<PreviewBody> {
    let message = match content {
        TimelineItemContent::MsgLike(message) => message,
        _ => return None,
    };
    Some(match &message.kind {
        MsgLikeKind::Message(message) => match message.msgtype() {
            MessageType::Image(_) => PreviewBody::Image,
            MessageType::Video(_) => PreviewBody::Video,
            MessageType::Audio(_) => PreviewBody::Audio,
            MessageType::File(_) => PreviewBody::File,
            MessageType::Location(_) => PreviewBody::Location,
            _ => text(message.body()),
        },
        MsgLikeKind::Sticker(_) => PreviewBody::Sticker,
        MsgLikeKind::Poll(_) => PreviewBody::Poll,
        MsgLikeKind::UnableToDecrypt(_) => PreviewBody::Encrypted,
        MsgLikeKind::Redacted => PreviewBody::Deleted,
        MsgLikeKind::LiveLocation(_) => PreviewBody::Location,
        MsgLikeKind::Other(_) => return None,
    })
}

fn room(account: &Account, id: &str) -> Option<matrix_sdk::Room> {
    RoomId::parse(id)
        .ok()
        .and_then(|id| account.client.get_room(&id))
}

impl Account {
    /// The latest message of each listed room the SDK has one for. The first
    /// call for a room starts tracking it, so its preview may only appear on
    /// a later call.
    pub async fn room_previews(&self, rooms: &[String]) -> Vec<RoomPreview> {
        let latest = self.client.latest_events().await;
        let mut previews = Vec::new();
        for id in rooms {
            let Some(room) = room(self, id) else {
                continue;
            };
            let _ = latest.listen_to_room(room.room_id()).await;
            let LatestEventValue::Remote {
                timestamp,
                sender,
                is_own,
                profile,
                content,
            } = room.latest_event().await
            else {
                continue;
            };
            let Some(body) = preview_body(&content) else {
                continue;
            };
            let sender_name = match profile {
                TimelineDetails::Ready(profile) => profile.display_name,
                _ => None,
            }
            .unwrap_or_else(|| sender.to_string());
            previews.push(RoomPreview {
                room: id.clone(),
                sender: sender.to_string(),
                sender_name,
                own: is_own,
                body,
                timestamp_ms: u64::from(timestamp.0),
            });
        }
        previews
    }

    /// Marks every listed room read up to its latest event (read marker and
    /// public receipt, unthreaded, so thread unread clears too) and drops a
    /// manual unread mark. Goes on past a failing room; Err if any failed.
    pub async fn mark_rooms_read(&self, rooms: &[String]) -> Result<(), RoomError> {
        let mut failed = false;
        for id in rooms {
            let Some(room) = room(self, id) else {
                failed = true;
                continue;
            };
            let Some(event) = latest_event_id(&room).await else {
                failed = true;
                continue;
            };
            let receipts = matrix_sdk::room::Receipts::new()
                .fully_read_marker(event.clone())
                .public_read_receipt(event);
            if room.send_multiple_receipts(receipts).await.is_err() {
                failed = true;
                continue;
            }
            if room.is_marked_unread() && room.set_unread_flag(false).await.is_err() {
                failed = true;
            }
        }
        if failed {
            Err(RoomError::Receipt)
        } else {
            Ok(())
        }
    }
}

/// The room's latest event: the SDK's latest-event value, else one event
/// fetched from the server.
async fn latest_event_id(room: &matrix_sdk::Room) -> Option<OwnedEventId> {
    if let matrix_sdk::latest_events::LatestEventValue::Remote(event) = (**room).latest_event()
        && let Some(id) = event.event_id()
    {
        return Some(id.to_owned());
    }
    let mut options = matrix_sdk::room::MessagesOptions::backward();
    options.limit = 1u32.into();
    room.messages(options)
        .await
        .ok()?
        .chunk
        .into_iter()
        .find_map(|event| event.event_id().map(ToOwned::to_owned))
}

#[cfg(test)]
mod tests {
    use super::{PREVIEW_CHARS, PreviewBody, text};

    #[test]
    fn previews_keep_one_bounded_line() {
        assert_eq!(
            text("\n  first line  \nsecond"),
            PreviewBody::Text("first line".into())
        );
        assert_eq!(text(""), PreviewBody::Text(String::new()));
        let long = "é".repeat(PREVIEW_CHARS + 20);
        let PreviewBody::Text(kept) = text(&long) else {
            unreachable!()
        };
        assert_eq!(kept.chars().count(), PREVIEW_CHARS);
    }
}
