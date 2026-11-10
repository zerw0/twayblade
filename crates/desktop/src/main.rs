#![recursion_limit = "512"]

mod appearance;

use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Center, Element, Fill, Subscription, Task, Theme, theme};
use matrix_client_core::auth::{self, Account};
use matrix_client_core::conversation::{Conversation, ConversationSnapshot, MessageRow};
use matrix_client_core::rooms::{RoomSnapshot, RoomSummary};
use matrix_client_core::spaces::{SpaceSnapshot, SpaceSummary};
use matrix_client_core::{LoginDiscovery, discover_login, validate_homeserver};
use std::collections::HashMap;
use zeroize::Zeroizing;

fn main() -> iced::Result {
    let application = iced::application(App::new, App::update, App::view)
        .default_font(appearance::SYSTEM)
        .title("Matrix Client")
        .window(iced::window::Settings {
            size: (640.0, 700.0).into(),
            min_size: Some((480.0, 560.0).into()),
            ..Default::default()
        })
        .centered()
        .theme(App::theme)
        .subscription(App::subscription);
    #[cfg(target_os = "macos")]
    let application = match std::fs::read("/System/Library/Fonts/SFNS.ttf") {
        Ok(font) => application.font(font),
        Err(_) => application,
    };
    application.run()
}

struct App {
    homeserver: String,
    discovering: bool,
    status: String,
    discovery: Option<LoginDiscovery>,
    username: String,
    password: Zeroizing<String>,
    account: Option<Account>,
    authenticating: bool,
    theme_mode: theme::Mode,
    rooms: Vec<RoomSummary>,
    room_search: String,
    selected_room: Option<String>,
    sync_status: &'static str,
    preview: bool,
    preview_workspace: bool,
    sync_generation: u64,
    preview_capture: Option<String>,
    conversation: Option<Conversation>,
    messages: Vec<MessageRow>,
    conversation_status: String,
    drafts: HashMap<String, String>,
    sending: bool,
    paginating: bool,
    history_start: bool,
    conversation_generation: u64,
    pending_sends: HashMap<String, u64>,
    follow_latest: bool,
    spaces: Vec<SpaceSummary>,
    selected_space: Option<String>,
    spaces_loading: bool,
    account_menu_open: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            homeserver: "https://matrix.org".into(),
            discovering: false,
            status: "Enter your homeserver to discover its sign-in options.".into(),
            discovery: None,
            username: String::new(),
            password: Zeroizing::new(String::new()),
            account: None,
            authenticating: true,
            theme_mode: theme::Mode::Light,
            rooms: Vec::new(),
            room_search: String::new(),
            selected_room: None,
            sync_status: "Connecting…",
            preview: false,
            preview_workspace: false,
            sync_generation: 0,
            preview_capture: None,
            conversation: None,
            messages: Vec::new(),
            conversation_status: String::new(),
            drafts: HashMap::new(),
            sending: false,
            paginating: false,
            history_start: false,
            conversation_generation: 0,
            pending_sends: HashMap::new(),
            follow_latest: true,
            spaces: Vec::new(),
            selected_space: None,
            spaces_loading: true,
            account_menu_open: false,
        }
    }
}

#[derive(Clone)]
enum Message {
    HomeserverChanged(String),
    Discover,
    Discovered(Result<LoginDiscovery, String>),
    UsernameChanged(String),
    PasswordChanged(String),
    SignIn,
    SignedIn(Result<Account, String>),
    Restored(Result<Option<Account>, String>),
    SignOut,
    SignedOut(Result<(), String>),
    ThemeChanged(theme::Mode),
    ChangeServer,
    RoomsUpdated(RoomSnapshot),
    SearchRooms(String),
    SelectRoom(String),
    Keyboard(iced::keyboard::Event),
    Reconnect,
    CapturePreview,
    PreviewCaptured(iced::window::Screenshot),
    CaptureSaved(Result<(), String>),
    ConversationUpdated(ConversationSnapshot),
    DraftChanged(String),
    SendDraft,
    DraftQueued {
        account_instance: u128,
        generation: u64,
        room: String,
        body: String,
        result: Result<(), &'static str>,
    },
    LoadEarlier,
    TimelineScrolled(bool),
    JumpLatest,
    SpacesUpdated(SpaceSnapshot),
    SelectSpace(Option<String>),
    ToggleAccountMenu,
    FocusSearch,
    EarlierLoaded {
        generation: u64,
        result: Result<bool, &'static str>,
    },
}

// UI events can contain passwords. Debug output must never include event payloads.
impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UI event [REDACTED]")
    }
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let preview_home = std::env::args().any(|arg| arg == "--preview-home");
        let preview_conversation =
            preview_home || std::env::args().any(|arg| arg == "--preview-conversation");
        let preview_workspace =
            preview_conversation || std::env::args().any(|arg| arg == "--preview-workspace");
        if preview_workspace || std::env::args().any(|arg| arg == "--preview") {
            let args: Vec<_> = std::env::args().collect();
            let capture = args
                .iter()
                .position(|arg| arg == "--capture")
                .and_then(|index| args.get(index + 1))
                .cloned();
            let capture_task = if capture.is_some() {
                Task::perform(
                    async {
                        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                    },
                    |_| Message::CapturePreview,
                )
            } else {
                Task::none()
            };
            return (
                Self {
                    authenticating: false,
                    preview: true,
                    preview_workspace,
                    preview_capture: capture,
                    sync_status: "Interface preview · offline",
                    spaces: if preview_conversation {
                        preview_spaces()
                    } else {
                        Vec::new()
                    },
                    spaces_loading: false,
                    selected_space: (preview_conversation && !preview_home)
                        .then(|| "!preview-space:example.org".into()),
                    rooms: if preview_conversation {
                        preview_rooms()
                    } else {
                        Vec::new()
                    },
                    selected_room: preview_conversation.then(|| {
                        if preview_home {
                            "!preview-3:example.org".into()
                        } else {
                            "!preview:example.org".into()
                        }
                    }),
                    messages: if preview_conversation {
                        vec![MessageRow {
                            sender: "Interface preview".into(),
                            display_name: Some("Preview member".into()),
                            timestamp_ms: 1_794_300_000_000,
                            body: "Sample message for checking layout. This preview is offline and contains no account data.".into(),
                            delivery: "Preview",
                        }]
                    } else {
                        Vec::new()
                    },
                    conversation_status: if preview_conversation {
                        "Offline layout preview · sending disabled".into()
                    } else {
                        String::new()
                    },
                    ..Self::default()
                },
                Task::batch([
                    iced::system::theme().map(Message::ThemeChanged),
                    if preview_workspace {
                        workspace_window()
                    } else {
                        Task::none()
                    },
                    capture_task,
                ]),
            );
        }
        let app = Self {
            status: "Checking for a saved session…".into(),
            ..Self::default()
        };
        let task = Task::perform(
            async { auth::restore().await.map_err(|e| e.to_string()) },
            Message::Restored,
        );
        (
            app,
            Task::batch([task, iced::system::theme().map(Message::ThemeChanged)]),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleAccountMenu => self.account_menu_open = !self.account_menu_open,
            Message::FocusSearch => return iced::widget::operation::focus("room-search"),
            Message::SpacesUpdated(snapshot) => {
                if self
                    .account
                    .as_ref()
                    .is_some_and(|account| account.instance_id() == snapshot.account_instance)
                {
                    self.spaces = snapshot.spaces;
                    self.spaces_loading = false;
                    if self
                        .selected_space
                        .as_ref()
                        .is_some_and(|id| !self.spaces.iter().any(|space| &space.id == id))
                    {
                        self.selected_space = None;
                    }
                    self.clear_hidden_conversation();
                }
            }
            Message::SelectSpace(id) => {
                if id
                    .as_ref()
                    .is_some_and(|id| !self.spaces.iter().any(|space| &space.id == id))
                {
                    return Task::none();
                }
                self.selected_space = id;
                self.room_search.clear();
                if self
                    .selected_room
                    .as_ref()
                    .is_some_and(|id| !self.room_in_selected_space(id))
                {
                    self.selected_room = None;
                    self.conversation = None;
                    self.messages.clear();
                    self.conversation_generation = self.conversation_generation.wrapping_add(1);
                    if let Some(account) = &self.account {
                        account.select_room(None);
                    }
                }
            }
            Message::TimelineScrolled(at_latest) => self.follow_latest = at_latest,
            Message::JumpLatest => {
                self.follow_latest = true;
                return iced::widget::operation::snap_to_end("messages");
            }
            Message::CapturePreview if self.preview => {
                return iced::window::latest()
                    .and_then(iced::window::screenshot)
                    .map(Message::PreviewCaptured);
            }
            Message::CapturePreview => {}
            Message::PreviewCaptured(screenshot) if self.preview => {
                if let Some(path) = self.preview_capture.take() {
                    return Task::perform(
                        async move {
                            let mut bytes = format!(
                            "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n",
                            screenshot.size.width, screenshot.size.height,
                        ).into_bytes();
                            bytes.extend_from_slice(&screenshot.rgba);
                            tokio::fs::write(path, bytes)
                                .await
                                .map_err(|_| "Could not write preview capture".into())
                        },
                        Message::CaptureSaved,
                    );
                }
            }
            Message::PreviewCaptured(_) => {}
            Message::CaptureSaved(result) => {
                if let Err(error) = result {
                    eprintln!("{error}");
                }
                return iced::exit();
            }
            Message::Reconnect => {
                self.sync_generation = self.sync_generation.wrapping_add(1);
                self.sync_status = "Reconnecting…";
            }
            Message::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                use iced::keyboard::{Key, key::Named};
                if key == Key::Named(Named::Tab) {
                    return if modifiers.shift() {
                        iced::widget::operation::focus_previous()
                    } else {
                        iced::widget::operation::focus_next()
                    };
                }
                if cfg!(target_os = "macos")
                    && modifiers.command()
                    && matches!(&key, Key::Character(value) if value.as_str() == "q")
                {
                    return iced::exit();
                }
                if modifiers.command()
                    && matches!(&key, Key::Character(value) if value.as_str() == "f")
                    && self.account.is_some()
                {
                    return iced::widget::operation::focus("room-search");
                }
            }
            Message::Keyboard(_) => {}
            Message::RoomsUpdated(snapshot) => {
                if self
                    .account
                    .as_ref()
                    .is_some_and(|account| account.instance_id() == snapshot.account_instance)
                {
                    self.rooms = snapshot.rooms;
                    self.sync_status = snapshot.status;
                    if self
                        .selected_room
                        .as_ref()
                        .is_some_and(|id| !self.rooms.iter().any(|room| &room.id == id))
                    {
                        self.selected_room = None;
                        self.conversation = None;
                        self.messages.clear();
                        self.conversation_generation = self.conversation_generation.wrapping_add(1);
                        if let Some(account) = &self.account {
                            account.select_room(None);
                        }
                    }
                }
            }
            Message::SearchRooms(value) => self.room_search = value,
            Message::SelectRoom(id) => {
                if let Some(account) = &self.account {
                    account.select_room(Some(&id));
                }
                self.sending = self.pending_sends.contains_key(&id);
                self.selected_room = Some(id);
                self.conversation_generation = self.conversation_generation.wrapping_add(1);
                self.conversation = None;
                self.messages.clear();
                self.conversation_status = "Loading messages…".into();
                self.paginating = false;
                self.history_start = false;
                self.follow_latest = true;
            }
            Message::ConversationUpdated(snapshot) => {
                if self
                    .account
                    .as_ref()
                    .is_some_and(|account| account.instance_id() == snapshot.account_instance)
                    && self.selected_room.as_ref() == Some(&snapshot.room_id)
                    && self.conversation_generation == snapshot.generation
                {
                    self.conversation = snapshot.conversation;
                    self.messages = snapshot.messages;
                    self.conversation_status = snapshot.status.into();
                    if self.follow_latest && !self.paginating {
                        return iced::widget::operation::snap_to_end("messages");
                    }
                }
            }
            Message::DraftChanged(body) => {
                if let Some(room) = &self.selected_room {
                    self.drafts.insert(room.clone(), body);
                }
            }
            Message::SendDraft if !self.sending && !self.authenticating => {
                if let (Some(conversation), Some(room)) =
                    (self.conversation.clone(), self.selected_room.clone())
                {
                    let body = self.drafts.get(&room).cloned().unwrap_or_default();
                    if !body.trim().is_empty() {
                        self.sending = true;
                        let generation = self.conversation_generation;
                        self.pending_sends.insert(room.clone(), generation);
                        let account_instance = self
                            .account
                            .as_ref()
                            .map(Account::instance_id)
                            .unwrap_or_default();
                        return Task::perform(
                            async move {
                                let result = conversation.send(body.clone()).await;
                                Message::DraftQueued {
                                    account_instance,
                                    generation,
                                    room,
                                    body,
                                    result,
                                }
                            },
                            |message| message,
                        );
                    }
                }
            }
            Message::SendDraft => {}
            Message::DraftQueued {
                account_instance,
                generation,
                room,
                body,
                result,
            } => {
                if !self
                    .account
                    .as_ref()
                    .is_some_and(|account| account.instance_id() == account_instance)
                {
                    return Task::none();
                }
                if result.is_ok() && self.drafts.get(&room) == Some(&body) {
                    self.drafts.remove(&room);
                }
                if self.pending_sends.get(&room) == Some(&generation) {
                    self.pending_sends.remove(&room);
                }
                if self.selected_room.as_ref() == Some(&room) {
                    self.sending = false;
                    match result {
                        Ok(()) => {
                            return iced::widget::operation::snap_to_end("messages");
                        }
                        Err(error) => self.conversation_status = error.into(),
                    }
                }
            }
            Message::LoadEarlier if !self.paginating && !self.history_start => {
                if let Some(conversation) = self.conversation.clone() {
                    self.paginating = true;
                    self.follow_latest = false;
                    let generation = self.conversation_generation;
                    return Task::perform(
                        async move {
                            Message::EarlierLoaded {
                                generation,
                                result: conversation.older().await,
                            }
                        },
                        |message| message,
                    );
                }
            }
            Message::LoadEarlier => {}
            Message::EarlierLoaded { generation, result } => {
                if generation == self.conversation_generation {
                    self.paginating = false;
                    match result {
                        Ok(start) => self.history_start = start,
                        Err(error) => self.conversation_status = error.into(),
                    }
                }
            }
            Message::ThemeChanged(mode) => self.theme_mode = mode,
            Message::ChangeServer if !self.authenticating => {
                self.discovery = None;
                self.password = Zeroizing::new(String::new());
                self.status = "Enter your homeserver to sign in.".into();
            }
            Message::ChangeServer => {}
            Message::UsernameChanged(value) => self.username = value,
            Message::PasswordChanged(value) => self.password = Zeroizing::new(value),
            Message::SignIn
                if !self.preview && !self.authenticating && self.password_login_available() =>
            {
                self.authenticating = true;
                self.status = "Signing in…".into();
                let homeserver = self.homeserver.clone();
                let username = self.username.clone();
                let password = std::mem::take(&mut self.password);
                return Task::perform(
                    async move {
                        auth::login_password(homeserver, username, password)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::SignedIn,
                );
            }
            Message::SignIn => {}
            Message::SignedIn(result) => {
                self.authenticating = false;
                match result {
                    Ok(account) => {
                        self.status = "Signed in. Your session is saved securely.".into();
                        self.account = Some(account);
                        return workspace_window();
                    }
                    Err(error) => self.status = error,
                }
            }
            Message::Restored(result) => {
                self.authenticating = false;
                match result {
                    Ok(Some(account)) => {
                        self.status =
                            "Saved session loaded. Server validity is not yet checked.".into();
                        self.account = Some(account);
                        return workspace_window();
                    }
                    Ok(None) => self.status = "Enter your homeserver to sign in.".into(),
                    Err(error) => self.status = error,
                }
            }
            Message::SignOut if !self.authenticating => {
                if let Some(account) = self.account.clone() {
                    self.authenticating = true;
                    self.status = "Signing out…".into();
                    return Task::perform(
                        async move { account.logout().await.map_err(|e| e.to_string()) },
                        Message::SignedOut,
                    );
                }
            }
            Message::SignOut => {}
            Message::SignedOut(result) => {
                self.authenticating = false;
                match result {
                    Ok(()) => {
                        self.account = None;
                        self.discovery = None;
                        self.rooms.clear();
                        self.selected_room = None;
                        self.room_search.clear();
                        self.conversation = None;
                        self.messages.clear();
                        self.drafts.clear();
                        self.pending_sends.clear();
                        self.spaces.clear();
                        self.selected_space = None;
                        self.spaces_loading = true;
                        self.conversation_generation = self.conversation_generation.wrapping_add(1);
                        self.status = "Signed out.".into();
                    }
                    Err(error) => self.status = error,
                }
            }
            Message::HomeserverChanged(value) => {
                self.homeserver = value;
                self.discovery = None;
            }
            Message::Discover if !self.discovering && !self.authenticating => {
                if let Err(error) = validate_homeserver(&self.homeserver) {
                    self.status = error.to_string();
                    return Task::none();
                }
                self.discovering = true;
                self.discovery = None;
                self.status = "Checking sign-in options…".into();
                let homeserver = self.homeserver.clone();
                return Task::perform(
                    async move { discover_login(&homeserver).await.map_err(|e| e.to_string()) },
                    Message::Discovered,
                );
            }
            Message::Discover => {}
            Message::Discovered(result) => {
                self.discovering = false;
                match result {
                    Ok(discovery) => {
                        self.status = "Your server is ready.".into();
                        self.discovery = Some(discovery);
                    }
                    Err(error) => self.status = error,
                }
            }
        }
        Task::none()
    }

    fn theme(&self) -> Theme {
        appearance::theme(self.theme_mode)
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            iced::system::theme_changes().map(Message::ThemeChanged),
            iced::keyboard::listen().map(Message::Keyboard),
        ];
        if let Some(account) = &self.account
            && !self.authenticating
        {
            subscriptions.push(
                Subscription::run_with((account.clone(), self.sync_generation), |data| {
                    data.0.room_updates()
                })
                .map(Message::RoomsUpdated),
            );
            subscriptions.push(
                Subscription::run_with((account.clone(), self.sync_generation), |data| {
                    data.0.space_updates()
                })
                .map(Message::SpacesUpdated),
            );
            if let Some(room) = &self.selected_room {
                subscriptions.push(
                    Subscription::run_with(
                        (account.clone(), room.clone(), self.conversation_generation),
                        |data| data.0.conversation_updates(data.1.clone(), data.2),
                    )
                    .map(Message::ConversationUpdated),
                );
            }
        }
        Subscription::batch(subscriptions)
    }

    fn password_login_available(&self) -> bool {
        self.discovery
            .as_ref()
            .and_then(|discovery| discovery.legacy_flows.as_ref())
            .is_some_and(|flows| flows.iter().any(|flow| flow == "m.login.password"))
    }

    fn room_in_selected_space(&self, room_id: &str) -> bool {
        self.selected_space.as_ref().map_or_else(
            || {
                !self.spaces_loading
                    && !self
                        .spaces
                        .iter()
                        .any(|space| space.room_ids.contains(room_id))
            },
            |id| {
                self.spaces
                    .iter()
                    .find(|space| &space.id == id)
                    .is_some_and(|space| space.room_ids.contains(room_id))
            },
        )
    }

    fn clear_hidden_conversation(&mut self) {
        if self
            .selected_room
            .as_ref()
            .is_some_and(|id| !self.room_in_selected_space(id))
        {
            self.selected_room = None;
            self.conversation = None;
            self.messages.clear();
            self.conversation_generation = self.conversation_generation.wrapping_add(1);
            if let Some(account) = &self.account {
                account.select_room(None);
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let theme = self.theme();
        let secondary = appearance::muted(&theme);
        if self.preview_workspace {
            return self.workspace("Interface preview · no account connected".into(), &theme);
        }
        if let Some(account) = &self.account {
            return self.workspace(account.user_id(), &theme);
        }
        let mark = container(text("[m]").size(28).color(theme.palette().primary))
            .width(64)
            .height(64)
            .center_x(Fill)
            .center_y(Fill)
            .style(appearance::panel);
        let mut content = column![
            mark,
            text("Welcome to Matrix").size(26).font(appearance::BOLD),
            text("A little closer to your conversations.")
                .size(14)
                .color(secondary),
        ]
        .spacing(14)
        .align_x(Center)
        .max_width(360);
        if let Some(discovery) = &self.discovery {
            let server_name = discovery
                .homeserver
                .trim_start_matches("https://")
                .trim_end_matches('/');
            let mut change_server = button(text("Change").size(12)).style(button::text);
            if !self.authenticating {
                change_server = change_server.on_press(Message::ChangeServer);
            }
            content = content.push(
                container(
                    row![
                        text(server_name).size(13),
                        iced::widget::space().width(Fill),
                        change_server
                    ]
                    .align_y(Center)
                    .spacing(8),
                )
                .padding([8, 12])
                .width(Fill)
                .style(appearance::panel),
            );
            if self.password_login_available() {
                let mut username = text_input("Username or Matrix ID", &self.username)
                    .size(14)
                    .padding([10, 12])
                    .style(appearance::input);
                let mut password = text_input("Password", self.password.as_str())
                    .secure(true)
                    .size(14)
                    .padding([10, 12])
                    .style(appearance::input);
                let mut sign_in = button(
                    text(if self.authenticating {
                        "Signing in…"
                    } else {
                        "Sign In"
                    })
                    .size(14)
                    .width(Fill)
                    .align_x(Center),
                )
                .width(Fill)
                .padding([10, 16])
                .style(appearance::primary);
                if !self.authenticating {
                    username = username.on_input(Message::UsernameChanged);
                    password = password
                        .on_input(Message::PasswordChanged)
                        .on_submit(Message::SignIn);
                    if !self.preview
                        && !self.username.trim().is_empty()
                        && !self.password.is_empty()
                    {
                        sign_in = sign_in.on_press(Message::SignIn);
                    }
                }
                content = content
                    .push(
                        column![
                            text("Account")
                                .size(12)
                                .font(appearance::SEMIBOLD)
                                .color(secondary),
                            username,
                            password
                        ]
                        .spacing(8)
                        .width(Fill),
                    )
                    .push(sign_in);
            } else {
                content = content.push(
                    text(
                        if discovery.oauth_metadata_available
                            || discovery
                                .legacy_flows
                                .as_ref()
                                .is_some_and(|flows| flows.iter().any(|flow| flow == "m.login.sso"))
                        {
                            "This server uses browser sign-in, which is coming in a future update."
                        } else {
                            "No supported sign-in method was found. Try another server."
                        },
                    )
                    .size(13),
                );
            }
        } else {
            let mut input = text_input("https://matrix.example.org", &self.homeserver)
                .size(14)
                .padding([10, 12])
                .style(appearance::input);
            let mut discover = button(
                text(if self.discovering {
                    "Connecting…"
                } else {
                    "Continue"
                })
                .size(14)
                .width(Fill)
                .align_x(Center),
            )
            .width(Fill)
            .padding([10, 16])
            .style(appearance::primary);
            if !self.discovering && !self.authenticating {
                input = input
                    .on_input(Message::HomeserverChanged)
                    .on_submit(Message::Discover);
                discover = discover.on_press(Message::Discover);
            }
            content = content
                .push(
                    column![
                        text("Homeserver")
                            .size(12)
                            .font(appearance::SEMIBOLD)
                            .color(secondary),
                        input,
                        text("The server where your Matrix account lives.")
                            .size(12)
                            .color(secondary),
                    ]
                    .spacing(8)
                    .width(Fill),
                )
                .push(discover);
        }
        content = content.push(text(&self.status).size(12).color(secondary));
        let footer = if self.discovery.is_some() && self.password_login_available() {
            "Your password is never saved."
        } else {
            "Private conversations. Your choice of server."
        };
        content = content.push(text(footer).size(12).color(secondary));
        container(scrollable(container(content).padding(32).center_x(Fill)))
            .center_x(Fill)
            .center_y(Fill)
            .into()
    }

    fn workspace<'a>(&'a self, user_id: String, theme: &Theme) -> Element<'a, Message> {
        let secondary = appearance::muted(theme);
        let selected_space = self
            .selected_space
            .as_ref()
            .and_then(|id| self.spaces.iter().find(|space| &space.id == id));
        let home_selected = self.selected_space.is_none();
        let home = button(
            container(appearance::icon(
                "home",
                19.0,
                if home_selected {
                    theme.palette().primary
                } else {
                    secondary
                },
            ))
            .center_x(Fill),
        )
        .width(Fill)
        .padding([11, 5])
        .style(move |theme, status| appearance::room_button(theme, status, home_selected))
        .on_press(Message::SelectSpace(None));
        let mut rail_items = column![
            iced::widget::tooltip(
                home,
                text("Home · rooms outside spaces").size(12),
                iced::widget::tooltip::Position::Right
            ),
            iced::widget::space().height(8),
        ]
        .spacing(8)
        .width(Fill);
        for space in &self.spaces {
            let selected = self.selected_space.as_ref() == Some(&space.id);
            let initial = space
                .name
                .chars()
                .next()
                .unwrap_or('#')
                .to_uppercase()
                .to_string();
            let unread = self.space_unread(space);
            let mut label = column![text(initial).size(15).font(appearance::SEMIBOLD)]
                .align_x(Center)
                .spacing(2);
            if unread > 0 {
                label = label.push(
                    text(if unread > 99 {
                        "99+".into()
                    } else {
                        unread.to_string()
                    })
                    .size(9),
                );
            }
            if space.level > 0 {
                label = label.push(text("↳").size(9));
            }
            let action = button(container(label).center_x(Fill))
                .width(Fill)
                .padding([9, 4])
                .style(move |theme, status| appearance::room_button(theme, status, selected))
                .on_press(Message::SelectSpace(Some(space.id.clone())));
            rail_items = rail_items.push(iced::widget::tooltip(
                action,
                text(format!(
                    "{}{}",
                    if space.level > 0 {
                        "Nested space: "
                    } else {
                        ""
                    },
                    space.name
                ))
                .size(12),
                iced::widget::tooltip::Position::Right,
            ));
        }
        let rail = container(scrollable(rail_items).height(Fill))
            .padding([16, 8])
            .width(56)
            .height(Fill)
            .style(appearance::sidebar);
        let search = text_input("Search", &self.room_search)
            .id("room-search")
            .size(12)
            .padding([8, 10])
            .style(appearance::input)
            .on_input(Message::SearchRooms);
        let mut list = column![].spacing(2).width(Fill);
        let query = self.room_search.to_lowercase();
        let mut count = 0;
        for room in self.rooms.iter().filter(|room| {
            !room.is_space
                && self.room_in_selected_space(&room.id)
                && room.name.to_lowercase().contains(&query)
        }) {
            count += 1;
            let selected = self.selected_room.as_deref() == Some(room.id.as_str());
            let mut labels = row![
                appearance::icon(
                    "room",
                    15.0,
                    if selected {
                        theme.palette().primary
                    } else {
                        secondary
                    }
                ),
                text(&room.name)
                    .font(if selected || room.unread > 0 {
                        appearance::SEMIBOLD
                    } else {
                        appearance::SYSTEM
                    })
                    .size(13)
                    .width(Fill),
            ]
            .spacing(9)
            .align_y(Center);
            if room.unread > 0 {
                labels = labels.push(
                    container(
                        text(room.unread.to_string())
                            .size(10)
                            .font(appearance::SEMIBOLD),
                    )
                    .padding([2, 6])
                    .style(appearance::panel),
                );
            }
            list = list.push(
                button(labels)
                    .width(Fill)
                    .padding([9, 10])
                    .style(move |theme, status| appearance::room_button(theme, status, selected))
                    .on_press(Message::SelectRoom(room.id.clone())),
            );
        }
        if count == 0 {
            list = list.push(
                container(
                    text(
                        if self.spaces_loading && !self.preview && selected_space.is_none() {
                            "Organizing your rooms…"
                        } else if !query.is_empty() {
                            "No matching conversations"
                        } else if selected_space.is_some() {
                            "No joined rooms in this space"
                        } else {
                            "Rooms outside spaces appear here. Choose a space to see its rooms."
                        },
                    )
                    .size(12)
                    .color(secondary),
                )
                .padding([12, 8]),
            );
        }
        let section_name = if selected_space.is_some() {
            "Rooms"
        } else {
            "Conversations"
        };
        let mut navigation = column![
            row![
                text(
                    selected_space
                        .map(|space| space.name.as_str())
                        .unwrap_or("Home")
                )
                .size(17)
                .font(appearance::BOLD)
                .width(Fill),
                button(appearance::icon("search", 16.0, secondary))
                    .padding(6)
                    .style(appearance::icon_button)
                    .on_press(Message::FocusSearch),
            ]
            .align_y(Center)
            .spacing(6),
            search,
            container(
                text(section_name)
                    .size(11)
                    .font(appearance::SEMIBOLD)
                    .color(secondary)
            )
            .padding([2, 6]),
            scrollable(list).height(Fill),
        ]
        .spacing(12);
        if self.account_menu_open {
            let mut sign_out = button(text("Sign Out").size(12)).style(button::text);
            let mut reconnect = button(text("Reconnect").size(12)).style(button::text);
            if !self.authenticating && !self.preview {
                sign_out = sign_out.on_press(Message::SignOut);
                reconnect = reconnect.on_press(Message::Reconnect);
            }
            navigation = navigation.push(
                container(
                    column![
                        text(user_id).size(11),
                        text(self.sync_status).size(11).color(secondary),
                        reconnect,
                        sign_out
                    ]
                    .spacing(8),
                )
                .width(Fill)
                .padding(12)
                .style(appearance::panel),
            );
        }
        let account_control = button(
            row![
                appearance::icon("account", 17.0, secondary),
                text(if self.preview { "Preview" } else { "Account" }).size(12),
                iced::widget::space().width(Fill),
                text(if self.account_menu_open { "⌄" } else { "⌃" })
                    .size(12)
                    .color(secondary),
            ]
            .align_y(Center)
            .spacing(8),
        )
        .width(Fill)
        .padding([9, 8])
        .style(appearance::icon_button)
        .on_press(Message::ToggleAccountMenu);
        navigation = navigation.push(account_control);
        let sidebar = container(navigation)
            .padding([16, 12])
            .width(248)
            .height(Fill)
            .style(appearance::sidebar);

        let selected = self
            .selected_room
            .as_ref()
            .and_then(|id| self.rooms.iter().find(|room| &room.id == id));
        let detail: Element<'_, Message> = if let Some(room) = selected {
            let mut history = column![].spacing(8).width(Fill);
            let earlier = button(
                text(if self.paginating {
                    "Loading…"
                } else {
                    "Load Earlier"
                })
                .size(12),
            )
            .style(button::text)
            .on_press_maybe(
                (self.conversation.is_some() && !self.paginating && !self.history_start)
                    .then_some(Message::LoadEarlier),
            );
            history = history.push(earlier);
            if self.messages.is_empty() {
                history = history.push(
                    text(if self.conversation.is_some() {
                        "No messages to display yet."
                    } else {
                        "Loading conversation…"
                    })
                    .size(14)
                    .color(secondary),
                );
            }
            for (index, message) in self.messages.iter().enumerate() {
                let grouped = index > 0 && can_group(&self.messages[index - 1], message);
                let display_name = message.display_name.as_deref().unwrap_or(&message.sender);
                let timestamp = message_time(message.timestamp_ms);
                let mut entry = column![].spacing(5).width(Fill);
                if !grouped {
                    entry = entry.push(
                        row![
                            iced::widget::tooltip(
                                text(display_name).size(13).font(appearance::SEMIBOLD),
                                text(&message.sender).size(12),
                                iced::widget::tooltip::Position::Top,
                            ),
                            iced::widget::space().width(Fill),
                            text(timestamp).size(11).color(secondary),
                        ]
                        .spacing(12),
                    );
                }
                entry = entry.push(text(&message.body).size(14));
                if !message.delivery.is_empty() {
                    entry = entry.push(text(message.delivery).size(11).color(secondary));
                }
                let avatar: Element<'_, Message> = if grouped {
                    iced::widget::space().width(28).into()
                } else {
                    container(
                        text(
                            display_name
                                .chars()
                                .next()
                                .unwrap_or('?')
                                .to_uppercase()
                                .to_string(),
                        )
                        .size(12)
                        .font(appearance::SEMIBOLD)
                        .color(theme.palette().primary),
                    )
                    .center_x(28)
                    .center_y(28)
                    .style(appearance::panel)
                    .into()
                };
                history = history.push(
                    container(row![avatar, entry].spacing(10).width(Fill))
                        .padding([if grouped { 0 } else { 10 }, 0]),
                );
            }
            let draft = self.drafts.get(&room.id).map(String::as_str).unwrap_or("");
            let mut composer = text_input("Message", draft)
                .size(14)
                .padding([8, 6])
                .style(appearance::composer_input);
            let mut send = button(appearance::icon("send", 16.0, iced::Color::WHITE))
                .padding(7)
                .style(appearance::primary);
            if self.conversation.is_some() && !self.authenticating {
                composer = composer.on_input(Message::DraftChanged);
                if !self.sending && !draft.trim().is_empty() {
                    composer = composer.on_submit(Message::SendDraft);
                    send = send.on_press(Message::SendDraft);
                }
            }
            column![
                container(
                    row![
                        container(appearance::icon("room", 17.0, theme.palette().primary))
                            .center_x(32)
                            .center_y(32)
                            .style(appearance::panel),
                        column![
                            text(&room.name).size(14).font(appearance::SEMIBOLD),
                            text(
                                selected_space
                                    .map(|space| space.name.as_str())
                                    .unwrap_or("Conversation")
                            )
                            .size(11)
                            .color(secondary),
                        ]
                        .spacing(2),
                        iced::widget::space().width(Fill),
                        button(appearance::icon("search", 17.0, secondary))
                            .padding(7)
                            .style(appearance::icon_button)
                            .on_press(Message::FocusSearch),
                    ]
                    .spacing(10)
                    .align_y(Center)
                )
                .padding([10, 18])
                .style(appearance::toolbar),
                scrollable(container(history).padding([12, 20]).width(Fill))
                    .id("messages")
                    .on_scroll(|viewport| {
                        let remaining = viewport.content_bounds().height
                            - viewport.bounds().height
                            - viewport.absolute_offset().y;
                        Message::TimelineScrolled(remaining <= 40.0)
                    })
                    .height(Fill),
                if !self.follow_latest {
                    container(
                        button(text("Jump to Latest").size(12))
                            .style(button::text)
                            .on_press(Message::JumpLatest),
                    )
                    .center_x(Fill)
                } else {
                    container(iced::widget::space().height(0))
                },
                container(text(&self.conversation_status).size(12).color(secondary))
                    .padding([0, 24]),
                container(
                    container(row![composer, send].spacing(8).align_y(Center))
                        .padding([5, 8])
                        .style(appearance::panel)
                )
                .padding([12, 18]),
            ]
            .into()
        } else {
            container(
                column![
                    text(if self.selected_space.is_some() {
                        "Choose a conversation"
                    } else {
                        "Your inbox"
                    })
                    .size(22)
                    .font(appearance::SEMIBOLD),
                    text("Choose a conversation in the sidebar.")
                        .size(14)
                        .color(secondary),
                ]
                .spacing(10)
                .align_x(Center),
            )
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill)
            .into()
        };
        row![
            rail,
            sidebar,
            container(
                container(detail)
                    .width(Fill)
                    .height(Fill)
                    .style(appearance::conversation_surface)
            )
            .padding([8, 10])
            .width(Fill)
            .height(Fill)
            .style(appearance::sidebar),
        ]
        .into()
    }

    fn space_unread(&self, space: &SpaceSummary) -> u64 {
        self.rooms
            .iter()
            .filter(|room| !room.is_space && space.room_ids.contains(&room.id))
            .fold(0_u64, |total, room| total.saturating_add(room.unread))
    }
}

fn workspace_window() -> Task<Message> {
    iced::window::latest().and_then(|id| iced::window::resize(id, iced::Size::new(1120.0, 760.0)))
}

fn message_time(timestamp_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(timestamp_ms)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%d %b · %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

fn can_group(previous: &MessageRow, current: &MessageRow) -> bool {
    previous.sender == current.sender
        && (0..300_000).contains(&current.timestamp_ms.saturating_sub(previous.timestamp_ms))
}

fn preview_rooms() -> Vec<RoomSummary> {
    [
        "Design discussion",
        "A longer conversation name for layout checks",
        "Design discussion",
        "Personal notes",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, name)| RoomSummary {
        id: if index == 0 {
            "!preview:example.org".into()
        } else {
            format!("!preview-{index}:example.org")
        },
        name: name.into(),
        unread: if index == 0 { 3 } else { 0 },
        is_space: false,
    })
    .collect()
}

fn preview_spaces() -> Vec<SpaceSummary> {
    vec![
        SpaceSummary {
            id: "!preview-space:example.org".into(),
            name: "Design team".into(),
            level: 0,
            room_ids: [
                "!preview:example.org",
                "!preview-1:example.org",
                "!preview-nested:example.org",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        },
        SpaceSummary {
            id: "!preview-nested:example.org".into(),
            name: "Research".into(),
            level: 1,
            room_ids: ["!preview-1:example.org"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        },
        SpaceSummary {
            id: "!preview-other:example.org".into(),
            name: "Friends".into(),
            level: 0,
            room_ids: ["!preview-2:example.org"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn private_room() -> RoomSummary {
        RoomSummary {
            id: "!private:example.org".into(),
            name: "Private conversation".into(),
            unread: 2,
            is_space: false,
        }
    }

    #[test]
    fn password_event_debug_does_not_disclose_input() {
        let secret = "test-only-password-input";
        let event = Message::PasswordChanged(secret.into());
        assert!(!format!("{event:?}").contains(secret));
    }

    #[test]
    fn completed_logout_clears_visible_account_state() {
        let mut app = App {
            rooms: vec![private_room()],
            selected_room: Some("!private:example.org".into()),
            room_search: "Private".into(),
            ..App::default()
        };
        let _ = app.update(Message::SignedOut(Ok(())));
        assert!(app.rooms.is_empty());
        assert!(app.selected_room.is_none());
        assert!(app.room_search.is_empty());
    }

    #[test]
    fn late_room_updates_are_ignored_after_logout() {
        let mut app = App::default();
        let _ = app.update(Message::RoomsUpdated(RoomSnapshot {
            account_instance: 123,
            rooms: vec![private_room()],
            status: "Connected",
        }));
        assert!(app.rooms.is_empty());
    }

    #[test]
    fn switching_rooms_keeps_each_draft_and_tracks_pending_send() {
        let mut app = App::default();
        let _ = app.update(Message::SelectRoom("!one:example.org".into()));
        let _ = app.update(Message::DraftChanged("first draft".into()));
        app.pending_sends.insert("!one:example.org".into(), 1);
        let _ = app.update(Message::SelectRoom("!two:example.org".into()));
        let _ = app.update(Message::DraftChanged("second draft".into()));
        assert!(!app.sending);
        let _ = app.update(Message::SelectRoom("!one:example.org".into()));
        assert!(app.sending);
        assert_eq!(app.drafts.get("!one:example.org").unwrap(), "first draft");
        assert_eq!(app.drafts.get("!two:example.org").unwrap(), "second draft");
    }

    #[test]
    fn late_queue_completion_cannot_erase_a_logged_out_draft() {
        let mut app = App::default();
        app.drafts
            .insert("!room:example.org".into(), "new draft".into());
        let _ = app.update(Message::DraftQueued {
            account_instance: 123,
            generation: 0,
            room: "!room:example.org".into(),
            body: "new draft".into(),
            result: Ok(()),
        });
        assert_eq!(app.drafts.get("!room:example.org").unwrap(), "new draft");
    }

    #[test]
    fn late_conversation_snapshot_cannot_display_messages_after_logout() {
        let mut app = App {
            selected_room: Some("!room:example.org".into()),
            ..App::default()
        };
        let _ = app.update(Message::ConversationUpdated(ConversationSnapshot {
            account_instance: 123,
            generation: 0,
            room_id: "!room:example.org".into(),
            conversation: None,
            messages: vec![MessageRow {
                sender: "test".into(),
                display_name: None,
                timestamp_ms: 0,
                body: "private".into(),
                delivery: "",
            }],
            status: "",
        }));
        assert!(app.messages.is_empty());
    }

    #[test]
    fn message_grouping_uses_identity_and_a_forward_time_window() {
        let first = MessageRow {
            sender: "@one:example.org".into(),
            display_name: Some("Sam".into()),
            timestamp_ms: 1_000_000,
            body: "test".into(),
            delivery: "",
        };
        let mut next = first.clone();
        next.timestamp_ms += 60_000;
        assert!(can_group(&first, &next));
        next.sender = "@two:example.org".into();
        assert!(!can_group(&first, &next));
        next.sender = first.sender.clone();
        next.timestamp_ms = first.timestamp_ms - 1;
        assert!(!can_group(&first, &next));
        next.timestamp_ms = first.timestamp_ms + 300_000;
        assert!(!can_group(&first, &next));
    }

    #[test]
    fn selecting_space_filters_by_id_and_preserves_other_room_drafts() {
        let mut app = App {
            spaces: preview_spaces(),
            rooms: preview_rooms(),
            spaces_loading: false,
            selected_room: Some("!preview-2:example.org".into()),
            ..App::default()
        };
        app.drafts
            .insert("!preview-2:example.org".into(), "unsent".into());
        let _ = app.update(Message::SelectSpace(Some(
            "!preview-space:example.org".into(),
        )));
        assert!(app.room_in_selected_space("!preview:example.org"));
        assert!(app.room_in_selected_space("!preview-1:example.org"));
        assert!(!app.room_in_selected_space("!preview-2:example.org"));
        assert!(app.selected_room.is_none());
        assert_eq!(app.drafts.get("!preview-2:example.org").unwrap(), "unsent");
        let _ = app.update(Message::SelectSpace(None));
        assert!(!app.room_in_selected_space("!preview-2:example.org"));
        assert!(app.room_in_selected_space("!preview-3:example.org"));
    }

    #[test]
    fn unknown_space_selection_does_not_hide_conversations() {
        let mut app = App {
            spaces_loading: false,
            ..App::default()
        };
        let _ = app.update(Message::SelectSpace(Some("!missing:example.org".into())));
        assert!(app.selected_space.is_none());
        assert!(app.room_in_selected_space("!room:example.org"));
    }

    #[test]
    fn space_badge_counts_room_ids_once_and_excludes_other_spaces() {
        let app = App {
            spaces: preview_spaces(),
            rooms: preview_rooms(),
            ..App::default()
        };
        assert_eq!(app.space_unread(&app.spaces[0]), 3);
        assert_eq!(app.space_unread(&app.spaces[1]), 0);
        assert_eq!(app.space_unread(&app.spaces[2]), 0);
    }

    #[test]
    fn stale_space_updates_are_ignored_after_logout() {
        let mut app = App::default();
        let _ = app.update(Message::SpacesUpdated(SpaceSnapshot {
            account_instance: 123,
            spaces: preview_spaces(),
        }));
        assert!(app.spaces.is_empty());
    }

    #[test]
    fn home_excludes_rooms_in_every_space_but_keeps_ungrouped_rooms() {
        let app = App {
            spaces: preview_spaces(),
            rooms: preview_rooms(),
            spaces_loading: false,
            ..App::default()
        };
        assert!(!app.room_in_selected_space("!preview:example.org"));
        assert!(!app.room_in_selected_space("!preview-1:example.org"));
        assert!(!app.room_in_selected_space("!preview-2:example.org"));
        assert!(app.room_in_selected_space("!preview-3:example.org"));
    }

    #[test]
    fn home_waits_for_space_membership_before_listing_rooms() {
        let app = App::default();
        assert!(!app.room_in_selected_space("!room:example.org"));
    }

    #[test]
    fn moving_a_room_into_a_space_clears_it_from_home_without_losing_draft() {
        let mut app = App {
            spaces: preview_spaces(),
            spaces_loading: false,
            selected_room: Some("!preview:example.org".into()),
            ..App::default()
        };
        app.drafts
            .insert("!preview:example.org".into(), "keep this".into());
        app.clear_hidden_conversation();
        assert!(app.selected_room.is_none());
        assert_eq!(app.drafts.get("!preview:example.org").unwrap(), "keep this");
    }
}
