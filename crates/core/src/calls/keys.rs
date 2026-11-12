//! Media keys are opaque CSPRNG material. The SDK owns all Olm encryption.
use super::{
    CallError,
    native::{KEY_EVENT, Member, members},
};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use matrix_sdk::{
    Room,
    deserialized_responses::{AlgorithmInfo, EncryptionInfo},
    event_handler::EventHandlerDropGuard,
    ruma::{events::AnyToDeviceEvent, serde::Raw},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tokio::sync::mpsc;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

#[derive(Zeroize, ZeroizeOnDrop)]
pub(crate) struct MediaKey([u8; 16]);
impl MediaKey {
    fn generate() -> Result<Self, CallError> {
        let mut key = Self([0; 16]);
        getrandom::fill(&mut key.0).map_err(|_| CallError::KeyRejected)?;
        Ok(key)
    }
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}
#[derive(Serialize, Deserialize)]
struct KeyEntry {
    index: u8,
    key: String,
}
#[derive(Serialize, Deserialize)]
struct KeyMember {
    claimed_device_id: String,
    id: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Session {
    call_id: String,
    application: String,
    scope: String,
}
#[derive(Serialize, Deserialize)]
struct Content {
    room_id: String,
    member: KeyMember,
    session: Option<Session>,
    keys: Option<KeyEntry>,
    media_key: Option<KeyEntry>,
    member_id: Option<String>,
    sent_ts: Option<u64>,
}
impl Drop for KeyEntry {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

pub(crate) struct ReceivedKey {
    pub identity: String,
    pub index: u8,
    pub key: MediaKey,
}
pub(crate) struct KeyReceiver {
    pub receive: mpsc::Receiver<ReceivedKey>,
    _guard: EventHandlerDropGuard,
}
impl KeyReceiver {
    pub fn new(room: Room) -> Self {
        let (send, receive) = mpsc::channel(32);
        let handle = room.client().add_event_handler({
            let room = room.clone();
            move |raw: Raw<AnyToDeviceEvent>, info: Option<EncryptionInfo>| {
                let room = room.clone();
                let send = send.clone();
                async move {
                    if raw.json().get().len() > 4096 {
                        return;
                    }
                    let Some(info) = info else { return };
                    let Ok(Some(kind)) = raw.get_field::<String>("type") else {
                        return;
                    };
                    if kind != KEY_EVENT {
                        return;
                    }
                    let Ok(Some(content)) = raw.get_field::<Content>("content") else {
                        return;
                    };
                    let Ok(Some(sender)) = raw.get_field::<String>("sender") else {
                        return;
                    };
                    let Ok(current) = members(&room).await else {
                        return;
                    };
                    if let Some(key) = accept(
                        &content,
                        &sender,
                        Some(&info),
                        room.room_id().as_str(),
                        &current,
                        now_ms(),
                    ) {
                        // A full queue fails closed: losing a key may temporarily
                        // mute a participant, but never permits clear media.
                        let _ = send.try_send(key);
                    }
                }
            }
        });
        Self {
            receive,
            _guard: room.client().event_handler_drop_guard(handle),
        }
    }
}
fn now_ms() -> u64 {
    u64::from(matrix_sdk::ruma::MilliSecondsSinceUnixEpoch::now().0)
}
fn accept(
    content: &Content,
    sender: &str,
    info: Option<&EncryptionInfo>,
    room_id: &str,
    current: &[Member],
    now: u64,
) -> Option<ReceivedKey> {
    let info = info?;
    if content.room_id != room_id
        || content.session.as_ref().is_some_and(|s| {
            s.application != "m.call" || !s.call_id.is_empty() || s.scope != "m.room"
        })
        || sender != info.sender.as_str()
        || info.forwarder.is_some()
        || content
            .sent_ts
            .is_some_and(|ts| ts > now.saturating_add(30_000) || now.saturating_sub(ts) > 120_000)
    {
        return None;
    }
    if !matches!(
        info.algorithm_info,
        AlgorithmInfo::OlmV1Curve25519AesSha2 { .. }
    ) {
        return None;
    }
    let device = info.sender_device.as_ref()?;
    if device.as_str() != content.member.claimed_device_id {
        return None;
    }
    let member = current
        .iter()
        .find(|m| m.user_id == info.sender && m.device_id == device.as_str())?;
    // Element Call 0.17's state-event mode publishes user:device as
    // membershipID, but sends a per-join UUID in this to-device member.id.
    // Bind to the authenticated user/device and room call, not this claim.
    if content
        .sent_ts
        .is_some_and(|ts| ts.saturating_add(30_000) < member.created_ms)
    {
        return None;
    }
    // The current transport's explicit member_id must match even when a
    // sender also includes the legacy session object. Modern messages require
    // an incarnation claim; only legacy member.id may be an opaque UUID.
    if content
        .member_id
        .as_ref()
        .is_some_and(|id| id != &member.member_id)
        || (content.session.is_none()
            && content.member_id.as_ref().or(content.member.id.as_ref()) != Some(&member.member_id))
    {
        return None;
    }
    let entry = content.media_key.as_ref().or(content.keys.as_ref())?;
    // Device identity must be linked to the Olm sender, not just claimed by its
    // plaintext. SDK supplies sender_device only after resolving the sender key.
    if entry.key.len() > 24 {
        return None;
    }
    let decoded = Zeroizing::new(
        STANDARD_NO_PAD
            .decode(&entry.key)
            .or_else(|_| STANDARD.decode(&entry.key))
            .ok()?,
    );
    let key = MediaKey(decoded.as_slice().try_into().ok()?);
    Some(ReceivedKey {
        identity: member.identity(),
        index: entry.index,
        key,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MemberChange {
    pub changed: bool,
    pub rotated: bool,
}
pub(crate) struct SenderKeys {
    pub key: MediaKey,
    pub index: u8,
    recipients: BTreeSet<(String, String)>,
}
impl SenderKeys {
    pub fn new() -> Result<Self, CallError> {
        Ok(Self {
            key: MediaKey::generate()?,
            index: 0,
            recipients: BTreeSet::new(),
        })
    }
    /// Rotate on any departure, including a different device of the same user.
    /// Returns whether recipients changed (the key must be sent) and whether
    /// the key rotated (the sender must switch to the new index).
    pub fn update_members(
        &mut self,
        own: &Member,
        members: &[Member],
    ) -> Result<MemberChange, CallError> {
        let next: BTreeSet<_> = members
            .iter()
            .filter(|m| m.user_id != own.user_id || m.device_id != own.device_id)
            .map(|m| (m.user_id.to_string(), m.device_id.clone()))
            .collect();
        let rotated = !self.recipients.is_subset(&next);
        if rotated {
            self.key = MediaKey::generate()?;
            self.index = self.index.wrapping_add(1);
        }
        let changed = next != self.recipients;
        self.recipients = next;
        Ok(MemberChange { changed, rotated })
    }
    pub async fn send(&self, room: &Room, own: &Member) -> Result<(), CallError> {
        // Recheck at send time. Never wildcard devices or room members who have
        // not joined this call. Concurrent departures cause a later rotation.
        let current = members(room).await?;
        let mut recipients = BTreeMap::new();
        for member in current {
            if self
                .recipients
                .contains(&(member.user_id.to_string(), member.device_id.clone()))
            {
                recipients
                    .entry(member.user_id)
                    .or_insert_with(Vec::new)
                    .push(matrix_sdk::ruma::to_device::DeviceIdOrAllDevices::DeviceId(
                        member.device_id.into(),
                    ));
            }
        }
        if recipients.is_empty() {
            return Ok(());
        }
        let content = Content {
            room_id: room.room_id().to_string(),
            member: KeyMember {
                claimed_device_id: own.device_id.clone(),
                id: Some(own.member_id.clone()),
            },
            session: Some(Session {
                call_id: String::new(),
                application: "m.call".into(),
                scope: "m.room".into(),
            }),
            media_key: Some(KeyEntry {
                key: STANDARD.encode(self.key.bytes()),
                index: self.index,
            }),
            member_id: Some(own.member_id.clone()),
            keys: Some(KeyEntry {
                key: STANDARD_NO_PAD.encode(self.key.bytes()),
                index: self.index,
            }),
            sent_ts: Some(now_ms()),
        };
        let failures = room
            .client()
            .send_encrypted_to_device(
                &KEY_EVENT.into(),
                recipients,
                Raw::new(&content)
                    .map_err(|_| CallError::KeyRejected)?
                    .cast_unchecked(),
            )
            .await
            .map_err(|_| CallError::KeyRejected)?;
        if failures.is_empty() {
            Ok(())
        } else {
            Err(CallError::KeyRejected)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn member(device: &str) -> Member {
        Member {
            expires_ms: None,
            user_id: matrix_sdk::ruma::user_id!("@a:example.org").to_owned(),
            device_id: device.into(),
            member_id: format!("@a:example.org:{device}"),
            created_ms: 1,
            service_url: None,
            alias: None,
        }
    }
    #[test]
    fn keys_rotate_only_after_departures_and_never_reuse_material() {
        let own = member("LOCAL");
        let a = member("A");
        let b = member("B");
        let mut keys = SenderKeys::new().unwrap();
        let initial = keys.key.0;
        let joined = keys.update_members(&own, &[a.clone(), b.clone()]).unwrap();
        assert!(joined.changed && !joined.rotated);
        assert!(
            keys.key.0 == initial,
            "joining preserves the current sender key generation"
        );
        let same = keys.update_members(&own, &[a.clone(), b.clone()]).unwrap();
        assert!(!same.changed && !same.rotated, "no resend without a change");
        let left = keys.update_members(&own, &[b]).unwrap();
        assert!(left.changed && left.rotated);
        assert!(keys.key.0 != initial, "departure rotates key material");
        assert_eq!(keys.index, 1);
        assert!(
            MediaKey::generate().unwrap().0 != keys.key.0,
            "independent keys differ"
        );
    }
    #[test]
    fn key_acceptance_binds_authenticated_device_room_call_and_membership() {
        use matrix_sdk::deserialized_responses::VerificationState;
        let a = member("A");
        let info = EncryptionInfo {
            sender: a.user_id.clone(),
            sender_device: Some("A".into()),
            forwarder: None,
            algorithm_info: AlgorithmInfo::OlmV1Curve25519AesSha2 {
                curve25519_public_key_base64: "authenticated-by-sdk".into(),
            },
            verification_state: VerificationState::Verified,
        };
        let mut content = Content {
            room_id: "!r:example.org".into(),
            member: KeyMember {
                claimed_device_id: "A".into(),
                id: Some(a.member_id.clone()),
            },
            session: Some(Session {
                call_id: "".into(),
                application: "m.call".into(),
                scope: "m.room".into(),
            }),
            media_key: None,
            member_id: None,
            keys: Some(KeyEntry {
                index: 0,
                key: STANDARD_NO_PAD.encode([7; 16]),
            }),
            sent_ts: Some(1000),
        };
        let check = |c: &Content, current: &[Member]| {
            accept(
                c,
                "@a:example.org",
                Some(&info),
                "!r:example.org",
                current,
                1000,
            )
            .is_some()
        };
        assert!(check(&content, std::slice::from_ref(&a)));
        content.keys.as_mut().unwrap().key = STANDARD.encode([7; 16]);
        assert!(
            check(&content, std::slice::from_ref(&a)),
            "Element Call RTCEncryptionManager uses padded base64"
        );
        assert!(!check(&content, &[]));
        assert!(
            accept(
                &content,
                "@a:example.org",
                None,
                "!r:example.org",
                std::slice::from_ref(&a),
                1000
            )
            .is_none(),
            "clear to-device key rejected"
        );
        let mut unknown = info.clone();
        unknown.sender_device = None;
        assert!(
            accept(
                &content,
                "@a:example.org",
                Some(&unknown),
                "!r:example.org",
                std::slice::from_ref(&a),
                1000
            )
            .is_none(),
            "unlinked Olm sender rejected"
        );
        content.sent_ts = Some(200_000);
        assert!(
            !check(&content, std::slice::from_ref(&a)),
            "future replay rejected"
        );
        content.sent_ts = Some(1000);
        content.member.id = Some("Element-per-join-UUID".into());
        assert!(
            check(&content, std::slice::from_ref(&a)),
            "state-mode keys bind authenticated device, not opaque UUID claim"
        );

        let mut modern:Content=serde_json::from_value(serde_json::json!({"room_id":"!r:example.org","member_id":a.member_id,"member":{"claimed_device_id":"A"},"media_key":{"index":1,"key":STANDARD.encode([7;16])}})).unwrap();
        assert!(
            check(&modern, std::slice::from_ref(&a)),
            "current Element keys omit session and sent_ts and may omit deprecated keys"
        );
        modern.member_id = Some("wrong-incarnation".into());
        assert!(!check(&modern, std::slice::from_ref(&a)));
        modern.session = Some(Session {
            call_id: String::new(),
            application: "m.call".into(),
            scope: "m.room".into(),
        });
        assert!(
            !check(&modern, std::slice::from_ref(&a)),
            "legacy fields cannot override an explicit wrong member_id"
        );
        content.member.claimed_device_id = "B".into();
        assert!(!check(&content, std::slice::from_ref(&a)));
        content.member.claimed_device_id = "A".into();
        content.session.as_mut().unwrap().call_id = "other-call".into();
        assert!(!check(&content, std::slice::from_ref(&a)));
        content.session.as_mut().unwrap().call_id.clear();
        content.room_id = "!other:example.org".into();
        assert!(!check(&content, &[a]));
    }
}
