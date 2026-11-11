//! SDK-filtered, decrypted notification items. Never format these for logging.
use crate::auth::Account;
use futures_util::{Stream, StreamExt};
use matrix_sdk::{
    deserialized_responses::RawAnySyncOrStrippedTimelineEvent,
    ruma::{
        OwnedEventId, OwnedRoomId,
        events::{AnySyncMessageLikeEvent, AnySyncTimelineEvent, SyncMessageLikeEvent},
    },
};
use matrix_sdk_ui::notification_client::{
    NotificationClient, NotificationEvent, NotificationProcessSetup, NotificationStatus,
};
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Clone, Default, PartialEq, Eq)]
pub struct NotificationContext {
    pub enabled: bool,
    pub active: bool,
    pub focused_room: Option<String>,
}

#[derive(Clone)]
pub struct NotificationItem {
    pub room: String,
    pub room_name: String,
    pub event: String,
    pub thread: Option<String>,
    pub sender: String,
    pub sender_name: String,
    pub sender_avatar: Option<String>,
    pub body: String,
    pub direct: bool,
    pub mention: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct NotificationPreview {
    pub sender: Option<String>,
    pub avatar: Option<String>,
    pub body: Option<String>,
}
impl NotificationItem {
    pub fn preview(&self, enabled: bool) -> NotificationPreview {
        NotificationPreview {
            sender: enabled.then(|| self.sender_name.clone()),
            avatar: enabled.then(|| self.sender_avatar.clone()).flatten(),
            body: enabled.then(|| self.body.clone()),
        }
    }
}

pub fn should_notify(own: bool, muted: bool, room: &str, context: &NotificationContext) -> bool {
    context.enabled
        && !own
        && !muted
        && !(context.active && context.focused_room.as_deref() == Some(room))
}

struct Candidate {
    room: OwnedRoomId,
    event: OwnedEventId,
}

fn project(
    room: &str,
    item: matrix_sdk_ui::notification_client::NotificationItem,
) -> Option<NotificationItem> {
    let NotificationEvent::Timeline(event) = &item.event else {
        return None;
    };
    let AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
        SyncMessageLikeEvent::Original(event),
    )) = event.as_ref()
    else {
        return None;
    };
    // Edits, redactions and state events are not new-message notifications.
    if matches!(
        event.content.relates_to,
        Some(matrix_sdk::ruma::events::room::message::Relation::Replacement(_))
    ) {
        return None;
    }
    Some(NotificationItem {
        room: room.to_owned(),
        room_name: item.room_computed_display_name,
        event: event.event_id.to_string(),
        thread: item.thread_id.map(|id| id.to_string()),
        sender: event.sender.to_string(),
        sender_name: item
            .sender_display_name
            .unwrap_or_else(|| event.sender.to_string()),
        sender_avatar: item.sender_avatar_url,
        body: event.content.msgtype.body().to_owned(),
        direct: item.is_direct_message_room,
        mention: item.has_mention.unwrap_or(false),
    })
}

impl Account {
    pub fn notification_updates(
        &self,
        context: watch::Receiver<NotificationContext>,
    ) -> impl Stream<Item = NotificationItem> + Send + 'static + use<> {
        let account = self.clone();
        async_stream::stream! {
            let (sender, mut candidates) = tokio::sync::mpsc::channel::<Candidate>(64);
            let weak = sender.downgrade();
            let started = matrix_sdk::ruma::MilliSecondsSinceUnixEpoch::now();
            let intake_context = context.clone();
            let intake_own = account.client.user_id().expect("authenticated account").to_owned();
            account.client.register_notification_handler(move |notification, room, _| {
                let weak = weak.clone();
                let context = intake_context.clone();
                let own = intake_own.clone();
                async move {
                    let Some(sender) = weak.upgrade() else { return; };
                    let RawAnySyncOrStrippedTimelineEvent::Sync(raw) = notification.event else { return; };
                    let sender_id = raw.get_field::<matrix_sdk::ruma::OwnedUserId>("sender").ok().flatten();
                    if !should_notify(sender_id.as_ref() == Some(&own), false, room.room_id().as_str(), &context.borrow()) { return; }
                    let timestamp = raw.get_field::<matrix_sdk::ruma::MilliSecondsSinceUnixEpoch>("origin_server_ts").ok().flatten();
                    if timestamp.is_none_or(|time| time < started) { return; }
                    if let Ok(Some(event)) = raw.get_field::<OwnedEventId>("event_id") {
                        // Drop excess work instead of delaying SDK sync.
                        let _ = sender.try_send(Candidate { room: room.room_id().to_owned(), event });
                    }
                }
            }).await;
            // An encrypted event with missing keys may not yet match a
            // mentions-only rule. Let the notification client obtain keys and
            // evaluate the decrypted event's SDK push actions as well.
            let encrypted_handler = account.client.add_event_handler({
                let weak = sender.downgrade();
                let context = context.clone();
                let own = account.client.user_id().expect("authenticated account").to_owned();
                move |event: matrix_sdk::ruma::events::room::encrypted::OriginalSyncRoomEncryptedEvent, room: matrix_sdk::Room| {
                    let weak = weak.clone(); let context = context.clone(); let own = own.clone();
                    async move {
                        if event.origin_server_ts < started || !should_notify(event.sender == own, false, room.room_id().as_str(), &context.borrow()) { return; }
                        if let Some(sender) = weak.upgrade() { let _ = sender.try_send(Candidate { room: room.room_id().to_owned(), event: event.event_id }); }
                    }
                }
            });
            let _encrypted_guard = account.client.event_handler_drop_guard(encrypted_handler);
            let _sender_lifetime = sender;
            let mut services = account.notification_service.subscribe();
            let service = loop {
                let service = services.borrow_and_update().clone();
                if let Some(service) = service { break service; }
                if services.changed().await.is_err() { return; }
            };
            let Ok(client) = NotificationClient::new(account.client.clone(), NotificationProcessSetup::SingleProcess { sync_service: service }).await else { return; };
            let client = Arc::new(client);
            let rules = account.client.notification_settings().await;
            let own = account.user_id();
            let source = async_stream::stream! { while let Some(candidate) = candidates.recv().await { yield candidate; } };
            let results = source.map(move |candidate| {
                let client = client.clone();
                async move {
                    let result = client.get_notification(&candidate.room, &candidate.event).await;
                    (candidate, result)
                }
            }).buffer_unordered(4);
            tokio::pin!(results);
            let mut recent = std::collections::VecDeque::new();
            while let Some((candidate, result)) = results.next().await {
                if recent.contains(&candidate.event) { continue; }
                if recent.len() == 512 { recent.pop_front(); }
                recent.push_back(candidate.event);
                let Ok(NotificationStatus::Event(item)) = result else { continue; };
                let Some(item) = project(candidate.room.as_str(), *item) else { continue; };
                let muted = rules.get_user_defined_room_notification_mode(&candidate.room).await == Some(matrix_sdk::notification_settings::RoomNotificationMode::Mute);
                if should_notify(item.sender == own, muted, &item.room, &context.borrow()) { yield item; }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skip_rules_cover_own_muted_focused_active_and_disabled() {
        let mut context = NotificationContext {
            enabled: true,
            active: true,
            focused_room: Some("room".into()),
        };
        assert!(!should_notify(false, false, "room", &context));
        assert!(should_notify(false, false, "other", &context));
        context.active = false;
        assert!(should_notify(false, false, "room", &context));
        assert!(!should_notify(true, false, "room", &context));
        assert!(!should_notify(false, true, "room", &context));
        context.enabled = false;
        assert!(!should_notify(false, false, "other", &context));
    }
    #[test]
    fn preview_off_removes_sender_avatar_and_body() {
        let item = NotificationItem {
            room: "room".into(),
            room_name: "Sample room".into(),
            event: "event".into(),
            thread: None,
            sender: "sender".into(),
            sender_name: "Sample sender".into(),
            sender_avatar: Some("mxc://sample/avatar".into()),
            body: "Sample message".into(),
            direct: true,
            mention: false,
        };
        let preview = item.preview(false);
        assert!(preview.sender.is_none() && preview.avatar.is_none() && preview.body.is_none());
        assert!(item.preview(true).body.is_some());
    }
}

/// Shared running/relaunch validation, independent of UI selection state.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ActivationState {
    Ready,
    Pending,
    Rejected,
}
impl Account {
    /// Stable across same-device restore; contains public IDs, no credential.
    pub fn notification_identity(&self) -> String {
        format!(
            "{}\n{}\n{}",
            self.client.homeserver(),
            self.user_id(),
            self.client
                .device_id()
                .map(|id| id.as_str())
                .unwrap_or_default()
        )
    }
    pub fn validate_notification_target(
        &self,
        identity: &str,
        room: &str,
        thread: Option<&str>,
    ) -> ActivationState {
        if identity != self.notification_identity()
            || thread.is_some_and(|id| matrix_sdk::ruma::EventId::parse(id).is_err())
        {
            return ActivationState::Rejected;
        }
        let Ok(id) = matrix_sdk::ruma::RoomId::parse(room) else {
            return ActivationState::Rejected;
        };
        match self.client.get_room(&id) {
            Some(room) if room.state() == matrix_sdk::RoomState::Joined => ActivationState::Ready,
            Some(_) => ActivationState::Rejected,
            None => ActivationState::Pending,
        }
    }
}
