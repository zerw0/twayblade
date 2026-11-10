//! Observable room summaries driven by the SDK's unified sync service.

use std::sync::Arc;

use futures_util::{Stream, StreamExt};
use matrix_sdk::Room;
use matrix_sdk_ui::room_list_service::filters;
use matrix_sdk_ui::sync_service::{State, SyncService};

use crate::auth::Account;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomSummary {
    pub id: String,
    pub name: String,
    pub unread: u64,
    pub is_space: bool,
    pub topic: Option<String>,
    /// Only `true` once the SDK has confirmed `m.room.encryption`.
    pub encrypted: bool,
    pub direct: bool,
    /// Server-provided activity ordering (sliding sync `bump_stamp`).
    pub recency: u64,
}

#[derive(Clone)]
pub struct RoomSnapshot {
    pub account_instance: u128,
    pub rooms: Vec<RoomSummary>,
    pub status: SyncStatus,
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
        unread: room.unread_notification_counts().notification_count,
        is_space: room.is_space(),
        topic: room.topic().filter(|topic| !topic.trim().is_empty()),
        encrypted: room.encryption_state().is_encrypted(),
        direct: room.direct_targets_length() > 0,
    }
}

// Dropping the UI subscription must stop both room and encryption sync tasks.
struct StopOnDrop(Arc<SyncService>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        let service = self.0.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move { service.stop().await });
        }
    }
}

impl Account {
    pub fn room_updates(&self) -> impl Stream<Item = RoomSnapshot> + Send + 'static + use<> {
        let account = self.clone();
        async_stream::stream! {
            let mut rooms = imbl::Vector::<RoomSummary>::new();
            let snapshot = |rooms: &imbl::Vector<RoomSummary>, status| RoomSnapshot {
                account_instance: account.instance_id(), rooms: rooms.iter().cloned().collect(), status,
            };
            yield snapshot(&rooms, SyncStatus::Connecting);
            let service = match SyncService::builder(account.client.clone())
                .with_offline_mode().build().await {
                Ok(service) => Arc::new(service),
                Err(_) => {
                    yield snapshot(&rooms, SyncStatus::Failed);
                    return;
                }
            };
            let _cleanup = StopOnDrop(service.clone());
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
            let initial = selection.borrow_and_update().clone();
            if let Some(id) = initial {
                service.room_list_service().set_room_subscriptions(&[id.as_ref()]).await;
            }
            service.start().await;
            let mut status = SyncStatus::Syncing;
            loop {
                yield snapshot(&rooms, status);
                tokio::select! {
                    result = selection.changed() => {
                        if result.is_err() { break; }
                        let selected = selection.borrow_and_update().clone();
                        let ids: Vec<_> = selected.iter().map(|id| id.as_ref()).collect();
                        service.room_list_service().set_room_subscriptions(&ids).await;
                    }
                    next = state.next() => {
                        status = match next {
                            Some(State::Running) => SyncStatus::Connected,
                            Some(State::Offline) => SyncStatus::Offline,
                            Some(State::Error(_)) => SyncStatus::Failed,
                            Some(State::Idle | State::Terminated) => SyncStatus::Stopped,
                            None => break,
                        };
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
