use gpui_kit::assets::IconName;
use gpui_kit::component::{
    bubble::{Bubble, BubbleVariant},
    button::{Button, ButtonVariants},
    input::{Input, Textarea},
    message::MessageAlignment,
    message_scroller::MessageScroller,
    text::TextView,
    *,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::{CloseProfile, FindConversation, MatrixApp},
    model,
};

impl MatrixApp {
    fn sign_in_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut form = v_flex()
            .w_80()
            .gap_4()
            .child(
                Icon::new(IconName::MessagesSquare)
                    .size_10()
                    .text_color(cx.theme().primary),
            )
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::BOLD)
                    .child("Welcome to Matrix"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Your conversations. Your server."),
            );
        if self.options.is_none() {
            form = form
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child("Homeserver"),
                )
                .child(Input::new(&self.homeserver).disabled(self.busy))
                .child(
                    Button::new("discover")
                        .primary()
                        .label("Continue")
                        .disabled(self.busy || self.backend.is_none())
                        .on_click(cx.listener(|this, _, window, cx| this.discover(window, cx))),
                );
        } else {
            form = form.child(
                h_flex()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .truncate()
                            .child(self.homeserver.read(cx).value()),
                    )
                    .child(
                        Button::new("change-server")
                            .ghost()
                            .small()
                            .label("Change")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.options = None;
                                this.password
                                    .update(cx, |state, cx| state.set_value("", window, cx));
                                cx.notify();
                            })),
                    ),
            );
            if self
                .options
                .as_ref()
                .is_some_and(|options| options.password)
            {
                form = form
                    .child(div().text_sm().child("Username"))
                    .child(Input::new(&self.username).disabled(self.busy))
                    .child(div().text_sm().child("Password"))
                    .child(Input::new(&self.password).disabled(self.busy))
                    .child(
                        Button::new("sign-in")
                            .primary()
                            .label("Sign in")
                            .disabled(
                                self.busy
                                    || self.username.read(cx).value().trim().is_empty()
                                    || self.password.read(cx).value().is_empty(),
                            )
                            .on_click(cx.listener(|this, _, window, cx| this.sign_in(window, cx))),
                    );
            }
        }
        if self.busy {
            form = form.child(div().text_sm().child("Working…"));
        }
        form = form.child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(self.status.clone()),
        );
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .p_6()
            .child(form)
            .into_any_element()
    }

    fn navigation_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.navigation.selected_space.as_ref();
        let mut rail = v_flex()
            .w_12()
            .h_full()
            .flex_none()
            .gap_3()
            .py_4()
            .px_2()
            .bg(cx.theme().sidebar)
            .child(
                Button::new("home")
                    .ghost()
                    .icon(IconName::House)
                    .tooltip("Home")
                    .selected(selected.is_none())
                    .on_click(
                        cx.listener(|this, _, window, cx| this.select_space(None, window, cx)),
                    ),
            );
        for space in &self.snapshot.spaces {
            let id = space.id.clone();
            let unread = self
                .snapshot
                .rooms
                .iter()
                .filter(|room| !room.is_space && space.room_ids.contains(&room.id))
                .fold(0u64, |sum, room| sum.saturating_add(room.unread));
            let label = format!(
                "{}{}",
                if space.level > 0 { "↳" } else { "" },
                space.name.chars().next().unwrap_or('?').to_uppercase()
            );
            rail = rail.child(
                Button::new(SharedString::from(format!("space-{}", space.id)))
                    .ghost()
                    .label(label)
                    .tooltip(if unread > 0 {
                        format!("{} · {unread} unread", space.name)
                    } else {
                        space.name.clone()
                    })
                    .selected(selected == Some(&space.id))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_space(Some(id.clone()), window, cx)
                    })),
            );
        }
        let title = self
            .snapshot
            .spaces
            .iter()
            .find(|space| Some(&space.id) == selected)
            .map(|space| space.name.as_str())
            .unwrap_or("Home");
        let query = self.search.read(cx).value().to_lowercase();
        let mut rooms = v_flex()
            .id("room-list")
            .gap_1()
            .p_2()
            .overflow_y_scroll()
            .flex_1()
            .min_h_0();
        let mut count = 0;
        for room in &self.snapshot.rooms {
            if room.is_space
                || !self.navigation.room_visible(&room.id, &self.snapshot)
                || !room.name.to_lowercase().contains(&query)
            {
                continue;
            }
            count += 1;
            let id = room.id.clone();
            let mut label = h_flex()
                .w_full()
                .gap_2()
                .child(Icon::new(IconName::Hash).small())
                .child(div().flex_1().min_w_0().truncate().child(room.name.clone()));
            if room.unread > 0 {
                label = label.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(room.unread.to_string()),
                );
            }
            rooms = rooms.child(
                Button::new(SharedString::from(format!("room-{}", room.id)))
                    .ghost()
                    .w_full()
                    .selected(self.navigation.selected_room.as_ref() == Some(&room.id))
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_room(id.clone(), window, cx)
                    })),
            );
        }
        if count == 0 {
            rooms = rooms.child(
                div()
                    .p_3()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(if self.snapshot.spaces_loaded {
                        "No conversations here yet"
                    } else {
                        "Organizing your rooms…"
                    }),
            );
        }
        let user = self.snapshot.user_id.clone();
        let sidebar = v_flex()
            .w_64()
            .h_full()
            .flex_none()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .px_4()
                    .pt_4()
                    .pb_3()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_owned()),
            )
            .child(div().px_3().pb_3().child(Input::new(&self.search).small()))
            .child(
                div()
                    .px_4()
                    .pb_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Conversations"),
            )
            .child(rooms)
            .child(
                v_flex()
                    .p_3()
                    .gap_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.snapshot.sync_status.clone()),
                    )
                    .child(
                        h_flex()
                            .justify_between()
                            .gap_1()
                            .child(
                                Button::new("my-profile")
                                    .ghost()
                                    .small()
                                    .icon(IconName::UserRound)
                                    .label(if self.preview {
                                        "Offline preview"
                                    } else {
                                        "My profile…"
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_profile(user.clone(), "You".into(), window, cx)
                                    })),
                            )
                            .child(
                                Button::new("logout")
                                    .ghost()
                                    .small()
                                    .icon(IconName::LogOut)
                                    .tooltip("Sign out")
                                    .disabled(self.preview || self.busy)
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.logout(window, cx)),
                                    ),
                            ),
                    ),
            );
        h_flex()
            .h_full()
            .flex_none()
            .items_stretch()
            .child(rail)
            .child(sidebar)
            .into_any_element()
    }

    fn message_group(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(range) = self.groups.get(ix) else {
            return div().into_any_element();
        };
        let first = &self.snapshot.messages[range.start];
        let outgoing = first.sender == self.snapshot.user_id;
        let id = first.sender.clone();
        let name = first.display_name.clone();
        let mut content = v_flex().min_w_0().gap_1().child(
            h_flex()
                .gap_2()
                .when(outgoing, |row| row.justify_end())
                .child(
                    Button::new(SharedString::from(format!("sender-{}", first.id)))
                        .ghost()
                        .xsmall()
                        .label(name.clone())
                        .tooltip("View profile")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_profile(id.clone(), name.clone(), window, cx)
                        })),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            chrono::DateTime::from_timestamp_millis(first.timestamp_ms)
                                .map(|time| {
                                    time.with_timezone(&chrono::Local)
                                        .format("%H:%M")
                                        .to_string()
                                })
                                .unwrap_or_default(),
                        ),
                ),
        );
        for message in &self.snapshot.messages[range.clone()] {
            content = content.child(
                Bubble::new()
                    .alignment(if outgoing {
                        MessageAlignment::End
                    } else {
                        MessageAlignment::Start
                    })
                    .with_variant(if outgoing {
                        BubbleVariant::Filled
                    } else {
                        BubbleVariant::Secondary
                    })
                    .child(
                        TextView::markdown(
                            SharedString::from(format!("message-{}", message.id)),
                            model::plain_text(&message.body),
                        )
                        .selectable(true),
                    ),
            );
            if !message.delivery.is_empty() {
                content = content.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(message.delivery.clone()),
                );
            }
        }
        let mut row = h_flex()
            .id(SharedString::from(format!("group-{}", first.id)))
            .w_full()
            .items_start()
            .gap_2()
            .px_4()
            .py_2();
        if !outgoing {
            let id = first.sender.clone();
            let name = first.display_name.clone();
            row = row.child(
                Button::new(SharedString::from(format!("avatar-{}", first.id)))
                    .ghost()
                    .small()
                    .label(
                        first
                            .display_name
                            .chars()
                            .next()
                            .unwrap_or('?')
                            .to_uppercase()
                            .to_string(),
                    )
                    .tooltip("View profile")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_profile(id.clone(), name.clone(), window, cx)
                    })),
            );
        }
        row = row.child(content.flex_1());
        row.into_any_element()
    }

    fn conversation_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let room = self
            .snapshot
            .rooms
            .iter()
            .find(|room| self.navigation.selected_room.as_ref() == Some(&room.id));
        let Some(room) = room else {
            return v_flex()
                .flex_1()
                .h_full()
                .items_center()
                .justify_center()
                .gap_3()
                .child(
                    Icon::new(IconName::MessagesSquare)
                        .size_10()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Choose a conversation"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Select a room from the sidebar."),
                )
                .into_any_element();
        };
        let weak = cx.weak_entity();
        let history = MessageScroller::new("messages", self.scroller.clone(), move |ix, _, cx| {
            weak.update(cx, |this, cx| this.message_group(ix, cx))
                .unwrap_or_else(|_| div().into_any_element())
        })
        .flex_1()
        .min_h_0()
        .with_jump_button_label("Jump to latest");
        let ready = self.snapshot.timeline_ready && !self.preview && !self.busy;
        let editable = !self.busy && (self.preview || self.snapshot.timeline_ready);
        let empty = self.composer.read(cx).value().trim().is_empty();
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .px_4()
                    .py_3()
                    .gap_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(Icon::new(IconName::Hash).text_color(cx.theme().primary))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(room.name.clone()),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("earlier")
                            .ghost()
                            .small()
                            .label(if self.paginating {
                                "Loading…"
                            } else {
                                "Load earlier"
                            })
                            .disabled(!ready || self.paginating)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.load_earlier(window, cx)),
                            ),
                    ),
            )
            .when(self.groups.is_empty(), |view| {
                view.child(
                    div()
                        .p_4()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(if self.snapshot.timeline_ready {
                            "No messages to display yet"
                        } else {
                            "Loading conversation…"
                        }),
                )
            })
            .child(history)
            .when(!self.status.is_empty(), |view| {
                view.child(
                    div()
                        .px_4()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.status.clone()),
                )
            })
            .when(!self.snapshot.timeline_status.is_empty(), |view| {
                view.child(
                    div()
                        .px_4()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.snapshot.timeline_status.clone()),
                )
            })
            .child(
                h_flex()
                    .items_end()
                    .gap_2()
                    .p_3()
                    .child(
                        div().id("composer-panel").flex_1().min_w_0().child(
                            Textarea::new(&self.composer)
                                .accessibility_id("composer")
                                .w_full()
                                .disabled(!editable)
                                .aria_label("Message"),
                        ),
                    )
                    .child(
                        Button::new("send")
                            .primary()
                            .icon(IconName::ArrowUp)
                            .tooltip("Send message · Enter")
                            .disabled(!ready || empty || self.sending.contains(&room.id))
                            .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
                    ),
            )
            .into_any_element()
    }

    fn profile_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(page) = &self.profile else {
            return div().into_any_element();
        };
        let profile_id = page
            .profile
            .as_ref()
            .map(|profile| profile.user_id.as_str())
            .unwrap_or(&page.id);
        let name = page
            .profile
            .as_ref()
            .map(|profile| profile.display_name.as_str())
            .unwrap_or(&page.name);
        let mut profile = v_flex()
            .w_72()
            .h_full()
            .flex_none()
            .p_4()
            .gap_4()
            .bg(cx.theme().sidebar)
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Profile"))
                    .child(
                        Button::new("close-profile")
                            .ghost()
                            .small()
                            .icon(IconName::X)
                            .tooltip("Close profile · Escape")
                            .on_click(cx.listener(|this, _, _, cx| this.close_profile(cx))),
                    ),
            )
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(name.to_owned()),
            )
            .child(
                TextView::markdown("profile-user-id", model::plain_text(profile_id))
                    .selectable(true),
            )
            .child(
                Button::new("copy-profile-id")
                    .ghost()
                    .small()
                    .icon(IconName::Copy)
                    .label("Copy Matrix ID")
                    .on_click({
                        let id = page.id.clone();
                        move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(id.clone()))
                    }),
            );
        if page.loading {
            profile = profile.child(div().text_sm().child("Loading profile…"));
        }
        if let Some(info) = &page.profile {
            if let Some(room) = &info.room_name {
                profile = profile.child(div().text_sm().child(format!("Conversation: {room}")));
            }
            if let Some(room_name) = &info.room_display_name
                && room_name != &info.display_name
            {
                profile = profile.child(
                    div()
                        .text_sm()
                        .child(format!("Name in this room: {room_name}")),
                );
            }
            if !info.server_profile_available {
                profile = profile.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Showing the saved conversation profile."),
                );
            }
        }
        if !page.error.is_empty() {
            let id = page.id.clone();
            let name = page.name.clone();
            profile = profile
                .child(div().text_sm().child(page.error.clone()))
                .child(
                    Button::new("retry-profile")
                        .label("Try again")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_profile(id.clone(), name.clone(), window, cx)
                        })),
                );
        }
        if self.preview {
            profile = profile.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Offline sample profile"),
            );
        }
        profile.into_any_element()
    }
}

impl Render for MatrixApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_sm()
            .track_focus(&self.focus)
            .key_context("MatrixClient")
            .on_action(cx.listener(|this, _: &FindConversation, window, cx| {
                this.search.update(cx, |state, cx| state.focus(window, cx))
            }))
            .on_action(cx.listener(|this, _: &CloseProfile, _, cx| this.close_profile(cx)))
            .child(if self.snapshot.user_id.is_empty() {
                self.sign_in_view(cx)
            } else {
                h_flex()
                    .size_full()
                    .items_stretch()
                    .child(self.navigation_view(cx))
                    .child(self.conversation_view(cx))
                    .when(self.profile.is_some(), |view| {
                        view.child(self.profile_view(cx))
                    })
                    .into_any_element()
            })
    }
}
