//! MSC4075/4310 room-call notifications. SDK sync and crypto own delivery.
use super::{ActiveCall, CallError, native};
use crate::auth::Account;
use futures_util::Stream;
use matrix_sdk::{
    Room, RoomState,
    deserialized_responses::{EncryptionInfo, VerificationState},
    event_handler::RawEvent,
    ruma::{RoomId, UserId, events::AnySyncTimelineEvent},
};
use serde_json::{Value, json};
use std::collections::HashMap;
use tokio::sync::mpsc;

pub const NOTIFICATION_EVENT: &str = "org.matrix.msc4075.rtc.notification";
pub const DECLINE_EVENT: &str = "org.matrix.msc4310.rtc.decline";
pub const SLOT_EVENT: &str = "org.matrix.msc4143.rtc.slot";
pub const SLOT: &str = "m.call#ROOM";
pub const RING_LIFETIME_MS: u64 = 90_000;
pub const MAX_INCOMING: usize = 8;

/// One invitation attempt. A joined peer wins a simultaneous decline/expiry.
pub(crate) struct OutgoingRing {
    pub event_id: String,
    pub since: u64,
    pub expires: u64,
}
impl OutgoingRing {
    pub fn phase(&self, now: u64, joined: bool, declined: bool) -> super::CallPhase {
        use super::{CallPhase, EndReason};
        if joined {
            CallPhase::Connected
        } else if declined {
            CallPhase::Ended(EndReason::Declined)
        } else if now >= self.expires {
            CallPhase::Ended(EndReason::RingExpired)
        } else {
            CallPhase::Ringing {
                since: self.since,
                expires: self.expires,
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallIntent {
    Audio,
    Video,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyType {
    Ring,
    Notification,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncomingCall {
    pub room_id: String,
    pub room_name: String,
    pub caller_user: String,
    /// Bound to the referenced current membership, never an arbitrary content claim.
    pub caller_device: Option<String>,
    pub caller_name: String,
    pub kind: CallIntent,
    pub notify_type: NotifyType,
    pub event_id: String,
    /// Unix milliseconds, including the bounded server/sender clock adjustment.
    pub since: u64,
    pub expires: u64,
    pub verified: bool,
    pub ringing: bool,
}
#[derive(Clone, Default)]
pub struct CallActivity {
    pub incoming_calls: Vec<IncomingCall>,
    pub room_calls: HashMap<String, ActiveCall>,
}
pub enum IncomingAction {
    Dismiss(String),
    Decline(String),
}

pub fn now_ms() -> u64 {
    u64::from(matrix_sdk::ruma::MilliSecondsSinceUnixEpoch::now().0)
}
pub fn notification_expiry(sender: u64, origin: u64, lifetime: u64, now: u64) -> u64 {
    let basis = if sender > origin.saturating_add(20_000) {
        origin
    } else {
        sender
    };
    basis
        .saturating_add(lifetime.min(120_000))
        .min(now.saturating_add(120_000))
}
pub(crate) fn notification_content(
    parent: &str,
    video: bool,
    direct: bool,
    users: Vec<String>,
    now: u64,
) -> Value {
    json!({"slot_id": SLOT, "msc4354_sticky_key": SLOT,
        "m.mentions": {"user_ids": users},
        "notification_type": if direct {"ring"} else {"notification"},
        // Older readers use these fields. The pinned SDK uses notification_type.
        "notify_type": if direct {"ring"} else {"notification"},
        "application": "m.call", "call_id": "",
        "m.call.intent": if video {"video"} else {"audio"},
        "sender_ts": now, "lifetime": RING_LIFETIME_MS,
        "m.relates_to": {"rel_type":"m.reference", "event_id":parent}})
}
pub(crate) fn reference(content: &Value) -> Option<&str> {
    (content["m.relates_to"]["rel_type"] == "m.reference")
        .then(|| content["m.relates_to"]["event_id"].as_str())
        .flatten()
}
pub(crate) fn is_notification(kind: &str) -> bool {
    [
        NOTIFICATION_EVENT,
        "m.call.notify",
        "org.matrix.msc4075.call.notify",
    ]
    .contains(&kind)
}

#[derive(Default)]
struct Inbox {
    calls: Vec<IncomingCall>,
    dismissed: Vec<(String, u64)>,
}
impl Inbox {
    fn insert(&mut self, call: IncomingCall, own: &str, ignored: bool, now: u64) {
        self.prune(now);
        if call.caller_user == own
            || ignored
            || call.expires <= now
            || self.dismissed.iter().any(|(id, _)| id == &call.event_id)
        {
            return;
        }
        // One newest invitation per room. Replayed/older events cannot replace it.
        if self.calls.iter().any(|old| {
            old.room_id == call.room_id
                && (old.since, &old.event_id) >= (call.since, &call.event_id)
        }) {
            return;
        }
        self.calls.retain(|old| old.room_id != call.room_id);
        self.calls.push(call);
        self.calls
            .sort_by(|a, b| (b.since, &b.event_id).cmp(&(a.since, &a.event_id)));
        self.calls.truncate(MAX_INCOMING);
    }
    fn dismiss(&mut self, room: &str) -> Option<IncomingCall> {
        let index = self.calls.iter().position(|c| c.room_id == room)?;
        let call = self.calls.remove(index);
        if self.dismissed.len() == 256 {
            self.dismissed.remove(0);
        }
        self.dismissed.push((call.event_id.clone(), call.expires));
        Some(call)
    }
    fn prune(&mut self, now: u64) {
        self.calls.retain(|c| c.expires > now);
        self.dismissed.retain(|(_, expiry)| *expiry > now);
    }
}
fn still_incoming(call: &IncomingCall, active: Option<&ActiveCall>, own: &str) -> bool {
    active.is_some_and(|a| {
        a.participants.iter().any(|p| {
            p.user_id == call.caller_user && Some(&p.device_id) == call.caller_device.as_ref()
        }) && !a.participants.iter().any(|p| p.user_id == own)
    })
}
struct Candidate {
    room: Room,
    event: Value,
    encryption: Option<EncryptionInfo>,
}
async fn parse(candidate: Candidate, own: &str) -> Option<IncomingCall> {
    let Candidate {
        room,
        event,
        encryption,
    } = candidate;
    let content = &event["content"];
    if room.state() != RoomState::Joined || !is_notification(event["type"].as_str()?) {
        return None;
    }
    let sender = event["sender"].as_str()?;
    let user = UserId::parse(sender).ok()?;
    if sender == own || room.client().is_user_ignored(&user).await {
        return None;
    }
    let now = now_ms();
    let origin = event["origin_server_ts"].as_u64()?;
    let modern = event["type"] == NOTIFICATION_EVENT;
    if modern && content.get("slot_id").is_some_and(|s| s != SLOT) {
        return None;
    }
    if !modern && (content["application"] != "m.call" || content["call_id"] != "") {
        return None;
    }
    if content.get("msc4354_sticky_key").is_some_and(|s| s != SLOT) {
        return None;
    }
    let expiry = notification_expiry(
        if modern {
            content["sender_ts"].as_u64()?
        } else {
            content["sender_ts"].as_u64().unwrap_or(origin)
        },
        origin,
        if modern {
            content["lifetime"].as_u64()?
        } else {
            content["lifetime"].as_u64().unwrap_or(RING_LIFETIME_MS)
        },
        now,
    );
    if expiry <= now {
        return None;
    }
    let mentions = &content["m.mentions"];
    let targeted = mentions["user_ids"]
        .as_array()
        .is_some_and(|ids| ids.iter().any(|id| id == own));
    let room_mention = mentions["room"] == true
        && room
            .power_levels()
            .await
            .ok()
            .is_some_and(|e| e.user_can_trigger_room_notification(&user));
    if !targeted && !room_mention {
        return None;
    }
    let members = native::members(&room).await.ok()?;
    if members.iter().any(|m| m.user_id.as_str() == own) {
        return None;
    }
    let member = if let Some(parent) = reference(content) {
        // Resolve the exact current state event, then bind its SDK-parsed device.
        let mut device = None;
        for raw in room.get_state_events_static::<matrix_sdk::ruma::events::call::member::CallMemberEventContent>().await.ok()? {
            if let Ok(e) = raw.deserialize() && let Some(e) = e.as_sync() && e.event_id().as_str() == parent && e.sender().as_str() == sender {
                device = members.iter().find(|m| m.user_id == user && e.state_key().as_ref() == native::state_key(user.clone(), &m.device_id, room.version().as_ref().map(|v|v.as_str()).unwrap_or("10")).as_ref()).cloned();
            }
        }
        device?
    } else {
        members
            .iter()
            .find(|m| {
                m.user_id == user
                    && content["device_id"]
                        .as_str()
                        .is_none_or(|id| id == m.device_id)
            })
            .cloned()?
    };
    let verified = encryption.as_ref().is_some_and(|info| {
        info.forwarder.is_none()
            && info.sender.as_str() == sender
            && info
                .sender_device
                .as_ref()
                .is_some_and(|d| d.as_str() == member.device_id)
            && info.verification_state == VerificationState::Verified
    });
    let name = room
        .get_member_no_sync(&user)
        .await
        .ok()
        .flatten()
        .and_then(|m| m.display_name().map(str::to_owned))
        .unwrap_or_else(|| sender.to_owned());
    let notify_type = match content["notification_type"]
        .as_str()
        .or(content["notify_type"].as_str())
    {
        Some("notification" | "notify") => NotifyType::Notification,
        Some("ring") => NotifyType::Ring,
        None if modern => NotifyType::Ring,
        _ => return None,
    };
    Some(IncomingCall {
        room_id: room.room_id().to_string(),
        room_name: room.display_name().await.ok()?.to_string(),
        caller_user: sender.into(),
        caller_device: Some(member.device_id),
        caller_name: name,
        kind: if content["m.call.intent"] == "video" {
            CallIntent::Video
        } else {
            CallIntent::Audio
        },
        notify_type,
        event_id: event["event_id"].as_str()?.into(),
        since: origin,
        expires: expiry,
        verified,
        ringing: notify_type == NotifyType::Ring,
    })
}
impl Account {
    /// A bounded SDK sync observer; the timer expires invitations only, it never fetches events.
    pub fn call_activity(
        &self,
        mut actions: mpsc::Receiver<IncomingAction>,
    ) -> impl Stream<Item = CallActivity> + Send + 'static + use<> {
        let account = self.clone();
        async_stream::stream! {
            let (send, mut receive) = mpsc::channel::<Candidate>(32);
            let handler = account.client.add_event_handler(move |_event: AnySyncTimelineEvent, raw: RawEvent, room: Room, encryption: Option<EncryptionInfo>| {
                let send = send.clone();
                async move {
                    if raw.0.get().len() > 16_384 { return; }
                    let Ok(event) = serde_json::from_str::<Value>(raw.0.get()) else { return; };
                    if event["type"].as_str().is_some_and(|t|is_notification(t) || t == DECLINE_EVENT) {
                        let _ = send.try_send(Candidate{room,event,encryption});
                    }
                }
            });
            let _guard = account.client.event_handler_drop_guard(handler);
            let mut sync = account.client.subscribe_to_all_room_updates();
            let mut ignored = account.client.subscribe_to_ignore_user_list_changes();
            let mut inbox = Inbox::default();
            loop {
                inbox.prune(now_ms());
                let calls = account.room_calls().await;
                let mut remove = Vec::new();
                for call in &inbox.calls {
                    let active = calls.get(&call.room_id);
                    if !account.call_room_joined(&call.room_id) || account.client.is_user_ignored(&UserId::parse(&call.caller_user).expect("validated sender")).await || !still_incoming(call, active, &account.user_id()) { remove.push(call.room_id.clone()); }
                }
                for room in remove { inbox.dismiss(&room); }
                let expires = inbox.calls.iter().map(|c|c.expires).chain(calls.values().filter_map(|c|c.expires_ms)).min().unwrap_or(now_ms().saturating_add(120_000));
                yield CallActivity { incoming_calls: inbox.calls.clone(), room_calls: calls };
                tokio::select! {
                    candidate = receive.recv() => {
                        let Some(candidate) = candidate else { break; };
                        if candidate.event["type"] == DECLINE_EVENT {
                            if candidate.event["sender"] == account.user_id() && let Some(id) = reference(&candidate.event["content"]) && let Some(call) = inbox.calls.iter().find(|c|c.event_id == id) { let room = call.room_id.clone(); inbox.dismiss(&room); }
                        } else if let Some(call) = parse(candidate, &account.user_id()).await { inbox.insert(call, &account.user_id(), false, now_ms()); }
                    }
                    action = actions.recv() => { match action {
                        Some(IncomingAction::Dismiss(room)) => {inbox.dismiss(&room);}
                        Some(IncomingAction::Decline(room)) => { if let Some(call) = inbox.dismiss(&room) { let _ = account.decline_call_notification(&call).await; } }
                        None => break,
                    }}
                    result = sync.recv() => { if matches!(result, Err(tokio::sync::broadcast::error::RecvError::Closed)) {break;} }
                    _ = futures_util::StreamExt::next(&mut ignored) => {}
                    _ = tokio::time::sleep(std::time::Duration::from_millis(expires.saturating_sub(now_ms()).max(1))) => {}
                }
            }
        }
    }
    pub async fn decline_call_notification(&self, call: &IncomingCall) -> Result<(), CallError> {
        if call.expires <= now_ms() {
            return Err(CallError::StaleRequest);
        }
        let room = RoomId::parse(&call.room_id)
            .ok()
            .and_then(|id| self.client.get_room(&id))
            .filter(|r| r.state() == RoomState::Joined)
            .ok_or(CallError::InvalidRoom)?;
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            room.send_raw(
                DECLINE_EVENT,
                json!({"m.relates_to":{"rel_type":"m.reference","event_id":call.event_id}}),
            ),
        )
        .await
        .map_err(|_| CallError::TransportFailed)?
        .map_err(|_| CallError::TransportFailed)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(i: u64) -> IncomingCall {
        IncomingCall {
            room_id: format!("!{i}:test"),
            room_name: String::new(),
            caller_user: "@a:test".into(),
            caller_device: Some("A".into()),
            caller_name: String::new(),
            kind: CallIntent::Audio,
            notify_type: NotifyType::Ring,
            event_id: format!("${i}"),
            since: i,
            expires: 1000,
            verified: false,
            ringing: true,
        }
    }
    #[test]
    fn outgoing_accept_decline_and_expiry_transitions() {
        use super::super::{CallPhase, EndReason};
        let ring = OutgoingRing {
            event_id: "$ring".into(),
            since: 100,
            expires: 1000,
        };
        assert_eq!(
            ring.phase(999, false, false),
            CallPhase::Ringing {
                since: 100,
                expires: 1000
            }
        );
        assert_eq!(
            ring.phase(1000, false, false),
            CallPhase::Ended(EndReason::RingExpired)
        );
        assert_eq!(
            ring.phase(101, false, true),
            CallPhase::Ended(EndReason::Declined)
        );
        assert_eq!(ring.phase(101, true, false), CallPhase::Connected);
        assert_eq!(ring.phase(1000, true, true), CallPhase::Connected);
    }
    #[test]
    fn caller_departure_and_answer_on_another_own_device_stop_ringing() {
        use super::super::Participant;
        let invitation = call(1);
        let peer = |user: &str, device: &str| Participant {
            user_id: user.into(),
            device_id: device.into(),
            display_name: None,
            speaking: None,
            muted: None,
            video: None,
            connection_quality: None,
            media_blocked: false,
            screen: false,
        };
        let mut active = ActiveCall {
            participants: vec![peer("@a:test", "A")],
            started_ms: None,
            expires_ms: None,
        };
        assert!(still_incoming(&invitation, Some(&active), "@b:test"));
        active.participants.push(peer("@b:test", "OTHER_DEVICE"));
        assert!(!still_incoming(&invitation, Some(&active), "@b:test"));
        active.participants = vec![peer("@a:test", "DIFFERENT_CALLER_DEVICE")];
        assert!(!still_incoming(&invitation, Some(&active), "@b:test"));
        assert!(!still_incoming(&invitation, None, "@b:test"));
    }
    #[test]
    fn expiry_caps_and_clock_skew() {
        assert_eq!(notification_expiry(1000, 1000, 90_000, 2000), 91_000);
        assert_eq!(notification_expiry(50_000, 1000, 90_000, 2000), 91_000);
        assert_eq!(notification_expiry(1000, 1000, u64::MAX, 2000), 121_000);
        assert_eq!(notification_expiry(1000, 1000, 0, 2000), 1000);
    }
    #[test]
    fn inbox_own_user_ignored_order_bounds_dismiss_and_expiry() {
        let mut inbox = Inbox::default();
        inbox.insert(call(1), "@a:test", false, 0);
        inbox.insert(call(1), "@b:test", true, 0);
        assert!(inbox.calls.is_empty());
        for i in 0..32 {
            inbox.insert(call(i), "@b:test", false, 0);
        }
        assert_eq!(inbox.calls.len(), MAX_INCOMING);
        assert_eq!(inbox.calls[0].since, 31);
        inbox.dismiss("!31:test");
        inbox.insert(call(31), "@b:test", false, 0);
        assert_eq!(inbox.calls[0].since, 30);
        inbox.prune(1000);
        assert!(inbox.calls.is_empty());
    }
}
