//! Space home (#177): the space's picture, name, access and members with
//! Invite, and a searchable tree of its rooms and subspaces. Subspaces load
//! their rooms when expanded; rooms you are not in offer Join.
use assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        input::Input,
        *,
    },
    *,
};
use twayblade_core::space_actions::{DirectoryRoom, DirectorySnapshot, JoinAccess};

use crate::app::MatrixApp;
use crate::backend::SpaceAction;
use crate::theme::scaled_px as px;
use crate::ui::explore::grouped;
use crate::ui::spaces::{ManageKind, Management};

/// Indent per tree level, in points.
const INDENT: f32 = 20.;

fn members(n: u64) -> String {
    match n {
        1 => "1 member".to_owned(),
        n => format!("{} members", grouped(n)),
    }
}

/// Whether a room's name or topic contains the (lowercased) query.
pub(crate) fn matches_query(room: &DirectoryRoom, query: &str) -> bool {
    query.is_empty()
        || room.name.to_lowercase().contains(query)
        || room
            .topic
            .as_deref()
            .is_some_and(|topic| topic.to_lowercase().contains(query))
}

/// Whether a room, or anything already loaded below it, matches the query.
/// `path` holds the spaces above it, so a space listing an ancestor cannot
/// recurse forever.
fn tree_matches(
    room: &DirectoryRoom,
    query: &str,
    children: &std::collections::HashMap<String, DirectorySnapshot>,
    path: &mut Vec<String>,
) -> bool {
    if matches_query(room, query) {
        return true;
    }
    if !room.space || path.contains(&room.id) {
        return false;
    }
    let Some(list) = children.get(&room.id) else {
        return false;
    };
    path.push(room.id.clone());
    let found = list
        .rooms
        .iter()
        .any(|child| tree_matches(child, query, children, path));
    path.pop();
    found
}

/// Sample rooms of a subspace for the offline preview.
fn preview_children(id: &str) -> DirectorySnapshot {
    DirectorySnapshot {
        space: id.to_owned(),
        name: "Product team".into(),
        rooms: vec![
            DirectoryRoom {
                id: "sample-roadmap".into(),
                name: "Roadmap".into(),
                topic: Some("What ships next".into()),
                joined: true,
                access: JoinAccess::Restricted,
                members: 14,
                ..Default::default()
            },
            DirectoryRoom {
                id: "sample-feedback".into(),
                name: "Customer feedback".into(),
                access: JoinAccess::Restricted,
                members: 9,
                ..Default::default()
            },
        ],
        end_reached: true,
        ..Default::default()
    }
}

impl MatrixApp {
    /// Expands or collapses a subspace, loading its rooms the first time.
    pub(crate) fn toggle_subspace(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(page) = &mut self.management else {
            return;
        };
        if page.expanded.remove(&id) {
            cx.notify();
            return;
        }
        page.expanded.insert(id.clone());
        if !page.children.contains_key(&id) && !page.loading.contains(&id) {
            self.load_subspace(id, window, cx);
        }
        cx.notify();
    }

    fn load_subspace(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = &mut self.management else {
            return;
        };
        page.failed.remove(&id);
        if self.preview {
            page.children.insert(id.clone(), preview_children(&id));
            return;
        }
        page.loading.insert(id.clone());
        let request = page.request;
        let account = self.snapshot.account_instance;
        let key = id.clone();
        self.job(
            window,
            cx,
            move |backend| backend.space_children(account, id),
            move |this, result, _, cx| {
                if this.snapshot.account_instance != account {
                    return;
                }
                let Some(page) = this.management.as_mut().filter(|p| p.request == request) else {
                    return;
                };
                page.loading.remove(&key);
                match result {
                    Ok(data) => {
                        page.children.insert(key, data);
                    }
                    Err(_) => {
                        page.failed.insert(key);
                    }
                }
                cx.notify();
            },
        );
    }

    /// The space home body: header, then the Rooms and spaces tree.
    pub(crate) fn space_home(
        &self,
        page: &Management,
        space: &str,
        data: &DirectorySnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let summary = self.snapshot.spaces.iter().find(|s| s.id == space);
        let avatar = data
            .avatar
            .as_deref()
            .or_else(|| summary.and_then(|s| s.avatar.as_deref()));
        let public = data.access == JoinAccess::Public;
        let can_add = summary.is_some_and(|s| s.can_add_rooms);
        let invite_id = space.to_owned();
        let header = h_flex()
            .w_full()
            .gap(px(16.))
            .items_start()
            .child(self.avatar_image(avatar, space, &data.name, 64., 16., cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.))
                    .child(
                        div()
                            .id("space-home-name")
                            .role(Role::Heading)
                            .aria_label(data.name.clone())
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(data.name.clone())
                            .test_support(),
                    )
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(muted)
                            .child(
                                Icon::new(if public {
                                    IconName::Globe
                                } else {
                                    IconName::Lock
                                })
                                .size(px(13.))
                                .text_color(muted),
                            )
                            .child(if public {
                                "Public space"
                            } else {
                                "Private space"
                            })
                            .child("·")
                            .child(members(data.members)),
                    )
                    .children(data.topic.clone().map(|topic| {
                        div()
                            .pt(px(2.))
                            .text_color(muted)
                            .line_clamp(2)
                            .child(topic)
                    }))
                    .child(
                        h_flex()
                            .pt(px(8.))
                            .gap(px(8.))
                            .child(
                                Button::new("space-home-invite")
                                    .small()
                                    .primary()
                                    .icon(IconName::UserPlus)
                                    .label("Invite")
                                    .accessibility_label(format!("Invite people to {}", data.name))
                                    .disabled(self.preview)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_management(
                                            ManageKind::InviteMember(invite_id.clone()),
                                            window,
                                            cx,
                                        )
                                    })),
                            )
                            .when(can_add, |row| {
                                row.child(
                                    Button::new("directory-create-room")
                                        .small()
                                        .icon(IconName::Plus)
                                        .label("Add Room…")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.open_management(ManageKind::Create, window, cx)
                                        })),
                                )
                            }),
                    ),
            );

        let query = page.name.read(cx).value().trim().to_lowercase();
        let mut rows = Vec::new();
        let mut path = vec![space.to_owned()];
        self.space_rows(page, &data.rooms, 0, &query, &mut path, &mut rows, cx);
        let empty = rows.is_empty();
        let status: Option<String> = if empty && !query.is_empty() {
            Some(format!("No rooms or spaces match “{}”.", query))
        } else if empty && data.end_reached {
            Some("This space has no rooms you can see yet.".into())
        } else {
            None
        };

        v_flex()
            .w_full()
            .max_w(px(760.))
            .gap(px(20.))
            .child(header)
            .child(
                v_flex()
                    .w_full()
                    .gap(px(8.))
                    .child(
                        h_flex()
                            .w_full()
                            .gap(px(12.))
                            .justify_between()
                            .child(
                                div()
                                    .id("space-home-rooms-heading")
                                    .role(Role::Heading)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Rooms and spaces"),
                            )
                            .child(
                                div().w(px(240.)).child(
                                    Input::new(&page.name)
                                        .small()
                                        .aria_label("Search rooms and spaces")
                                        .prefix(
                                            Icon::new(IconName::Search)
                                                .size(px(13.))
                                                .text_color(muted),
                                        ),
                                ),
                            ),
                    )
                    .child(
                        v_flex()
                            .id("space-home-rooms")
                            .role(Role::List)
                            .aria_label("Rooms and spaces")
                            .w_full()
                            .gap(px(2.))
                            .children(rows),
                    )
                    .children(status.map(|text| {
                        div()
                            .id("space-home-status")
                            .role(Role::Status)
                            .text_color(muted)
                            .child(text)
                            .test_support()
                    })),
            )
            .into_any_element()
    }

    /// Appends the rows for `rooms` at `depth`, and below each expanded
    /// subspace (or, while searching, each subspace with loaded matches) its
    /// own rooms.
    #[allow(clippy::too_many_arguments)]
    fn space_rows(
        &self,
        page: &Management,
        rooms: &[DirectoryRoom],
        depth: usize,
        query: &str,
        path: &mut Vec<String>,
        rows: &mut Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) {
        // Spaces first, then suggested rooms, as in Element.
        let mut ordered: Vec<&DirectoryRoom> = rooms.iter().collect();
        ordered.sort_by_key(|room| (!room.space, !room.suggested));
        for room in ordered {
            if !tree_matches(room, query, &page.children, path) {
                continue;
            }
            let cycle = path.contains(&room.id);
            let expanded = room.space
                && !cycle
                && (page.expanded.contains(&room.id)
                    || (!query.is_empty() && page.children.contains_key(&room.id)));
            rows.push(self.space_row(page, room, depth, expanded, cycle, cx));
            if !expanded {
                continue;
            }
            let muted = cx.theme().muted_foreground;
            let pad = px(8. + (depth + 1) as f32 * INDENT + 20.);
            if let Some(children) = page.children.get(&room.id) {
                path.push(room.id.clone());
                self.space_rows(page, &children.rooms, depth + 1, query, path, rows, cx);
                path.pop();
                if children.rooms.is_empty() && query.is_empty() {
                    rows.push(
                        div()
                            .pl(pad)
                            .py(px(4.))
                            .text_size(px(12.))
                            .text_color(muted)
                            .child("No rooms you can see.")
                            .into_any_element(),
                    );
                }
                if !children.end_reached && query.is_empty() {
                    let id = room.id.clone();
                    rows.push(
                        div()
                            .pl(pad)
                            .child(
                                Button::new(SharedString::from(format!("space-more-{id}")))
                                    .ghost()
                                    .small()
                                    .label(format!("All rooms in {}…", room.name))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_management(
                                            ManageKind::Directory(id.clone()),
                                            window,
                                            cx,
                                        )
                                    })),
                            )
                            .into_any_element(),
                    );
                }
            } else if page.loading.contains(&room.id) {
                rows.push(
                    div()
                        .pl(pad)
                        .py(px(4.))
                        .text_size(px(12.))
                        .text_color(muted)
                        .child("Loading…")
                        .into_any_element(),
                );
            } else if page.failed.contains(&room.id) {
                let id = room.id.clone();
                rows.push(
                    h_flex()
                        .pl(pad)
                        .py(px(2.))
                        .gap(px(8.))
                        .text_size(px(12.))
                        .text_color(muted)
                        .child("Rooms couldn’t be loaded.")
                        .child(
                            Button::new(SharedString::from(format!("space-retry-{id}")))
                                .ghost()
                                .xsmall()
                                .label("Try Again")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.load_subspace(id.clone(), window, cx);
                                    cx.notify();
                                })),
                        )
                        .into_any_element(),
                );
            }
        }
    }

    fn space_row(
        &self,
        page: &Management,
        room: &DirectoryRoom,
        depth: usize,
        expanded: bool,
        cycle: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut details: Vec<String> = Vec::new();
        if room.suggested {
            details.push("Suggested".into());
        }
        if room.space {
            details.push(match room.children {
                0 => "Space".into(),
                1 => "1 room".into(),
                n => format!("{n} rooms"),
            });
        }
        details.push(members(room.members));
        if !room.joined && !room.invited {
            match room.access {
                JoinAccess::Invite => details.push("Invitation required".into()),
                JoinAccess::Unknown => details.push("Access unavailable".into()),
                _ => {}
            }
        }
        if let Some(topic) = &room.topic {
            details.push(topic.clone());
        }
        let detail = details.join(" · ");
        let name = room.name.clone();
        let id = room.id.clone();

        let disclosure = if room.space && !cycle {
            let toggle = id.clone();
            Button::new(SharedString::from(format!("space-toggle-{id}")))
                .ghost()
                .size(px(20.))
                .icon(
                    Icon::new(if expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .size(px(14.)),
                )
                .accessibility_label(format!(
                    "{} {name}",
                    if expanded { "Collapse" } else { "Expand" }
                ))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.toggle_subspace(toggle.clone(), window, cx)
                }))
                .into_any_element()
        } else {
            div().w(px(20.)).flex_none().into_any_element()
        };

        let label = v_flex()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .truncate()
                    .text_left()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(name.clone()),
            )
            .child(
                div()
                    .truncate()
                    .text_left()
                    .text_size(px(12.))
                    .text_color(muted)
                    .child(detail.clone()),
            );
        let identity = h_flex()
            .flex_1()
            .min_w_0()
            .gap(px(10.))
            .child(self.avatar_image(room.avatar.as_deref(), &room.id, &name, 28., 14., cx))
            .child(label);
        let joinable =
            room.invited || matches!(room.access, JoinAccess::Public | JoinAccess::Restricted);
        let trailing = if room.joined {
            Some(
                h_flex()
                    .flex_none()
                    .gap(px(4.))
                    .text_size(px(12.))
                    .text_color(muted)
                    .child(Icon::new(IconName::Check).size(px(13.)).text_color(muted))
                    .child("Joined")
                    .into_any_element(),
            )
        } else if joinable {
            let join = id.clone();
            let via = room.via.clone();
            Some(
                Button::new(SharedString::from(format!("join-{id}")))
                    .small()
                    .primary()
                    .label("Join")
                    .accessibility_label(format!("Join {name}"))
                    .disabled(self.preview || page.busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.run_space_action(
                            SpaceAction::Join(join.clone(), via.clone()),
                            window,
                            cx,
                        )
                    }))
                    .into_any_element(),
            )
        } else {
            None
        };
        // A subspace also opens as its own space home.
        let browse = (room.space && !cycle).then(|| {
            let browse = id.clone();
            Button::new(SharedString::from(format!("browse-{id}")))
                .ghost()
                .size(px(24.))
                .icon(Icon::new(IconName::ArrowRight).size(px(14.)))
                .accessibility_label(format!("Open {name} space home"))
                .tooltip("Open Space Home")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_management(ManageKind::Directory(browse.clone()), window, cx)
                }))
        });

        h_flex()
            .id(SharedString::from(format!("space-row-{id}")))
            .role(Role::ListItem)
            .aria_label(format!(
                "{name}, {}{detail}",
                if room.joined { "Joined, " } else { "" }
            ))
            .w_full()
            .min_h(px(44.))
            .py(px(4.))
            .pl(px(8. + depth as f32 * INDENT))
            .pr(px(8.))
            .gap(px(8.))
            .rounded(px(6.))
            .hover(|style| style.bg(cx.theme().secondary.opacity(0.5)))
            // A joined room opens on click; a subspace expands with its arrow.
            .when(room.joined && !room.space, |row| {
                let open = id.clone();
                row.cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_management_page(window, cx);
                        this.select_room(open.clone(), window, cx);
                    }))
            })
            .child(disclosure)
            .child(identity)
            .children(trailing)
            .children(browse)
            .test_support()
            .into_any_element()
    }
}
