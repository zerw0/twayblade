//! Background application services for the GPUI frontend. No FFI dependency.

use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use matrix_client_core::{
    auth::{self, Account},
    conversation::Conversation,
};

pub(super) use matrix_client_core::conversation::{
    Attachment, DeliveryState, MessageKind, MessageRow as Message, Reaction, ReplyTarget,
};
use matrix_client_core::conversation::{ConversationError, TimelineStatus};
pub(super) use matrix_client_core::room_state::{Member, MemberRole, MembersState};
use tokio::{runtime::Runtime, sync::watch, task::JoinHandle};
use zeroize::Zeroizing;

use matrix_client_core::{
    DiscoveryError, auth::AuthError, profile::ProfileError, rooms::SyncStatus,
};
pub(super) use matrix_client_core::{
    profile::UserProfile as Profile, rooms::RoomSummary as Room, spaces::SpaceSummary as Space,
};

#[derive(Clone, Default)]
pub(super) struct Snapshot {
    pub(super) revision: u64,
    pub(super) account_instance: u128,
    pub(super) user_id: String,
    pub(super) rooms: Vec<Room>,
    pub(super) spaces: Vec<Space>,
    pub(super) spaces_loaded: bool,
    pub(super) selected_room: String,
    pub(super) messages: Vec<Message>,
    pub(super) timeline_ready: bool,
    pub(super) members: Vec<Member>,
    pub(super) members_state: MembersState,
    pub(super) typing: Vec<String>,
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
    typing_intent: Option<watch::Sender<Option<(String, bool)>>>,
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
        self.typing_intent = None;
        self.epoch = self.epoch.wrapping_add(1);
        self.selection = self.selection.wrapping_add(1);
    }
}

pub(super) struct Backend {
    _instance: Option<auth::InstanceLock>,
    runtime: Runtime,
    state: Arc<Mutex<State>>,
    updates: watch::Sender<Snapshot>,
    // Serialize lifecycle changes. GPUI invokes operations only on background workers.
    lifecycle: Mutex<()>,
}

fn publish(sender: &watch::Sender<Snapshot>, update: impl FnOnce(&mut Snapshot)) {
    sender.send_modify(|snapshot| {
        update(snapshot);
        snapshot.revision = snapshot.revision.wrapping_add(1);
    });
}

pub(super) enum TimelineAction {
    React { message: String, key: String },
    Retry { message: String },
    Cancel { message: String },
    Read { event_id: String },
}

pub(super) fn timeline_status_text(status: TimelineStatus) -> &'static str {
    match status {
        TimelineStatus::Loading => "Loading messages…",
        TimelineStatus::Ready => "",
        TimelineStatus::Unavailable => "This conversation is no longer available.",
        TimelineStatus::Failed => "Messages could not be loaded. Select the conversation to retry.",
        TimelineStatus::HistoryFailed => "Some history could not be loaded. Try Load Earlier.",
    }
}

fn auth_failure(error: AuthError) -> ServiceError {
    failure(match error {
        AuthError::Storage => {
            "Secure session storage could not be opened. Unlock your OS credential store and retry."
        }
        AuthError::Restore => {
            "The saved session could not be restored. It has been retained for recovery."
        }
        AuthError::Login => "Sign-in failed. Check your credentials, homeserver and connection.",
        AuthError::Logout => {
            "Sign-out could not be completed. The session has been retained; retry when connected."
        }
        AuthError::ExistingSession => {
            "A saved session already exists. Restart to restore it before signing in again."
        }
        AuthError::AlreadyRunning => {
            "Matrix Client is already running. Close the other window first."
        }
    })
}

fn discovery_failure(error: DiscoveryError) -> ServiceError {
    failure(match error {
        DiscoveryError::InvalidHomeserver => {
            "Enter a server name such as matrix.org, or an HTTPS homeserver URL."
        }
        DiscoveryError::InsecureHomeserver => {
            "This server does not offer a secure (HTTPS) connection."
        }
        DiscoveryError::Connection => {
            "Could not connect to this homeserver. Check its URL and network connection."
        }
    })
}

fn profile_failure(error: ProfileError) -> ServiceError {
    failure(match error {
        ProfileError::InvalidUser => "This user ID is not valid.",
        ProfileError::Unavailable => {
            "This profile could not be loaded. Check your connection and try again."
        }
    })
}

fn sync_status_text(status: SyncStatus) -> &'static str {
    match status {
        SyncStatus::Connecting => "Connecting…",
        SyncStatus::Syncing => "Syncing conversations…",
        SyncStatus::Connected => "Connected · syncing",
        SyncStatus::Offline => "Offline · reconnecting…",
        SyncStatus::Failed => "Synchronization failed · reconnect to retry",
        SyncStatus::Stopped => "Synchronization stopped",
    }
}

fn conversation_failure(error: ConversationError) -> ServiceError {
    failure(match error {
        ConversationError::EmptyMessage => "Write a message first.",
        ConversationError::Queue => "Message could not be queued. Your draft has been kept.",
        ConversationError::Pagination => "Older messages could not be loaded. Try again.",
        ConversationError::Unavailable => "This message is no longer available for that action.",
        ConversationError::Reaction => "The reaction could not be updated. Try again.",
        ConversationError::Retry => "The message could not be retried. Try again.",
        ConversationError::Cancel => "The failed message could not be cancelled. Try again.",
        ConversationError::Receipt => "The read marker could not be updated.",
    })
}

impl Backend {
    pub(super) fn new() -> Result<Arc<Self>, ServiceError> {
        Self::with_lock(Some(auth::lock_instance().map_err(auth_failure)?))
    }

    pub(super) fn with_lock(
        instance: Option<auth::InstanceLock>,
    ) -> Result<Arc<Self>, ServiceError> {
        let runtime = Runtime::new().map_err(|_| failure("Background runtime could not start."))?;
        let (updates, _) = watch::channel(Snapshot::default());
        Ok(Arc::new(Self {
            _instance: instance,
            runtime,
            updates,
            state: Arc::new(Mutex::new(State::default())),
            lifecycle: Mutex::new(()),
        }))
    }

    pub(super) fn discover(&self, homeserver: String) -> Result<LoginOptions, ServiceError> {
        let discovery = self
            .runtime
            .block_on(matrix_client_core::discover_login(&homeserver))
            .map_err(discovery_failure)?;
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
        if let Some(account) = self
            .runtime
            .block_on(auth::restore())
            .map_err(auth_failure)?
        {
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
            .map_err(auth_failure)?;
        self.connect(account)
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        self.updates.borrow().clone()
    }

    /// watch coalesces updates, so there is no unbounded event queue. Its
    /// receiver is executor-agnostic and can be awaited on the UI thread.
    pub(super) fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.updates.subscribe()
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
        if let Some(typing) = &state.typing_intent {
            typing.send_replace(None);
        }
        state.selection = state.selection.wrapping_add(1);
        let selection = state.selection;
        let epoch = state.epoch;
        account.select_room(Some(&room_id));
        publish(&self.updates, |snapshot| {
            snapshot.selected_room = room_id.clone();
            snapshot.messages.clear();
            snapshot.timeline_ready = false;
            snapshot.members.clear();
            snapshot.members_state = MembersState::Loading;
            snapshot.typing.clear();
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
                    view.timeline_status = timeline_status_text(snapshot.status).into();
                    view.messages = snapshot.messages;
                    view.members = snapshot.members;
                    view.members_state = snapshot.members_state;
                    view.typing = snapshot.typing;
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
            .map_err(profile_failure)?;
        let state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        if state.account.as_ref().map(Account::instance_id) != Some(account.instance_id()) {
            return Err(failure("The account changed. Open the profile again."));
        }
        Ok(profile)
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
        if let Some(typing) = &state.typing_intent {
            typing.send_replace(None);
        }
        if let Some(account) = &state.account {
            account.select_room(None);
        }
        publish(&self.updates, |snapshot| {
            snapshot.selected_room.clear();
            snapshot.messages.clear();
            snapshot.timeline_ready = false;
            snapshot.members.clear();
            snapshot.members_state = MembersState::Loading;
            snapshot.typing.clear();
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
            .map_err(conversation_failure)
    }

    pub(super) fn load_earlier(
        &self,
        room_id: String,
        account: u128,
    ) -> Result<bool, ServiceError> {
        let conversation = {
            let state = self
                .state
                .lock()
                .map_err(|_| failure("Session is unavailable."))?;
            if self.updates.borrow().selected_room != room_id
                || state.account.as_ref().map(Account::instance_id) != Some(account)
            {
                return Err(failure("Conversation changed."));
            }
            state
                .conversation
                .clone()
                .ok_or_else(|| failure("Conversation is still loading."))?
        };
        self.runtime
            .block_on(conversation.older())
            .map_err(conversation_failure)
    }

    pub(super) fn set_typing(&self, room_id: String, typing: bool) {
        if let Ok(state) = self.state.lock()
            && self.updates.borrow().selected_room == room_id
            && let Some(intent) = &state.typing_intent
        {
            intent.send_replace(Some((room_id, typing)));
        }
    }

    pub(super) fn timeline_action(
        &self,
        room_id: String,
        expected_account: u128,
        action: TimelineAction,
    ) -> Result<(), ServiceError> {
        let conversation = {
            let state = self
                .state
                .lock()
                .map_err(|_| failure("Session is unavailable."))?;
            if state.account.as_ref().map(Account::instance_id) != Some(expected_account)
                || self.updates.borrow().selected_room != room_id
            {
                return Err(failure("Conversation changed."));
            }
            state
                .conversation
                .clone()
                .ok_or_else(|| failure("Conversation is still loading."))?
        };
        self.runtime
            .block_on(async move {
                match action {
                    TimelineAction::React { message, key } => {
                        conversation.toggle_reaction(&message, &key).await
                    }
                    TimelineAction::Retry { message } => conversation.retry(&message).await,
                    TimelineAction::Cancel { message } => conversation.cancel(&message).await,
                    TimelineAction::Read { event_id } => conversation.mark_read(&event_id).await,
                }
            })
            .map_err(conversation_failure)
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
            self.runtime
                .block_on(account.logout())
                .map_err(auth_failure)?;
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
                account_instance: account.instance_id(),
                sync_status: "Connecting…".into(),
                ..Default::default()
            };
        });
        let (typing_tx, mut typing_rx) = watch::channel::<Option<(String, bool)>>(None);
        state.typing_intent = Some(typing_tx);
        let typing_account = account.clone();
        state.tasks.push(self.runtime.spawn(async move {
            let mut previous: Option<String> = None;
            while typing_rx.changed().await.is_ok() {
                let intent = typing_rx.borrow_and_update().clone();
                let next_room = intent.as_ref().map(|(room, _)| room.as_str());
                if previous.as_deref() != next_room
                    && let Some(room) = previous.take()
                {
                    let _ = typing_account.typing_notice(&room, false).await;
                }
                if let Some((room, active)) = intent {
                    let _ = typing_account.typing_notice(&room, active).await;
                    previous = Some(room);
                }
            }
        }));
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
                    view.sync_status = sync_status_text(snapshot.status).into();
                    view.rooms = snapshot.rooms;
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
                    view.spaces = snapshot.spaces;
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
        // No instance lock: unit tests must not touch the user data directory.
        let client = Backend::with_lock(None).unwrap();
        assert!(client.select_room("!room:example.org".into()).is_err());
        assert!(
            client
                .send_text("!room:example.org".into(), "test".into())
                .is_err()
        );
        assert!(client.profile("@someone:example.org".into(), None).is_err());
        assert!(client.load_earlier("!room:example.org".into(), 0).is_err());
        let snapshot = client.snapshot();
        assert!(snapshot.user_id.is_empty());
        assert!(snapshot.selected_room.is_empty());
        assert!(snapshot.messages.is_empty());
        assert_eq!(snapshot.revision, 0);
    }
}
