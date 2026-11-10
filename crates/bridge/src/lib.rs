//! Narrow native UI bridge. Tokens, SDK objects and crypto stores stay in Rust.
#![recursion_limit = "512"]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use matrix_client_core::{
    auth::{self, Account},
    conversation::Conversation,
};
use tokio::{runtime::Runtime, sync::watch, task::JoinHandle};
use zeroize::Zeroizing;

uniffi::setup_scaffolding!();

#[derive(Clone, Default, uniffi::Record)]
pub struct NativeRoom {
    pub id: String,
    pub name: String,
    pub unread: u64,
    pub is_space: bool,
}

#[derive(Clone, Default, uniffi::Record)]
pub struct NativeSpace {
    pub id: String,
    pub name: String,
    pub level: u8,
    pub room_ids: Vec<String>,
}

#[derive(Clone, Default, uniffi::Record)]
pub struct NativeMessage {
    pub sender: String,
    pub display_name: String,
    pub body: String,
    pub timestamp_ms: i64,
    pub delivery: String,
}

#[derive(Clone, uniffi::Record)]
pub struct NativeProfile {
    pub user_id: String,
    pub display_name: String,
    pub room_name: Option<String>,
    pub room_display_name: Option<String>,
    pub server_profile_available: bool,
}

#[derive(Clone, Default, uniffi::Record)]
pub struct NativeSnapshot {
    pub revision: u64,
    pub user_id: String,
    pub rooms: Vec<NativeRoom>,
    pub spaces: Vec<NativeSpace>,
    pub spaces_loaded: bool,
    pub selected_room: String,
    pub messages: Vec<NativeMessage>,
    pub timeline_ready: bool,
    pub sync_status: String,
    pub timeline_status: String,
}

#[derive(uniffi::Record)]
pub struct NativeLoginOptions {
    pub homeserver: String,
    pub password: bool,
    pub browser: bool,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum BridgeError {
    #[error("{message}")]
    Operation { message: String },
}

fn failure(message: impl ToString) -> BridgeError {
    BridgeError::Operation {
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

#[derive(uniffi::Object)]
pub struct NativeClient {
    runtime: Runtime,
    state: Arc<Mutex<State>>,
    updates: watch::Sender<NativeSnapshot>,
    receiver: Mutex<watch::Receiver<NativeSnapshot>>,
    // Serialize lifecycle changes. Swift invokes blocking exports off the main actor.
    lifecycle: Mutex<()>,
}

fn publish(sender: &watch::Sender<NativeSnapshot>, update: impl FnOnce(&mut NativeSnapshot)) {
    sender.send_modify(|snapshot| {
        update(snapshot);
        snapshot.revision = snapshot.revision.wrapping_add(1);
    });
}

#[uniffi::export]
impl NativeClient {
    #[uniffi::constructor]
    pub fn new() -> Result<Arc<Self>, BridgeError> {
        let runtime = Runtime::new().map_err(|_| failure("Background runtime could not start."))?;
        let (updates, receiver) = watch::channel(NativeSnapshot::default());
        Ok(Arc::new(Self {
            runtime,
            updates,
            receiver: Mutex::new(receiver),
            state: Arc::new(Mutex::new(State::default())),
            lifecycle: Mutex::new(()),
        }))
    }

    pub fn discover(&self, homeserver: String) -> Result<NativeLoginOptions, BridgeError> {
        let discovery = self
            .runtime
            .block_on(matrix_client_core::discover_login(&homeserver))
            .map_err(failure)?;
        Ok(NativeLoginOptions {
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

    pub fn restore(&self) -> Result<bool, BridgeError> {
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

    pub fn login(
        &self,
        homeserver: String,
        username: String,
        password: String,
    ) -> Result<(), BridgeError> {
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

    pub fn snapshot(&self) -> NativeSnapshot {
        self.updates.borrow().clone()
    }

    /// Bounded long-poll on a Swift background task, not the main actor.
    /// watch coalesces updates, so there is no unbounded event queue.
    pub fn next_snapshot(&self) -> Result<NativeSnapshot, BridgeError> {
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

    pub fn select_room(&self, room_id: String) -> Result<(), BridgeError> {
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
                        .map(|message| NativeMessage {
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

    pub fn profile(
        &self,
        user_id: String,
        room_id: Option<String>,
    ) -> Result<NativeProfile, BridgeError> {
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
        Ok(NativeProfile {
            user_id: profile.user_id,
            display_name: profile.display_name,
            room_name: profile.room_name,
            room_display_name: profile.room_display_name,
            server_profile_available: profile.server_profile_available,
        })
    }

    pub fn send_text(&self, room_id: String, body: String) -> Result<(), BridgeError> {
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

    pub fn load_earlier(&self, room_id: String) -> Result<bool, BridgeError> {
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

    pub fn logout(&self) -> Result<(), BridgeError> {
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
            *snapshot = NativeSnapshot::default();
            snapshot.revision = revision;
        });
        Ok(())
    }

    pub fn stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.stop();
        }
    }
}

impl NativeClient {
    fn connect(&self, account: Account) -> Result<(), BridgeError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        state.stop();
        state.account = Some(account.clone());
        let epoch = state.epoch;
        publish(&self.updates, |snapshot| {
            let revision = snapshot.revision;
            *snapshot = NativeSnapshot {
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
                        .map(|room| NativeRoom {
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
                        .map(|space| NativeSpace {
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

impl Drop for NativeClient {
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
        let client = NativeClient::new().unwrap();
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
