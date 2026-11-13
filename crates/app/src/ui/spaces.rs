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
use std::collections::HashSet;
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
}
impl ManageKind {
    /// Short forms open as a small centred dialog; browsing and leaving keep
    /// the full conversation area because they list many rooms.
    pub fn is_dialog(&self) -> bool {
        matches!(self, Self::Create | Self::Dm | Self::JoinSpace)
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
    pub parent: Option<String>,
}
impl MatrixApp {
    pub fn open_management(
        &mut self,
        kind: ManageKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            .filter(|id| {
                self.snapshot
                    .spaces
                    .iter()
                    .any(|s| &s.id == id && s.can_add_rooms)
            });
        let name = cx.new(|cx| InputState::new(window, cx));
        let topic = cx.new(|cx| InputState::new(window, cx));
        let subscriptions = [&name, &topic]
            .into_iter()
            .map(|input| {
                cx.subscribe_in(
                    input,
                    window,
                    |_, _, _: &gpui_kit::component::input::InputEvent, _, cx| cx.notify(),
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
            parent: initial_parent,
        });
        if self.preview {
            if let Some(page) = &mut self.management {
                match &page.kind {
                    ManageKind::Directory(id) => {
                        page.directory = Some(DirectorySnapshot {
                            space: id.clone(),
                            name: "Design team".into(),
                            rooms: vec![
                                DirectoryRoom {
                                    id: "general".into(),
                                    name: "General".into(),
                                    joined: true,
                                    access: JoinAccess::Public,
                                    ..Default::default()
                                },
                                DirectoryRoom {
                                    id: "sample-research".into(),
                                    name: "Research lab".into(),
                                    topic: Some("Experiments and early ideas".into()),
                                    suggested: true,
                                    access: JoinAccess::Public,
                                    ..Default::default()
                                },
                                DirectoryRoom {
                                    id: "sample-private".into(),
                                    name: "Private planning".into(),
                                    access: JoinAccess::Invite,
                                    ..Default::default()
                                },
                                DirectoryRoom {
                                    id: "sample-restricted".into(),
                                    name: "Members lounge".into(),
                                    access: JoinAccess::Restricted,
                                    ..Default::default()
                                },
                                DirectoryRoom {
                                    id: "sample-nested".into(),
                                    name: "Product team".into(),
                                    space: true,
                                    access: JoinAccess::Public,
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
                            if let Some(room) = directory.rooms.iter_mut().find(|r| r.id == joining)
                            {
                                room.joined = true;
                                room.invited = false;
                            }
                        } else {
                            let space = matches!(page.kind, ManageKind::JoinSpace);
                            this.management = None;
                            if space {
                                this.navigation.selected_space = Some(id);
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
                        this.management = None;
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

    pub fn management_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let page = self.management.as_ref().expect("management page");
        let title = match &page.kind {
            ManageKind::Directory(_) => "Browse Rooms",
            ManageKind::Leave(_) => "Leave Space",
            ManageKind::Create => "Create Room",
            ManageKind::Dm => "Start a Direct Message",
            ManageKind::JoinSpace => "Join a Space",
        };
        let mut body = v_flex()
            .id("management-body")
            .flex_1()
            .min_h_0()
            .p(px(20.))
            .gap(px(12.))
            .overflow_y_scroll();
        match &page.kind {
            ManageKind::Directory(space) => {
                if let Some(data) = &page.directory {
                    body = body.child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(data.name.clone()),
                    );
                    if self
                        .snapshot
                        .spaces
                        .iter()
                        .any(|s| s.id == *space && s.can_add_rooms)
                    {
                        body = body.child(
                            Button::new("directory-create-room")
                                .small()
                                .self_start()
                                .label("Create Room…")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_management(ManageKind::Create, window, cx)
                                })),
                        );
                    }
                    for room in &data.rooms {
                        let id = room.id.clone();
                        let via = room.via.clone();
                        let directory_id = id.clone();
                        let mut row = h_flex()
                            .gap(px(12.))
                            .py(px(8.))
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(room.name.clone()),
                                    )
                                    .when_some(room.topic.clone(), |v, topic| {
                                        v.child(
                                            div()
                                                .text_color(cx.theme().muted_foreground)
                                                .truncate()
                                                .child(topic),
                                        )
                                    })
                                    .child(div().text_color(cx.theme().muted_foreground).child(
                                        if room.suggested {
                                            "Suggested"
                                        } else {
                                            match room.access {
                                                JoinAccess::Restricted => "Restricted membership",
                                                JoinAccess::Invite => "Invitation required",
                                                JoinAccess::Unknown => "Access unavailable",
                                                JoinAccess::Public => {
                                                    if room.space {
                                                        "Space"
                                                    } else {
                                                        "Room"
                                                    }
                                                }
                                            }
                                        },
                                    )),
                            );
                        if room.joined {
                            row = row.child(
                                div()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Joined"),
                            );
                        } else if room.invited
                            || matches!(room.access, JoinAccess::Public | JoinAccess::Restricted)
                        {
                            row = row.child(
                                Button::new(SharedString::from(format!("join-{id}")))
                                    .small()
                                    .label("Join")
                                    .disabled(self.preview || page.busy)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.run_space_action(
                                            SpaceAction::Join(id.clone(), via.clone()),
                                            window,
                                            cx,
                                        )
                                    })),
                            );
                        }
                        if room.space {
                            row = row.child(
                                Button::new(SharedString::from(format!("browse-{directory_id}")))
                                    .ghost()
                                    .small()
                                    .label("Browse")
                                    .disabled(page.busy)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_management(
                                            ManageKind::Directory(directory_id.clone()),
                                            window,
                                            cx,
                                        )
                                    })),
                            );
                        }
                        body = body.child(row);
                    }
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
                    .child("Room name")
                    .child(Input::new(&page.name).aria_label("Room name"))
                    .child("Topic (optional)")
                    .child(Input::new(&page.topic).aria_label("Room topic"));
                body = body.child(
                    Button::new("room-encryption")
                        .outline()
                        .label("Encrypt messages and files")
                        .selected(page.encrypted)
                        .toggled(page.encrypted)
                        .disabled(page.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(p) = &mut this.management {
                                p.encrypted = !p.encrypted;
                            }
                            cx.notify();
                        })),
                );
                body = body.child("Space (optional)").child(
                    Button::new("parent-home")
                        .ghost()
                        .label("No space")
                        .selected(page.parent.is_none())
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(p) = &mut this.management {
                                p.parent = None;
                            }
                            cx.notify();
                        })),
                );
                for space in self.snapshot.spaces.iter().filter(|s| s.can_add_rooms) {
                    let id = space.id.clone();
                    body = body.child(
                        Button::new(SharedString::from(format!("parent-{id}")))
                            .ghost()
                            .label(space.name.clone())
                            .selected(page.parent.as_ref() == Some(&space.id))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(p) = &mut this.management {
                                    p.parent = Some(id.clone());
                                }
                                cx.notify();
                            })),
                    );
                }
                let name = page.name.clone();
                let topic = page.topic.clone();
                let parent = page.parent.clone();
                let encrypted = page.encrypted;
                body = body.child(
                    Button::new("create-room-submit")
                        .primary()
                        .label("Create Room")
                        .disabled(
                            self.preview
                                || page.busy
                                || page.name.read(cx).value().trim().is_empty(),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run_space_action(
                                SpaceAction::Create(CreateRoom {
                                    name: name.read(cx).value().to_string(),
                                    topic: topic.read(cx).value().to_string(),
                                    encrypted,
                                    parent: parent.clone(),
                                }),
                                window,
                                cx,
                            )
                        })),
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
                    .child(Input::new(&page.name).aria_label(if dm {
                        "Matrix ID"
                    } else {
                        "Space ID or alias"
                    }));
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
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.management = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(body)
            .into_any_element()
    }
}
