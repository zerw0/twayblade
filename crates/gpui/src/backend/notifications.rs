use super::*;
use crate::preferences::DeviceNotifications;
struct Cleanup {
    ids: Arc<Mutex<Vec<String>>>,
    updates: watch::Sender<Snapshot>,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        let ids = self.ids.lock().map(|ids| ids.clone()).unwrap_or_default();
        let updates = self.updates.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                for id in ids {
                    os::dismiss(&id).await;
                }
                let signed_out = {
                    let snapshot = updates.borrow();
                    snapshot.user_id.is_empty() || snapshot.session_state != SessionState::Active
                };
                if signed_out {
                    let _ = os::set_badge(0).await;
                }
            });
        }
    }
}
use matrix_client_core::notifications::{NotificationContext, NotificationItem};
use matrix_client_platform::notifications as os;

#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct NotificationConfig {
    pub device: DeviceNotifications,
    pub context: NotificationContext,
    pub unread: u64,
}
impl Backend {
    pub(crate) fn set_notification_config(&self, config: NotificationConfig) {
        self.notification_config.send_if_modified(|current| {
            if *current == config {
                false
            } else {
                *current = config;
                true
            }
        });
    }
    pub(crate) fn open_notification_settings(&self) -> Result<(), ServiceError> {
        os::open_settings()
            .map_err(|_| failure("System notification settings could not be opened."))
    }
}

pub(super) fn presentation(item: &NotificationItem, preview: bool) -> (String, String) {
    let display = item.preview(preview);
    match (display.sender, display.body) {
        (Some(sender), Some(body)) => (format!("{sender} · {}", item.room_name), body),
        _ => ("Matrix Client".into(), "New message".into()),
    }
}

pub(super) async fn run(
    account: Account,
    epoch: u64,
    shared: Arc<Mutex<State>>,
    updates: watch::Sender<Snapshot>,
    mut config: watch::Receiver<NotificationConfig>,
) {
    let (context_sender, context) = watch::channel(NotificationContext::default());
    let stream = account.notification_updates(context);
    tokio::pin!(stream);
    let mut permission = os::permission().await;
    let mut responses = tokio::task::JoinSet::new();
    let mut response_order =
        std::collections::VecDeque::<(String, tokio::task::AbortHandle)>::new();
    let cleanup = Cleanup {
        ids: Default::default(),
        updates: updates.clone(),
    };
    let mut last_badge = None;
    let mut previous = NotificationConfig::default();
    loop {
        let current = config.borrow_and_update().clone();
        if current.device.enabled && permission == os::Permission::NotRequested {
            permission = os::request_permission().await;
        } else if current.context.active && !previous.context.active {
            permission = os::permission().await;
        }
        let publish_state = |permission, failed| {
            if let Ok(state) = shared.lock()
                && state.epoch == epoch
            {
                publish(&updates, |snapshot| {
                    snapshot.notification_permission = permission;
                    snapshot.notification_failed = failed;
                });
            }
        };
        let badge = if current.device.badge {
            current.unread
        } else {
            0
        };
        if last_badge != Some(badge) {
            let _ = os::set_badge(badge).await;
            last_badge = Some(badge);
        }
        let mut context = current.context.clone();
        context.enabled = current.device.enabled && permission == os::Permission::Granted;
        context_sender.send_replace(context);
        if current != previous || updates.borrow().notification_permission != permission {
            publish_state(permission, false);
        }
        previous = current.clone();
        tokio::select! {
            changed = config.changed() => { if changed.is_err() { break; } }
            Some(result) = responses.join_next(), if !responses.is_empty() => {
                if let Ok(Some(target)) = result {
                    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
                    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if let Ok(state) = shared.lock() && state.epoch == epoch {
                        publish(&updates, |snapshot| snapshot.notification_click = Some((serial, target)));
                    }
                }
            }
            item = stream.next() => {
                let Some(item) = item else { publish_state(permission, true); break; };
                let live = config.borrow().clone();
                if !matrix_client_core::notifications::should_notify(false, false, &item.room, &live.context) || !live.device.enabled { continue; }
                if updates.borrow().notifications.room_mode(&item.room) == matrix_client_core::notifications::RoomMode::Mute { continue; }
                let (title, body) = presentation(&item, live.device.preview);
                let notification = os::Notification { title, body, sound: live.device.sound, action_label: "Open", target: os::Activation { account: account.instance_id(), room: item.room, thread: item.thread } };
                match os::notify(notification).await {
                    Ok(pending) => {
                        response_order.retain(|(_, handle)| !handle.is_finished());
                        if response_order.len() >= 64 && let Some((id, handle)) = response_order.pop_front() { handle.abort(); os::dismiss(&id).await; }
                        let id = pending.id;
                        let expired = id.clone();
                        let abort = responses.spawn(async move {
                            let response = tokio::time::timeout(std::time::Duration::from_secs(24 * 60 * 60), pending.response).await;
                            if response.is_err() { os::dismiss(&expired).await; }
                            response.ok().flatten()
                        });
                        response_order.push_back((id, abort));
                        if let Ok(mut ids) = cleanup.ids.lock() { *ids = response_order.iter().map(|(id, _)| id.clone()).collect(); }
                    }
                    Err(_) => publish_state(permission, true),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_presentation_contains_no_sender_or_message() {
        let item = NotificationItem {
            room: "room".into(),
            room_name: "Sample room".into(),
            event: "event".into(),
            thread: None,
            sender: "sender".into(),
            sender_name: "Sample sender".into(),
            sender_avatar: None,
            body: "Sample body".into(),
            direct: true,
            mention: false,
        };
        assert_eq!(
            presentation(&item, false),
            ("Matrix Client".into(), "New message".into())
        );
        assert!(presentation(&item, true).0.contains("Sample sender"));
        assert_eq!(presentation(&item, true).1, "Sample body");
    }
}
