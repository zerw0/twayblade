//! Explore Rooms: the public room directory of your own server or one of the
//! largest public homeservers, searchable, with Join / Ask to Join / Open.
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
use twayblade_core::directory::{DirectoryRoom, JoinRule, RoomTarget, ServerName};

use crate::app::MatrixApp;
use crate::theme::scaled_px as px;

/// Large public homeservers with public room directories, after your own.
pub(crate) const PUBLIC_SERVERS: [&str; 4] = ["matrix.org", "mozilla.org", "gnome.org", "kde.org"];

impl MatrixApp {
    pub(crate) fn open_explore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_management(crate::ui::spaces::ManageKind::Explore, window, cx);
        if self.preview {
            self.snapshot.directory.rooms = sample_rooms();
        }
        self.explore_search(window, cx);
    }

    /// Asks the selected server for rooms matching the search field.
    pub(crate) fn explore_search(&mut self, window: &Window, cx: &mut Context<Self>) {
        let filter = self
            .management
            .as_ref()
            .map(|page| page.name.read(cx).value().trim().to_owned())
            .filter(|text| !text.is_empty());
        let server = self
            .explore_server
            .as_deref()
            .and_then(|name| ServerName::try_from(name).ok());
        if self.preview {
            cx.notify();
            return;
        }
        self.browse_directory(server, filter, window, cx);
        cx.notify();
    }

    pub(crate) fn explore_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let page = self.management.as_ref().expect("management page");
        let muted = cx.theme().muted_foreground;
        let directory = &self.snapshot.directory;
        let selected = self.explore_server.clone();
        let own = self
            .snapshot
            .user_id
            .split_once(':')
            .map(|(_, server)| server.to_owned())
            .unwrap_or_else(|| "Your server".into());
        let chip = |id: SharedString, label: String, value: Option<String>| {
            let active = selected == value;
            Button::new(id)
                .small()
                .rounded(px(14.))
                .when(active, |button| button.primary())
                .when(!active, |button| button.ghost().bg(cx.theme().secondary))
                .label(label.clone())
                .selected(active)
                .accessibility_label(format!("Rooms on {label}"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.explore_server = value.clone();
                    this.explore_search(window, cx);
                }))
        };
        let mut servers = h_flex()
            .id("explore-servers")
            .role(Role::Group)
            .aria_label("Server")
            .flex_wrap()
            .gap(px(6.))
            .child(chip("explore-server-own".into(), own, None));
        for server in PUBLIC_SERVERS {
            servers = servers.child(chip(
                SharedString::from(format!("explore-server-{server}")),
                server.to_owned(),
                Some(server.to_owned()),
            ));
        }
        let rows = directory
            .rooms
            .iter()
            .map(|room| self.explore_row(room, cx));
        let status: Option<String> = if directory.loading && directory.rooms.is_empty() {
            Some("Loading rooms…".into())
        } else if directory.error.is_some() {
            Some("This server's room directory couldn't be loaded.".into())
        } else if directory.rooms.is_empty() {
            Some("No public rooms found.".into())
        } else {
            None
        };
        v_flex()
            .w_full()
            .gap(px(12.))
            .child(servers)
            .child(
                div().w_full().child(
                    Input::new(&page.name)
                        .aria_label("Search rooms")
                        .prefix(Icon::new(IconName::Search).size(px(14.)).text_color(muted)),
                ),
            )
            .child(
                v_flex()
                    .id("explore-results")
                    .role(Role::List)
                    .aria_label("Public rooms")
                    .w_full()
                    .gap(px(2.))
                    .children(rows),
            )
            .children(status.map(|text| {
                div()
                    .id("explore-status")
                    .role(Role::Status)
                    .text_color(muted)
                    .child(text)
            }))
            .when(
                !directory.rooms.is_empty() && !directory.exhausted,
                |body| {
                    body.child(
                        Button::new("explore-more")
                            .small()
                            .ghost()
                            .label(if directory.loading {
                                "Loading…"
                            } else {
                                "Show More"
                            })
                            .disabled(directory.loading)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.directory_more(window, cx);
                            })),
                    )
                },
            )
            .into_any_element()
    }

    fn explore_row(&self, room: &DirectoryRoom, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let name = room
            .name
            .clone()
            .or_else(|| room.alias.clone())
            .unwrap_or_else(|| room.room_id.clone());
        let members = match room.members {
            1 => "1 member".to_owned(),
            n => format!("{} members", grouped(n)),
        };
        let action: (&'static str, bool) = if room.joined {
            ("Open", true)
        } else {
            match room.join_rule {
                JoinRule::Public => ("Join", true),
                JoinRule::Knock | JoinRule::KnockRestricted => ("Ask to Join", true),
                _ => ("Invite only", false),
            }
        };
        let target_id = room.alias.clone().unwrap_or_else(|| room.room_id.clone());
        let room_id = room.room_id.clone();
        let joined = room.joined;
        let knock =
            !joined && matches!(room.join_rule, JoinRule::Knock | JoinRule::KnockRestricted);
        let via: Vec<ServerName> = self
            .explore_server
            .as_deref()
            .and_then(|name| ServerName::try_from(name).ok())
            .into_iter()
            .collect();
        h_flex()
            .id(SharedString::from(format!("explore-room-{}", room.room_id)))
            .role(Role::ListItem)
            .aria_label(format!(
                "{name}, {members}{}",
                room.topic
                    .as_deref()
                    .map(|topic| format!(", {topic}"))
                    .unwrap_or_default()
            ))
            .w_full()
            .min_h(px(52.))
            .py(px(6.))
            .px(px(6.))
            .gap(px(12.))
            .rounded(px(6.))
            .hover(|style| style.bg(cx.theme().secondary.opacity(0.5)))
            .child(self.avatar_image(
                room.avatar.as_deref(),
                &room.room_id,
                &name,
                36.,
                if room.is_space { 9. } else { 18. },
                cx,
            ))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(name.clone()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .child(members),
                            ),
                    )
                    .children(room.topic.clone().map(|topic| {
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(muted)
                            .child(topic)
                    })),
            )
            .child(
                Button::new(SharedString::from(format!("explore-join-{}", room.room_id)))
                    .small()
                    .when(action.1 && !joined, |button| button.primary())
                    .label(action.0)
                    .accessibility_label(format!("{} {name}", action.0))
                    .disabled(!action.1 || (self.preview && !joined))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if joined {
                            this.close_management_page(window, cx);
                            this.select_room(room_id.clone(), window, cx);
                            return;
                        }
                        match RoomTarget::parse(&target_id, via.clone()) {
                            Ok(target) if knock => this.knock_room(target, None, window, cx),
                            Ok(target) => this.join_room(target, window, cx),
                            Err(_) => this.status = "This room's address isn't valid.".into(),
                        }
                        cx.notify();
                    })),
            )
            .test_support()
            .into_any_element()
    }
}

/// Offline preview rooms; sample data only.
fn sample_rooms() -> Vec<DirectoryRoom> {
    let room = |id: &str, name: &str, topic: &str, members, join_rule, joined| DirectoryRoom {
        room_id: format!("!{id}:example.org"),
        alias: Some(format!("#{id}:example.org")),
        name: Some(name.into()),
        topic: Some(topic.into()),
        avatar: None,
        members,
        world_readable: true,
        guest_can_join: false,
        join_rule,
        is_space: false,
        joined,
    };
    vec![
        room(
            "matrix",
            "Matrix HQ",
            "The Matrix protocol, clients and servers",
            11_904,
            JoinRule::Public,
            false,
        ),
        room(
            "rust",
            "Rust",
            "The Rust programming language",
            4_212,
            JoinRule::Public,
            false,
        ),
        room(
            "design",
            "Design team",
            "Day-to-day design talk",
            3,
            JoinRule::Public,
            true,
        ),
        room(
            "beta",
            "Beta testers",
            "Early builds and feedback",
            186,
            JoinRule::Knock,
            false,
        ),
    ]
}

/// 11904 -> "11,904", as member counts read in lists.
pub(crate) fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn member_counts_group_thousands() {
        assert_eq!(super::grouped(7), "7");
        assert_eq!(super::grouped(186), "186");
        assert_eq!(super::grouped(4212), "4,212");
        assert_eq!(super::grouped(1_234_567), "1,234,567");
    }
}
