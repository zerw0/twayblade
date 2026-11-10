//! SDK timeline projection and send queue; never log event content.

use crate::{
    auth::Account,
    room_state::{Member, MembersState, room_members},
};
use futures_util::{Stream, StreamExt};
use matrix_sdk::ruma::{
    RoomId, UserId,
    events::room::message::{MessageType, RoomMessageEventContent},
};
use matrix_sdk_ui::timeline::{
    EventSendState, MsgLikeContent, Profile, RoomExt, Timeline, TimelineDetails, TimelineFocus,
    TimelineItem, TimelineItemContent, TimelineReadReceiptTracking, VirtualTimelineItem,
};
use std::sync::Arc;

mod attachments;
mod viewer;
pub use attachments::{AttachmentDraft, AttachmentError, sanitize_filename};
pub use viewer::{ImageError, ViewerImage, decode_viewer_image};

/// What a timeline row represents. The frontend owns the wording.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageKind {
    #[default]
    Text,
    Emote,
    Notice,
    Image,
    File,
    Undecryptable,
    Redacted,
}

#[derive(Clone, Default)]
pub struct MessageRow {
    pub id: String,
    pub sender: String,
    pub display_name: String,
    pub timestamp_ms: i64,
    /// Plain text; never log this or reply excerpts.
    pub body: String,
    pub kind: MessageKind,
    pub edited: bool,
    pub delivery: DeliveryState,
    pub reply: Option<ReplyTarget>,
    pub reactions: Vec<Reaction>,
    pub thread_replies: u32,
    pub thread_unread: u64,
    pub read_by: Vec<String>,
    pub attachment: Option<Attachment>,
    pub upload_percent: Option<u8>,
    pub can_retry: bool,
    pub event_id: Option<String>,
    pub new_messages: bool,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum DeliveryState {
    #[default]
    Remote,
    Queued,
    Failed,
    Sent,
}

#[derive(Clone)]
pub enum ReplyTarget {
    Ready { sender: String, excerpt: String },
    Pending,
    Unavailable,
}

#[derive(Clone)]
pub struct Reaction {
    pub key: String,
    pub count: usize,
    pub mine: bool,
}

#[derive(Clone, Default)]
pub struct Attachment {
    pub preview_png: Option<Arc<Vec<u8>>>,
    pub filename: String,
    pub size: Option<u64>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum TimelineStatus {
    Loading,
    #[default]
    Ready,
    Unavailable,
    Failed,
    HistoryFailed,
}

#[derive(Debug, thiserror::Error)]
pub enum ConversationError {
    #[error("empty_message")]
    EmptyMessage,
    #[error("queue_failed")]
    Queue,
    #[error("pagination_failed")]
    Pagination,
    #[error("event_unavailable")]
    Unavailable,
    #[error("reaction_failed")]
    Reaction,
    #[error("retry_failed")]
    Retry,
    #[error("cancel_failed")]
    Cancel,
    #[error("receipt_failed")]
    Receipt,
}

#[derive(Clone)]
pub struct Conversation {
    timeline: Arc<Timeline>,
}

#[derive(Clone)]
pub struct ConversationSnapshot {
    pub thread_root: Option<String>,
    pub unread: u64,
    pub generation: u64,
    pub account_instance: u128,
    pub room_id: String,
    pub conversation: Option<Conversation>,
    pub messages: Vec<MessageRow>,
    pub status: TimelineStatus,
    pub members: Vec<Member>,
    pub members_state: MembersState,
    pub typing: Vec<String>,
}

impl Conversation {
    pub async fn send(&self, body: String) -> Result<(), ConversationError> {
        if body.trim().is_empty() {
            return Err(ConversationError::EmptyMessage);
        }
        self.timeline
            .send(RoomMessageEventContent::text_plain(body).into())
            .await
            .map(|_| ())
            .map_err(|_| ConversationError::Queue)
    }

    pub async fn older(&self) -> Result<bool, ConversationError> {
        self.timeline
            .paginate_backwards(30)
            .await
            .map_err(|_| ConversationError::Pagination)
    }

    pub async fn mark_read(&self, event_id: &str) -> Result<(), ConversationError> {
        let event_id = matrix_sdk::ruma::EventId::parse(event_id)
            .map_err(|_| ConversationError::Unavailable)?;
        if !self.timeline.is_threaded() {
            self.timeline
                .send_multiple_receipts(
                    matrix_sdk::room::Receipts::new().fully_read_marker(event_id.clone()),
                )
                .await
                .map_err(|_| ConversationError::Receipt)?;
        }
        // The SDK infers Main or the thread ID from this timeline's focus.
        // An unthreaded/fully-read marker here would clear closed-thread unread.
        self.timeline
            .send_single_receipt(
                matrix_sdk::ruma::api::client::receipt::create_receipt::v3::ReceiptType::Read,
                event_id,
            )
            .await
            .map(|_| ())
            .map_err(|_| ConversationError::Receipt)
    }

    pub async fn unread(&self, root: &str) -> Result<u64, ConversationError> {
        let root =
            matrix_sdk::ruma::EventId::parse(root).map_err(|_| ConversationError::Unavailable)?;
        let room = self.timeline.room();
        let (cache, _guard) = room
            .client()
            .event_cache()
            .thread(room.room_id(), &root)
            .await
            .map_err(|_| ConversationError::Unavailable)?;
        cache
            .num_unread_messages()
            .await
            .map_err(|_| ConversationError::Unavailable)
    }

    pub async fn toggle_reaction(&self, id: &str, key: &str) -> Result<(), ConversationError> {
        let items = self.timeline.items().await;
        let event = items
            .iter()
            .find(|item| item.unique_id().0 == id)
            .and_then(|item| item.as_event())
            .ok_or(ConversationError::Unavailable)?;
        self.timeline
            .toggle_reaction(&event.identifier(), key)
            .await
            .map(|_| ())
            .map_err(|_| ConversationError::Reaction)
    }

    async fn failed_handle(
        &self,
        id: &str,
    ) -> Result<matrix_sdk::send_queue::SendHandle, ConversationError> {
        let items = self.timeline.items().await;
        let event = items
            .iter()
            .find(|item| item.unique_id().0 == id)
            .and_then(|item| item.as_event())
            .ok_or(ConversationError::Unavailable)?;
        if !matches!(
            event.send_state(),
            Some(EventSendState::SendingFailed { .. })
        ) {
            return Err(ConversationError::Unavailable);
        }
        event
            .local_echo_send_handle()
            .ok_or(ConversationError::Unavailable)
    }

    pub async fn retry(&self, id: &str) -> Result<(), ConversationError> {
        self.failed_handle(id)
            .await?
            .unwedge()
            .await
            .map_err(|_| ConversationError::Retry)?;
        // Recoverable errors disable this room's SDK queue; retry must resume it.
        self.timeline.room().send_queue().set_enabled(true);
        Ok(())
    }

    pub async fn cancel(&self, id: &str) -> Result<(), ConversationError> {
        match self.failed_handle(id).await?.abort().await {
            Ok(true) => Ok(()),
            Ok(false) => Err(ConversationError::Unavailable),
            Err(_) => Err(ConversationError::Cancel),
        }
    }
}

fn sender_name(profile: &TimelineDetails<Profile>, sender: &UserId) -> String {
    match profile {
        TimelineDetails::Ready(profile) => profile.display_name.as_ref().map(|name| {
            if profile.display_name_ambiguous {
                format!("{name} ({sender})")
            } else {
                name.clone()
            }
        }),
        _ => None,
    }
    .unwrap_or_else(|| sender.to_string())
}

fn plain_excerpt(body: &str) -> String {
    let mut chars = body
        .split_whitespace()
        .enumerate()
        .flat_map(|(ix, word)| (ix > 0).then_some(' ').into_iter().chain(word.chars()));
    let mut excerpt: String = chars.by_ref().take(120).collect();
    if chars.next().is_some() {
        excerpt.push('…');
    }
    excerpt
}

fn attachment(filename: &str, size: Option<matrix_sdk::ruma::UInt>) -> Option<Attachment> {
    Some(Attachment {
        preview_png: None,
        filename: filename.to_owned(),
        size: size.map(u64::from),
    })
}

fn message_content(msgtype: &MessageType) -> (MessageKind, String, Option<Attachment>) {
    match msgtype {
        MessageType::Emote(message) => (MessageKind::Emote, message.body.clone(), None),
        MessageType::Notice(message) => (MessageKind::Notice, message.body.clone(), None),
        MessageType::Image(message) => (
            MessageKind::Image,
            String::new(),
            attachment(
                message.filename(),
                message.info.as_ref().and_then(|i| i.size),
            ),
        ),
        MessageType::File(message) => (
            MessageKind::File,
            String::new(),
            attachment(
                message.filename(),
                message.info.as_ref().and_then(|i| i.size),
            ),
        ),
        // Until inline playback exists (post-1.0), audio and video are files to save,
        // not text: their `body` is only a filename or caption.
        MessageType::Audio(message) => (
            MessageKind::File,
            String::new(),
            attachment(
                message.filename(),
                message.info.as_ref().and_then(|i| i.size),
            ),
        ),
        MessageType::Video(message) => (
            MessageKind::File,
            String::new(),
            attachment(
                message.filename(),
                message.info.as_ref().and_then(|i| i.size),
            ),
        ),
        _ => (MessageKind::Text, msgtype.body().to_owned(), None),
    }
}

fn reply_target(content: &MsgLikeContent) -> Option<ReplyTarget> {
    content
        .in_reply_to
        .as_ref()
        .map(|reply| match &reply.event {
            TimelineDetails::Ready(event) => ReplyTarget::Ready {
                sender: sender_name(&event.sender_profile, &event.sender),
                excerpt: event
                    .content
                    .as_message()
                    .map(|message| plain_excerpt(message.body()))
                    .unwrap_or_default(),
            },
            TimelineDetails::Error(_) => ReplyTarget::Unavailable,
            _ => ReplyTarget::Pending,
        })
}

fn reactions(content: &MsgLikeContent, own_user: &UserId) -> Vec<Reaction> {
    content
        .reactions
        .iter()
        .filter(|(_, senders)| !senders.is_empty())
        .map(|(key, senders)| Reaction {
            key: key.clone(),
            count: senders.len(),
            mine: senders.contains_key(own_user),
        })
        .collect()
}

fn thread_reply_count(content: &MsgLikeContent) -> u32 {
    content
        .thread_summary
        .as_ref()
        .map_or(0, |summary| summary.num_replies)
}

fn delivery(state: Option<&EventSendState>) -> DeliveryState {
    match state {
        Some(EventSendState::NotSentYet { .. }) => DeliveryState::Queued,
        Some(EventSendState::SendingFailed { .. }) => DeliveryState::Failed,
        Some(EventSendState::Sent { .. }) => DeliveryState::Sent,
        None => DeliveryState::Remote,
    }
}

fn rows<'a>(
    items: impl Iterator<Item = &'a Arc<TimelineItem>>,
    own_user: &UserId,
) -> Vec<MessageRow> {
    let mut after_marker = false;
    items
        .filter_map(|item| {
            if matches!(item.as_virtual(), Some(VirtualTimelineItem::ReadMarker)) {
                after_marker = true;
                return None;
            }
            let event = item.as_event()?;
            let content = event.content();
            let (kind, body, attachment) = if let Some(message) = content.as_message() {
                message_content(message.msgtype())
            } else if content.is_unable_to_decrypt() {
                (MessageKind::Undecryptable, String::new(), None)
            } else if content.is_redacted() {
                (MessageKind::Redacted, String::new(), None)
            } else {
                return None;
            };
            let msglike = match content {
                TimelineItemContent::MsgLike(content) => Some(content),
                _ => None,
            };
            Some(MessageRow {
                id: item.unique_id().0.clone(),
                event_id: event.event_id().map(ToString::to_string),
                new_messages: std::mem::take(&mut after_marker),
                sender: event.sender().to_string(),
                display_name: sender_name(event.sender_profile(), event.sender()),
                timestamp_ms: u64::from(event.timestamp().get()) as i64,
                body,
                edited: content
                    .as_message()
                    .is_some_and(|message| message.is_edited()),
                kind,
                attachment,
                delivery: delivery(event.send_state()),
                reply: msglike.and_then(reply_target),
                reactions: msglike
                    .map(|content| reactions(content, own_user))
                    .unwrap_or_default(),
                thread_replies: msglike.map_or(0, thread_reply_count),
                thread_unread: 0,
                read_by: event
                    .read_receipts()
                    .keys()
                    .filter(|user| user.as_str() != own_user.as_str())
                    .map(ToString::to_string)
                    .collect(),
                upload_percent: match event.send_state() {
                    Some(EventSendState::NotSentYet {
                        progress: Some(progress),
                    }) => Some(
                        ((progress.progress.current.saturating_mul(100)
                            / progress.progress.total.max(1))
                        .min(100)) as u8,
                    ),
                    _ => None,
                },
                can_retry: matches!(
                    event.send_state(),
                    Some(EventSendState::SendingFailed { .. })
                ) && event.local_echo_send_handle().is_some(),
            })
        })
        .collect()
}

fn fetch_reply_details<'a>(
    timeline: &Arc<Timeline>,
    items: impl Iterator<Item = &'a Arc<TimelineItem>>,
    attempted: &mut std::collections::HashSet<String>,
    tasks: &mut tokio::task::JoinSet<()>,
) {
    for item in items {
        if tasks.len() >= 4 {
            break;
        }
        let Some(event) = item.as_event() else {
            continue;
        };
        let TimelineItemContent::MsgLike(content) = event.content() else {
            continue;
        };
        if content
            .in_reply_to
            .as_ref()
            .is_some_and(|reply| matches!(reply.event, TimelineDetails::Unavailable))
            && let Some(id) = event.event_id()
            && attempted.insert(id.to_string())
        {
            let timeline = timeline.clone();
            let id = id.to_owned();
            tasks.spawn(async move {
                let _ = timeline.fetch_details_for_event(&id).await;
            });
        }
    }
}

async fn projected_rows(
    timeline: &Arc<Timeline>,
    items: impl Iterator<Item = &Arc<TimelineItem>>,
    own_user: &UserId,
) -> Vec<MessageRow> {
    let mut messages = rows(items, own_user);
    let conversation = Conversation {
        timeline: timeline.clone(),
    };
    for message in &mut messages {
        if message.thread_replies > 0
            && let Some(id) = &message.event_id
        {
            message.thread_unread = conversation.unread(id).await.unwrap_or(0);
        }
    }
    messages
}

impl Account {
    pub fn select_room(&self, room: Option<&str>) {
        let id = room.and_then(|id| RoomId::parse(id).ok());
        self.selected_room.send_replace(id);
    }

    pub fn conversation_updates(
        &self,
        room_id: String,
        generation: u64,
    ) -> impl Stream<Item = ConversationSnapshot> + Send + 'static + use<> {
        self.focused_updates(room_id, generation, None)
    }

    pub fn thread_updates(
        &self,
        room_id: String,
        root: String,
        generation: u64,
    ) -> impl Stream<Item = ConversationSnapshot> + Send + 'static + use<> {
        self.focused_updates(room_id, generation, Some(root))
    }

    fn focused_updates(
        &self,
        room_id: String,
        generation: u64,
        root: Option<String>,
    ) -> impl Stream<Item = ConversationSnapshot> + Send + 'static + use<> {
        let account = self.clone();
        async_stream::stream! {
            let mut snapshot = ConversationSnapshot {
                thread_root: root.clone(), unread: 0,
                generation,
                account_instance: account.instance_id(), room_id: room_id.clone(),
                conversation: None, messages: Vec::new(), status: TimelineStatus::Loading,
                members: Vec::new(), members_state: MembersState::Loading, typing: Vec::new(),
            };
            let room = RoomId::parse(&room_id).ok().and_then(|id| account.client.get_room(&id));
            let Some(room) = room else {
                snapshot.status = TimelineStatus::Unavailable;
                yield snapshot; return;
            };
            let focus = match root.as_deref() {
                Some(root) => match matrix_sdk::ruma::EventId::parse(root) {
                    Ok(root_event_id) => TimelineFocus::Thread { root_event_id },
                    Err(_) => { snapshot.status = TimelineStatus::Unavailable; yield snapshot; return; }
                },
                None => TimelineFocus::Live { hide_threaded_events: true },
            };
            let timeline = match room.timeline_builder().with_focus(focus).track_read_marker_and_receipts(TimelineReadReceiptTracking::MessageLikeEvents).build().await {
                Ok(timeline) => Arc::new(timeline),
                Err(_) => {
                    snapshot.status = TimelineStatus::Failed;
                    yield snapshot; return;
                }
            };
            let conversation = Conversation { timeline: timeline.clone() };
            let (mut items, updates) = timeline.subscribe().await;
            snapshot.conversation = Some(conversation);
            snapshot.messages = projected_rows(&timeline, items.iter(), account.client.user_id().expect("authenticated account")).await;
            if let Some(root) = &root { snapshot.unread = snapshot.conversation.as_ref().expect("timeline").unread(root).await.unwrap_or(0); }
            snapshot.status = TimelineStatus::Ready;
            yield snapshot.clone();
            let mut updates = Box::pin(updates);
            // Fetch an initial page without blocking consumption of timeline updates.
            // Pagination must keep running while the consumer is between snapshots.
            // Otherwise a concurrent `older()` can await its lock while this stream
            // is suspended at a yield. JoinSet also aborts it on selection/account drop.
            let mut initial_page = tokio::task::JoinSet::new();
            let initial_timeline = timeline.clone();
            initial_page.spawn(async move { initial_timeline.paginate_backwards(30).await });
            let mut initial_pending = true;
            let (_typing_guard, mut typing) = room.subscribe_to_typing_notifications();
            let mut room_updates = room.subscribe_to_updates();
            let mut cache_updates = account.client.event_cache().subscribe_to_room_generic_updates();
            let mut member_load = Box::pin(room_members(room.clone()));
            let mut member_pending = true;
            let mut attempted = std::collections::HashSet::new();
            let mut detail_tasks = tokio::task::JoinSet::new();
            loop {
                fetch_reply_details(&timeline, items.iter(), &mut attempted, &mut detail_tasks);
                tokio::select! {
                    change = cache_updates.recv() => {
                        if matches!(change, Err(tokio::sync::broadcast::error::RecvError::Closed)) { break; }
                        snapshot.messages = projected_rows(&timeline, items.iter(), account.client.user_id().expect("authenticated account")).await;
                        if let Some(root) = &root { snapshot.unread = snapshot.conversation.as_ref().expect("timeline").unread(root).await.unwrap_or(0); }
                        yield snapshot.clone();
                    }
                    result = initial_page.join_next(), if initial_pending => {
                        initial_pending = false;
                        snapshot.status = if matches!(result, Some(Ok(Ok(_)))) { TimelineStatus::Ready } else { TimelineStatus::HistoryFailed };
                        yield snapshot.clone();
                    }
                    result = &mut member_load, if member_pending => {
                        member_pending = false;
                        match result {
                            Ok(members) => { snapshot.members = members; snapshot.members_state = MembersState::Ready; }
                            Err(_) => snapshot.members_state = MembersState::Failed,
                        }
                        yield snapshot.clone();
                    }
                    change = room_updates.recv() => {
                        if matches!(change, Err(tokio::sync::broadcast::error::RecvError::Closed)) { break; }
                        if !member_pending { member_load = Box::pin(room_members(room.clone())); member_pending = true; }
                    }
                    change = typing.recv() => {
                        match change {
                            Ok(ids) => snapshot.typing = ids.into_iter().map(|id| id.to_string()).collect(),
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => snapshot.typing.clear(),
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                        yield snapshot.clone();
                    }
                    _ = detail_tasks.join_next(), if !detail_tasks.is_empty() => {}
                    next = updates.next() => {
                        let Some(diffs) = next else { break; };
                        for diff in diffs { diff.apply(&mut items); }
                        snapshot.messages = projected_rows(&timeline, items.iter(), account.client.user_id().expect("authenticated account")).await;
            if let Some(root) = &root { snapshot.unread = snapshot.conversation.as_ref().expect("timeline").unread(root).await.unwrap_or(0); }
                        yield snapshot.clone();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matrix_sdk::ruma::{MilliSecondsSinceUnixEpoch, event_id, user_id};
    use matrix_sdk_ui::timeline::{
        EmbeddedEvent, InReplyToDetails, ReactionInfo, ThreadSummary, TimelineEventItemId,
    };

    #[test]
    fn sdk_message_kinds_and_attachment_metadata_are_projected() {
        for (msgtype, expected) in [
            ("m.text", MessageKind::Text),
            ("m.emote", MessageKind::Emote),
            ("m.notice", MessageKind::Notice),
        ] {
            let content: MessageType =
                serde_json::from_value(serde_json::json!({"msgtype":msgtype,"body":"sample"}))
                    .expect("SDK fixture");
            let (kind, body, attachment) = message_content(&content);
            assert!(kind == expected && body == "sample" && attachment.is_none());
        }
        for (msgtype, expected) in [
            ("m.image", MessageKind::Image),
            ("m.file", MessageKind::File),
            ("m.audio", MessageKind::File),
            ("m.video", MessageKind::File),
        ] {
            let content: MessageType = serde_json::from_value(serde_json::json!({
                "msgtype":msgtype,"body":"sample caption","filename":"sample.bin",
                "url":"mxc://example.invalid/sample", "info":{"size":2048}
            }))
            .expect("SDK attachment fixture");
            let (kind, body, attachment) = message_content(&content);
            assert!(kind == expected && body.is_empty());
            let attachment = attachment.expect("attachment metadata");
            assert!(attachment.filename == "sample.bin" && attachment.size == Some(2048));
        }
    }

    #[test]
    fn reply_profile_and_unicode_plain_excerpt_are_bounded() {
        let text = format!("  sample\n{}", "🦀".repeat(150));
        let excerpt = plain_excerpt(&text);
        assert!(
            excerpt.chars().count() == 121 && excerpt.ends_with('…') && !excerpt.contains('\n')
        );
        let mut content = MsgLikeContent::redacted();
        content.in_reply_to = Some(InReplyToDetails {
            event_id: event_id!("$sample:example.invalid").to_owned(),
            event: TimelineDetails::Unavailable,
        });
        assert!(matches!(reply_target(&content), Some(ReplyTarget::Pending)));
        content.in_reply_to.as_mut().expect("reply").event =
            TimelineDetails::Ready(Box::new(EmbeddedEvent {
                content: TimelineItemContent::MsgLike(MsgLikeContent::redacted()),
                sender: user_id!("@alex:example.invalid").to_owned(),
                sender_profile: TimelineDetails::Ready(Profile {
                    display_name: Some("Alex".into()),
                    ..Default::default()
                }),
                timestamp: MilliSecondsSinceUnixEpoch::now(),
                identifier: TimelineEventItemId::EventId(
                    event_id!("$sample:example.invalid").to_owned(),
                ),
            }));
        assert!(
            matches!(reply_target(&content), Some(ReplyTarget::Ready { sender, excerpt }) if sender == "Alex" && excerpt.is_empty())
        );
        assert!(
            sender_name(
                &TimelineDetails::Unavailable,
                user_id!("@alex:example.invalid")
            ) == "@alex:example.invalid"
        );
    }

    #[test]
    fn sdk_reactions_count_distinct_senders_and_own_reaction_and_thread_count() {
        let mut content = MsgLikeContent::redacted();
        for sender in [
            user_id!("@alex:example.invalid"),
            user_id!("@me:example.invalid"),
        ] {
            content.reactions.entry("👍".into()).or_default().insert(
                sender.to_owned(),
                ReactionInfo {
                    timestamp: MilliSecondsSinceUnixEpoch::now(),
                    send_state: None,
                },
            );
        }
        let mapped = reactions(&content, user_id!("@me:example.invalid"));
        assert!(
            mapped.len() == 1 && mapped[0].key == "👍" && mapped[0].count == 2 && mapped[0].mine
        );
        assert!(!reactions(&content, user_id!("@else:example.invalid"))[0].mine);
        assert!(thread_reply_count(&content) == 0);
        content.thread_summary = Some(ThreadSummary {
            latest_event: TimelineDetails::Unavailable,
            num_replies: 7,
        });
        assert!(thread_reply_count(&content) == 7);
    }

    #[test]
    fn sdk_send_states_are_typed_without_exposing_sdk_errors() {
        assert!(delivery(None) == DeliveryState::Remote);
        assert!(
            delivery(Some(&EventSendState::NotSentYet { progress: None })) == DeliveryState::Queued
        );
        assert!(
            delivery(Some(&EventSendState::Sent {
                event_id: event_id!("$sample:example.invalid").to_owned()
            })) == DeliveryState::Sent
        );
        assert!(
            delivery(Some(&EventSendState::SendingFailed {
                error: Arc::new(matrix_sdk::Error::UnknownError(Box::new(
                    std::io::Error::other("sample failure")
                ))),
                is_recoverable: true,
            })) == DeliveryState::Failed
        );
    }
}
