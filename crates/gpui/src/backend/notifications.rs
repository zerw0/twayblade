use super::*;
use crate::preferences::DeviceNotifications;
struct Cleanup {
    account: u128,
    shared: Arc<Mutex<State>>,
    ids: Arc<Mutex<Vec<String>>>,
    updates: watch::Sender<Snapshot>,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        let ids = self.ids.lock().map(|ids| ids.clone()).unwrap_or_default();
        let updates = self.updates.clone();
        let account = self.account;
        let shared = self.shared.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let signed_out = {
                    let snapshot = updates.borrow();
                    let account_gone = shared
                        .lock()
                        .map(|state| {
                            state.account.as_ref().map(Account::instance_id) != Some(account)
                        })
                        .unwrap_or(true);
                    snapshot.user_id.is_empty()
                        || account_gone
                        || snapshot.account_instance != account
                        || snapshot.session_state != SessionState::Active
                };
                if signed_out {
                    for id in ids {
                        os::dismiss(&id).await;
                    }
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
    let mut launches = os::launch_activations();
    let mut activation_tick = tokio::time::interval(std::time::Duration::from_secs(1));
    let mut permission = os::permission().await;
    let mut responses = tokio::task::JoinSet::<Option<os::Activation>>::new();
    let mut response_order =
        std::collections::VecDeque::<(String, tokio::task::AbortHandle)>::new();
    let cleanup = Cleanup {
        account: account.instance_id(),
        shared: shared.clone(),
        ids: Default::default(),
        updates: updates.clone(),
    };
    let mut last_badge = None;
    let mut previous = NotificationConfig::default();
    let mut launch_started: Option<(os::LaunchActivation, std::time::Instant)> = None;
    loop {
        let launch = launches.borrow_and_update().clone();
        if let Some(target) = launch {
            if launch_started
                .as_ref()
                .is_none_or(|(previous, _)| previous != &target)
            {
                launch_started = Some((target.clone(), std::time::Instant::now()));
            }
            let timed_out = launch_started
                .as_ref()
                .is_some_and(|(_, start)| start.elapsed() > std::time::Duration::from_secs(60));
            use matrix_client_core::notifications::ActivationState;
            let validation = account.validate_notification_target(
                &target.identity,
                &target.room,
                target.thread.as_deref(),
            );
            if timed_out || !target.valid() || validation == ActivationState::Rejected {
                os::acknowledge_launch(&target);
            } else if validation == ActivationState::Ready
                && let Ok(state) = shared.lock()
                && state.epoch == epoch
            {
                let known = updates
                    .borrow()
                    .rooms
                    .iter()
                    .any(|room| room.id == target.room);
                if known {
                    publish_click(
                        &updates,
                        os::Activation {
                            account: account.instance_id(),
                            room: target.room.clone(),
                            thread: target.thread.clone(),
                        },
                    );
                    os::acknowledge_launch(&target);
                }
            }
        }
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
            _ = activation_tick.tick() => {},
            changed = launches.changed() => { if changed.is_err() { break; } },
            changed = config.changed() => { if changed.is_err() { break; } }
            Some(result) = responses.join_next(), if !responses.is_empty() => {
                if let Ok(Some(target)) = result
                    && let Ok(state) = shared.lock() && state.epoch == epoch
                        && target.account == account.instance_id()
                        && account.validate_notification_target(&account.notification_identity(),&target.room,target.thread.as_deref()) == matrix_client_core::notifications::ActivationState::Ready {
                        publish_click(&updates,target);
                }
            }
            item = stream.next() => {
                let Some(item) = item else { publish_state(permission, true); break; };
                let live = config.borrow().clone();
                if !matrix_client_core::notifications::should_notify(false, false, &item.room, &live.context) || !live.device.enabled { continue; }
                if updates.borrow().notifications.room_mode(&item.room) == matrix_client_core::notifications::RoomMode::Mute { continue; }
                let (title, body) = presentation(&item, live.device.preview);
                let launch = os::LaunchActivation::new(account.notification_identity(),item.room.clone(),item.thread.clone());
                let notification = os::Notification { title, body, sound: live.device.sound, action_label: "Open", target: os::Activation { account: account.instance_id(), room: item.room, thread: item.thread },launch:Some(launch) };
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

fn publish_click(updates: &watch::Sender<Snapshot>, target: os::Activation) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    publish(updates, |view| {
        view.notification_click = Some((serial, target))
    });
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
