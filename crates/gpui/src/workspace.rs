//! Signed-in workspace and sign-in window, laid out after the approved mockup:
//! space rail → conversations sidebar → conversation → optional details panel.
//! Controls whose feature does not exist yet are omitted rather than faked.

use chrono::{Local, TimeZone};
use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{
    bubble::{Bubble, BubbleContent, BubbleVariant},
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
    app::{CloseProfile, FindConversation, MatrixApp, ToggleDetails},
    backend::{
        DeliveryState, Member, MemberRole, MembersState, MessageKind, ReplyTarget, Room,
        TimelineAction,
    },
    model,
    theme::{self, Palette},
};

const RAIL_WIDTH: f32 = 64.;
const SIDEBAR_WIDTH: f32 = 264.;
const DETAILS_WIDTH: f32 = 280.;
const BUBBLE_MAX_WIDTH: f32 = 560.;

/// One space-rail tile. `space: None` is Home.
struct RailTile {
    space: Option<String>,
    name: String,
    content: AnyElement,
    fill: Option<Hsla>,
    selected: bool,
    unread: u64,
}

fn avatar(label: &str, fill: Hsla, size: f32, radius: f32, palette: &Palette) -> Div {
    div()
        .flex_none()
        .size(px(size))
        .rounded(px(radius))
        .bg(fill)
        .text_color(palette.on_avatar)
        .text_size(px((size * 0.4).max(10.)))
        .font_weight(FontWeight::BOLD)
        .flex()
        .items_center()
        .justify_center()
        .child(label.to_owned())
}

fn room_avatar(room: &Room, size: f32, palette: &Palette) -> Div {
    let radius = if room.direct { size / 2. } else { size * 0.27 };
    avatar(
        &theme::initials(&room.name),
        theme::avatar_color(&room.id),
        size,
        radius,
        palette,
    )
}

fn badge(count: u64, cx: &App) -> Div {
    div()
        .flex_none()
        .min_w(px(18.))
        .h(px(18.))
        .px(px(5.))
        .rounded(px(9.))
        .bg(cx.theme().primary)
        .text_color(cx.theme().primary_foreground)
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .flex()
        .items_center()
        .justify_center()
        .child(if count > 99 {
            "99+".to_owned()
        } else {
            count.to_string()
        })
}

fn time_label(timestamp_ms: i64) -> String {
    Local
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .map(|time| time.format("%H:%M").to_string())
        .unwrap_or_default()
}

fn day_label(timestamp_ms: i64) -> String {
    let Some(date) = Local
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .map(|time| time.date_naive())
    else {
        return String::new();
    };
    let today = Local::now().date_naive();
    if date == today {
        "Today".into()
    } else if today.pred_opt() == Some(date) {
        "Yesterday".into()
    } else {
        date.format("%A, %-d %B %Y").to_string()
    }
}

fn day_of(timestamp_ms: i64) -> Option<chrono::NaiveDate> {
    Local
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .map(|time| time.date_naive())
}

fn secret_store_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS Keychain"
    } else if cfg!(target_os = "windows") {
        "Windows Credential Manager"
    } else {
        "system keyring"
    }
}

impl MatrixApp {
    fn field_label(&self, text: &'static str) -> Div {
        div()
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(text)
    }

    fn sign_in_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let subtitle = match &self.options {
            None => "Enter the server that hosts your Matrix account.".to_owned(),
            Some(options) => format!("Choose how to sign in to {}.", options.homeserver),
        };
        let mut form = v_flex().w(px(360.)).gap(px(14.)).child(
            v_flex()
                .items_center()
                .gap(px(10.))
                .pb(px(8.))
                .child(
                    div()
                        .size(px(56.))
                        .rounded(px(14.))
                        .bg(cx.theme().primary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Icon::new(IconName::MessagesSquare)
                                .size(px(28.))
                                .text_color(cx.theme().primary_foreground),
                        ),
                )
                .child(
                    div()
                        .text_size(px(22.))
                        .font_weight(FontWeight::BOLD)
                        .child("Sign in"),
                )
                .child(div().text_color(muted).text_center().child(subtitle)),
        );
        match &self.options {
            None => {
                form = form
                    .child(
                        v_flex()
                            .gap(px(6.))
                            .child(self.field_label("Server"))
                            .child(Input::new(&self.homeserver).disabled(self.busy))
                            .child(div().text_size(px(11.5)).text_color(muted).child(
                                "The part after the colon in your Matrix ID, such as matrix.org.",
                            )),
                    )
                    .child(
                        Button::new("discover")
                            .primary()
                            .w_full()
                            .label("Continue")
                            .disabled(self.busy || self.backend.is_none())
                            .on_click(cx.listener(|this, _, window, cx| this.discover(window, cx))),
                    );
            }
            Some(options) => {
                form = form.child(
                    h_flex()
                        .h(px(32.))
                        .pl(px(10.))
                        .pr(px(4.))
                        .gap(px(8.))
                        .rounded(px(8.))
                        .bg(cx.theme().secondary)
                        .child(Icon::new(IconName::Lock).small().text_color(muted))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(options.homeserver.clone()),
                        )
                        .child(
                            Button::new("change-server")
                                .ghost()
                                .xsmall()
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
                if options.password {
                    let ready = !self.busy
                        && !self.username.read(cx).value().trim().is_empty()
                        && !self.password.read(cx).value().is_empty();
                    let sign_in = Button::new("sign-in")
                        .w_full()
                        .label("Sign in")
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, window, cx| this.sign_in(window, cx)));
                    form = form
                        .child(
                            v_flex()
                                .gap(px(6.))
                                .child(self.field_label("Username"))
                                .child(Input::new(&self.username).disabled(self.busy)),
                        )
                        .child(
                            v_flex()
                                .gap(px(6.))
                                .child(self.field_label("Password"))
                                .child(Input::new(&self.password).disabled(self.busy)),
                        )
                        .child(sign_in.primary());
                }
            }
        }
        let status = if self.busy && self.status.is_empty() {
            "Working…".to_owned()
        } else {
            self.status.clone()
        };
        if !status.is_empty() {
            form = form.child(
                div()
                    .text_size(px(12.))
                    .text_color(muted)
                    .text_center()
                    .child(status),
            );
        }
        form = form.child(
            div()
                .pt(px(8.))
                .text_size(px(11.5))
                .text_color(muted)
                .text_center()
                .child(format!(
                    "Your password is sent only to your server. Matrix Client keeps your session in the {}.",
                    secret_store_name()
                )),
        );
        v_flex()
            .id("sign-in")
            .size_full()
            .items_center()
            .pt(px(72.))
            .px_6()
            .overflow_y_scroll()
            .child(form)
            .into_any_element()
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
            None if selected => (cx.theme().primary, cx.theme().primary_foreground),
            None => (palette.pressed, cx.theme().foreground),
        };
        let element_id = SharedString::from(match &space {
            Some(id) => format!("space-{id}"),
            None => "home".to_owned(),
        });
        let radius = if selected { 12. } else { 20. };
        let tooltip = if unread > 0 {
            format!("{name} · {unread} unread")
        } else {
            name.clone()
        };
        h_flex()
            .relative()
            .w(px(RAIL_WIDTH))
            .justify_center()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .w(px(4.))
                    .h(px(if selected { 28. } else { 0. }))
                    .rounded_r(px(3.))
                    .bg(cx.theme().foreground),
            )
            .child(
                div()
                    .relative()
                    .size(px(40.))
                    .child(
                        Button::new(element_id)
                            .ghost()
                            .size(px(40.))
                            .p_0()
                            .rounded(px(radius))
                            .accessibility_label(name)
                            .tooltip(tooltip)
                            .child(
                                div()
                                    .size(px(40.))
                                    .rounded(px(radius))
                                    .bg(bg)
                                    .text_color(fg)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(content),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_space(space.clone(), window, cx)
                            })),
                    )
                    .when(unread > 0 && !selected, |tile| {
                        tile.child(
                            div()
                                .absolute()
                                .right(px(-5.))
                                .bottom(px(-5.))
                                .border_2()
                                .border_color(palette.rail)
                                .rounded(px(11.))
                                .child(badge(unread, cx)),
                        )
                    }),
            )
            .into_any_element()
    }

    fn rail_view(&self, cx: &mut Context<Self>) -> AnyElement {
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
            .filter(|room| !room.is_space && !in_any_space(&room.id))
            .fold(0u64, |sum, room| sum.saturating_add(room.unread));
        let mut rail = v_flex()
            .id("space-rail")
            .w(px(RAIL_WIDTH))
            .h_full()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .py(px(12.))
            .bg(palette.rail)
            .border_r_1()
            .border_color(cx.theme().border)
            .overflow_y_scroll();
        rail = rail.child(self.rail_tile(
            RailTile {
                space: None,
                name: "Home".into(),
                content: Icon::new(IconName::House).into_any_element(),
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
        if !self.snapshot.spaces.is_empty() {
            rail = rail.child(div().w(px(28.)).h(px(1.)).bg(cx.theme().border));
        }
        for space in self.snapshot.spaces.clone() {
            let unread = self
                .snapshot
                .rooms
                .iter()
                .filter(|room| !room.is_space && space.room_ids.contains(&room.id))
                .fold(0u64, |sum, room| sum.saturating_add(room.unread));
            let label = div()
                .text_size(px(13.))
                .font_weight(FontWeight::BOLD)
                .child(theme::initials(&space.name))
                .into_any_element();
            let name = if space.level > 0 {
                format!("{} (subspace)", space.name)
            } else {
                space.name.clone()
            };
            rail = rail.child(self.rail_tile(
                RailTile {
                    fill: Some(theme::avatar_color(&space.id)),
                    selected: selected.as_ref() == Some(&space.id),
                    space: Some(space.id.clone()),
                    name,
                    content: label,
                    unread,
                },
                cx,
            ));
        }
        rail.into_any_element()
    }

    fn room_row(&self, room: &Room, cx: &mut Context<Self>) -> AnyElement {
        let palette = theme::palette(cx);
        let selected = self.navigation.selected_room.as_ref() == Some(&room.id);
        let id = room.id.clone();
        let unread = room.unread > 0 && !selected;
        Button::new(SharedString::from(format!("room-{}", room.id)))
            .ghost()
            .w_full()
            .h(px(32.))
            .px(px(8.))
            .rounded(px(7.))
            .selected(selected)
            .accessibility_label(if room.unread > 0 {
                format!("{}, {} unread", room.name, room.unread)
            } else {
                room.name.clone()
            })
            .child(
                h_flex()
                    .w_full()
                    .gap(px(9.))
                    .child(room_avatar(room, 22., &palette))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_left()
                            .when(unread, |name| name.font_weight(FontWeight::SEMIBOLD))
                            .child(room.name.clone()),
                    )
                    .when(unread, |row| row.child(badge(room.unread, cx))),
            )
            .on_click(
                cx.listener(move |this, _, window, cx| this.select_room(id.clone(), window, cx)),
            )
            .into_any_element()
    }

    fn sidebar_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = theme::palette(cx);
        let muted = cx.theme().muted_foreground;
        let selected = self.navigation.selected_space.as_ref();
        let title = self
            .snapshot
            .spaces
            .iter()
            .find(|space| Some(&space.id) == selected)
            .map(|space| space.name.clone())
            .unwrap_or_else(|| "Home".into());
        let query = self.search.read(cx).value().to_lowercase();
        let visible: Vec<Room> = self
            .snapshot
            .rooms
            .iter()
            .filter(|room| {
                !room.is_space
                    && self.navigation.room_visible(&room.id, &self.snapshot)
                    && room.name.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        let mut list = v_flex()
            .id("room-list")
            .flex_1()
            .min_h_0()
            .px(px(8.))
            .pb(px(8.))
            .gap(px(14.))
            .overflow_y_scroll();
        for (heading, direct) in [("Rooms", false), ("Direct messages", true)] {
            let rooms: Vec<_> = visible
                .iter()
                .filter(|room| room.direct == direct)
                .collect();
            if rooms.is_empty() {
                continue;
            }
            let mut section = v_flex().gap(px(1.)).child(
                div()
                    .px(px(8.))
                    .pb(px(4.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(muted)
                    .child(heading),
            );
            for room in rooms {
                section = section.child(self.room_row(room, cx));
            }
            list = list.child(section);
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
        let user = self.snapshot.user_id.clone();
        let account = h_flex()
            .px(px(12.))
            .py(px(10.))
            .gap(px(9.))
            .border_t_1()
            .border_color(cx.theme().border)
            .child(avatar(
                &theme::initials(&user),
                theme::avatar_color(&user),
                28.,
                14.,
                &palette,
            ))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(user.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(self.snapshot.sync_status.clone()),
                    ),
            )
            .child(
                Button::new("my-profile")
                    .ghost()
                    .small()
                    .icon(IconName::UserRound)
                    .accessibility_label("My profile")
                    .tooltip("My profile")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_profile(user.clone(), "You".into(), window, cx)
                    })),
            )
            .child(
                Button::new("logout")
                    .ghost()
                    .small()
                    .icon(IconName::LogOut)
                    .accessibility_label("Sign out")
                    .tooltip("Sign out")
                    .disabled(self.preview || self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.logout(window, cx))),
            );
        v_flex()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex_none()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                v_flex()
                    .px(px(14.))
                    .pt(px(14.))
                    .pb(px(10.))
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::BOLD)
                            .truncate()
                            .child(title),
                    )
                    .child(
                        Input::new(&self.search)
                            .small()
                            .prefix(Icon::new(IconName::Search).small().text_color(muted)),
                    ),
            )
            .child(list)
            .child(account)
            .into_any_element()
    }

    fn message_body(
        &self,
        message: &crate::backend::Message,
        outgoing: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = theme::palette(cx);
        let bubble = match message.kind {
            MessageKind::Text
            | MessageKind::Emote
            | MessageKind::Notice
            | MessageKind::Image
            | MessageKind::File => Bubble::new()
                .max_w_full()
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
                .content(BubbleContent::new().child({
                    let mut content = v_flex().min_w_0().gap(px(5.));
                    if let Some(reply) = &message.reply {
                        let (sender, excerpt) = match reply {
                            ReplyTarget::Ready { sender, excerpt } => {
                                (sender.as_str(), excerpt.as_str())
                            }
                            ReplyTarget::Pending => ("Reply", "Loading original message…"),
                            ReplyTarget::Unavailable => ("Reply", "Original message unavailable"),
                        };
                        content = content.child(
                            v_flex()
                                .id(SharedString::from(format!("reply-{}", message.id)))
                                .border_l_2()
                                .border_color(if outgoing {
                                    palette.on_avatar
                                } else {
                                    cx.theme().primary
                                })
                                .pl(px(8.))
                                .text_size(px(11.))
                                .gap(px(2.))
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(sender.to_owned()),
                                )
                                .child(
                                    TextView::markdown(
                                        SharedString::from(format!("reply-text-{}", message.id)),
                                        model::plain_text(excerpt),
                                    )
                                    .selectable(true),
                                )
                                .test_support(),
                        );
                    }
                    content.child(
                        TextView::markdown(
                            SharedString::from(format!("message-{}", message.id)),
                            model::plain_text(&model::message_text(message)),
                        )
                        .selectable(true)
                        .when(
                            matches!(message.kind, MessageKind::Emote | MessageKind::Notice),
                            |text| text.italic(),
                        ),
                    )
                }))
                .into_any_element(),
            MessageKind::Undecryptable => h_flex()
                .gap(px(7.))
                .px(px(12.))
                .py(px(7.))
                .rounded(px(16.))
                .bg(palette.warning_background)
                .text_color(palette.warning)
                .child(Icon::new(IconName::Lock).small())
                .child("Unable to decrypt this message.")
                .into_any_element(),
            MessageKind::Redacted => div()
                .px(px(12.))
                .py(px(7.))
                .rounded(px(16.))
                .border_1()
                .border_color(cx.theme().border)
                .text_color(cx.theme().muted_foreground)
                .italic()
                .child("Message deleted")
                .into_any_element(),
        };
        let mut row = v_flex()
            .min_w_0()
            .max_w_full()
            .gap(px(4.))
            .when(outgoing, |row| row.items_end())
            .when(!outgoing, |row| row.items_start())
            .child(bubble);
        let pending = self.navigation.selected_room.as_ref().is_some_and(|room| {
            self.pending_actions
                .contains(&(room.clone(), message.id.clone()))
        });
        let disabled =
            self.preview || pending || self.backend.is_none() || !self.snapshot.timeline_ready;
        if !message.reactions.is_empty() {
            let mut chips = h_flex().gap(px(4.)).flex_wrap();
            for reaction in &message.reactions {
                let id = message.id.clone();
                let key = reaction.key.clone();
                let action = if reaction.mine { "Remove" } else { "Add" };
                chips = chips.child(
                    Button::new(SharedString::from(format!("reaction-{}-{}", id, key)))
                        .outline()
                        .xsmall()
                        .rounded(px(12.))
                        .label(format!("{} {}", key, reaction.count))
                        .selected(reaction.mine)
                        .toggled(reaction.mine)
                        .accessibility_label(format!(
                            "{action} {} reaction, {} people",
                            key, reaction.count
                        ))
                        .tooltip(format!("{action} {} reaction", key))
                        .disabled(disabled)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.timeline_action(
                                id.clone(),
                                TimelineAction::React {
                                    message: id.clone(),
                                    key: key.clone(),
                                },
                                window,
                                cx,
                            )
                        })),
                );
            }
            row = row.child(chips);
        }
        if message.thread_replies > 0 {
            row = row.child(
                div()
                    .id(SharedString::from(format!("thread-{}", message.id)))
                    .text_size(px(11.))
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{} {}",
                        message.thread_replies,
                        if message.thread_replies == 1 {
                            "reply"
                        } else {
                            "replies"
                        }
                    ))
                    .test_support(),
            );
        }
        if message.delivery == DeliveryState::Failed {
            let mut failure = h_flex()
                .gap(px(4.))
                .text_size(px(11.))
                .text_color(palette.error)
                .child("Send failed");
            if message.can_retry {
                for (label, retry) in [("Retry", true), ("Cancel", false)] {
                    let id = message.id.clone();
                    failure = failure.child(
                        Button::new(SharedString::from(format!(
                            "{}-{}",
                            label.to_lowercase(),
                            id
                        )))
                        .text()
                        .xsmall()
                        .label(label)
                        .tooltip(format!("{label} failed message"))
                        .disabled(disabled)
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                let action = if retry {
                                    TimelineAction::Retry {
                                        message: id.clone(),
                                    }
                                } else {
                                    TimelineAction::Cancel {
                                        message: id.clone(),
                                    }
                                };
                                this.timeline_action(id.clone(), action, window, cx);
                            },
                        )),
                    );
                }
            }
            row = row.child(failure);
        }
        if message.event_id.is_some()
            && self
                .snapshot
                .messages
                .iter()
                .rev()
                .find(|item| item.event_id.is_some())
                .map(|item| &item.id)
                == Some(&message.id)
            && let (Some(event_id), Some(room)) = (
                message.event_id.clone(),
                self.navigation.selected_room.clone(),
            )
        {
            let weak = cx.weak_entity();
            let account = self.snapshot.account_instance;
            row = row.relative().child(
                canvas(
                    move |bounds, window, cx| {
                        let visible = bounds.intersect(&window.content_mask().bounds);
                        let visible = visible.size.width > px(1.) && visible.size.height > px(1.);
                        window.defer(cx, move |window, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.report_latest_visibility(
                                    room, event_id, account, visible, window, cx,
                                )
                            });
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        }
        row.into_any_element()
    }

    fn message_group(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(range) = self.groups.get(ix).cloned() else {
            return div().into_any_element();
        };
        let palette = theme::palette(cx);
        let muted = cx.theme().muted_foreground;
        let first = self.snapshot.messages[range.start].clone();
        let outgoing = first.sender == self.snapshot.user_id;
        let new_day = ix == 0
            || self
                .groups
                .get(ix - 1)
                .map(|previous| self.snapshot.messages[previous.start].timestamp_ms)
                .map(day_of)
                != Some(day_of(first.timestamp_ms));
        let mut group = v_flex()
            .id(SharedString::from(format!("group-{}", first.id)))
            .w_full()
            .px(px(20.))
            .py(px(6.))
            .gap(px(10.));
        if new_day {
            group = group.child(
                h_flex()
                    .gap(px(10.))
                    .pt(px(6.))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(muted)
                    .child(div().flex_1().h(px(1.)).bg(cx.theme().border))
                    .child(day_label(first.timestamp_ms))
                    .child(div().flex_1().h(px(1.)).bg(cx.theme().border)),
            );
        }
        if first.new_messages {
            group = group.child(
                h_flex()
                    .id(SharedString::from(format!("new-messages-{}", first.id)))
                    .gap(px(10.))
                    .text_size(px(11.))
                    .text_color(cx.theme().primary)
                    .child(div().flex_1().h(px(1.)).bg(cx.theme().primary))
                    .child("New messages")
                    .child(div().flex_1().h(px(1.)).bg(cx.theme().primary))
                    .test_support(),
            );
        }
        let messages = &self.snapshot.messages[range];
        let last = messages.len().saturating_sub(1);
        if outgoing {
            let mut column = v_flex()
                .w_full()
                .min_w_0()
                .ml_auto()
                .max_w(px(BUBBLE_MAX_WIDTH))
                .items_end()
                .gap(px(3.))
                .child(
                    div()
                        .pr(px(4.))
                        .text_size(px(11.))
                        .text_color(muted)
                        .child(time_label(first.timestamp_ms)),
                );
            for (ix, message) in messages.iter().enumerate() {
                column = column.child(self.message_body(message, true, cx));
                let label = match message.delivery {
                    DeliveryState::Queued => "Sending…",
                    DeliveryState::Sent if ix == last => "Sent",
                    _ => "",
                };
                if !label.is_empty() {
                    column = column.child(
                        div()
                            .pr(px(4.))
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(label),
                    );
                }
            }
            return group
                .child(h_flex().w_full().child(column))
                .test_support()
                .into_any_element();
        }
        let id = first.sender.clone();
        let name = first.display_name.clone();
        let avatar_button = Button::new(SharedString::from(format!("avatar-{}", first.id)))
            .ghost()
            .size(px(30.))
            .p_0()
            .rounded(px(15.))
            .accessibility_label(format!("Open profile of {}", first.display_name))
            .tooltip(format!("Open profile of {}", first.display_name))
            .child(avatar(
                &theme::initials(&first.display_name),
                theme::avatar_color(&first.sender),
                30.,
                15.,
                &palette,
            ))
            .on_click(cx.listener({
                let id = id.clone();
                let name = name.clone();
                move |this, _, window, cx| this.open_profile(id.clone(), name.clone(), window, cx)
            }));
        let mut column = v_flex()
            .flex_1()
            .min_w_0()
            .max_w(px(BUBBLE_MAX_WIDTH))
            .gap(px(3.))
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(
                        Button::new(SharedString::from(format!("sender-{}", first.id)))
                            .text()
                            .xsmall()
                            .label(name.clone())
                            .tooltip(first.sender.clone())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_profile(id.clone(), name.clone(), window, cx)
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(time_label(first.timestamp_ms)),
                    ),
            );
        for message in messages {
            column = column.child(self.message_body(message, false, cx));
        }
        group
            .child(
                h_flex()
                    .w_full()
                    .items_start()
                    .gap(px(10.))
                    .child(avatar_button)
                    .child(column),
            )
            .test_support()
            .into_any_element()
    }

    fn conversation_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = theme::palette(cx);
        let muted = cx.theme().muted_foreground;
        let room = self
            .snapshot
            .rooms
            .iter()
            .find(|room| self.navigation.selected_room.as_ref() == Some(&room.id))
            .cloned();
        let Some(room) = room else {
            return v_flex()
                .flex_1()
                .h_full()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .bg(cx.theme().background)
                .child(
                    Icon::new(IconName::MessagesSquare)
                        .size(px(36.))
                        .text_color(muted),
                )
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::BOLD)
                        .child("Choose a conversation"),
                )
                .child(
                    div()
                        .text_color(muted)
                        .child("Select a room from the sidebar."),
                )
                .into_any_element();
        };
        let ready = self.snapshot.timeline_ready && !self.preview && !self.busy;
        let editable = !self.busy && (self.preview || self.snapshot.timeline_ready);
        let empty = self.composer.read(cx).value().trim().is_empty();
        let details_shown = self.details_open || self.profile.is_some();

        let mut title = h_flex().gap(px(6.)).child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::BOLD)
                .truncate()
                .child(room.name.clone()),
        );
        if room.encrypted {
            title = title.child(Icon::new(IconName::Lock).xsmall().text_color(muted));
        }
        let header = h_flex()
            .h(px(52.))
            .flex_none()
            .pl(px(16.))
            .pr(px(12.))
            .gap(px(10.))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(room_avatar(&room, 28., &palette))
            .child(v_flex().flex_1().min_w_0().child(title).when_some(
                room.topic.clone(),
                |column, topic| {
                    column.child(
                        div()
                            .text_size(px(11.5))
                            .text_color(muted)
                            .truncate()
                            .child(topic),
                    )
                },
            ))
            .child(
                Button::new("earlier")
                    .ghost()
                    .small()
                    .icon(IconName::ChevronsUp)
                    .accessibility_label("Load earlier messages")
                    .tooltip(if self.paginating {
                        "Loading earlier messages…"
                    } else {
                        "Load earlier messages"
                    })
                    .disabled(!ready || self.paginating || self.pagination_exhausted)
                    .on_click(cx.listener(|this, _, window, cx| this.load_earlier(window, cx))),
            )
            .child(
                Button::new("details")
                    .ghost()
                    .small()
                    .icon(IconName::PanelRight)
                    .accessibility_label("Room details")
                    .tooltip("Room details")
                    .selected(details_shown)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_details(cx))),
            );

        let body = if self.groups.is_empty() {
            v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .child(room_avatar(&room, 56., &palette))
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::BOLD)
                        .child(room.name.clone()),
                )
                .child(
                    div()
                        .text_color(muted)
                        .child(if self.snapshot.timeline_ready {
                            "No messages yet."
                        } else {
                            "Loading conversation…"
                        }),
                )
                .into_any_element()
        } else {
            let weak = cx.weak_entity();
            MessageScroller::new("messages", self.scroller.clone(), move |ix, _, cx| {
                weak.update(cx, |this, cx| this.message_group(ix, cx))
                    .unwrap_or_else(|_| div().into_any_element())
            })
            .flex_1()
            .min_h_0()
            .with_jump_button_label("Jump to latest")
            .into_any_element()
        };

        let notices: Vec<String> = [&self.status, &self.snapshot.timeline_status]
            .into_iter()
            .filter(|text| !text.is_empty())
            .cloned()
            .collect();
        let typing = model::typing_text(&self.snapshot);
        let composer = v_flex()
            .px(px(20.))
            .pb(px(14.))
            .pt(px(6.))
            .gap(px(4.))
            .when(!typing.is_empty(), |composer| {
                composer.child(
                    div()
                        .id("typing-notice")
                        .pl(px(14.))
                        .text_size(px(11.5))
                        .text_color(muted)
                        .child(typing)
                        .test_support(),
                )
            })
            .children(notices.into_iter().map(|text| {
                div()
                    .pl(px(14.))
                    .text_size(px(11.5))
                    .text_color(muted)
                    .child(text)
            }))
            .child(
                h_flex()
                    .items_end()
                    .gap(px(6.))
                    .py(px(5.))
                    .pl(px(12.))
                    .pr(px(5.))
                    .rounded(px(20.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(palette.field)
                    .child(
                        div()
                            .id("composer-panel")
                            .flex_1()
                            .min_w_0()
                            .py(px(2.))
                            .child(
                                Textarea::new(&self.composer)
                                    .accessibility_id("composer")
                                    .appearance(false)
                                    .w_full()
                                    .disabled(!editable)
                                    .aria_label("Message"),
                            ),
                    )
                    .child(
                        Button::new("send")
                            .primary()
                            .small()
                            .rounded(px(15.))
                            .icon(IconName::ArrowUp)
                            .accessibility_label("Send message")
                            .tooltip("Send message · Return")
                            .disabled(!ready || empty || self.sending.contains(&room.id))
                            .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
                    ),
            );

        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(cx.theme().background)
            .child(header)
            .child(body)
            .child(composer)
            .into_any_element()
    }

    fn panel(&self, cx: &App) -> Stateful<Div> {
        v_flex()
            .id("details-panel")
            .w(px(DETAILS_WIDTH))
            .h_full()
            .flex_none()
            .bg(cx.theme().background)
            .border_l_1()
            .border_color(cx.theme().border)
            .overflow_y_scroll()
    }

    fn panel_header(
        &self,
        title: &'static str,
        close: &'static str,
        cx: &mut Context<Self>,
    ) -> Div {
        h_flex()
            .h(px(52.))
            .flex_none()
            .px(px(16.))
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().font_weight(FontWeight::BOLD).child(title))
            .child(
                Button::new(close)
                    .ghost()
                    .small()
                    .icon(IconName::X)
                    .accessibility_label("Close panel")
                    .tooltip("Close · Escape")
                    .on_click(cx.listener(|this, _, _, cx| this.dismiss_panel(cx))),
            )
    }

    fn member_row(&self, member: &Member, cx: &Context<Self>) -> AnyElement {
        let id = member.user_id.clone();
        let name = member.display_name.clone();
        let role = match member.role {
            MemberRole::Admin => "Admin",
            MemberRole::Moderator => "Moderator",
            MemberRole::Member => "",
        };
        let labels = v_flex()
            .flex_1()
            .min_w_0()
            .text_left()
            .child(
                h_flex()
                    .gap(px(6.))
                    .child(div().truncate().child(name.clone()))
                    .when(!role.is_empty(), |row| {
                        row.child(
                            div()
                                .text_size(px(10.))
                                .text_color(cx.theme().muted_foreground)
                                .child(role),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(cx.theme().muted_foreground)
                    .truncate()
                    .child(id.clone()),
            );
        let content = h_flex()
            .w_full()
            .gap(px(8.))
            .child(avatar(
                &theme::initials(&name),
                theme::avatar_color(&id),
                26.,
                13.,
                &theme::palette(cx),
            ))
            .child(labels);
        Button::new(SharedString::from(format!("member-{id}")))
            .ghost()
            .w_full()
            .h(px(52.))
            .px(px(6.))
            .accessibility_label(format!("Open profile of {name} {role}"))
            .tooltip(format!("Open profile of {name}"))
            .child(content)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_profile(id.clone(), name.clone(), window, cx)
            }))
            .into_any_element()
    }

    fn details_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = theme::palette(cx);
        let muted = cx.theme().muted_foreground;
        let Some(room) = self
            .snapshot
            .rooms
            .iter()
            .find(|room| self.navigation.selected_room.as_ref() == Some(&room.id))
            .cloned()
        else {
            return div().into_any_element();
        };
        let encryption = if room.encrypted {
            h_flex()
                .gap(px(8.))
                .child(Icon::new(IconName::Lock).small())
                .child(div().flex_1().child("Encrypted"))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(muted)
                        .child("Enabled for this room"),
                )
        } else {
            h_flex()
                .gap(px(8.))
                .px(px(10.))
                .py(px(8.))
                .rounded(px(8.))
                .bg(palette.warning_background)
                .text_color(palette.warning)
                .text_size(px(12.))
                .child(Icon::new(IconName::TriangleAlert).small())
                .child("Not encrypted. Your server can read these messages.")
        };
        let id = room.id.clone();
        self.panel(cx)
            .child(self.panel_header("Details", "close-details", cx))
            .child(
                v_flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(16.))
                    .py(px(20.))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(room_avatar(&room, 64., &palette))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::BOLD)
                            .text_center()
                            .child(room.name.clone()),
                    )
                    .when_some(room.topic.clone(), |column, topic| {
                        column.child(
                            div()
                                .text_size(px(12.))
                                .text_color(muted)
                                .text_center()
                                .child(topic),
                        )
                    }),
            )
            .child(
                v_flex()
                    .px(px(12.))
                    .py(px(12.))
                    .gap(px(8.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Members ({})", self.snapshot.members.len())),
                    )
                    .child(match self.snapshot.members_state {
                        MembersState::Loading => div()
                            .text_color(muted)
                            .child("Loading members…")
                            .into_any_element(),
                        MembersState::Failed => div()
                            .text_color(muted)
                            .child("Members could not be loaded.")
                            .into_any_element(),
                        MembersState::Ready => uniform_list(
                            "members",
                            self.snapshot.members.len(),
                            cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .filter_map(|ix| this.snapshot.members.get(ix))
                                    .map(|member| this.member_row(member, cx))
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .h(px(260.))
                        .w_full()
                        .into_any_element(),
                    }),
            )
            .child(
                v_flex().p(px(16.)).gap(px(12.)).child(encryption).child(
                    Button::new("copy-room-id")
                        .ghost()
                        .small()
                        .icon(IconName::Copy)
                        .label("Copy room ID")
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(id.clone()))
                        }),
                ),
            )
            .into_any_element()
    }

    fn profile_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(page) = &self.profile else {
            return div().into_any_element();
        };
        let palette = theme::palette(cx);
        let muted = cx.theme().muted_foreground;
        let profile_id = page
            .profile
            .as_ref()
            .map(|profile| profile.user_id.clone())
            .unwrap_or_else(|| page.id.clone());
        let name = page
            .profile
            .as_ref()
            .map(|profile| profile.display_name.clone())
            .unwrap_or_else(|| page.name.clone());
        let mut body = v_flex()
            .items_center()
            .gap(px(6.))
            .px(px(16.))
            .py(px(20.))
            .child(avatar(
                &theme::initials(&name),
                theme::avatar_color(&profile_id),
                64.,
                32.,
                &palette,
            ))
            .child(
                div()
                    .text_size(px(17.))
                    .font_weight(FontWeight::BOLD)
                    .text_center()
                    .child(name.clone()),
            )
            .child(
                TextView::markdown("profile-user-id", model::plain_text(&profile_id))
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
            body = body.child(div().text_color(muted).child("Loading profile…"));
        }
        if let Some(info) = &page.profile {
            if let Some(room_name) = &info.room_display_name
                && room_name != &info.display_name
            {
                body = body.child(
                    div()
                        .text_color(muted)
                        .child(format!("Name in this room: {room_name}")),
                );
            }
            if !info.server_profile_available {
                body = body.child(
                    div()
                        .text_color(muted)
                        .child("Showing the saved conversation profile."),
                );
            }
        }
        if !page.error.is_empty() {
            let id = page.id.clone();
            let name = page.name.clone();
            body = body
                .child(div().text_color(palette.error).child(page.error.clone()))
                .child(
                    Button::new("retry-profile")
                        .small()
                        .label("Try again")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_profile(id.clone(), name.clone(), window, cx)
                        })),
                );
        }
        if self.preview {
            body = body.child(div().text_color(muted).child("Offline sample profile"));
        }
        self.panel(cx)
            .child(self.panel_header("Profile", "close-profile", cx))
            .child(body)
            .into_any_element()
    }
}

impl Render for MatrixApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = if self.snapshot.user_id.is_empty() {
            self.sign_in_view(cx)
        } else {
            let panel = if self.profile.is_some() {
                Some(self.profile_view(cx))
            } else if self.details_open && self.navigation.selected_room.is_some() {
                Some(self.details_view(cx))
            } else {
                None
            };
            h_flex()
                .size_full()
                .items_stretch()
                .child(self.rail_view(cx))
                .child(self.sidebar_view(cx))
                .child(self.conversation_view(cx))
                .children(panel)
                .into_any_element()
        };
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(px(13.))
            .track_focus(&self.focus)
            .key_context("MatrixClient")
            .on_action(cx.listener(|this, _: &FindConversation, window, cx| {
                this.search.update(cx, |state, cx| state.focus(window, cx))
            }))
            .on_action(cx.listener(|this, _: &CloseProfile, _, cx| this.dismiss_panel(cx)))
            .on_action(cx.listener(|this, _: &ToggleDetails, _, cx| this.toggle_details(cx)))
            .child(content)
    }
}
