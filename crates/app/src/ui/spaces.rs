//! Native space directory, membership confirmation and conversation creation.
use crate::theme::scaled_px as px;
use crate::ui::chrome::ToolbarControl;
use crate::{
    app::MatrixApp,
    backend::{SpaceAction, SpaceOutcome},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        input::{Input, InputState},
        *,
    },
    *,
};
use std::collections::{HashMap, HashSet};
use twayblade_core::space_actions::{
    CreateRoom, DirectoryRoom, DirectorySnapshot, JoinAccess, LeaveCandidate,
};

#[derive(Clone)]
pub(crate) enum ManageKind {
    Directory(String),
    Leave(String),
    Create,
    Dm,
    JoinSpace,
    /// Invite someone to this room (room ID).
    InviteMember(String),
    /// Leave this room or direct message (room ID).
    LeaveRoom(String),
    /// Share your matrix.to link so someone can message you.
    InviteContact,
    /// Confirm signing the open account out of this device.
    SignOut,
    /// Set or change your status message.
    StatusMessage,
    /// Public rooms of your server or a large public homeserver.
    Explore,
    /// Create a poll in this room (room ID) (#181).
    CreatePoll(String),
    /// Pick a sticker to send in this room (room ID) (#183).
    Stickers(String),
    /// Create a new space.
    CreateSpace,
}
impl ManageKind {
    /// Short forms open as a small centred dialog; browsing and leaving keep
    /// the full conversation area because they list many rooms.
    pub fn is_dialog(&self) -> bool {
        matches!(
            self,
            Self::Create
                | Self::Dm
                | Self::JoinSpace
                | Self::InviteMember(_)
                | Self::LeaveRoom(_)
                | Self::InviteContact
                | Self::SignOut
                | Self::StatusMessage
                | Self::CreatePoll(_)
                | Self::Stickers(_)
                | Self::CreateSpace
        )
    }
}
pub(crate) struct Management {
    pub _subscriptions: Vec<Subscription>,
    pub kind: ManageKind,
    pub request: u64,
    pub busy: bool,
    pub status: String,
    pub directory: Option<DirectorySnapshot>,
    pub leave: Vec<LeaveCandidate>,
    pub selected: HashSet<String>,
    pub name: Entity<InputState>,
    pub topic: Entity<InputState>,
    pub encrypted: bool,
    pub voice_channel: bool,
    /// Create Space: anyone can join (else invitation only).
    pub public: bool,
    pub subspace: bool,
    pub parent: Option<String>,
    /// The dialog sheet: Tab stays inside it while it is open.
    pub focus: FocusHandle,
    /// The contact link was copied; the button says so until the dialog closes.
    pub copied: bool,
    pub parent_from_context: bool,
    /// Create Poll (#181): one field per answer, and whether results show
    /// before the poll ends.
    pub poll_answers: Vec<Entity<InputState>>,
    pub poll_disclosed: bool,
    /// Stickers: the packs, once loaded.
    pub sticker_packs: Option<Vec<twayblade_core::stickers::StickerPack>>,
    /// Space home: subspaces shown open, their loaded rooms, and the ones
    /// still loading or failed.
    pub expanded: HashSet<String>,
    pub children: HashMap<String, DirectorySnapshot>,
    pub loading: HashSet<String>,
    pub failed: HashSet<String>,
    /// The full page a dialog opened over (Invite on the space home), shown
    /// again when the dialog closes.
    pub behind: Option<Box<Management>>,
}
impl MatrixApp {
    pub fn open_management(
        &mut self,
        kind: ManageKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_management_inner(kind, true, window, cx);
    }

    pub(crate) fn open_management_inner(
        &mut self,
        kind: ManageKind,
        record_history: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let before = self.current_nav_page();
        let is_dialog = kind.is_dialog();
        if !is_dialog {
            self.pending_history_thread = None;
            self.search_open = false;
        }
        self.close_viewer(window, cx);
        self.close_thread(cx);
        self.close_profile(cx);
        self.details_open = false;
        self.management_request = self.management_request.wrapping_add(1);
        let request = self.management_request;
        let initial_parent = self
            .management
            .as_ref()
            .and_then(|p| p.directory.as_ref().map(|d| d.space.clone()))
            .or_else(|| self.navigation.selected_space.clone())
            .filter(|id| self.snapshot.spaces.iter().any(|s| &s.id == id));
        // The status dialog opens on the current status, ready to edit.
        let current_status = matches!(kind, ManageKind::StatusMessage)
            .then(|| {
                self.snapshot
                    .own_presence
                    .as_ref()
                    .and_then(|presence| presence.status_msg.clone())
            })
            .flatten()
            .unwrap_or_default();
        let name = cx.new(|cx| {
            let input = InputState::new(window, cx);
            match kind {
                ManageKind::StatusMessage => input
                    .placeholder("What are you up to?")
                    .default_value(current_status),
                ManageKind::Explore => input.placeholder("Search rooms by name or topic"),
                ManageKind::CreatePoll(_) => input.placeholder("Ask a question"),
                ManageKind::Directory(_) => input.placeholder("Search"),
                _ => input,
            }
        });
        let topic = cx.new(|cx| InputState::new(window, cx));
        let subscriptions = [&name, &topic]
            .into_iter()
            .map(|input| {
                cx.subscribe_in(
                    input,
                    window,
                    |this, _, event: &gpui_kit::component::input::InputEvent, window, cx| {
                        // Explore searches the directory as you type; the
                        // backend debounces and drops superseded queries.
                        if matches!(event, gpui_kit::component::input::InputEvent::Change)
                            && this
                                .management
                                .as_ref()
                                .is_some_and(|page| matches!(page.kind, ManageKind::Explore))
                        {
                            this.explore_search(window, cx);
                        }
                        cx.notify()
                    },
                )
            })
            .collect();
        let (action, selected) = match &kind {
            ManageKind::Directory(id) => (Some(SpaceAction::Directory(id.clone())), HashSet::new()),
            ManageKind::Leave(id) => (
                Some(SpaceAction::PlanLeave(id.clone())),
                HashSet::from([id.clone()]),
            ),
            _ => (None, HashSet::new()),
        };
        let behind = if is_dialog {
            self.management
                .take()
                .filter(|page| !page.kind.is_dialog())
                .map(Box::new)
        } else {
            None
        };
        self.management = Some(Management {
            _subscriptions: subscriptions,
            kind,
            request,
            busy: action.is_some() && !self.preview,
            status: if self.preview {
                "Sample data · No account connected".into()
            } else {
                String::new()
            },
            directory: None,
            leave: Vec::new(),
            selected,
            name,
            topic,
            encrypted: true,
            public: false,
            voice_channel: false,
            subspace: false,
            parent_from_context: initial_parent.is_some(),
            parent: initial_parent,
            focus: cx.focus_handle(),
            copied: false,
            poll_answers: Vec::new(),
            poll_disclosed: true,
            sticker_packs: None,
            expanded: HashSet::new(),
            children: HashMap::new(),
            loading: HashSet::new(),
            failed: HashSet::new(),
            behind,
        });
        // Dialogs with a text field start in it, so typing works at once. The
        // leave confirmation starts on the sheet itself, not on its
        // destructive button, so Return alone never leaves a room.
        if let Some(page) = &self.management {
            match page.kind {
                ManageKind::Create
                | ManageKind::Dm
                | ManageKind::JoinSpace
                | ManageKind::InviteMember(_)
                | ManageKind::StatusMessage
                | ManageKind::CreatePoll(_)
                | ManageKind::CreateSpace
                | ManageKind::Explore => page.name.update(cx, |state, cx| state.focus(window, cx)),
                ManageKind::LeaveRoom(_) | ManageKind::InviteContact | ManageKind::SignOut => {
                    page.focus.focus(window, cx)
                }
                _ => {}
            }
        }
        if record_history && !is_dialog {
            self.record_history(before);
        }
        if self.preview {
            if let Some(page) = &mut self.management {
                match &page.kind {
                    ManageKind::Directory(id) => {
                        page.directory = Some(DirectorySnapshot {
                            space: id.clone(),
                            name: "Design team".into(),
                            topic: Some("Product design, research and critique".into()),
                            members: 24,
                            access: JoinAccess::Invite,
                            avatar: None,
                            rooms: vec![
                                DirectoryRoom {
                                    id: "general".into(),
                                    name: "General".into(),
                                    joined: true,
                                    members: 24,
                                    access: JoinAccess::Public,
                                    ..Default::default()
                                },
                                DirectoryRoom {
                                    id: "sample-research".into(),
                                    name: "Research lab".into(),
                                    topic: Some("Experiments and early ideas".into()),
                                    suggested: true,
                                    members: 8,
                                    access: JoinAccess::Public,
                                    ..Default::default()
                                },
                                DirectoryRoom {
                                    id: "sample-private".into(),
                                    name: "Private planning".into(),
                                    access: JoinAccess::Invite,
                                    members: 3,
                                    ..Default::default()
                                },
                                DirectoryRoom {
                                    id: "sample-restricted".into(),
                                    name: "Members lounge".into(),
                                    access: JoinAccess::Restricted,
                                    members: 17,
                                    ..Default::default()
                                },
                                DirectoryRoom {
                                    id: "sample-nested".into(),
                                    name: "Product team".into(),
                                    space: true,
                                    joined: true,
                                    access: JoinAccess::Public,
                                    members: 15,
                                    children: 2,
                                    ..Default::default()
                                },
                            ],
                            end_reached: true,
                            inaccessible: 1,
                        })
                    }
                    ManageKind::Leave(id) => {
                        page.leave = vec![
                            LeaveCandidate {
                                id: id.clone(),
                                name: "Design team".into(),
                                space: true,
                                ..Default::default()
                            },
                            LeaveCandidate {
                                id: "general".into(),
                                name: "General".into(),
                                shared: true,
                                ..Default::default()
                            },
                        ]
                    }
                    _ => {}
                }
            }
        } else if let Some(action) = action {
            self.run_space_action(action, window, cx);
        }
        cx.notify();
    }

    pub fn run_space_action(
        &mut self,
        action: SpaceAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview {
            return;
        }
        let Some(page) = &mut self.management else {
            return;
        };
        page.busy = true;
        page.status.clear();
        let request = page.request;
        let account = self.snapshot.account_instance;
        let joining = match &action {
            SpaceAction::Join(id, _) => Some(id.clone()),
            _ => None,
        };
        self.job(
            window,
            cx,
            move |b| b.space_action(account, request, action),
            move |this, result, window, cx| {
                if this.snapshot.account_instance != account
                    || !this
                        .management
                        .as_ref()
                        .is_some_and(|p| p.request == request)
                {
                    return;
                }
                let page = this.management.as_mut().expect("current page");
                page.busy = false;
                match result {
                    Ok(SpaceOutcome::Directory(data)) => page.directory = Some(data),
                    Ok(SpaceOutcome::Leave(rooms)) => page.leave = rooms,
                    Ok(SpaceOutcome::Room(id)) => {
                        if let Some(joining) = joining
                            && let Some(directory) = &mut page.directory
                        {
                            for room in directory
                                .rooms
                                .iter_mut()
                                .chain(page.children.values_mut().flat_map(|c| c.rooms.iter_mut()))
                                .filter(|r| r.id == joining)
                            {
                                room.joined = true;
                                room.invited = false;
                            }
                        } else {
                            let space = matches!(
                                page.kind,
                                ManageKind::JoinSpace | ManageKind::CreateSpace
                            );
                            if matches!(page.kind, ManageKind::Dm) {
                                this.navigation.selected_space = None;
                            } else if matches!(page.kind, ManageKind::Create) {
                                this.navigation.selected_space = page.parent.clone();
                            }
                            this.management = None;
                            if space {
                                this.select_space(Some(id), window, cx);
                            } else {
                                this.select_room(id, window, cx);
                            }
                        }
                    }
                    Ok(SpaceOutcome::PartialRoom(id)) => {
                        this.management = None;
                        this.select_room(id, window, cx);
                        this.status =
                            "The room was created, but its space links could not be completed."
                                .into();
                    }
                    Ok(SpaceOutcome::Done) => {
                        if matches!(page.kind, ManageKind::Create) && page.subspace {
                            this.navigation.selected_space = page.parent.clone();
                            this.status = "Subspace created.".into();
                        }
                        if matches!(page.kind, ManageKind::InviteMember(_)) {
                            this.status = "Invitation sent.".into();
                        }
                        if matches!(page.kind, ManageKind::LeaveRoom(_)) {
                            this.status = "You left the room.".into();
                        }
                        this.close_management_page(window, cx);
                    }
                    Err(error) => page.status = error.to_string(),
                }
            },
        );
        cx.notify();
    }

    pub fn respond_invite(
        &mut self,
        id: String,
        accept: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview || !self.invite_pending.insert(id.clone()) {
            return;
        }
        let account = self.snapshot.account_instance;
        let target = id.clone();
        self.job(
            window,
            cx,
            move |b| b.space_action(account, 0, SpaceAction::Invite(id, accept)),
            move |this, result, _, _| {
                if this.snapshot.account_instance != account {
                    return;
                }
                this.invite_pending.remove(&target);
                match result {
                    Ok(_) => this.snapshot.invites.retain(|i| i.id != target),
                    Err(e) => this.status = e.to_string(),
                }
            },
        );
        cx.notify();
    }

    pub fn invite_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut section = v_flex()
            .id("invites")
            .role(Role::Group)
            .aria_label("Invites")
            .gap(px(6.))
            .child(
                div()
                    .px(px(6.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child("Invites"),
            );
        for invite in &self.snapshot.invites {
            let kind = if invite.space {
                "Space invitation"
            } else if invite.direct {
                "Direct message invitation"
            } else {
                "Room invitation"
            };
            let accept_id = invite.id.clone();
            let decline_id = accept_id.clone();
            let disabled = self.preview || self.invite_pending.contains(&invite.id);
            section = section.child(
                v_flex()
                    .id(SharedString::from(format!("invite-{}", invite.id)))
                    .role(Role::Group)
                    .aria_label(format!("{}, {kind}", invite.name))
                    .px(px(6.))
                    .gap(px(4.))
                    .child(div().truncate().child(invite.name.clone()))
                    .child(div().text_color(cx.theme().muted_foreground).child(kind))
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .child(
                                Button::new(SharedString::from(format!("accept-{}", invite.id)))
                                    .small()
                                    .label("Accept")
                                    .accessibility_label(format!("Accept {}", invite.name))
                                    .tooltip(format!("Accept {}", invite.name))
                                    .disabled(disabled)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.respond_invite(accept_id.clone(), true, window, cx)
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!("decline-{}", invite.id)))
                                    .ghost()
                                    .small()
                                    .label("Decline")
                                    .accessibility_label(format!("Decline {}", invite.name))
                                    .tooltip(format!("Decline {}", invite.name))
                                    .disabled(disabled)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.respond_invite(decline_id.clone(), false, window, cx)
                                    })),
                            ),
                    ),
            );
        }
        section.into_any_element()
    }

    /// The heading of a management page or dialog, also its accessible name.
    pub fn management_title(kind: &ManageKind) -> &'static str {
        match kind {
            ManageKind::Directory(_) => "Space Home",
            ManageKind::Leave(_) => "Leave Space",
            ManageKind::Create => "Create Channel or Subspace",
            ManageKind::Dm => "Start a Direct Message",
            ManageKind::JoinSpace => "Join a Space",
            ManageKind::InviteMember(_) => "Invite People",
            ManageKind::LeaveRoom(_) => "Leave Room",
            ManageKind::InviteContact => "Invite a Contact",
            ManageKind::SignOut => "Sign Out",
            ManageKind::StatusMessage => "Status Message",
            ManageKind::CreatePoll(_) => "Create Poll",
            ManageKind::Stickers(_) => "Stickers",
            ManageKind::Explore => "Explore Rooms",
            ManageKind::CreateSpace => "Create Space",
        }
    }

    pub fn management_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let page = self.management.as_ref().expect("management page");
        let title = Self::management_title(&page.kind);
        let mut body = v_flex()
            .id("management-body")
            .items_start()
            .flex_1()
            .min_h_0()
            .p(px(20.))
            .gap(px(12.))
            .overflow_y_scroll();
        match &page.kind {
            ManageKind::Directory(space) => {
                if let Some(data) = &page.directory {
                    body = body.child(self.space_home(page, space, data, cx));
                    if data.inaccessible > 0 {
                        body = body.child(div().text_color(cx.theme().muted_foreground).child(
                            format!(
                                "{} {} inaccessible to this account.",
                                data.inaccessible,
                                if data.inaccessible == 1 {
                                    "room is"
                                } else {
                                    "rooms are"
                                }
                            ),
                        ));
                    }
                    if !data.end_reached {
                        let id = space.clone();
                        body = body.child(
                            Button::new("directory-more")
                                .small()
                                .label("Load More")
                                .disabled(page.busy || self.preview)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.run_space_action(
                                        SpaceAction::NextDirectory(id.clone()),
                                        window,
                                        cx,
                                    )
                                })),
                        );
                    }
                } else {
                    body = body.child(if page.busy {
                        "Loading rooms…"
                    } else {
                        "Rooms could not be loaded."
                    });
                    if !page.busy {
                        let id = space.clone();
                        body = body.child(
                            Button::new("directory-retry")
                                .icon(assets::IconName::RefreshCw)
                                .small()
                                .self_start()
                                .label("Try Again")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.run_space_action(
                                        SpaceAction::Directory(id.clone()),
                                        window,
                                        cx,
                                    )
                                })),
                        );
                    }
                }
            }
            ManageKind::Leave(space) => {
                if let Some(root) = page.leave.iter().find(|r| &r.id == space) {
                    body = body.child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(root.name.clone()),
                    );
                    if root.last_owner {
                        body = body.child("You are the last owner of this space.");
                    }
                }
                body = body.child("Choose which rooms to leave along with this space. Unselected rooms stay joined.");
                for room in page.leave.iter().filter(|r| &r.id != space) {
                    let id = room.id.clone();
                    let checked = page.selected.contains(&room.id);
                    body = body.child(
                        Button::new(SharedString::from(format!("leave-choice-{id}")))
                            .outline()
                            .w_full()
                            .justify_start()
                            .label(format!(
                                "{}{}{}{}",
                                room.name,
                                if room.space { " · Space" } else { "" },
                                if room.shared {
                                    " · Also in another space"
                                } else {
                                    ""
                                },
                                if room.last_owner {
                                    " · You are the last owner"
                                } else {
                                    ""
                                }
                            ))
                            .selected(checked)
                            .toggled(checked)
                            .disabled(page.busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(p) = &mut this.management
                                    && !p.selected.remove(&id)
                                {
                                    p.selected.insert(id.clone());
                                }
                                cx.notify();
                            })),
                    );
                }
                let id = space.clone();
                let selected = page.selected.clone();
                body = body.child(
                    Button::new("confirm-leave-space")
                        .danger()
                        .label(if page.selected.len() <= 1 {
                            "Leave Space".to_owned()
                        } else {
                            format!("Leave Space and {} Selected Rooms", page.selected.len() - 1)
                        })
                        .disabled(self.preview || page.busy || page.leave.is_empty())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run_space_action(
                                SpaceAction::Leave(id.clone(), selected.clone()),
                                window,
                                cx,
                            )
                        })),
                );
            }
            ManageKind::Create => {
                body = body
                    .child("Name")
                    .child(
                        div()
                            .w_full()
                            .child(Input::new(&page.name).aria_label("Name")),
                    )
                    .child("Topic (optional)")
                    .child(
                        div()
                            .w_full()
                            .child(Input::new(&page.topic).aria_label("Room topic")),
                    );
                body = body.child(
                    h_flex()
                        .justify_between()
                        .gap(px(12.))
                        .py(px(4.))
                        .child("Voice channel")
                        .child(
                            switch::Switch::new("voice-channel-kind")
                                .accessibility_label("Voice channel")
                                .checked(page.voice_channel)
                                .color(cx.theme().primary)
                                .disabled(page.busy)
                                .on_change(cx.listener(|this, checked: &bool, _, cx| {
                                    if let Some(page) = &mut this.management {
                                        page.voice_channel = *checked;
                                        if page.voice_channel {
                                            page.subspace = false;
                                            page.encrypted = true;
                                        }
                                    }
                                    cx.notify();
                                })),
                        ),
                );
                body = body.child(
                    h_flex()
                        .justify_between()
                        .gap(px(12.))
                        .py(px(4.))
                        .child("Subspace")
                        .child(
                            switch::Switch::new("create-subspace")
                                .accessibility_label("Subspace")
                                .checked(page.subspace)
                                .color(cx.theme().primary)
                                .disabled(page.busy)
                                .on_change(cx.listener(|this, checked: &bool, _, cx| {
                                    if let Some(page) = &mut this.management {
                                        page.subspace = *checked;
                                        if page.subspace {
                                            page.voice_channel = false;
                                        }
                                    }
                                    cx.notify();
                                })),
                        ),
                );
                if !page.voice_channel && !page.subspace {
                    body = body.child(
                        h_flex()
                            .justify_between()
                            .gap(px(12.))
                            .py(px(4.))
                            .child("Encrypt messages and files")
                            .child(
                                switch::Switch::new("room-encryption")
                                    .accessibility_label("Encrypt messages and files")
                                    .checked(page.encrypted)
                                    .color(cx.theme().primary)
                                    .disabled(page.busy)
                                    .on_change(cx.listener(|this, checked: &bool, _, cx| {
                                        if let Some(page) = &mut this.management {
                                            page.encrypted = *checked;
                                        }
                                        cx.notify();
                                    })),
                            ),
                    );
                }
                // The creating space is fixed by the place the form was opened.
                // A picker is only needed for the global keyboard entry point.
                if !page.parent_from_context {
                    body = body.child("Choose a space");
                    // Keep joined spaces selectable even when cached permission
                    // metadata is incomplete. Core refreshes state and enforces
                    // creation permissions before sending any create request.
                    for space in &self.snapshot.spaces {
                        let id = space.id.clone();
                        body = body.child(
                            Button::new(SharedString::from(format!("parent-{id}")))
                                .ghost()
                                .label(space.name.clone())
                                .selected(page.parent.as_ref() == Some(&space.id))
                                .when(page.parent.as_ref() == Some(&space.id), |button| {
                                    button
                                        .bg(cx.theme().primary)
                                        .border_color(cx.theme().primary)
                                        .text_color(cx.theme().primary_foreground)
                                })
                                .disabled(page.busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(p) = &mut this.management {
                                        p.parent = Some(id.clone());
                                    }
                                    cx.notify();
                                })),
                        );
                    }
                }
                let name = page.name.clone();
                let topic = page.topic.clone();
                let parent = page.parent.clone();
                let encrypted = page.encrypted;
                let voice_channel = page.voice_channel;
                let subspace = page.subspace;
                body = body.child(
                    Button::new("create-room-submit")
                        .primary()
                        .label(if subspace {
                            "Create Subspace"
                        } else if voice_channel {
                            "Create Voice Channel"
                        } else {
                            "Create Text Channel"
                        })
                        .disabled(
                            self.preview
                                || page.busy
                                || page.name.read(cx).value().trim().is_empty()
                                || parent.is_none(),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let draft = CreateRoom {
                                name: name.read(cx).value().to_string(),
                                topic: topic.read(cx).value().to_string(),
                                encrypted,
                                parent: parent.clone(),
                            };
                            this.run_space_action(
                                if subspace {
                                    SpaceAction::CreateSubspace(draft)
                                } else if voice_channel {
                                    SpaceAction::CreateVoice(draft)
                                } else {
                                    SpaceAction::Create(draft)
                                },
                                window,
                                cx,
                            )
                        })),
                );
            }
            ManageKind::LeaveRoom(room) => {
                let room = room.clone();
                body = body
                    .child("You will stop receiving messages from this conversation. You can rejoin a public room later; private rooms need a new invitation.")
                    .child(
                        Button::new("leave-room-confirm")
                            .danger()
                            .icon(gpui_kit::assets::IconName::LogOut)
                            .label("Leave")
                            .disabled(self.preview || page.busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.run_space_action(
                                    SpaceAction::LeaveRoom(room.clone()),
                                    window,
                                    cx,
                                );
                            })),
                    );
            }
            ManageKind::SignOut => {
                let security = &self.snapshot.security;
                let others = self.snapshot.accounts.len() > 1;
                let mut text = format!(
                    "{} will be signed out of this device, and its messages and encryption keys will be removed from this computer.",
                    self.snapshot.user_id
                );
                if security.ready && !security.backup_exists {
                    text.push_str(" Secure backup is off: encrypted history you can’t read on another device will be lost. Set up backup in Settings › Security first.");
                }
                if others {
                    text.push_str(" Your other accounts stay signed in.");
                }
                body = body.child(text).child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .gap(px(8.))
                        .child(
                            Button::new("sign-out-cancel")
                                .ghost()
                                .label("Cancel")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.management = None;
                                    this.restore_focus(window, cx);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("sign-out-confirm")
                                .danger()
                                .icon(gpui_kit::assets::IconName::LogOut)
                                .label("Sign Out")
                                .disabled(self.preview || self.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.management = None;
                                    this.logout(window, cx);
                                })),
                        ),
                );
            }
            ManageKind::InviteContact => {
                let user = self.snapshot.user_id.clone();
                let link = contact_link(&user);
                let copied = page.copied;
                let copy = link.clone();
                body = body
                    .child("Send this link to someone you want to talk to. It opens in any Matrix app, such as Element, and starts a conversation with you.")
                    .child(
                        v_flex()
                            .id("contact-link")
                            .w_full()
                            .gap(px(2.))
                            .px(px(10.))
                            .py(px(8.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().secondary.opacity(0.4))
                            .child(div().font_weight(FontWeight::MEDIUM).child(user.clone()))
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(cx.theme().muted_foreground)
                                    .truncate()
                                    .child(link.clone()),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                Button::new("contact-dm")
                                    .ghost()
                                    .label("Start a Direct Message…")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_management(ManageKind::Dm, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("contact-copy")
                                    .primary()
                                    // Same width for "Copy Link" and "Copied",
                                    // so the row doesn't shift when it changes.
                                    .min_w(px(112.))
                                    .icon(if copied {
                                        assets::IconName::Check
                                    } else {
                                        assets::IconName::Copy
                                    })
                                    .label(if copied { "Copied" } else { "Copy Link" })
                                    .disabled(user.is_empty())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copy.clone(),
                                        ));
                                        if let Some(page) = &mut this.management {
                                            page.copied = true;
                                        }
                                        cx.notify();
                                    })),
                            ),
                    );
            }
            ManageKind::InviteMember(room) => {
                let room = room.clone();
                let input = page.name.clone();
                body = body
                    .child("Matrix ID (for example, @name:example.org)")
                    .child(
                        div()
                            .w_full()
                            .child(Input::new(&page.name).aria_label("Matrix ID")),
                    )
                    .child(self.people_suggestions(
                        page.name.read(cx).value().as_ref(),
                        Some(&room),
                        cx,
                    ))
                    .child(
                        Button::new("invite-submit")
                            .primary()
                            .label("Send Invitation")
                            .disabled(
                                self.preview
                                    || page.busy
                                    || page.name.read(cx).value().trim().is_empty(),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let user = input.read(cx).value().trim().to_owned();
                                this.run_space_action(
                                    SpaceAction::InviteMember(room.clone(), user),
                                    window,
                                    cx,
                                );
                            })),
                    );
            }
            ManageKind::Explore => {
                body = body.child(self.explore_body(cx));
            }
            ManageKind::CreatePoll(_) => {
                body = body.child(self.poll_dialog(page, cx));
            }
            ManageKind::Stickers(_) => {
                body = body.child(self.sticker_dialog(page, cx));
            }
            ManageKind::CreateSpace => {
                let public = page.public;
                let choice =
                    |id: &'static str, label: &'static str, detail: &'static str, value: bool| {
                        let selected = public == value;
                        Button::new(id)
                            .ghost()
                            .w_full()
                            .h_auto()
                            .py(px(8.))
                            .px(px(10.))
                            .justify_start()
                            .rounded(px(8.))
                            .border_1()
                            .border_color(if selected {
                                cx.theme().primary
                            } else {
                                cx.theme().border
                            })
                            .selected(selected)
                            .accessibility_label(format!("{label}: {detail}"))
                            .child(
                                v_flex()
                                    .w_full()
                                    .items_start()
                                    .text_left()
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child(label))
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(cx.theme().muted_foreground)
                                            .child(detail),
                                    ),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(page) = &mut this.management {
                                    page.public = value;
                                }
                                cx.notify();
                            }))
                    };
                let name = page.name.clone();
                let topic = page.topic.clone();
                body = body
                    .child("Name")
                    .child(
                        div()
                            .w_full()
                            .child(Input::new(&page.name).aria_label("Space name")),
                    )
                    .child("Description (optional)")
                    .child(
                        div()
                            .w_full()
                            .child(Input::new(&page.topic).aria_label("Space description")),
                    )
                    .child(
                        v_flex()
                            .w_full()
                            .id("space-access")
                            .gap(px(6.))
                            .role(Role::RadioGroup)
                            .aria_label("Who can join")
                            .child(choice(
                                "space-private",
                                "Private",
                                "Only people you invite can join.",
                                false,
                            ))
                            .child(choice(
                                "space-public",
                                "Public",
                                "Anyone can find and join it.",
                                true,
                            )),
                    )
                    .child(
                        Button::new("create-space-submit")
                            .primary()
                            .label("Create Space")
                            .disabled(
                                self.preview
                                    || page.busy
                                    || page.name.read(cx).value().trim().is_empty(),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let draft = twayblade_core::space_actions::CreateSpace {
                                    name: name.read(cx).value().trim().to_owned(),
                                    topic: topic.read(cx).value().trim().to_owned(),
                                    public: this
                                        .management
                                        .as_ref()
                                        .is_some_and(|page| page.public),
                                };
                                this.run_space_action(SpaceAction::CreateSpace(draft), window, cx);
                            })),
                    );
            }
            ManageKind::StatusMessage => {
                use twayblade_core::presence::State;
                let length = page.name.read(cx).value().trim().chars().count();
                let too_long = length > STATUS_LIMIT;
                body = body
                    .child("Shown next to your name to people who share a room with you.")
                    .child(
                        div()
                            .w_full()
                            .child(Input::new(&page.name).aria_label("Status message")),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(if too_long {
                                crate::theme::palette(cx).error
                            } else {
                                cx.theme().muted_foreground
                            })
                            .child(format!("{length} / {STATUS_LIMIT}")),
                    );
                let had_status = self
                    .snapshot
                    .own_presence
                    .as_ref()
                    .is_some_and(|presence| presence.status_msg.is_some());
                let state = self
                    .snapshot
                    .own_presence
                    .as_ref()
                    .map_or(State::Online, |presence| presence.state);
                let input = page.name.clone();
                body = body.child(
                    h_flex()
                        .gap(px(8.))
                        .child(
                            Button::new("status-save")
                                .primary()
                                .label("Save")
                                .disabled(self.preview || too_long || length == 0)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let text = input.read(cx).value().trim().to_owned();
                                    this.set_presence(state, Some(text), window, cx);
                                    this.close_management_page(window, cx);
                                })),
                        )
                        .when(had_status, |row| {
                            row.child(
                                Button::new("status-clear")
                                    .label("Clear")
                                    .disabled(self.preview)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.set_presence(state, None, window, cx);
                                        this.close_management_page(window, cx);
                                    })),
                            )
                        }),
                );
            }
            ManageKind::Dm | ManageKind::JoinSpace => {
                let dm = matches!(page.kind, ManageKind::Dm);
                body = body
                    .child(if dm {
                        "Matrix ID (for example, @name:example.org)"
                    } else {
                        "Space ID or alias"
                    })
                    .child(
                        div()
                            .w_full()
                            .child(Input::new(&page.name).aria_label(if dm {
                                "Matrix ID"
                            } else {
                                "Space ID or alias"
                            })),
                    )
                    .when(dm, |body| {
                        body.child(self.people_suggestions(
                            page.name.read(cx).value().as_ref(),
                            None,
                            cx,
                        ))
                    });
                let input = page.name.clone();
                body = body.child(
                    Button::new("membership-submit")
                        .primary()
                        .label(if dm {
                            "Start Conversation"
                        } else {
                            "Join Space"
                        })
                        .disabled(
                            self.preview
                                || page.busy
                                || page.name.read(cx).value().trim().is_empty(),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let id = input.read(cx).value().trim().to_owned();
                            this.run_space_action(
                                if dm {
                                    SpaceAction::Dm(id)
                                } else {
                                    SpaceAction::JoinSpace(id)
                                },
                                window,
                                cx,
                            );
                        })),
                );
            }
        }
        if page.busy {
            body = body.child("Working…");
        }
        if !page.status.is_empty() {
            body = body.child(div().child(page.status.clone()));
        }
        let dialog = page.kind.is_dialog();
        // A dialog's header must not be a window drag region.
        let header = if dialog {
            h_flex().id("management-header").h(px(44.)).flex_none()
        } else {
            self.toolbar("management-toolbar", cx)
        };
        v_flex()
            .when(!dialog, |view| view.flex_1().h_full())
            .when(dialog, |view| view.max_h(px(560.)))
            .min_w_0()
            .bg(cx.theme().background)
            .child(
                header
                    .px(px(16.))
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                    .child(
                        Button::new("close-management")
                            .toolbar_control()
                            .ghost()
                            .small()
                            .icon(assets::IconName::X)
                            .accessibility_label("Close")
                            .tooltip("Close · Escape")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_management_page(window, cx);
                            })),
                    ),
            )
            .child(body)
            .into_any_element()
    }
}

/// Someone you already have a direct conversation with.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KnownPerson {
    pub user_id: String,
    pub name: String,
    pub avatar: Option<String>,
    /// Their direct conversation, which Start a Direct Message reopens.
    pub room: String,
}

/// People from your direct conversations, most recent first, matching
/// `filter` (name or Matrix ID, case-insensitive), at most `limit`.
pub(crate) fn known_people(
    rooms: &[crate::backend::Room],
    filter: &str,
    exclude: &[String],
    limit: usize,
) -> Vec<KnownPerson> {
    let filter = filter.trim().to_lowercase();
    let mut direct: Vec<_> = rooms
        .iter()
        .filter(|room| room.direct && !room.archived)
        .filter_map(|room| room.partner.as_ref().map(|partner| (room, partner)))
        .collect();
    direct.sort_by_key(|(room, _)| std::cmp::Reverse(room.recency));
    let mut people: Vec<KnownPerson> = Vec::new();
    for (room, partner) in direct {
        if people.iter().any(|p| &p.user_id == partner) || exclude.contains(partner) {
            continue;
        }
        if !filter.is_empty()
            && !room.name.to_lowercase().contains(&filter)
            && !partner.to_lowercase().contains(&filter)
        {
            continue;
        }
        people.push(KnownPerson {
            user_id: partner.clone(),
            name: room.name.clone(),
            avatar: room.avatar.clone(),
            room: room.id.clone(),
        });
        if people.len() == limit {
            break;
        }
    }
    people
}

impl MatrixApp {
    /// People you know under the Matrix ID field. In the invite dialog a
    /// pick fills the field (members of `invite_room` are left out); in
    /// Start a Direct Message it reopens your conversation with them.
    fn people_suggestions(
        &self,
        filter: &str,
        invite_room: Option<&String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let exclude: Vec<String> = if invite_room.is_some() {
            self.snapshot
                .members
                .iter()
                .map(|member| member.user_id.clone())
                .collect()
        } else {
            Vec::new()
        };
        let people = known_people(&self.snapshot.rooms, filter, &exclude, 6);
        if people.is_empty() {
            return div().into_any_element();
        }
        let muted = cx.theme().muted_foreground;
        let invite = invite_room.is_some();
        v_flex()
            .id("people-suggestions")
            .role(Role::Group)
            .aria_label("People you know")
            .w_full()
            .gap(px(2.))
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(muted)
                    .child("People you know"),
            )
            .children(people.into_iter().map(|person| {
                let label = if invite {
                    format!("Invite {} ({})", person.name, person.user_id)
                } else {
                    format!("Open your conversation with {}", person.name)
                };
                let KnownPerson {
                    user_id,
                    name,
                    avatar,
                    room,
                } = person;
                Button::new(SharedString::from(format!("suggest-{user_id}")))
                    .ghost()
                    .w_full()
                    .h(px(40.))
                    .px(px(6.))
                    .justify_start()
                    .rounded(px(6.))
                    .accessibility_label(label)
                    .child(
                        h_flex()
                            .w_full()
                            .gap(px(10.))
                            .child(self.avatar_image(
                                avatar.as_deref(),
                                &user_id,
                                &name,
                                28.,
                                14.,
                                cx,
                            ))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(div().truncate().text_left().child(name))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_left()
                                            .text_size(px(11.))
                                            .text_color(muted)
                                            .child(user_id.clone()),
                                    ),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if invite {
                            if let Some(page) = &this.management {
                                page.name.update(cx, |state, cx| {
                                    state.set_value(user_id.clone(), window, cx)
                                });
                            }
                        } else {
                            this.close_management_page(window, cx);
                            this.select_room(room.clone(), window, cx);
                        }
                        cx.notify();
                    }))
            }))
            .into_any_element()
    }
}

/// Matches core's presence limit, so Save is disabled before core refuses.
const STATUS_LIMIT: usize = 256;

/// A matrix.to link to a user, the address every Matrix client opens.
pub(crate) fn contact_link(user: &str) -> String {
    format!("https://matrix.to/#/{user}")
}
