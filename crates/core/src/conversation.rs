//! SDK timeline projection and send queue; never log event content.

use crate::auth::Account;
use futures_util::{FutureExt, Stream, StreamExt, future::BoxFuture};
use matrix_sdk::ruma::{RoomId, events::room::message::RoomMessageEventContent};
use matrix_sdk_ui::timeline::{EventSendState, RoomExt, Timeline, TimelineDetails, TimelineItem};
use std::sync::Arc;

/// What a timeline row represents. The frontend owns the wording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    Text,
    Undecryptable,
    Redacted,
}

#[derive(Clone)]
pub struct MessageRow {
    pub id: String,
    pub sender: String,
    pub display_name: Option<String>,
    pub timestamp_ms: i64,
    /// Empty unless `kind` is `Text`.
    pub body: String,
    pub kind: MessageKind,
    pub delivery: &'static str,
}

#[derive(Clone)]
pub struct Conversation {
    timeline: Arc<Timeline>,
}

#[derive(Clone)]
pub struct ConversationSnapshot {
    pub generation: u64,
    pub account_instance: u128,
    pub room_id: String,
    pub conversation: Option<Conversation>,
    pub messages: Vec<MessageRow>,
    pub status: &'static str,
}

impl Conversation {
    pub fn send(&self, body: String) -> BoxFuture<'_, Result<(), &'static str>> {
        async move {
            if body.trim().is_empty() {
                return Err("Write a message first.");
            }
            self.timeline
                .send(RoomMessageEventContent::text_plain(body).into())
                .await
                .map(|_| ())
                .map_err(|_| "Message could not be queued. Your draft has been kept.")
        }
        .boxed()
    }

    pub async fn older(&self) -> Result<bool, &'static str> {
        self.timeline
            .paginate_backwards(30)
            .await
            .map_err(|_| "Older messages could not be loaded. Try again.")
    }
}

fn rows<'a>(items: impl Iterator<Item = &'a Arc<TimelineItem>>) -> Vec<MessageRow> {
    items
        .filter_map(|item| {
            let event = item.as_event()?;
            let content = event.content();
            let (kind, body) = if let Some(message) = content.as_message() {
                (MessageKind::Text, message.body().to_owned())
            } else if content.is_unable_to_decrypt() {
                (MessageKind::Undecryptable, String::new())
            } else if content.is_redacted() {
                (MessageKind::Redacted, String::new())
            } else {
                return None;
            };
            let delivery = match event.send_state() {
                Some(EventSendState::NotSentYet { .. }) => "Sending…",
                Some(EventSendState::SendingFailed { .. }) => "Send failed",
                Some(EventSendState::Sent { .. }) => "Sent",
                None => "",
            };
            Some(MessageRow {
                id: item.unique_id().0.clone(),
                sender: event.sender().to_string(),
                display_name: match event.sender_profile() {
                    TimelineDetails::Ready(profile) => profile.display_name.as_ref().map(|name| {
                        if profile.display_name_ambiguous {
                            format!("{name} ({})", event.sender())
                        } else {
                            name.clone()
                        }
                    }),
                    _ => None,
                },
                timestamp_ms: u64::from(event.timestamp().get()) as i64,
                body,
                kind,
                delivery,
            })
        })
        .collect()
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
        let account = self.clone();
        async_stream::stream! {
            let mut snapshot = ConversationSnapshot {
                generation,
                account_instance: account.instance_id(), room_id: room_id.clone(),
                conversation: None, messages: Vec::new(), status: "Loading messages…",
            };
            let room = RoomId::parse(&room_id).ok().and_then(|id| account.client.get_room(&id));
            let Some(room) = room else {
                snapshot.status = "This conversation is no longer available.";
                yield snapshot; return;
            };
            let timeline = match room.timeline().await {
                Ok(timeline) => Arc::new(timeline),
                Err(_) => {
                    snapshot.status = "Messages could not be loaded. Select the conversation to retry.";
                    yield snapshot; return;
                }
            };
            let conversation = Conversation { timeline: timeline.clone() };
            let (mut items, updates) = timeline.subscribe().await;
            snapshot.conversation = Some(conversation);
            snapshot.messages = rows(items.iter());
            snapshot.status = "";
            yield snapshot.clone();
            let mut updates = Box::pin(updates);
            // Fetch an initial page without blocking consumption of timeline updates.
            let initial_page = timeline.paginate_backwards(30);
            tokio::pin!(initial_page);
            let mut initial_pending = true;
            loop {
                tokio::select! {
                    result = &mut initial_page, if initial_pending => {
                        initial_pending = false;
                        snapshot.status = if result.is_err() { "Some history could not be loaded. Try Load Earlier." } else { "" };
                        yield snapshot.clone();
                    }
                    next = updates.next() => {
                        let Some(diffs) = next else { break; };
                        for diff in diffs { diff.apply(&mut items); }
                        snapshot.messages = rows(items.iter());
                        yield snapshot.clone();
                    }
                }
            }
        }
    }
}
