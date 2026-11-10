//! Background application services for the GPUI frontend. No FFI dependency.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use matrix_client_core::{
    auth::{self, Account},
    conversation::Conversation,
};
use tokio::{runtime::Runtime, sync::watch, task::JoinHandle};
use zeroize::Zeroizing;

#[derive(Clone, Default)]
pub(super) struct Room {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) unread: u64,
    pub(super) is_space: bool,
}

#[derive(Clone, Default)]
pub(super) struct Space {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) level: u8,
    pub(super) room_ids: Vec<String>,
}

#[derive(Clone, Default)]
pub(super) struct Message {
    pub(super) id: String,
    pub(super) sender: String,
    pub(super) display_name: String,
    pub(super) body: String,
    pub(super) timestamp_ms: i64,
    pub(super) delivery: String,
}

#[derive(Clone)]
pub(super) struct Profile {
    pub(super) user_id: String,
    pub(super) display_name: String,
    pub(super) room_name: Option<String>,
    pub(super) room_display_name: Option<String>,
    pub(super) server_profile_available: bool,
}

#[derive(Clone, Default)]
pub(super) struct Snapshot {
    pub(super) revision: u64,
    pub(super) user_id: String,
    pub(super) rooms: Vec<Room>,
    pub(super) spaces: Vec<Space>,
    pub(super) spaces_loaded: bool,
    pub(super) selected_room: String,
    pub(super) messages: Vec<Message>,
    pub(super) timeline_ready: bool,
    pub(super) sync_status: String,
    pub(super) timeline_status: String,
}

pub(super) struct LoginOptions {
    pub(super) homeserver: String,
    pub(super) password: bool,
    pub(super) browser: bool,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum ServiceError {
    #[error("{message}")]
    Operation { message: String },
}

fn failure(message: impl ToString) -> ServiceError {
    ServiceError::Operation {
        message: message.to_string(),
    }
}

#[derive(Default)]
struct State {
    account: Option<Account>,
    conversation: Option<Conversation>,
    epoch: u64,
    selection: u64,
    tasks: Vec<JoinHandle<()>>,
    timeline_task: Option<JoinHandle<()>>,
}

impl State {
    fn stop(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
        if let Some(task) = self.timeline_task.take() {
            task.abort();
        }
        self.conversation = None;
        self.epoch = self.epoch.wrapping_add(1);
        self.selection = self.selection.wrapping_add(1);
    }
}

pub(super) struct Backend {
    runtime: Runtime,
    state: Arc<Mutex<State>>,
    updates: watch::Sender<Snapshot>,
    receiver: Mutex<watch::Receiver<Snapshot>>,
    // Serialize lifecycle changes. GPUI invokes operations only on background workers.
    lifecycle: Mutex<()>,
}

fn publish(sender: &watch::Sender<Snapshot>, update: impl FnOnce(&mut Snapshot)) {
    sender.send_modify(|snapshot| {
        update(snapshot);
        snapshot.revision = snapshot.revision.wrapping_add(1);
    });
}

impl Backend {
    pub(super) fn new() -> Result<Arc<Self>, ServiceError> {
        let runtime = Runtime::new().map_err(|_| failure("Background runtime could not start."))?;
        let (updates, receiver) = watch::channel(Snapshot::default());
        Ok(Arc::new(Self {
            runtime,
            updates,
            receiver: Mutex::new(receiver),
            state: Arc::new(Mutex::new(State::default())),
            lifecycle: Mutex::new(()),
        }))
    }

    pub(super) fn discover(&self, homeserver: String) -> Result<LoginOptions, ServiceError> {
        let discovery = self
            .runtime
            .block_on(matrix_client_core::discover_login(&homeserver))
            .map_err(failure)?;
        Ok(LoginOptions {
            homeserver: discovery.homeserver,
            password: discovery
                .legacy_flows
                .as_ref()
                .is_some_and(|flows| flows.iter().any(|flow| flow == "m.login.password")),
            browser: discovery.oauth_metadata_available
                || discovery
                    .legacy_flows
                    .as_ref()
                    .is_some_and(|flows| flows.iter().any(|flow| flow == "m.login.sso")),
        })
    }

    pub(super) fn restore(&self) -> Result<bool, ServiceError> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        if let Some(account) = self.runtime.block_on(auth::restore()).map_err(failure)? {
            self.connect(account)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(super) fn login(
        &self,
        homeserver: String,
        username: String,
        password: String,
    ) -> Result<(), ServiceError> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        let password = Zeroizing::new(password);
        let account = self
            .runtime
            .block_on(auth::login_password(homeserver, username, password))
            .map_err(failure)?;
        self.connect(account)
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        self.updates.borrow().clone()
    }

    /// Bounded long-poll on a background worker.
    /// watch coalesces updates, so there is no unbounded event queue.
    pub(super) fn next_snapshot(&self) -> Result<Snapshot, ServiceError> {
        let mut receiver = self
            .receiver
            .lock()
            .map_err(|_| failure("Updates are unavailable."))?;
        self.runtime.block_on(async {
            let _ = tokio::time::timeout(Duration::from_secs(1), receiver.changed()).await;
        });
        let snapshot = receiver.borrow_and_update().clone();
        Ok(snapshot)
    }

    pub(super) fn select_room(&self, room_id: String) -> Result<(), ServiceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        let account = state
            .account
            .clone()
            .ok_or_else(|| failure("Sign in first."))?;
        if let Some(task) = state.timeline_task.take() {
            task.abort();
        }
        state.conversation = None;
        state.selection = state.selection.wrapping_add(1);
        let selection = state.selection;
        let epoch = state.epoch;
        account.select_room(Some(&room_id));
        publish(&self.updates, |snapshot| {
            snapshot.selected_room = room_id.clone();
            snapshot.messages.clear();
            snapshot.timeline_ready = false;
            snapshot.timeline_status = "Loading messages…".into();
        });
        let shared = self.state.clone();
        let updates = self.updates.clone();
        state.timeline_task = Some(self.runtime.spawn(async move {
            let stream = account.conversation_updates(room_id, selection);
            tokio::pin!(stream);
            while let Some(snapshot) = stream.next().await {
                let Ok(mut state) = shared.lock() else {
                    return;
                };
                if state.epoch != epoch || state.selection != selection {
                    return;
                }
                state.conversation = snapshot.conversation;
                publish(&updates, |view| {
                    view.timeline_ready = state.conversation.is_some();
                    view.timeline_status = snapshot.status.into();
                    view.messages = snapshot
                        .messages
                        .into_iter()
                        .map(|message| Message {
                            id: message.id,
                            display_name: message
                                .display_name
                                .unwrap_or_else(|| message.sender.clone()),
                            sender: message.sender,
                            body: message.body,
                            timestamp_ms: message.timestamp_ms,
                            delivery: message.delivery.into(),
                        })
                        .collect();
                });
            }
        }));
        Ok(())
    }

    pub(super) fn profile(
        &self,
        user_id: String,
        room_id: Option<String>,
    ) -> Result<Profile, ServiceError> {
        let account = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?
            .account
            .clone()
            .ok_or_else(|| failure("Sign in first."))?;
        let profile = self
            .runtime
            .block_on(account.profile(user_id, room_id))
            .map_err(failure)?;
        let state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        if state.account.as_ref().map(Account::instance_id) != Some(account.instance_id()) {
            return Err(failure("The account changed. Open the profile again."));
        }
        Ok(Profile {
            user_id: profile.user_id,
            display_name: profile.display_name,
            room_name: profile.room_name,
            room_display_name: profile.room_display_name,
            server_profile_available: profile.server_profile_available,
        })
    }

    pub(super) fn clear_selection(&self) -> Result<(), ServiceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        if let Some(task) = state.timeline_task.take() {
            task.abort();
        }
        state.selection = state.selection.wrapping_add(1);
        state.conversation = None;
        if let Some(account) = &state.account {
            account.select_room(None);
        }
        publish(&self.updates, |snapshot| {
            snapshot.selected_room.clear();
            snapshot.messages.clear();
            snapshot.timeline_ready = false;
            snapshot.timeline_status.clear();
        });
        Ok(())
    }

    pub(super) fn send_text(&self, room_id: String, body: String) -> Result<(), ServiceError> {
        let conversation = {
            let state = self
                .state
                .lock()
                .map_err(|_| failure("Session is unavailable."))?;
            if self.updates.borrow().selected_room != room_id {
                return Err(failure("Conversation changed. Your draft was kept."));
            }
            state
                .conversation
                .clone()
                .ok_or_else(|| failure("Conversation is still loading."))?
        };
        self.runtime
            .block_on(conversation.send(body))
            .map_err(failure)
    }

    pub(super) fn load_earlier(&self, room_id: String) -> Result<bool, ServiceError> {
        let conversation = {
            let state = self
                .state
                .lock()
                .map_err(|_| failure("Session is unavailable."))?;
            if self.updates.borrow().selected_room != room_id {
                return Err(failure("Conversation changed."));
            }
            state
                .conversation
                .clone()
                .ok_or_else(|| failure("Conversation is still loading."))?
        };
        self.runtime.block_on(conversation.older()).map_err(failure)
    }

    pub(super) fn logout(&self) -> Result<(), ServiceError> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        let account = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?
            .account
            .clone();
        if let Some(account) = account {
            self.runtime.block_on(account.logout()).map_err(failure)?;
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        state.stop();
        state.account = None;
        publish(&self.updates, |snapshot| {
            let revision = snapshot.revision;
            *snapshot = Snapshot::default();
            snapshot.revision = revision;
        });
        Ok(())
    }

    pub(super) fn stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.stop();
        }
    }
}

impl Backend {
    fn connect(&self, account: Account) -> Result<(), ServiceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        state.stop();
        state.account = Some(account.clone());
        let epoch = state.epoch;
        publish(&self.updates, |snapshot| {
            let revision = snapshot.revision;
            *snapshot = Snapshot {
                revision,
                user_id: account.user_id(),
                sync_status: "Connecting…".into(),
                ..Default::default()
            };
        });
        let rooms_account = account.clone();
        let shared = self.state.clone();
        let updates = self.updates.clone();
        state.tasks.push(self.runtime.spawn(async move {
            let stream = rooms_account.room_updates();
            tokio::pin!(stream);
            while let Some(snapshot) = stream.next().await {
                let Ok(state) = shared.lock() else {
                    return;
                };
                if state.epoch != epoch {
                    return;
                }
                publish(&updates, |view| {
                    view.sync_status = snapshot.status.into();
                    view.rooms = snapshot
                        .rooms
                        .into_iter()
                        .map(|room| Room {
                            id: room.id,
                            name: room.name,
                            unread: room.unread,
                            is_space: room.is_space,
                        })
                        .collect();
                });
            }
        }));
        let shared = self.state.clone();
        let updates = self.updates.clone();
        state.tasks.push(self.runtime.spawn(async move {
            let stream = account.space_updates();
            tokio::pin!(stream);
            while let Some(snapshot) = stream.next().await {
                let Ok(state) = shared.lock() else {
                    return;
                };
                if state.epoch != epoch {
                    return;
                }
                publish(&updates, |view| {
                    view.spaces_loaded = true;
                    view.spaces = snapshot
                        .spaces
                        .into_iter()
                        .map(|space| Space {
                            id: space.id,
                            name: space.name,
                            level: space.level,
                            room_ids: space.room_ids.into_iter().collect(),
                        })
                        .collect();
                });
            }
        }));
        Ok(())
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            state.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unauthenticated_commands_do_not_create_or_publish_account_state() {
        let client = Backend::new().unwrap();
        assert!(client.select_room("!room:example.org".into()).is_err());
        assert!(
            client
                .send_text("!room:example.org".into(), "test".into())
                .is_err()
        );
        assert!(client.profile("@someone:example.org".into(), None).is_err());
        assert!(client.load_earlier("!room:example.org".into()).is_err());
        let snapshot = client.snapshot();
        assert!(snapshot.user_id.is_empty());
        assert!(snapshot.selected_room.is_empty());
        assert!(snapshot.messages.is_empty());
        assert_eq!(snapshot.revision, 0);
    }
}
