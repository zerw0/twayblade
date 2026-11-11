//! Window chrome: title bar, space rail, sidebar and the account menu.
use super::*;

/// One space-rail tile. `space: None` is Home.
struct RailTile {
    space: Option<String>,
    name: String,
    content: AnyElement,
    fill: Option<Hsla>,
    selected: bool,
    unread: u64,
}

impl MatrixApp {
    /// A toolbar row. On macOS it is also the window's title bar: dragging moves
    /// the window and double-clicking zooms it, as in native apps.
    pub(crate) fn toolbar(&self, id: &'static str, cx: &mut Context<Self>) -> Stateful<Div> {
        h_flex()
            .id(id)
            .h(px(TOOLBAR_HEIGHT))
            .flex_none()
            .when(cfg!(target_os = "macos"), |bar| {
                bar.window_control_area(WindowControlArea::Drag)
                    .on_double_click(|_, window, _| window.titlebar_double_click())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.window_drag = true),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.window_drag = false),
                    )
                    .on_mouse_down_out(cx.listener(|this, _, _, _| this.window_drag = false))
                    .on_mouse_move(cx.listener(|this, _, window, _| {
                        if this.window_drag {
                            this.window_drag = false;
                            window.start_window_move();
                        }
                    }))
            })
    }

    fn rail_tile(&self, tile: RailTile, cx: &mut Context<Self>) -> AnyElement {
        let RailTile {
            space,
            name,
            content,
            fill,
            selected,
            unread,
        } = tile;
        let palette = theme::palette(cx);
        let (bg, fg) = match fill {
            Some(fill) => (fill, palette.on_avatar),
            None => (cx.theme().secondary, cx.theme().foreground),
        };
        let element_id = SharedString::from(match &space {
            Some(id) => format!("space-{id}"),
            None => "home".to_owned(),
        });
        let tooltip = if unread > 0 {
            format!("{name} · {unread} unread")
        } else {
            name.clone()
        };
        // A bar on the rail's left edge marks the selection (tall) or unread (short),
        // and the tile itself never changes shape.
        let bar = if selected {
            24.
        } else if unread > 0 {
            8.
        } else {
            0.
        };
        h_flex()
            .relative()
            .w(px(RAIL_WIDTH))
            .h(px(36.))
            .justify_center()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px((36. - bar) / 2.))
                    .w(px(3.))
                    .h(px(bar))
                    .rounded_r(px(3.))
                    .bg(cx.theme().foreground),
            )
            .child(
                Button::new(element_id)
                    .ghost()
                    .size(px(34.))
                    .p_0()
                    .rounded(px(9.))
                    .accessibility_label(name)
                    .tooltip(tooltip)
                    .child(
                        div()
                            .size(px(34.))
                            .rounded(px(9.))
                            .overflow_hidden()
                            .bg(bg)
                            .text_color(fg)
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(!selected, |tile| tile.opacity(0.85))
                            .child(content),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_space(space.clone(), window, cx)
                    })),
            )
            .into_any_element()
    }

    pub(crate) fn rail_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = theme::palette(cx);
        let selected = self.navigation.selected_space.clone();
        let in_any_space = |id: &str| {
            self.snapshot
                .spaces
                .iter()
                .any(|space| space.room_ids.iter().any(|room| room == id))
        };
        let home_unread = self
            .snapshot
            .rooms
            .iter()
            .filter(|room| !room.is_space && !room.muted && !in_any_space(&room.id))
            .fold(0u64, |sum, room| sum.saturating_add(room.unread));
        let mut rail = v_flex()
            .id("space-rail")
            .w(px(RAIL_WIDTH))
            .h_full()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .py(px(10.))
            .bg(palette.rail)
            .border_r_1()
            .border_color(cx.theme().border)
            .overflow_y_scroll();
        rail = rail.child(self.rail_tile(
            RailTile {
                space: None,
                name: "Home".into(),
                content: Icon::new(IconName::House).size(px(15.)).into_any_element(),
                fill: None,
                selected: selected.is_none(),
                unread: if self.snapshot.spaces_loaded {
                    home_unread
                } else {
                    0
                },
            },
            cx,
        ));
        for space in self.snapshot.spaces.clone() {
            let unread = self
                .snapshot
                .rooms
                .iter()
                .filter(|room| !room.is_space && !room.muted && space.room_ids.contains(&room.id))
                .fold(0u64, |sum, room| sum.saturating_add(room.unread));
            // A space's own picture fills the tile; initials remain the fallback.
            let label = if space.avatar.is_some() {
                self.avatar_image(space.avatar.as_deref(), &space.id, &space.name, 34., 9., cx)
                    .into_any_element()
            } else {
                div().child(theme::initials(&space.name)).into_any_element()
            };
            let name = if space.level > 0 {
                format!("{} (subspace)", space.name)
            } else {
                space.name.clone()
            };
            let weak = cx.weak_entity();
            let browse_id = space.id.clone();
            let leave_id = space.id.clone();
            let tile = self.rail_tile(
                RailTile {
                    fill: Some(theme::avatar_color(&space.id)),
                    selected: selected.as_ref() == Some(&space.id),
                    space: Some(space.id.clone()),
                    name,
                    content: label,
                    unread,
                },
                cx,
            );
            rail = rail.child(
                div()
                    .id(SharedString::from(format!("space-context-{}", space.id)))
                    .child(tile)
                    .context_menu(move |menu, _, _| {
                        let browse = browse_id.clone();
                        let leave = leave_id.clone();
                        let view = weak.clone();
                        let leave_view = weak.clone();
                        menu.item(PopupMenuItem::new("Browse rooms").on_click(
                            move |_, window, cx| {
                                let _ = view.update(cx, |this, cx| {
                                    this.open_management(
                                        crate::ui::spaces::ManageKind::Directory(browse.clone()),
                                        window,
                                        cx,
                                    )
                                });
                            },
                        ))
                        .item(
                            PopupMenuItem::new("Leave space…").on_click(move |_, window, cx| {
                                let _ = leave_view.update(cx, |this, cx| {
                                    this.open_management(
                                        crate::ui::spaces::ManageKind::Leave(leave.clone()),
                                        window,
                                        cx,
                                    )
                                });
                            }),
                        )
                    }),
            );
        }
        rail = rail.child(
            Button::new("join-space")
                .ghost()
                .size(px(34.))
                .rounded(px(9.))
                .border_1()
                .border_dashed()
                .border_color(cx.theme().border)
                .icon(IconName::Plus)
                .accessibility_label("Join a space")
                .tooltip("Join a Space…")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_management(crate::ui::spaces::ManageKind::JoinSpace, window, cx)
                })),
        );
        rail.into_any_element()
    }

    pub(crate) fn room_row(&self, room: &Room, cx: &mut Context<Self>) -> AnyElement {
        let palette = theme::palette(cx);
        let density = self.preferences.appearance.sidebar_density;
        let selected = self.navigation.selected_room.as_ref() == Some(&room.id);
        let id = room.id.clone();
        let unread = room.unread > 0 && !selected && !room.muted;
        let muted = cx.theme().muted_foreground;
        // DMs and rooms with their own picture show it; others show # or a lock.
        let icon = if room.direct || room.avatar.is_some() {
            let avatar = self
                .avatar_image(
                    room.avatar.as_deref(),
                    &room.id,
                    &room.name,
                    16.,
                    if room.direct { 8. } else { 4. },
                    cx,
                )
                .text_size(px(8.))
                .into_any_element();
            if room.direct {
                self.with_presence(avatar, room.presence.as_ref(), 16., cx)
            } else {
                avatar
            }
        } else {
            div()
                .w(px(16.))
                .flex()
                .justify_center()
                .child(
                    Icon::new(if room.public {
                        IconName::Hash
                    } else {
                        IconName::Lock
                    })
                    .size(px(13.))
                    .text_color(muted),
                )
                .into_any_element()
        };
        // Mentions get the accent; ordinary unread counts stay quiet grey.
        let badge =
            (!selected && !room.muted && (room.mentions > 0 || room.unread > 0)).then(|| {
                if room.mentions > 0 {
                    badge(room.mentions, cx)
                        .bg(cx.theme().primary)
                        .text_color(cx.theme().primary_foreground)
                } else {
                    badge(room.unread, cx)
                }
            });
        let row = Button::new(SharedString::from(format!("room-{}", room.id)))
            .ghost()
            .w_full()
            .h(px(density.row()))
            .p_0()
            .rounded(px(6.))
            .accessibility_label(if room.muted {
                format!("{}, notifications muted", room.name)
            } else if room.mentions > 0 {
                format!("{}, {} mentions", room.name, room.mentions)
            } else if room.unread > 0 {
                format!("{}, {} unread", room.name, room.unread)
            } else {
                room.name.clone()
            })
            .child(
                h_flex()
                    .size_full()
                    .px(px(8.))
                    .gap(px(8.))
                    .rounded(px(6.))
                    .when(selected, |row| row.bg(palette.selection))
                    .when(room.muted && !selected, |row| row.opacity(0.55))
                    .text_color(cx.theme().foreground)
                    .child(icon)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_left()
                            .text_size(px(13.))
                            .when(unread || selected, |name| {
                                name.font_weight(FontWeight::SEMIBOLD)
                            })
                            .child(room.name.clone()),
                    )
                    .children(badge),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                // Choosing a search result ends the conversation search.
                if !this.search.read(cx).value().is_empty() && !this.search_open {
                    this.search
                        .update(cx, |state, cx| state.set_value("", window, cx));
                }
                this.select_room(id.clone(), window, cx)
            }));
        let room_id = room.id.clone();
        let favourite = room.favourite;
        let weak = cx.entity().downgrade();
        let available = self.preview || self.snapshot.notifications.ready;
        let selected_mode = self.snapshot.notifications.room_mode(&room.id);
        div()
            .id(SharedString::from(format!("room-context-{}", room.id)))
            .child(row)
            .context_menu(move |mut menu, _, _| {
                use twayblade_core::notifications::{RoomMode, RuleChange};
                let favourite_view = weak.clone();
                let favourite_room = room_id.clone();
                menu = menu
                    .item(
                        PopupMenuItem::new(if favourite {
                            "Remove from Favourites"
                        } else {
                            "Add to Favourites"
                        })
                        .on_click(move |_, window, cx| {
                            let _ = favourite_view.update(cx, |this, cx| {
                                this.set_favourite(favourite_room.clone(), !favourite, window, cx)
                            });
                        }),
                    )
                    .separator();
                if !available {
                    return menu;
                }
                for (label, mode) in [
                    ("Notifications: Default", RoomMode::Default),
                    ("All messages", RoomMode::All),
                    ("Mentions and keywords", RoomMode::Mentions),
                    ("Mute", RoomMode::Mute),
                ] {
                    let weak = weak.clone();
                    let room_id = room_id.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .checked(mode == selected_mode)
                            .on_click(move |_, window, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.change_notification_rule(
                                        RuleChange::Room(room_id.clone(), mode),
                                        window,
                                        cx,
                                    )
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    /// One collapsible sidebar section with an uppercase header and an optional
    /// hover "+" action.
    pub(crate) fn sidebar_section(
        &self,
        key: &'static str,
        label: &'static str,
        rooms: Vec<&Room>,
        add: Option<(&'static str, crate::ui::spaces::ManageKind)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = self.preferences.appearance.sidebar_density;
        let collapsed = self.collapsed_sections.contains(key);
        let group_name = SharedString::from(format!("section-{key}"));
        let header = h_flex()
            .group(group_name.clone())
            .h(px(24.))
            .child(
                div()
                    .id(SharedString::from(format!("section-toggle-{key}")))
                    .flex_1()
                    .min_w_0()
                    .h(px(22.))
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .hover(|style| style.bg(cx.theme().secondary.opacity(0.5)))
                    .child(
                        h_flex()
                            .gap(px(4.))
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                Icon::new(if collapsed {
                                    IconName::ChevronRight
                                } else {
                                    IconName::ChevronDown
                                })
                                .size(px(10.)),
                            )
                            .child(label),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.collapsed_sections.remove(key) {
                            this.collapsed_sections.insert(key);
                        }
                        cx.notify();
                    })),
            )
            .when_some(add, |header, (tooltip, kind)| {
                header.child(
                    div()
                        .opacity(0.)
                        .group_hover(group_name.clone(), |style| style.opacity(1.))
                        .child(
                            Button::new(SharedString::from(format!("section-add-{key}")))
                                .ghost()
                                .size(px(20.))
                                .icon(IconName::Plus)
                                .accessibility_label(tooltip)
                                .tooltip(tooltip)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_management(kind.clone(), window, cx)
                                })),
                        ),
                )
            });
        let selected = self.navigation.selected_room.as_deref();
        let mut section = v_flex().pt(px(density.section_gap())).child(header);
        for room in rooms {
            // Collapsed sections still show the open room and anything needing attention.
            let keep = !collapsed
                || selected == Some(room.id.as_str())
                || (!room.muted && (room.mentions > 0 || room.unread > 0));
            if keep {
                section = section.child(self.room_row(room, cx));
            }
        }
        section.into_any_element()
    }

    /// A status dot over an avatar's corner, only when the server shares presence.
    /// Your avatar in the title bar: profile, status (when the server shares
    /// presence) and account settings.
    pub(crate) fn own_menu(&self, user: String, cx: &mut Context<Self>) -> AnyElement {
        use gpui_kit::component::menu::DropdownMenu;
        use twayblade_core::presence::State;
        let avatar = self.avatar_image(self.own_avatar.as_deref(), &user, &user, 22., 11., cx);
        let own = self.snapshot.own_presence.clone();
        let enabled = self.snapshot.presence_enabled && !self.preview;
        let current = own.as_ref().map(|presence| presence.state);
        let status_msg = own
            .as_ref()
            .and_then(|presence| presence.status_msg.clone());
        let weak = cx.entity().downgrade();
        Button::new("my-profile")
            .ghost()
            .h(px(28.))
            .px(px(3.))
            .accessibility_label("Account and status")
            .tooltip(user.clone())
            .child(self.with_presence(avatar.into_any_element(), own.as_ref(), 22., cx))
            .dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, _| {
                let profile = weak.clone();
                let id = user.clone();
                menu = menu.item(PopupMenuItem::new("Your Profile").on_click(
                    move |_, window, cx| {
                        let _ = profile.update(cx, |this, cx| {
                            this.open_profile(id.clone(), "You".into(), window, cx)
                        });
                    },
                ));
                if enabled {
                    menu = menu.separator();
                    for (label, state) in [
                        ("Online", State::Online),
                        ("Away", State::Unavailable),
                        ("Invisible", State::Offline),
                    ] {
                        let weak = weak.clone();
                        let message = status_msg.clone();
                        menu = menu.item(
                            PopupMenuItem::new(label)
                                .checked(current == Some(state))
                                .on_click(move |_, window, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.set_presence(state, message.clone(), window, cx)
                                    });
                                }),
                        );
                    }
                    if status_msg.is_some() {
                        let weak = weak.clone();
                        menu = menu.item(PopupMenuItem::new("Clear Status Message").on_click(
                            move |_, window, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    let state = this
                                        .snapshot
                                        .own_presence
                                        .as_ref()
                                        .map_or(State::Online, |presence| presence.state);
                                    this.set_presence(state, None, window, cx)
                                });
                            },
                        ));
                    }
                }
                let settings = weak.clone();
                menu.separator()
                    .item(
                        PopupMenuItem::new("Account Settings…").on_click(move |_, _, cx| {
                            let _ = settings.update(cx, |this, cx| {
                                this.open_settings(crate::ui::settings::Section::Account, cx)
                            });
                        }),
                    )
            })
            .into_any_element()
    }

    pub(crate) fn with_presence(
        &self,
        avatar: AnyElement,
        presence: Option<&twayblade_core::presence::Presence>,
        size: f32,
        cx: &App,
    ) -> AnyElement {
        use twayblade_core::presence::State;
        let palette = theme::palette(cx);
        let Some(presence) = presence.filter(|_| self.snapshot.presence_enabled) else {
            return avatar;
        };
        let dot = (size * 0.38).clamp(7., 11.);
        let (fill, hollow) = match presence.state {
            State::Online => (palette.online, false),
            State::Unavailable => (palette.warning, false),
            State::Offline => (cx.theme().muted_foreground, true),
        };
        div()
            .relative()
            .flex_none()
            .child(avatar)
            .child(
                div()
                    .absolute()
                    .right(px(-2.))
                    .bottom(px(-2.))
                    .size(px(dot))
                    .rounded_full()
                    .border_2()
                    .border_color(cx.theme().background)
                    .bg(if hollow { cx.theme().background } else { fill })
                    .when(hollow, |dot| {
                        dot.child(
                            div()
                                .size_full()
                                .rounded_full()
                                .border_1()
                                .border_color(fill),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(crate) fn search_messages_row(&self, query: &str, cx: &mut Context<Self>) -> AnyElement {
        Button::new("search-messages")
            .ghost()
            .w_full()
            .h(px(30.))
            .px(px(8.))
            .justify_start()
            .accessibility_label(format!("Search messages for {query}"))
            .child(
                h_flex()
                    .w_full()
                    .gap(px(8.))
                    .child(
                        Icon::new(IconName::Search)
                            .size(px(14.))
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_left()
                            .child(format!("Search messages for “{query}”")),
                    )
                    .child(
                        div()
                            .px(px(4.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(cx.theme().border)
                            .text_size(px(10.5))
                            .text_color(cx.theme().muted_foreground)
                            .child("↩"),
                    ),
            )
            .on_click(cx.listener(|this, _, window, cx| this.run_message_search(window, cx)))
            .into_any_element()
    }

    pub(crate) fn sidebar_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let query = self.search.read(cx).value().to_lowercase();
        let visible: Vec<Room> = self
            .snapshot
            .rooms
            .iter()
            .filter(|room| {
                // A query finds conversations everywhere (Home, every space,
                // DMs), not only in the space currently shown.
                !room.is_space
                    && if query.is_empty() {
                        self.navigation.room_visible(&room.id, &self.snapshot)
                    } else {
                        room.name.to_lowercase().contains(&query)
                    }
            })
            .cloned()
            .collect();
        let selected = self.navigation.selected_space.as_ref();
        let title = self
            .snapshot
            .spaces
            .iter()
            .find(|space| Some(&space.id) == selected)
            .map(|space| space.name.clone())
            .unwrap_or_else(|| "Home".into());
        let title = if query.is_empty() {
            title
        } else {
            "All conversations".into()
        };
        let mut list = v_flex()
            .id("room-list")
            .flex_1()
            .min_h_0()
            .px(px(8.))
            .pb(px(10.))
            .overflow_y_scroll();
        let typed = self.search.read(cx).value().trim().to_owned();
        if !typed.is_empty() {
            list = list.child(div().pt(px(6.)).child(self.search_messages_row(&typed, cx)));
        }
        if !self.snapshot.invites.is_empty() {
            list = list.child(self.invite_section(cx));
        }
        let favourites: Vec<&Room> = visible.iter().filter(|room| room.favourite).collect();
        let channels: Vec<&Room> = visible
            .iter()
            .filter(|room| !room.favourite && !room.direct)
            .collect();
        let direct: Vec<&Room> = visible
            .iter()
            .filter(|room| !room.favourite && room.direct)
            .collect();
        if !favourites.is_empty() {
            list =
                list.child(self.sidebar_section("favourites", "Favourites", favourites, None, cx));
        }
        if !channels.is_empty() {
            list = list.child(self.sidebar_section(
                "rooms",
                "Rooms",
                channels,
                Some(("Create Room…", crate::ui::spaces::ManageKind::Create)),
                cx,
            ));
        }
        if !direct.is_empty() {
            list = list.child(self.sidebar_section(
                "direct",
                "Direct messages",
                direct,
                Some(("New Direct Message…", crate::ui::spaces::ManageKind::Dm)),
                cx,
            ));
        }
        if visible.is_empty() {
            list = list.child(
                div()
                    .p(px(8.))
                    .text_color(muted)
                    .child(if !query.is_empty() {
                        "No conversations match your search."
                    } else if self.snapshot.spaces_loaded {
                        "No conversations here yet."
                    } else {
                        "Organizing your rooms…"
                    }),
            );
        }
        v_flex()
            .w(px(SIDEBAR_WIDTH))
            .flex_none()
            .h_full()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .h(px(44.))
                    .flex_none()
                    .pl(px(12.))
                    .pr(px(8.))
                    .gap(px(4.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        Button::new("new-room")
                            .ghost()
                            .size(px(26.))
                            .icon(IconName::Plus)
                            .accessibility_label("Create a room")
                            .tooltip("Create Room…  ⇧⌘N")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_management(
                                    crate::ui::spaces::ManageKind::Create,
                                    window,
                                    cx,
                                )
                            })),
                    )
                    .child(
                        Button::new("new-dm")
                            .size(px(26.))
                            .rounded(px(7.))
                            .icon(IconName::SquarePen)
                            .accessibility_label("Start a direct message")
                            .tooltip("New Direct Message…  ⌘N")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_management(crate::ui::spaces::ManageKind::Dm, window, cx)
                            })),
                    ),
            )
            .child(list)
            .into_any_element()
    }

    /// The window title bar: navigation on the left, search in the middle,
    /// status and account on the right. On macOS it is also the drag region.
    pub(crate) fn window_titlebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = theme::palette(cx);
        let muted = cx.theme().muted_foreground;
        let user = self.snapshot.user_id.clone();
        let connected = self.snapshot.sync_status.starts_with("Connected");
        let nav_button = |id: &'static str, icon: IconName, label: &'static str, enabled: bool| {
            Button::new(id)
                .ghost()
                .size(px(26.))
                .icon(icon)
                .accessibility_label(label)
                .tooltip(label)
                .disabled(!enabled)
        };
        self.toolbar("window-titlebar", cx)
            .h(px(40.))
            .gap(px(12.))
            .pl(px(if cfg!(target_os = "macos") {
                TRAFFIC_LIGHTS_WIDTH
            } else {
                8.
            }))
            .pr(px(10.))
            .bg(palette.chrome)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        nav_button(
                            "toggle-sidebar",
                            IconName::PanelLeft,
                            "Toggle Sidebar  ⌃⌘S",
                            true,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.sidebar_open = !this.sidebar_open;
                            cx.notify();
                        })),
                    )
                    .child(
                        nav_button(
                            "nav-back",
                            IconName::ChevronLeft,
                            "Back  ⌘[",
                            !self.history_back.is_empty(),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.navigate_history(true, window, cx)
                        })),
                    )
                    .child(
                        nav_button(
                            "nav-forward",
                            IconName::ChevronRight,
                            "Forward  ⌘]",
                            !self.history_forward.is_empty(),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.navigate_history(false, window, cx)
                        })),
                    ),
            )
            .child(
                div()
                    .id("title-search")
                    .w(px(400.))
                    .max_w(relative(0.4))
                    .flex_none()
                    .child(
                        Input::new(&self.search)
                            .small()
                            .prefix(Icon::new(IconName::Search).xsmall().text_color(muted))
                            .suffix(
                                div()
                                    .px(px(5.))
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .text_size(px(10.5))
                                    .text_color(muted)
                                    .child("⌘K"),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .justify_end()
                    .gap(px(4.))
                    // A healthy connection shows nothing (your avatar already carries
                    // your status); a dot and text appear only when it needs attention.
                    .when(!connected, |bar| {
                        bar.child({
                            let status = self.snapshot.sync_status.clone();
                            h_flex()
                                .id("sync-status")
                                .min_w_0()
                                .gap(px(6.))
                                .px(px(6.))
                                .text_size(px(11.5))
                                .text_color(muted)
                                .child(div().flex_none().size(px(7.)).rounded(px(4.)).bg(
                                    if connected {
                                        palette.online
                                    } else {
                                        palette.warning
                                    },
                                ))
                                .when(!connected, |row| {
                                    row.child(div().truncate().child(status.clone()))
                                })
                                .tooltip(move |window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(status.clone())
                                        .build(window, cx)
                                })
                        })
                    })
                    .child(
                        Button::new("open-notifications")
                            .ghost()
                            .size(px(26.))
                            .icon(IconName::Bell)
                            .accessibility_label("Notification settings")
                            .tooltip("Notifications")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_settings(crate::ui::settings::Section::Notifications, cx)
                            })),
                    )
                    .child(
                        Button::new("open-settings")
                            .ghost()
                            .size(px(26.))
                            .icon(IconName::Settings)
                            .accessibility_label("Settings")
                            .tooltip("Settings  ⌘,")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_settings(crate::ui::settings::Section::Appearance, cx)
                            })),
                    )
                    .child(self.own_menu(user.clone(), cx)),
            )
            .into_any_element()
    }
}
