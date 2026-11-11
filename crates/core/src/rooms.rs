//! Observable room summaries driven by the SDK's unified sync service.

use std::sync::Arc;

use futures_util::{Stream, StreamExt};
use matrix_sdk::{Room, ruma::RoomId};
use matrix_sdk_ui::room_list_service::filters;
use matrix_sdk_ui::sync_service::{State, SyncService};

use crate::auth::Account;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomSummary {
    pub id: String,
    pub name: String,
    pub unread: u64,
    pub muted: bool,
    pub is_space: bool,
    pub topic: Option<String>,
    /// Only `true` once the SDK has confirmed `m.room.encryption`.
    pub encrypted: bool,
    pub direct: bool,
    pub partner: Option<String>,
    pub presence: Option<crate::presence::Presence>,
    /// `mxc://` avatar, if the room has one.
    pub avatar: Option<String>,
    /// Highlight (mention/keyword) count, shown as an accent badge.
    pub mentions: u64,
    /// Anyone can join; otherwise shown with a lock.
    pub public: bool,
    /// Tagged `m.favourite` (syncs across Matrix clients).
    pub favourite: bool,
    pub pinned: usize,
    /// Server-provided activity ordering (sliding sync `bump_stamp`).
    pub recency: u64,
}

#[derive(Clone)]
pub struct RoomSnapshot {
    pub account_instance: u128,
    pub rooms: Vec<RoomSummary>,
    pub invites: Vec<crate::space_actions::Invite>,
    pub status: SyncStatus,
    pub notifications: crate::notifications::RulesSnapshot,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    Connecting,
    Syncing,
    Connected,
    Offline,
    Failed,
    Stopped,
}

// Project only changed SDK entries; the service owns membership, recency and sorting.
fn summary(room: Room) -> RoomSummary {
    RoomSummary {
        recency: room.recency_stamp().map(u64::from).unwrap_or(0),
        id: room.room_id().to_string(),
        name: room
            .cached_display_name()
            .map(|name| name.to_string())
            .or_else(|| room.name())
            .unwrap_or_else(|| room.room_id().to_string()),
        // Native sync may omit/lag the server total. Keep that total when it
        // is higher, and use SDK-computed notifications as a fallback; never
        // add per-thread summaries to it a second time.
        unread: room
            .unread_notification_counts()
            .notification_count
            .max(room.num_unread_notifications()),
        muted: false,
        is_space: room.is_space(),
        topic: room.topic().filter(|topic| !topic.trim().is_empty()),
        encrypted: room.encryption_state().is_encrypted(),
        direct: room.direct_targets_length() > 0,
        partner: {
            let targets = room.direct_targets();
            (targets.len() == 1).then(|| targets.into_iter().next().unwrap().to_string())
        },
        presence: None,
        avatar: room.avatar_url().map(|url| url.to_string()),
        mentions: room.unread_notification_counts().highlight_count,
        public: room.is_public().unwrap_or(false),
        favourite: room.is_favourite(),
        pinned: room.pinned_event_ids().map_or(0, |ids| ids.len()),
    }
}

// Dropping the UI subscription must stop both room and encryption sync tasks.
struct StopOnDrop(
    Arc<SyncService>,
    tokio::sync::watch::Sender<Option<Arc<SyncService>>>,
);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.1.send_replace(None);
        let service = self.0.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move { service.stop().await });
        }
    }
}

pub(crate) struct RoomSyncLease {
    account: Account,
    rooms: Vec<matrix_sdk::ruma::OwnedRoomId>,
}
impl Drop for RoomSyncLease {
    fn drop(&mut self) {
        if let Ok(mut requests) = self.account.sync_requests.lock() {
            for id in &self.rooms {
                if let Some(count) = requests.get_mut(id) {
                    *count -= 1;
                    if *count == 0 {
                        requests.remove(id);
                    }
                }
            }
            self.account
                .sync_request_updates
                .send_replace(requests.keys().cloned().collect());
        }
    }
}

impl Account {
    /// Temporarily subscribe operations to the required SDK room state without changing
    /// the frontend selection. Overlapping operations share reference counts.
    pub(crate) async fn sync_rooms(
        &self,
        rooms: Vec<matrix_sdk::ruma::OwnedRoomId>,
    ) -> Result<RoomSyncLease, ()> {
        let rooms: Vec<_> = rooms
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        {
            let mut requests = self.sync_requests.lock().map_err(|_| ())?;
            for id in &rooms {
                *requests.entry(id.clone()).or_default() += 1;
            }
            self.sync_request_updates
                .send_replace(requests.keys().cloned().collect());
        }
        let lease = RoomSyncLease {
            account: self.clone(),
            rooms,
        };
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            for id in &lease.rooms {
                let room = self.client.get_room(id).ok_or(())?;
                // Native sliding sync is intentionally partial: `sync_up()`
                // waits for all room state and cannot complete on this path.
                // Wait only for the SDK records needed for permissions/routing.
                loop {
                    if room.create_content().is_some()
                        && room.power_levels().await.is_ok()
                        && room
                            .get_member_no_sync(room.own_user_id())
                            .await
                            .ok()
                            .flatten()
                            .is_some()
                    {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
            Ok::<_, ()>(())
        })
        .await
        .map_err(|_| ())??;
        Ok(lease)
    }

    pub fn room_updates(&self) -> impl Stream<Item = RoomSnapshot> + Send + 'static + use<> {
        let account = self.clone();
        async_stream::stream! {
            let mut session_state = account.session_state();
            let mut rooms = imbl::Vector::<RoomSummary>::new();
            let snapshot = |rooms: &imbl::Vector<RoomSummary>, status| RoomSnapshot {
                account_instance: account.instance_id(), rooms: rooms.iter().cloned().collect(), invites: Vec::new(), status,
                notifications: Default::default(),
            };
            if *session_state.borrow_and_update() != crate::auth::SessionState::Active {
                yield snapshot(&rooms, SyncStatus::Stopped);
                return;
            }
            yield snapshot(&rooms, SyncStatus::Connecting);
            let service = match SyncService::builder(account.client.clone())
                .with_offline_mode().build().await {
                Ok(service) => Arc::new(service),
                Err(_) => {
                    yield snapshot(&rooms, SyncStatus::Failed);
                    return;
                }
            };
            account.notification_service.send_replace(Some(service.clone()));
            let _cleanup = StopOnDrop(service.clone(), account.notification_service.clone());
            let mut state = Box::pin(service.state());
            let list = match service.room_list_service().all_rooms().await {
                Ok(list) => list,
                Err(_) => { yield snapshot(&rooms, SyncStatus::Failed); return; }
            };
            // No frontend list cap: the SDK still fetches sliding-sync pages.
            let (changes, controller) = list.entries_with_dynamic_adapters(usize::MAX);
            controller.set_filter(Box::new(filters::new_filter_joined()));
            let mut changes = Box::pin(changes);
            let mut selection = account.selected_room.subscribe();
            let mut all_updates = account.client.subscribe_to_all_room_updates();
            let mut requested = account.sync_request_updates.subscribe();
            let mut initial = requested.borrow_and_update().clone();
            initial.extend(selection.borrow_and_update().clone());
            let ids: Vec<_> = initial.iter().map(|id| id.as_ref()).collect();
            service.room_list_service().set_room_subscriptions(&ids).await;
            service.start().await;
            let mut status = SyncStatus::Syncing;
            let rules = account.notification_rules().await.ok();
            let mut rule_changes = rules.as_ref().map(|rules| rules.subscribe());
            let mut rule_snapshot = match &rules { Some(rules) => rules.snapshot().await, None => Default::default() };
            // The SDK stops for good after an error or termination; restart it
            // while the session is active, with capped exponential backoff.
            let mut restart: Option<std::pin::Pin<Box<tokio::time::Sleep>>> = None;
            let mut failures = 0u32;
            loop {
                let mut current = snapshot(&rooms, status);
                current.notifications = rule_snapshot.clone();
                for room in &mut current.rooms {
                    room.muted = rule_snapshot.room_mode(&room.id) == crate::notifications::RoomMode::Mute;
                    if room.muted { room.unread = 0; }
                }
                current.invites = account.invites().await;
                yield current;
                tokio::select! {
                    _ = async { match &mut rule_changes { Some(changes) => { let _ = changes.recv().await; }, None => std::future::pending().await } } => {
                        if let Some(rules) = &rules { rule_snapshot = rules.snapshot().await; }
                    }
                    change = all_updates.recv() => {
                        let changed = match change {
                            Ok(change) => Some(change.iter_all_room_ids().map(|id| id.to_string()).collect::<std::collections::HashSet<_>>()),
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => None,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        };
                        // Entry identities/order may be unchanged while SDK
                        // unread/profile state changes behind their handles.
                        for cached in rooms.iter_mut() {
                            if changed.as_ref().is_none_or(|ids| ids.contains(&cached.id))
                                && let Ok(id) = RoomId::parse(&cached.id)
                                && let Some(room) = account.client.get_room(&id) {
                                    *cached = summary(room);
                                }
                        }
                    }
                    _ = session_state.changed() => {
                        if *session_state.borrow_and_update() != crate::auth::SessionState::Active {
                            service.stop().await;
                            yield snapshot(&rooms, SyncStatus::Stopped);
                            break;
                        }
                    }
                    result = selection.changed() => {
                        if result.is_err() { break; }
                        let selected = selection.borrow_and_update().clone();
                        let mut subscriptions = requested.borrow().clone(); subscriptions.extend(selected);
                        let ids: Vec<_> = subscriptions.iter().map(|id| id.as_ref()).collect();
                        service.room_list_service().set_room_subscriptions(&ids).await;
                    }
                    change = requested.changed() => {
                        if change.is_err() { break; }
                        let mut subscriptions = requested.borrow_and_update().clone(); subscriptions.extend(selection.borrow().clone());
                        let ids: Vec<_> = subscriptions.iter().map(|id| id.as_ref()).collect();
                        service.room_list_service().set_room_subscriptions(&ids).await;
                    }
                    next = state.next() => {
                        status = match next {
                            Some(State::Running) => { failures = 0; restart = None; SyncStatus::Connected }
                            Some(State::Offline) => SyncStatus::Offline,
                            Some(State::Error(_) | State::Idle | State::Terminated) => {
                                if restart.is_none() {
                                    restart = Some(Box::pin(tokio::time::sleep(restart_delay(failures))));
                                    failures = failures.saturating_add(1);
                                }
                                SyncStatus::Offline
                            }
                            None => break,
                        };
                    }
                    _ = async { match &mut restart { Some(sleep) => sleep.as_mut().await, None => std::future::pending().await } } => {
                        restart = None;
                        status = SyncStatus::Connecting;
                        service.start().await;
                    }
                    next = changes.next() => {
                        let Some(diffs) = next else { break; };
                        for diff in diffs { diff.map(|entry| summary(entry.into_inner())).apply(&mut rooms); }
                    }

                }
            }
        }
    }
}

/// 1 s, doubling up to 60 s.
fn restart_delay(failures: u32) -> std::time::Duration {
    std::time::Duration::from_secs(1u64 << failures.min(6)).min(std::time::Duration::from_secs(60))
}

impl Account {
    /// Set or clear the `m.favourite` tag; the room list updates through sync.
    pub async fn set_favourite(&self, room_id: &str, favourite: bool) -> Result<(), RoomError> {
        let room = matrix_sdk::ruma::RoomId::parse(room_id)
            .ok()
            .and_then(|id| self.client.get_room(&id))
            .ok_or(RoomError::Unavailable)?;
        room.set_is_favourite(favourite, None)
            .await
            .map_err(|_| RoomError::Tag)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RoomError {
    #[error("room_unavailable")]
    Unavailable,
    #[error("room_tag_failed")]
    Tag,
}

#[cfg(test)]
mod restart_tests {
    use super::restart_delay;
    use std::time::Duration;
    #[test]
    fn sync_restart_backs_off_exponentially_and_caps() {
        let delays: Vec<_> = (0..9).map(restart_delay).collect();
        assert_eq!(delays[0], Duration::from_secs(1));
        assert_eq!(delays[1], Duration::from_secs(2));
        assert_eq!(delays[5], Duration::from_secs(32));
        assert!(delays[6..].iter().all(|d| *d == Duration::from_secs(60)));
        assert_eq!(restart_delay(u32::MAX), Duration::from_secs(60));
    }
}
