//! Observable room summaries driven by the SDK's unified sync service.

use std::sync::Arc;

use futures_util::{Stream, StreamExt};
use matrix_sdk::RoomState;
use matrix_sdk_ui::sync_service::{State, SyncService};

use crate::auth::Account;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomSummary {
    pub id: String,
    pub name: String,
    pub unread: u64,
    pub is_space: bool,
}

#[derive(Clone)]
pub struct RoomSnapshot {
    pub account_instance: u128,
    pub rooms: Vec<RoomSummary>,
    pub status: &'static str,
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
    fn room_snapshot(&self, status: &'static str) -> RoomSnapshot {
        let mut rooms: Vec<_> = self
            .client
            .rooms()
            .into_iter()
            .filter(|room| room.state() == RoomState::Joined)
            .map(|room| RoomSummary {
                id: room.room_id().to_string(),
                name: room
                    .cached_display_name()
                    .map(|name| name.to_string())
                    .or_else(|| room.name())
                    .unwrap_or_else(|| "Conversation".into()),
                unread: room.unread_notification_counts().notification_count,
                is_space: room.is_space(),
            })
            .collect();
        rooms.sort_by_cached_key(|room| (room.name.to_lowercase(), room.id.clone()));
        RoomSnapshot {
            account_instance: self.instance_id(),
            rooms,
            status,
        }
    }

    pub fn room_updates(&self) -> impl Stream<Item = RoomSnapshot> + Send + 'static + use<> {
        let account = self.clone();
        async_stream::stream! {
            yield account.room_snapshot("Connecting…");
            let service = match SyncService::builder(account.client.clone())
                .with_offline_mode().build().await {
                Ok(service) => Arc::new(service),
                Err(_) => {
                    yield account.room_snapshot("Unable to start synchronization");
                    return;
                }
            };
            let _cleanup = StopOnDrop(service.clone());
            let mut state = Box::pin(service.state());
            let (_, changes) = account.client.rooms_stream();
            let mut changes = Box::pin(changes);
            let mut notable = account.client.room_info_notable_update_receiver();
            let mut selection = account.selected_room.subscribe();
            let initial = selection.borrow_and_update().clone();
            if let Some(id) = initial {
                service.room_list_service().set_room_subscriptions(&[id.as_ref()]).await;
            }
            service.start().await;
            let mut status = "Syncing conversations…";
            loop {
                yield account.room_snapshot(status);
                tokio::select! {
                    result = selection.changed() => {
                        if result.is_err() { break; }
                        let selected = selection.borrow_and_update().clone();
                        let ids: Vec<_> = selected.iter().map(|id| id.as_ref()).collect();
                        service.room_list_service().set_room_subscriptions(&ids).await;
                    }
                    next = state.next() => {
                        status = match next {
                            Some(State::Running) => "Connected · syncing",
                            Some(State::Offline) => "Offline · reconnecting…",
                            Some(State::Error(_)) => "Synchronization failed · reconnect to retry",
                            Some(State::Idle | State::Terminated) => "Synchronization stopped",
                            None => break,
                        };
                    }
                    next = changes.next() => {
                        if next.is_none() { break; }
                    }
                    result = notable.recv() => {
                        if matches!(result, Err(tokio::sync::broadcast::error::RecvError::Closed)) {
                            break;
                        }
                    }
                }
            }
        }
    }
}
