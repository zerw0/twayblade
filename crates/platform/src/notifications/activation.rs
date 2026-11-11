//! Relaunch metadata contains only non-secret identifiers, never message content.
#[cfg(target_os = "macos")]
use super::NotificationError;
use serde::{Deserialize, Serialize};
use std::{
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchActivation {
    pub identity: String,
    pub room: String,
    pub thread: Option<String>,
    pub issued_ms: u64,
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
impl LaunchActivation {
    pub fn new(identity: String, room: String, thread: Option<String>) -> Self {
        Self {
            identity,
            room,
            thread,
            issued_ms: now_ms(),
        }
    }
    pub fn valid(&self) -> bool {
        self.valid_at(now_ms())
    }
    fn valid_at(&self, now: u64) -> bool {
        !self.identity.is_empty()
            && self.identity.len() <= 4096
            && !self.room.is_empty()
            && self.room.len() <= 1024
            && self
                .thread
                .as_ref()
                .is_none_or(|r| !r.is_empty() && r.len() <= 1024)
            && self.issued_ms <= now.saturating_add(60_000)
            && now.saturating_sub(self.issued_ms) <= 24 * 60 * 60 * 1000
    }
    #[cfg(target_os = "macos")]
    pub(super) fn encode(&self) -> Result<String, NotificationError> {
        serde_json::to_string(self).map_err(|_| NotificationError)
    }
}
#[cfg(any(target_os = "macos", test))]
fn decode(payload: &str) -> Option<LaunchActivation> {
    if payload.len() > 8192 {
        return None;
    }
    let target: LaunchActivation = serde_json::from_str(payload).ok()?;
    target.valid().then_some(target)
}
fn queue() -> &'static watch::Sender<Option<LaunchActivation>> {
    static QUEUE: OnceLock<watch::Sender<Option<LaunchActivation>>> = OnceLock::new();
    QUEUE.get_or_init(|| watch::channel(None).0)
}
pub fn launch_activations() -> watch::Receiver<Option<LaunchActivation>> {
    queue().subscribe()
}
pub fn acknowledge_launch(target: &LaunchActivation) {
    queue().send_if_modified(|current| {
        if current.as_ref() == Some(target) {
            *current = None;
            true
        } else {
            false
        }
    });
}
/// Explicit logout clears deferred targets and macOS notifications retained from
/// earlier process instances (the application supports one account).
pub fn clear_launch_activation() {
    queue().send_replace(None);
    #[cfg(target_os = "macos")]
    super::macos::clear_delivered();
}
#[cfg(any(target_os = "macos", test))]
pub(super) fn receive_launch(payload: &str) {
    if let Some(target) = decode(payload) {
        queue().send_replace(Some(target));
    }
}
/// Install before application launch completion. Does not request permission.
pub fn initialize_activation() {
    #[cfg(target_os = "macos")]
    super::macos::initialize();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_metadata_round_trips_without_content_and_expires() {
        let target = LaunchActivation::new(
            "account-device".into(),
            "!r:example.org".into(),
            Some("$root".into()),
        );
        let payload = serde_json::to_string(&target).unwrap();
        assert!(decode(&payload).is_some());
        assert!(target.valid_at(target.issued_ms + 24 * 60 * 60 * 1000));
        assert!(!target.valid_at(target.issued_ms + 24 * 60 * 60 * 1000 + 1));
        assert!(!target.valid_at(target.issued_ms.saturating_sub(60_001)));
        assert!(decode(&"x".repeat(8193)).is_none());
        assert!(decode("{\"body\":\"private\"}").is_none());
    }
    #[test]
    fn cold_responses_wait_for_a_late_subscriber_and_old_ack_cannot_drop_a_new_click() {
        let first = LaunchActivation::new("first-device".into(), "!first:example.org".into(), None);
        receive_launch(&serde_json::to_string(&first).unwrap());
        let receiver = launch_activations();
        assert!(receiver.borrow().as_ref() == Some(&first));
        let second = LaunchActivation::new(
            "second-device".into(),
            "!second:example.org".into(),
            Some("$root".into()),
        );
        receive_launch(&serde_json::to_string(&second).unwrap());
        acknowledge_launch(&first);
        assert!(receiver.borrow().as_ref() == Some(&second));
        acknowledge_launch(&second);
        assert!(receiver.borrow().is_none());
    }
}
