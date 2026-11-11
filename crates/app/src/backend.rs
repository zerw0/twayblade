//! Background application services for the GPUI frontend. No FFI dependency.

use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use twayblade_core::{
    auth::{self, Account, SessionState},
    conversation::Conversation,
};

use tokio::{runtime::Runtime, sync::watch, task::JoinHandle};
pub(super) use twayblade_core::conversation::{
    Attachment, DeliveryState, MessageKind, MessageRow as Message, Reaction, ReplyTarget,
};
use twayblade_core::conversation::{ConversationError, TimelineStatus};
pub(super) use twayblade_core::room_state::{Member, MemberRole, MembersState};
use zeroize::Zeroizing;
pub(super) mod calls;
mod directory;
mod media;
use directory::{DirectoryState, PreviewState};
pub(super) use media::MediaState;
mod search;
pub(super) use search::SearchState;
mod notifications;
mod presence;
pub(super) use notifications::NotificationConfig;

use twayblade_core::{DiscoveryError, auth::AuthError, profile::ProfileError, rooms::SyncStatus};
pub(super) use twayblade_core::{
    profile::UserProfile as Profile, rooms::RoomSummary as Room, spaces::SpaceSummary as Space,
};

#[derive(Clone, Default)]
pub(super) struct Snapshot {
    pub(super) call: Option<twayblade_core::calls::CallState>,
    pub(super) room_calls: std::collections::HashMap<String, twayblade_core::calls::ActiveCall>,
    pub(super) revision: u64,
    pub(super) account_instance: u128,
    pub(super) session_state: SessionState,
    pub(super) security: twayblade_core::security::SecuritySnapshot,
    pub(super) notifications: twayblade_core::notifications::RulesSnapshot,
    pub(super) notification_permission: twayblade_platform::notifications::Permission,
    pub(super) notification_failed: bool,
    pub(super) notification_click: Option<(u64, twayblade_platform::notifications::Activation)>,
    pub(super) user_id: String,
    pub(super) rooms: Vec<Room>,
    pub(super) unread_total: u64,
    pub(super) spaces: Vec<Space>,
    pub(super) invites: Vec<twayblade_core::space_actions::Invite>,
    pub(super) spaces_loaded: bool,
    pub(super) selected_room: String,
    pub(super) messages: Vec<Message>,
    pub(super) media: MediaState,
    pub(super) media_thumbnails: std::collections::HashMap<String, Arc<Vec<u8>>>,
    pub(super) media_thumbnail_order: std::collections::VecDeque<String>,
    pub(super) directory: DirectoryState,
    pub(super) room_preview: PreviewState,
    pub(super) search: SearchState,
    pub(super) focused_event: Option<String>,
    pub(super) presence_enabled: bool,
    pub(super) own_presence: Option<twayblade_core::presence::Presence>,
    pub(super) presence: std::collections::HashMap<String, twayblade_core::presence::Presence>,
    pub(super) thread: Option<ThreadSnapshot>,
    pub(super) timeline_ready: bool,
    pub(super) members: Vec<Member>,
    pub(super) members_state: MembersState,
    pub(super) typing: Vec<twayblade_core::room_state::TypingUser>,
    pub(super) typing_others: usize,
    pub(super) reduce_motion: bool,
    pub(super) sync_status: String,
    pub(super) timeline_status: String,
}

#[derive(Clone)]
pub(super) struct ThreadSnapshot {
    pub room: String,
    pub root: String,
    pub request: u64,
    pub messages: Vec<Message>,
    pub status: TimelineStatus,
    pub ready: bool,
    pub unread: u64,
}

pub(super) enum ThreadAction {
    Send(String),
    Older,
    Timeline(TimelineAction),
}

pub(super) struct LoginOptions {
    pub(super) homeserver: String,
    pub(super) password: bool,
    pub(super) browser: bool,
    pub(super) sso: bool,
    pub(super) providers: Vec<twayblade_core::IdentityProvider>,
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
    call_request: u64,
    call_task: Option<JoinHandle<()>>,
    call_control: Option<tokio::sync::mpsc::Sender<calls::Control>>,
    account: Option<Account>,
    security: Option<twayblade_core::security::Security>,
    conversation: Option<Conversation>,
    epoch: u64,
    selection: u64,
    tasks: Vec<JoinHandle<()>>,
    timeline_task: Option<JoinHandle<()>>,
    media_task: Option<JoinHandle<()>>,
    media_request: u64,
    media_cancel: Option<watch::Sender<bool>>,
    discovery_task: Option<JoinHandle<()>>,
    discovery_request: u64,
    preview_task: Option<JoinHandle<()>>,
    preview_request: u64,
    membership_task: Option<JoinHandle<()>>,
    membership_request: u64,
    search_task: Option<JoinHandle<()>>,
    search_request: u64,
    presence_task: Option<JoinHandle<()>>,
    presence_request: u64,
    presence_visible: Vec<String>,
    thread_task: Option<JoinHandle<()>>,
    thread_conversation: Option<Conversation>,
    thread_request: u64,
    directory: Option<twayblade_core::space_actions::Directory>,
    directory_request: u64,
    leave_request: u64,
    leave_plan: Option<(String, Arc<twayblade_core::space_actions::LeavePlan>)>,
    typing_intent: Option<watch::Sender<Option<(String, bool)>>>,
}

impl State {
    fn stop(&mut self) {
        self.cancel_call();
        self.cancel_discovery();
        self.cancel_media();
        self.cancel_search();
        self.cancel_presence();
        self.security = None;
        self.directory = None;
        self.leave_plan = None;
        for task in self.tasks.drain(..) {
            task.abort();
        }
        if let Some(task) = self.timeline_task.take() {
            task.abort();
        }
        if let Some(task) = self.thread_task.take() {
            task.abort();
        }
        self.thread_conversation = None;
        self.conversation = None;
        self.typing_intent = None;
        self.epoch = self.epoch.wrapping_add(1);
        self.selection = self.selection.wrapping_add(1);
    }
}

enum BrowserLogin {
    OAuth(auth::OAuthLogin),
    Sso(auth::SsoLogin),
}
impl BrowserLogin {
    fn browser_url(&self) -> &str {
        match self {
            Self::OAuth(login) => login.browser_url(),
            Self::Sso(login) => login.browser_url(),
        }
    }
    async fn finish(self, cancel: watch::Receiver<bool>) -> Result<Account, AuthError> {
        match self {
            Self::OAuth(login) => login.finish(cancel).await,
            Self::Sso(login) => login.finish(cancel).await,
        }
    }
}

pub(super) struct Backend {
    _instance: Option<auth::InstanceLock>,
    motion_task: JoinHandle<()>,
    runtime: Runtime,
    state: Arc<Mutex<State>>,
    updates: watch::Sender<Snapshot>,
    notification_config: watch::Sender<NotificationConfig>,
    // Serialize lifecycle changes. GPUI invokes operations only on background workers.
    lifecycle: Mutex<()>,
    oauth: Mutex<Option<BrowserLogin>>,
}

pub(super) fn render_viewer_image(
    image: &mut twayblade_core::conversation::ViewerImage,
) -> Arc<gpui_kit::RenderImage> {
    let mut frames = std::mem::take(&mut image.frames);
    for frame in &mut frames {
        for pixel in frame.buffer_mut().pixels_mut() {
            pixel.0.swap(0, 2);
        }
    }
    Arc::new(gpui_kit::RenderImage::new(frames))
}

fn publish(sender: &watch::Sender<Snapshot>, update: impl FnOnce(&mut Snapshot)) {
    sender.send_modify(|snapshot| {
        update(snapshot);
        snapshot.reduce_motion = twayblade_platform::reduce_motion();
        snapshot.revision = snapshot.revision.wrapping_add(1);
    });
}

pub(super) enum SpaceAction {
    Directory(String),
    NextDirectory(String),
    Join(String, Vec<String>),
    JoinSpace(String),
    Invite(String, bool),
    PlanLeave(String),
    Leave(String, std::collections::HashSet<String>),
    Create(twayblade_core::space_actions::CreateRoom),
    Dm(String),
}
pub(super) enum SpaceOutcome {
    Directory(twayblade_core::space_actions::DirectorySnapshot),
    Leave(Vec<twayblade_core::space_actions::LeaveCandidate>),
    Room(String),
    PartialRoom(String),
    Done,
}
fn space_failure(error: twayblade_core::space_actions::SpaceError) -> ServiceError {
    use twayblade_core::space_actions::SpaceError::*;
    failure(match error {
        InvalidId => "Enter a valid Matrix room or user ID and a room name when creating a room.",
        NotSpace => "This ID belongs to a room. Enter a space ID or alias.",
        Unavailable => "This space or invitation is no longer available. Refresh and try again.",
        Permission => "You do not have permission to add rooms to this space.",
        Hierarchy => "Rooms could not be loaded. The space may be private or inaccessible.",
        Join => {
            "Could not join. Restricted rooms require membership in an allowed space; private rooms require an invitation."
        }
        Leave => {
            "Some rooms could not be left. Your remaining memberships are preserved; review and retry."
        }
        Create => {
            "The conversation could not be created. Check your connection and server permissions."
        }
        ParentLink { .. } => {
            "The room was created, but its space links could not be completed. Do not create it again."
        }
    })
}

pub(super) enum SecurityAction {
    Setup,
    Refresh,
    SignOutBrowser(String),
    Recover(Zeroizing<String>),
    Reset,
    Request(String),
    VerifyUser(String),
    Accept,
    Confirm,
    Cancel(bool),
    SignOut(String, Zeroizing<String>),
}
pub(super) enum SecurityOutcome {
    Done,
    RecoveryKey(Zeroizing<String>),
    BrowserUrl(String),
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
        AuthError::OAuth => "Browser sign-in failed. Check your connection and try again.",
        AuthError::Callback => "The browser response was invalid. Start sign-in again.",
        AuthError::Cancelled => "Browser sign-in cancelled.",
        AuthError::Timeout => "Browser sign-in timed out after five minutes. Try again.",
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
        AuthError::AlreadyRunning => "Twayblade is already running. Close the other window first.",
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
        ProfileError::InvalidImage => "Choose an image up to 8 MB and 4096 pixels per side.",
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
        SyncStatus::Connected => "Connected",
        SyncStatus::Offline => "Offline · reconnecting…",
        SyncStatus::Failed => "Synchronization failed · reconnect to retry",
        SyncStatus::Stopped => "Synchronization stopped",
    }
}

fn media_failure(error: twayblade_core::conversation::AttachmentError) -> ServiceError {
    use twayblade_core::conversation::AttachmentError::*;
    failure(&match error {
        Limit { maximum } => format!(
            "This file exceeds the server’s upload limit of {} MB.",
            maximum / 1_000_000
        ),
        File => "This file could not be read.".into(),
        Connection => "The server’s upload limit could not be checked. Try again.".into(),
        Upload => "The attachment could not be queued or cancelled.".into(),
        Download => "The attachment could not be saved.".into(),
        Unavailable => "This attachment is no longer available.".into(),
    })
}

fn conversation_failure(error: ConversationError) -> ServiceError {
    failure(match error {
        ConversationError::EmptyMessage => "Write a message first.",
        ConversationError::Formatting(_) => {
            "This message is too large to format. Shorten it and try again."
        }
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
        let (updates, _) = watch::channel(Snapshot {
            reduce_motion: twayblade_platform::reduce_motion(),
            ..Default::default()
        });
        let mut motion = twayblade_platform::reduce_motion_changes();
        let motion_updates = updates.clone();
        let motion_task = runtime.spawn(async move {
            while motion.changed().await.is_ok() {
                let reduced = *motion.borrow_and_update();
                let changed = motion_updates.borrow().reduce_motion != reduced;
                if changed {
                    publish(&motion_updates, |view| view.reduce_motion = reduced);
                }
            }
        });
        Ok(Arc::new(Self {
            _instance: instance,
            motion_task,
            runtime,
            updates,
            notification_config: watch::channel(NotificationConfig::default()).0,
            state: Arc::new(Mutex::new(State::default())),
            lifecycle: Mutex::new(()),
            oauth: Mutex::new(None),
        }))
    }

    pub(super) fn discover(&self, homeserver: String) -> Result<LoginOptions, ServiceError> {
        let discovery = self
            .runtime
            .block_on(twayblade_core::discover_login(&homeserver))
            .map_err(discovery_failure)?;
        Ok(LoginOptions {
            homeserver: discovery.homeserver,
            password: discovery
                .legacy_flows
                .as_ref()
                .is_some_and(|flows| flows.iter().any(|flow| flow == "m.login.password")),
            browser: discovery.oauth_metadata_available,
            sso: discovery.sso,
            providers: discovery.identity_providers,
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
        let recovery = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?
            .account
            .clone()
            .filter(|account| *account.session_state().borrow() == SessionState::SoftLoggedOut);
        let account = self
            .runtime
            .block_on(async move {
                if let Some(account) = recovery {
                    account.reauthenticate_password(password).await
                } else {
                    auth::login_password(homeserver, username, password).await
                }
            })
            .map_err(auth_failure)?;
        self.connect(account)
    }

    pub(super) fn begin_oauth(
        &self,
        homeserver: String,
        sso: bool,
        provider: Option<String>,
    ) -> Result<String, ServiceError> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        let recovery = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?
            .account
            .clone()
            .filter(|account| *account.session_state().borrow() == SessionState::SoftLoggedOut);
        let pending = self
            .runtime
            .block_on(async move {
                match (sso, recovery) {
                    (true, Some(account)) => auth::SsoLogin::reauthenticate(account, provider)
                        .await
                        .map(BrowserLogin::Sso),
                    (false, Some(account)) => auth::OAuthLogin::reauthenticate(account)
                        .await
                        .map(BrowserLogin::OAuth),
                    (true, None) => auth::SsoLogin::begin(homeserver, provider)
                        .await
                        .map(BrowserLogin::Sso),
                    (false, None) => auth::OAuthLogin::begin(homeserver)
                        .await
                        .map(BrowserLogin::OAuth),
                }
            })
            .map_err(auth_failure)?;
        let url = pending.browser_url().to_owned();
        *self
            .oauth
            .lock()
            .map_err(|_| failure("Session is unavailable."))? = Some(pending);
        Ok(url)
    }

    pub(super) fn finish_oauth(&self, cancel: watch::Receiver<bool>) -> Result<(), ServiceError> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        let pending = self
            .oauth
            .lock()
            .map_err(|_| failure("Session is unavailable."))?
            .take()
            .ok_or_else(|| failure("Start browser sign-in again."))?;
        let account = self
            .runtime
            .block_on(pending.finish(cancel))
            .map_err(auth_failure)?;
        self.connect(account)
    }

    pub(super) fn recovery_options(&self) -> Result<LoginOptions, ServiceError> {
        let account = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?
            .account
            .clone()
            .ok_or_else(|| failure("No saved session."))?;
        if *account.session_state().borrow() != SessionState::SoftLoggedOut {
            return Err(failure("This session cannot be resumed."));
        }
        self.discover(account.homeserver())
    }

    pub(super) fn discard_revoked_session(&self) -> Result<(), ServiceError> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        let account = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?
            .account
            .clone()
            .ok_or_else(|| failure("No saved session."))?;
        self.runtime
            .block_on(account.discard_revoked_session())
            .map_err(auth_failure)?;
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

    pub(super) fn security_action(
        &self,
        expected: u128,
        action: SecurityAction,
    ) -> Result<SecurityOutcome, ServiceError> {
        let security = {
            let state = self
                .state
                .lock()
                .map_err(|_| failure("Session is unavailable."))?;
            if state.account.as_ref().map(Account::instance_id) != Some(expected) {
                return Err(failure("The account changed."));
            }
            state
                .security
                .clone()
                .ok_or_else(|| failure("Security information is still loading."))?
        };
        let result = self.runtime.block_on(async {
            use SecurityAction::*;
            match action {
                Setup => return security.setup_backup().await.map(SecurityOutcome::RecoveryKey),
                Refresh => security.refresh().await?,
                SignOutBrowser(device) => return security.device_management_url(device).await.map(SecurityOutcome::BrowserUrl),
                Recover(key) => security.recover(key).await?,
                Reset => return security.reset_recovery().await.map(SecurityOutcome::RecoveryKey),
                Request(device) => security.request_verification(device).await?,
                VerifyUser(user) => security.request_user_verification(user).await?,
                Accept => security.accept_verification().await?,
                Confirm => security.confirm_verification().await?,
                Cancel(mismatch) => security.cancel_verification(mismatch).await?,
                SignOut(device, password) => security.sign_out_device(device, password).await?,
            }
            Ok(SecurityOutcome::Done)
        }).map_err(|error| failure(match error {
            twayblade_core::security::SecurityError::Unavailable => "Security information could not be loaded. Check your connection.",
            twayblade_core::security::SecurityError::Setup => "Secure backup could not be set up. Verify this device before trying again.",
            twayblade_core::security::SecurityError::Recovery => "Recovery failed. Check your recovery key or passphrase and connection.",
            twayblade_core::security::SecurityError::Verification => "Verification could not be completed. Start again on both devices.",
            twayblade_core::security::SecurityError::DeviceAuthentication => "The device could not be signed out. Check your password or your server’s authentication requirements.",
        }))?;
        let state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        if state.account.as_ref().map(Account::instance_id) != Some(expected) {
            return Err(failure("The account changed."));
        }
        Ok(result)
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
        self.select_room_focus(room_id, None)
    }
    pub(super) fn open_event(&self, room: String, event: String) -> Result<(), ServiceError> {
        self.select_room_focus(room, Some(event))
    }
    fn select_room_focus(
        &self,
        room_id: String,
        target: Option<String>,
    ) -> Result<(), ServiceError> {
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
        if let Some(task) = state.thread_task.take() {
            task.abort();
        }
        state.cancel_discovery();
        state.cancel_media();
        state.cancel_presence();
        state.thread_conversation = None;
        state.conversation = None;
        if let Some(typing) = &state.typing_intent {
            typing.send_replace(None);
        }
        state.selection = state.selection.wrapping_add(1);
        let selection = state.selection;
        let epoch = state.epoch;
        account.select_room(Some(&room_id));
        publish(&self.updates, |snapshot| {
            snapshot.focused_event = None;
            snapshot.media = MediaState::default();
            snapshot.media_thumbnails.clear();
            snapshot.media_thumbnail_order.clear();
            snapshot.room_preview = PreviewState::default();
            snapshot.directory.loading = false;
            snapshot.selected_room = room_id.clone();
            snapshot.messages.clear();
            snapshot.thread = None;
            snapshot.timeline_ready = false;
            snapshot.members.clear();
            snapshot.members_state = MembersState::Loading;
            snapshot.typing.clear();
            snapshot.typing_others = 0;
            snapshot.timeline_status = "Loading messages…".into();
        });
        let shared = self.state.clone();
        let updates = self.updates.clone();
        state.timeline_task = Some(self.runtime.spawn(async move {
            let stream: std::pin::Pin<Box<dyn futures_util::Stream<Item = twayblade_core::conversation::ConversationSnapshot> + Send>> = match target.clone() {
                Some(event) => Box::pin(account.event_updates(room_id, event, selection)),
                None => Box::pin(account.conversation_updates(room_id, selection)),
            };
            tokio::pin!(stream);
            let mut thumbnails = std::collections::HashMap::<String, Arc<Vec<u8>>>::new();
            let mut attempted = std::collections::HashSet::new();
            let mut pending = std::collections::VecDeque::<String>::new();
            let mut work = tokio::task::JoinSet::new();
            let mut current: Option<Conversation> = None;
            loop {
                while work.len() < 4 && let Some(id) = pending.pop_front() && let Some(conversation) = current.clone() {
                    work.spawn(async move {
                        let png = tokio::time::timeout(std::time::Duration::from_secs(15), conversation.attachment_thumbnail(&id)).await.ok().and_then(Result::ok);
                        (id, png)
                    });
                }
                tokio::select! {
                    snapshot = stream.next() => {
                        let Some(mut snapshot) = snapshot else { break; };
                        current = snapshot.conversation.clone();
                        let candidates = snapshot.messages.iter().rev().filter(|message| message.kind == twayblade_core::conversation::MessageKind::Image).take(128).map(|message| message.id.clone()).collect::<std::collections::HashSet<_>>();
                        attempted.retain(|id| candidates.contains(id));
                        pending.retain(|id| candidates.contains(id));
                        for message in snapshot.messages.iter_mut().rev() {
                            if let Some(attachment) = &mut message.attachment { attachment.preview_png = thumbnails.get(&message.id).cloned(); }
                            if candidates.contains(&message.id) && attempted.insert(message.id.clone()) { pending.push_back(message.id.clone()); }
                        }
                        let Ok(mut state) = shared.lock() else { return; };
                        if state.epoch != epoch || state.selection != selection { return; }
                        state.conversation = snapshot.conversation;
                        publish(&updates, |view| {
                            view.timeline_ready = state.conversation.is_some();
                            view.timeline_status = timeline_status_text(snapshot.status).into();
                            view.focused_event = target.as_ref().filter(|id| snapshot.messages.iter().any(|m| m.event_id.as_ref() == Some(id))).cloned();
                            view.messages = snapshot.messages;
                            view.members = snapshot.members;
                            for member in &mut view.members { member.presence = view.presence.get(&member.user_id).cloned(); }
                            view.members_state = snapshot.members_state;
                            view.typing = snapshot.typing;
                            view.typing_others = snapshot.typing_others;
                        });
                    }
                    completed = work.join_next(), if !work.is_empty() => {
                        if let Some(Ok((id, Some(png)))) = completed {
                            if thumbnails.len() >= 32 { thumbnails.clear(); }
                            thumbnails.insert(id.clone(), png.clone());
                            let Ok(state) = shared.lock() else { return; };
                            if state.epoch != epoch || state.selection != selection { return; }
                            publish(&updates, |view| { if let Some(attachment) = view.messages.iter_mut().find(|message| message.id == id).and_then(|message| message.attachment.as_mut()) { attachment.preview_png = Some(png); } });
                        }
                    }
                }
            }
        }));
        drop(state);
        self.observe_presence(Vec::new())
    }

    pub(super) fn select_thread(
        &self,
        room: String,
        root: Option<String>,
        account_id: u128,
        request: u64,
    ) -> Result<(), ServiceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session unavailable."))?;
        let account = state
            .account
            .clone()
            .filter(|a| a.instance_id() == account_id)
            .ok_or_else(|| failure("Account changed."))?;
        if self.updates.borrow().selected_room != room || request < state.thread_request {
            return Err(failure("Conversation changed."));
        }
        state.thread_request = request;
        if let Some(task) = state.thread_task.take() {
            task.abort();
        }
        state.thread_conversation = None;
        publish(&self.updates, |view| view.thread = None);
        let Some(root) = root else {
            return Ok(());
        };
        let epoch = state.epoch;
        let selection = state.selection;
        let shared = self.state.clone();
        let updates = self.updates.clone();
        state.thread_task = Some(self.runtime.spawn(async move {
            let stream = account.thread_updates(room.clone(), root.clone(), request);
            tokio::pin!(stream);
            while let Some(snapshot) = stream.next().await {
                let Ok(mut state) = shared.lock() else {
                    return;
                };
                if state.epoch != epoch
                    || state.selection != selection
                    || state.thread_request != request
                {
                    return;
                }
                state.thread_conversation = snapshot.conversation;
                publish(&updates, |view| {
                    view.thread = Some(ThreadSnapshot {
                        room: room.clone(),
                        root: root.clone(),
                        request,
                        messages: snapshot.messages,
                        status: snapshot.status,
                        ready: state.thread_conversation.is_some(),
                        unread: snapshot.unread,
                    })
                });
            }
        }));
        Ok(())
    }

    pub(super) fn thread_action(
        &self,
        room: String,
        root: String,
        account: u128,
        request: u64,
        action: ThreadAction,
    ) -> Result<bool, ServiceError> {
        let conversation = {
            let state = self
                .state
                .lock()
                .map_err(|_| failure("Session unavailable."))?;
            if state.account.as_ref().map(Account::instance_id) != Some(account)
                || state.thread_request != request
                || !self
                    .updates
                    .borrow()
                    .thread
                    .as_ref()
                    .is_some_and(|t| t.root == root && t.room == room)
            {
                return Err(failure("Thread changed."));
            }
            state
                .thread_conversation
                .clone()
                .ok_or_else(|| failure("Thread is loading."))?
        };
        self.runtime
            .block_on(async move {
                match action {
                    ThreadAction::Send(body) => conversation.send(body).await.map(|_| false),
                    ThreadAction::Older => conversation.older().await,
                    ThreadAction::Timeline(action) => match action {
                        TimelineAction::Read { event_id } => {
                            conversation.mark_read(&event_id).await
                        }
                        TimelineAction::React { message, key } => {
                            conversation.toggle_reaction(&message, &key).await
                        }
                        TimelineAction::Retry { message } => conversation.retry(&message).await,
                        TimelineAction::Cancel { message } => conversation.cancel(&message).await,
                    }
                    .map(|_| false),
                }
            })
            .map_err(conversation_failure)
    }

    pub(super) fn space_action(
        &self,
        expected: u128,
        request: u64,
        action: SpaceAction,
    ) -> Result<SpaceOutcome, ServiceError> {
        let account = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| failure("Session unavailable."))?;
            match &action {
                SpaceAction::Directory(_) => {
                    if request < state.directory_request {
                        return Err(failure("Directory changed."));
                    }
                    state.directory_request = request;
                }
                SpaceAction::PlanLeave(_) => {
                    if request < state.leave_request {
                        return Err(failure("Leave selection changed."));
                    }
                    state.leave_request = request;
                }
                _ => {}
            }
            state
                .account
                .clone()
                .filter(|a| a.instance_id() == expected)
                .ok_or_else(|| failure("Account changed."))?
        };
        self.runtime.block_on(async {
            use SpaceAction::*;
            let result = match action {
                Directory(id) => {
                    let directory = account.directory(&id).await.map_err(space_failure)?;
                    let snapshot = directory.next_page().await.map_err(space_failure)?;
                    let mut state = self
                        .state
                        .lock()
                        .map_err(|_| failure("Session unavailable."))?;
                    if state.account.as_ref().map(Account::instance_id) != Some(expected) {
                        return Err(failure("Account changed."));
                    }
                    if state.directory_request != request {
                        return Err(failure("Directory changed."));
                    }
                    state.directory = Some(directory);
                    Ok(SpaceOutcome::Directory(snapshot))
                }
                NextDirectory(id) => {
                    let directory = self
                        .state
                        .lock()
                        .map_err(|_| failure("Session unavailable."))?
                        .directory
                        .clone()
                        .ok_or_else(|| failure("Open the directory again."))?;
                    let snapshot = directory.snapshot().await;
                    if snapshot.space != id {
                        return Err(failure("Directory changed."));
                    }
                    directory
                        .next_page()
                        .await
                        .map(SpaceOutcome::Directory)
                        .map_err(space_failure)
                }
                JoinSpace(id) => account
                    .join_space(&id)
                    .await
                    .map(SpaceOutcome::Room)
                    .map_err(space_failure),
                Join(id, via) => account
                    .join_room_or_space(&id, via)
                    .await
                    .map(SpaceOutcome::Room)
                    .map_err(space_failure),
                Invite(id, accept) => account
                    .respond_to_invite(&id, accept)
                    .await
                    .map(|_| SpaceOutcome::Done)
                    .map_err(space_failure),
                PlanLeave(id) => {
                    let plan = account.plan_leave_space(&id).await.map_err(space_failure)?;
                    let rooms = plan.rooms.clone();
                    let mut state = self
                        .state
                        .lock()
                        .map_err(|_| failure("Session unavailable."))?;
                    if state.account.as_ref().map(Account::instance_id) != Some(expected) {
                        return Err(failure("Account changed."));
                    }
                    if state.leave_request != request {
                        return Err(failure("Leave selection changed."));
                    }
                    state.leave_plan = Some((id, Arc::new(plan)));
                    Ok(SpaceOutcome::Leave(rooms))
                }
                Leave(id, selected) => {
                    let plan = self
                        .state
                        .lock()
                        .map_err(|_| failure("Session unavailable."))?
                        .leave_plan
                        .as_ref()
                        .filter(|(space, _)| space == &id)
                        .map(|(_, p)| p.clone())
                        .ok_or_else(|| failure("Review the leave selection again."))?;
                    plan.leave(selected)
                        .await
                        .map(|_| SpaceOutcome::Done)
                        .map_err(space_failure)
                }
                Create(draft) => match account.create_room(draft).await {
                    Ok(id) => Ok(SpaceOutcome::Room(id)),
                    Err(twayblade_core::space_actions::SpaceError::ParentLink { room }) => {
                        Ok(SpaceOutcome::PartialRoom(room))
                    }
                    Err(e) => Err(space_failure(e)),
                },
                Dm(id) => account
                    .start_dm(&id)
                    .await
                    .map(SpaceOutcome::Room)
                    .map_err(space_failure),
            };
            if self
                .state
                .lock()
                .map_err(|_| failure("Session unavailable."))?
                .account
                .as_ref()
                .map(Account::instance_id)
                != Some(expected)
            {
                return Err(failure("Account changed."));
            }
            result
        })
    }

    pub(super) fn update_profile(
        &self,
        expected: u128,
        update: twayblade_core::profile::ProfileUpdate,
    ) -> Result<Profile, ServiceError> {
        let account = self
            .state
            .lock()
            .map_err(|_| failure("Session unavailable."))?
            .account
            .clone()
            .filter(|a| a.instance_id() == expected)
            .ok_or_else(|| failure("Account changed."))?;
        self.runtime
            .block_on(account.update_profile(update))
            .map_err(profile_failure)?;
        self.profile(account.user_id().to_string(), None)
    }

    pub(super) fn member_action(
        &self,
        room: String,
        user: String,
        action: twayblade_core::room_state::MemberAction,
    ) -> Result<(), ServiceError> {
        let account = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?
            .account
            .clone()
            .ok_or_else(|| failure("Sign in first."))?;
        self.runtime
            .block_on(account.member_action(&room, &user, action))
            .map_err(|_| failure("That action failed. You may not have permission in this room."))
    }

    pub(super) fn set_favourite(&self, room: String, favourite: bool) -> Result<(), ServiceError> {
        let account = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?
            .account
            .clone()
            .ok_or_else(|| failure("Sign in first."))?;
        self.runtime
            .block_on(account.set_favourite(&room, favourite))
            .map_err(|_| failure("Favourite could not be changed. Try again."))
    }

    /// Square avatar PNG for `mxc` at `size` physical pixels. Bounded in core.
    pub(super) fn avatar(&self, mxc: String, size: u32) -> Result<Vec<u8>, ServiceError> {
        let account = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?
            .account
            .clone()
            .ok_or_else(|| failure("Sign in first."))?;
        self.runtime
            .block_on(account.avatar_thumbnail(mxc, size))
            .map_err(|_| failure("Avatar unavailable."))
    }

    pub(super) fn own_avatar_url(&self) -> Option<String> {
        let account = self.state.lock().ok()?.account.clone()?;
        self.runtime.block_on(account.own_avatar_url())
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
        if let Some(task) = state.thread_task.take() {
            task.abort();
        }
        state.cancel_discovery();
        state.cancel_media();
        state.cancel_presence();
        state.thread_conversation = None;
        state.conversation = None;
        if let Some(typing) = &state.typing_intent {
            typing.send_replace(None);
        }
        if let Some(account) = &state.account {
            account.select_room(None);
        }
        publish(&self.updates, |snapshot| {
            snapshot.focused_event = None;
            snapshot.media = MediaState::default();
            snapshot.media_thumbnails.clear();
            snapshot.media_thumbnail_order.clear();
            snapshot.room_preview = PreviewState::default();
            snapshot.directory.loading = false;
            snapshot.selected_room.clear();
            snapshot.messages.clear();
            snapshot.thread = None;
            snapshot.timeline_ready = false;
            snapshot.members.clear();
            snapshot.members_state = MembersState::Loading;
            snapshot.typing.clear();
            snapshot.typing_others = 0;
            snapshot.timeline_status.clear();
        });
        drop(state);
        self.observe_presence(Vec::new())
    }

    pub(super) fn media_conversation(
        &self,
        room: &str,
        account: u128,
    ) -> Result<Conversation, ServiceError> {
        let state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        if state.account.as_ref().map(Account::instance_id) != Some(account)
            || self.updates.borrow().selected_room != room
        {
            return Err(failure("Conversation changed."));
        }
        state
            .conversation
            .clone()
            .ok_or_else(|| failure("Conversation is still loading."))
    }
    pub(super) fn prepare_attachment(
        &self,
        room: String,
        account: u128,
        source: crate::ui::attachments::AttachmentSource,
    ) -> Result<twayblade_core::conversation::AttachmentDraft, ServiceError> {
        let conversation = self.media_conversation(&room, account)?;
        self.runtime
            .block_on(async {
                match source {
                    crate::ui::attachments::AttachmentSource::File(path) => {
                        conversation.prepare_file(path).await
                    }
                    crate::ui::attachments::AttachmentSource::Bytes(name, bytes) => {
                        conversation.prepare_bytes(name, bytes).await
                    }
                }
            })
            .map_err(media_failure)
    }
    pub(super) fn send_attachment(
        &self,
        room: String,
        account: u128,
        draft: twayblade_core::conversation::AttachmentDraft,
    ) -> Result<(), ServiceError> {
        let conversation = self.media_conversation(&room, account)?;
        self.runtime
            .block_on(conversation.send_attachment(draft))
            .map_err(media_failure)
    }
    pub(super) fn viewer_image(
        &self,
        room: String,
        account: u128,
        message: String,
        cancel: watch::Receiver<bool>,
    ) -> Result<
        (
            Arc<twayblade_core::conversation::ViewerImage>,
            Arc<gpui_kit::RenderImage>,
        ),
        twayblade_core::conversation::ImageError,
    > {
        use twayblade_core::conversation::ImageError;
        let gallery = {
            let state = self.state.lock().map_err(|_| ImageError::Unavailable)?;
            let view = self.updates.borrow();
            if state.account.as_ref().map(Account::instance_id) != Some(account)
                || view.selected_room != room
            {
                return Err(ImageError::Unavailable);
            }
            view.media
                .items
                .iter()
                .find(|i| i.event_id == message)
                .cloned()
                .zip(state.account.clone())
        };
        if let Some((item, account)) = gallery {
            let mut image = self
                .runtime
                .block_on(account.gallery_image(&item, cancel))?;
            let render = render_viewer_image(&mut image);
            return Ok((Arc::new(image), render));
        }
        let conversation = {
            let state = self.state.lock().map_err(|_| ImageError::Unavailable)?;
            let view = self.updates.borrow();
            if state.account.as_ref().map(Account::instance_id) != Some(account)
                || view.selected_room != room
            {
                return Err(ImageError::Unavailable);
            }
            if view
                .thread
                .as_ref()
                .is_some_and(|t| t.room == room && t.messages.iter().any(|m| m.id == message))
            {
                state.thread_conversation.clone()
            } else {
                state.conversation.clone()
            }
            .ok_or(ImageError::Unavailable)?
        };
        let mut image = self
            .runtime
            .block_on(conversation.viewer_image(&message, cancel))?;
        let render = render_viewer_image(&mut image);
        Ok((Arc::new(image), render))
    }

    pub(super) fn media_action(
        &self,
        room: String,
        account: u128,
        message: String,
        path: Option<std::path::PathBuf>,
    ) -> Result<(), ServiceError> {
        let main = self.media_conversation(&room, account)?;
        let conversation = {
            let state = self
                .state
                .lock()
                .map_err(|_| failure("Session unavailable."))?;
            if self
                .updates
                .borrow()
                .thread
                .as_ref()
                .is_some_and(|t| t.room == room && t.messages.iter().any(|m| m.id == message))
            {
                state
                    .thread_conversation
                    .clone()
                    .ok_or_else(|| failure("Thread changed."))?
            } else {
                main
            }
        };
        self.runtime
            .block_on(async {
                match path {
                    Some(path) => conversation.download_attachment(&message, &path).await,
                    None => conversation.cancel_attachment(&message).await,
                }
            })
            .map_err(media_failure)
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
        let account = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| failure("Session is unavailable."))?;
            state.cancel_call();
            publish(&self.updates, |v| {
                if let Some(call) = &mut v.call {
                    call.phase = twayblade_core::calls::CallPhase::Ended(
                        twayblade_core::calls::EndReason::Logout,
                    );
                }
            });
            state.account.clone()
        };
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
        twayblade_platform::notifications::clear_launch_activation();
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
    pub(super) fn notification_rule(
        &self,
        change: twayblade_core::notifications::RuleChange,
    ) -> Result<(), ServiceError> {
        let account = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?
            .account
            .clone()
            .ok_or_else(|| failure("Sign in to change notifications."))?;
        self.runtime.block_on(async {
            let rules = account
                .notification_rules()
                .await
                .map_err(|_| failure("Notification settings could not be loaded. Try again."))?;
            rules
                .change(change)
                .await
                .map_err(|_| failure("Notification settings could not be saved. Try again."))
        })
    }
    fn connect(&self, account: Account) -> Result<(), ServiceError> {
        let _runtime = self.runtime.enter();
        let security = twayblade_core::security::Security::new(&account);
        let mut security_updates = security.subscribe();
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session is unavailable."))?;
        state.stop();
        state.account = Some(account.clone());
        calls::observe(self, &mut state, account.clone());
        state.security = Some(security);
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
        state.tasks.push(self.runtime.spawn(notifications::run(
            account.clone(),
            epoch,
            self.state.clone(),
            self.updates.clone(),
            self.notification_config.subscribe(),
        )));
        let shared = self.state.clone();
        let updates = self.updates.clone();
        state.tasks.push(self.runtime.spawn(async move {
            while security_updates.changed().await.is_ok() {
                let snapshot = security_updates.borrow_and_update().clone();
                let Ok(state) = shared.lock() else {
                    return;
                };
                if state.epoch != epoch {
                    return;
                }
                publish(&updates, |view| view.security = snapshot);
            }
        }));
        let mut session_changes = account.session_state();
        let shared = self.state.clone();
        let updates = self.updates.clone();
        state.tasks.push(self.runtime.spawn(async move {
            loop {
                let session_state = *session_changes.borrow_and_update();
                if session_state != SessionState::Active {
                    if let Ok(mut state) = shared.lock() {
                        if state.epoch != epoch {
                            return;
                        }
                        state.stop();
                        publish(&updates, |view| {
                            view.session_state = session_state;
                            view.timeline_ready = false;
                            view.sync_status.clear();
                        });
                    }
                    return;
                }
                if session_changes.changed().await.is_err() {
                    return;
                }
            }
        }));
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
                    view.unread_total = snapshot
                        .rooms
                        .iter()
                        .fold(0u64, |sum, room| sum.saturating_add(room.unread));
                    view.rooms = snapshot.rooms;
                    for room in &mut view.rooms {
                        room.presence = room
                            .partner
                            .as_ref()
                            .and_then(|id| view.presence.get(id))
                            .cloned();
                    }
                    view.notifications = snapshot.notifications;
                    view.invites = snapshot.invites;
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
        drop(state);
        self.observe_presence(Vec::new())
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.motion_task.abort();
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
