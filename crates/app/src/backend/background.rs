//! Saved accounts other than the open one (#97). Each keeps a room-list sync
//! for its unread counts and posts OS notifications; timelines, security,
//! presence and calls stay with the open account.
//!
//! Switching promotes an inactive account's client instead of opening its
//! store a second time, and demotes the open one, so an account's
//! `instance_id` (which notification clicks carry) survives the switch.
use super::*;
use std::collections::HashMap;
use twayblade_platform::notifications as os;

/// Inactive accounts by saved-account ID. Async, so the task that opens them
/// can hold it across `restore_account` and a switch waits for it.
pub(super) type Inactive = Arc<tokio::sync::Mutex<HashMap<String, Background>>>;

pub(super) struct Background {
    pub(super) account: Account,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for Background {
    fn drop(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
    }
}

/// Unread messages outside muted rooms, and mentions, over a room list.
pub(super) fn totals(rooms: &[twayblade_core::rooms::RoomSummary]) -> (u64, u64) {
    rooms
        .iter()
        .filter(|room| !room.is_space && !room.muted)
        .fold((0u64, 0u64), |(unread, mentions), room| {
            (
                unread.saturating_add(room.unread),
                mentions.saturating_add(room.mentions),
            )
        })
}

/// Writes one account's counts into the switcher entries; false when nothing
/// changed, so an idle account does not wake the window.
pub(super) fn set_counts(
    entries: &mut [AccountEntry],
    id: &str,
    instance: u128,
    (unread, mentions): (u64, u64),
) -> bool {
    let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) else {
        return false;
    };
    if entry.instance == instance && entry.unread == unread && entry.mentions == mentions {
        return false;
    }
    entry.instance = instance;
    entry.unread = unread;
    entry.mentions = mentions;
    true
}

impl Background {
    pub(super) fn start(
        account: Account,
        runtime: &tokio::runtime::Handle,
        updates: watch::Sender<Snapshot>,
        config: watch::Receiver<NotificationConfig>,
    ) -> Self {
        let (rules_sender, rules) = watch::channel(Default::default());
        let tasks = vec![
            runtime.spawn(rooms(account.clone(), updates.clone(), rules_sender)),
            runtime.spawn(notify(account.clone(), updates, config, rules)),
        ];
        Self { account, tasks }
    }
}

/// The room list drives the account's sync; its counts go to the switcher.
async fn rooms(
    account: Account,
    updates: watch::Sender<Snapshot>,
    rules: watch::Sender<twayblade_core::notifications::RulesSnapshot>,
) {
    let id = account.id();
    let instance = account.instance_id();
    let mut failures = 0u32;
    loop {
        let stream = account.room_updates();
        tokio::pin!(stream);
        while let Some(snapshot) = stream.next().await {
            rules.send_replace(snapshot.notifications.clone());
            let counts = totals(&snapshot.rooms);
            let changed = set_counts(
                &mut updates.borrow().accounts.clone(),
                &id,
                instance,
                counts,
            );
            if changed {
                publish(&updates, |view| {
                    set_counts(&mut view.accounts, &id, instance, counts);
                });
            }
        }
        if *account.session_state().borrow() != SessionState::Active {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1u64 << failures.min(5))).await;
        failures = failures.saturating_add(1);
    }
}

/// OS notifications for an account that is not open. Nothing is focused in
/// it, so every allowed message notifies; a click switches to the account.
async fn notify(
    account: Account,
    updates: watch::Sender<Snapshot>,
    mut config: watch::Receiver<NotificationConfig>,
    rules: watch::Receiver<twayblade_core::notifications::RulesSnapshot>,
) {
    use twayblade_core::notifications::{NotificationContext, RoomMode};
    let (context_sender, context) = watch::channel(NotificationContext::default());
    let stream = account.notification_updates(context);
    tokio::pin!(stream);
    let mut responses = tokio::task::JoinSet::<Option<os::Activation>>::new();
    loop {
        let current = config.borrow_and_update().clone();
        // The open account asks for permission; this one only reads it.
        let permission = os::permission().await;
        context_sender.send_replace(NotificationContext {
            enabled: current.device.enabled && permission == os::Permission::Granted,
            active: false,
            focused_room: None,
        });
        tokio::select! {
            changed = config.changed() => if changed.is_err() { return },
            Some(result) = responses.join_next(), if !responses.is_empty() => {
                if let Ok(Some(target)) = result {
                    notifications::publish_click(&updates, target);
                }
            }
            item = stream.next() => {
                let Some(item) = item else { return };
                let live = config.borrow().clone();
                if !live.device.enabled || rules.borrow().room_mode(&item.room) == RoomMode::Mute {
                    continue;
                }
                let (title, body) = notifications::presentation(&item, live.device.preview);
                let image = notifications::avatar(&account, &item, live.device.preview).await;
                let notification = os::Notification {
                    title,
                    body,
                    sound: live.device.sound,
                    image,
                    action_label: "Open",
                    target: os::Activation {
                        account: account.instance_id(),
                        room: item.room,
                        thread: item.thread,
                    },
                    // A cold launch opens the saved launch account, not this one.
                    launch: None,
                };
                if let Ok(pending) = os::notify(notification).await {
                    // Bounded like the open account's: the oldest waits are dropped.
                    if responses.len() >= 64 {
                        responses.abort_all();
                    }
                    let id = pending.id;
                    responses.spawn(async move {
                        let response = tokio::time::timeout(
                            std::time::Duration::from_secs(24 * 60 * 60),
                            pending.response,
                        )
                        .await;
                        if response.is_err() {
                            os::dismiss(&id).await;
                        }
                        response.ok().flatten().map(|response| response.target)
                    });
                }
            }
        }
    }
}

impl Backend {
    /// Takes an inactive account's client out of the background, waiting for
    /// a restore that is still opening it.
    pub(super) fn take_inactive(&self, id: &str) -> Option<Account> {
        let background = self.runtime.block_on(self.inactive.lock()).remove(id)?;
        let account = background.account.clone();
        release_in_runtime(&self.runtime, background);
        Some(account)
    }

    /// Keeps a client that stops being the open one running in the background.
    pub(super) fn demote(&self, account: Account) {
        if *account.session_state().borrow() != SessionState::Active {
            release_in_runtime(&self.runtime, account);
            return;
        }
        let id = account.id();
        let background = Background::start(
            account,
            self.runtime.handle(),
            self.updates.clone(),
            self.notification_config.subscribe(),
        );
        let replaced = self
            .runtime
            .block_on(self.inactive.lock())
            .insert(id, background);
        release_in_runtime(&self.runtime, replaced);
    }

    /// Opens every saved account except the open one in the background and
    /// drops any no longer saved. Runs as a task; the open account is read
    /// once the lock is held, so a switch in between is respected.
    pub(super) fn sync_inactive(&self) {
        let inactive = self.inactive.clone();
        let state = self.state.clone();
        let updates = self.updates.clone();
        let config = self.notification_config.clone();
        let runtime = self.runtime.handle().clone();
        self.runtime.spawn(async move {
            let mut accounts = inactive.lock().await;
            let open = state
                .lock()
                .ok()
                .and_then(|state| state.account.as_ref().map(Account::id));
            let saved = auth::saved_accounts().unwrap_or_default();
            accounts.retain(|id, _| {
                Some(id) != open.as_ref() && saved.iter().any(|saved| &saved.id == id)
            });
            for saved in saved {
                if Some(&saved.id) == open.as_ref() || accounts.contains_key(&saved.id) {
                    continue;
                }
                match auth::restore_account(&saved.id).await {
                    Ok(account) => {
                        let background = Background::start(
                            account,
                            &runtime,
                            updates.clone(),
                            config.subscribe(),
                        );
                        accounts.insert(saved.id, background);
                    }
                    Err(_) => {
                        tracing::warn!(name: "inactive_account_restore_failed", restored = false)
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{AccountEntry, set_counts, totals};
    use twayblade_core::rooms::RoomSummary;

    #[test]
    fn counts_skip_muted_rooms_and_spaces_and_publish_only_changes() {
        let room = |unread, mentions, muted, is_space| RoomSummary {
            unread,
            mentions,
            muted,
            is_space,
            ..Default::default()
        };
        let rooms = [
            room(3, 1, false, false),
            room(7, 2, true, false),
            room(4, 0, false, true),
            room(2, 0, false, false),
        ];
        assert_eq!(totals(&rooms), (5, 1));
        let mut entries = vec![
            AccountEntry {
                id: "a".into(),
                ..Default::default()
            },
            AccountEntry {
                id: "b".into(),
                ..Default::default()
            },
        ];
        assert!(set_counts(&mut entries, "b", 7, (5, 1)));
        assert_eq!(
            (entries[1].instance, entries[1].unread, entries[1].mentions),
            (7, 5, 1)
        );
        assert_eq!(entries[0].unread, 0, "other entries are untouched");
        assert!(
            !set_counts(&mut entries, "b", 7, (5, 1)),
            "unchanged counts do not publish"
        );
        assert!(
            !set_counts(&mut entries, "gone", 7, (1, 0)),
            "removed accounts are ignored"
        );
    }
}
