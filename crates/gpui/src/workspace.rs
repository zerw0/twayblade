//! Signed-in workspace and sign-in window, laid out after the approved mockup:
//! space rail → conversations sidebar → conversation → optional details panel.
//! Controls whose feature does not exist yet are omitted rather than faked.
use crate::motion;
use crate::theme::scaled_px as px;

use crate::preferences::{ChatStyle, Density, Timestamps};
use chrono::{Local, TimeZone};
use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
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
    app::{
        CloseProfile, FindConversation, JoinSpace, MatrixApp, NewDirectMessage, NewRoom,
        ToggleDetails,
    },
    backend::{
        DeliveryState, Member, MemberRole, MembersState, MessageKind, ReplyTarget, Room,
        TimelineAction,
    },
    model,
    theme::{self, Palette},
};

const RAIL_WIDTH: f32 = 56.;
const SIDEBAR_WIDTH: f32 = 240.;
const DETAILS_WIDTH: f32 = 280.;
const BUBBLE_MAX_WIDTH: f32 = 520.;
const MEMBER_ROW_HEIGHT: f32 = 30.;
/// Unified toolbar height, matching a macOS window with a toolbar.
const TOOLBAR_HEIGHT: f32 = 52.;
/// Clearance for the traffic lights at the left of the macOS toolbar.
const TRAFFIC_LIGHTS_WIDTH: f32 = 78.;
/// A new time marker appears after this much silence within a day.
const TIME_GAP_MS: i64 = 30 * 60 * 1000;

/// One space-rail tile. `space: None` is Home.
struct RailTile {
    space: Option<String>,
    name: String,
    content: AnyElement,
    fill: Option<Hsla>,
    selected: bool,
    unread: u64,
}

pub(super) fn avatar(label: &str, fill: Hsla, size: f32, radius: f32, palette: &Palette) -> Div {
    div()
        .flex_none()
        .size(px(size))
        .rounded(px(radius))
        .bg(fill)
        .text_color(palette.on_avatar)
        .text_size(px((size * 0.4).max(10.)))
        .font_weight(FontWeight::SEMIBOLD)
        .flex()
        .items_center()
        .justify_center()
        .child(label.to_owned())
}

/// Unread count as a quiet grey pill, like Mail's sidebar counts.
fn badge(count: u64, cx: &App) -> Div {
    div()
        .flex_none()
        .min_w(px(18.))
        .h(px(16.))
        .px(px(5.))
        .rounded(px(8.))
        .bg(cx.theme().secondary)
        .text_color(cx.theme().muted_foreground)
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

    fn session_recovery_view(&self, cx: &mut Context<Self>) -> AnyElement {
        use twayblade_core::auth::SessionState;
        if self.snapshot.session_state == SessionState::SoftLoggedOut && self.options.is_some() {
            return self.sign_in_view(cx);
        }
        let soft = self.snapshot.session_state == SessionState::SoftLoggedOut;
        let mut form = v_flex().w(px(360.)).gap(px(16.)).child(div().text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child("You were signed out"))
            .child(div().text_color(cx.theme().muted_foreground).child(if soft { "Re-authenticate to continue with this device. Your local keys and history will be kept." } else if self.confirm_session_reset { "Delete this device’s local keys and cached history? Recovery or another verified device may be needed to read encrypted messages after signing in again." } else { "This session was revoked. Sign in again to continue." }));
        if soft {
            form =
                form.child(
                    Button::new("reauthenticate")
                        .primary()
                        .label("Re-authenticate")
                        .disabled(self.busy || self.preview)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.prepare_reauthentication(window, cx)
                        })),
                );
        } else if self.confirm_session_reset {
            form = form
                .child(
                    Button::new("confirm-session-reset")
                        .label("Delete local data and continue")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.discard_revoked_session(window, cx)
                        })),
                )
                .child(
                    Button::new("cancel-session-reset")
                        .label("Cancel")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_session_reset = false;
                            cx.notify();
                        })),
                );
        } else {
            form = form.child(
                Button::new("sign-in-again")
                    .primary()
                    .label("Sign in again")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.confirm_session_reset = true;
                        cx.notify();
                    })),
            );
        }
        if self.preview {
            form = form.child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child("Sample data"),
            );
        }
        if !self.status.is_empty() {
            form = form.child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.status.clone()),
            );
        }
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .child(form)
            .into_any_element()
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
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(
                            if self.snapshot.session_state
                                == twayblade_core::auth::SessionState::SoftLoggedOut
                            {
                                "You were signed out"
                            } else {
                                "Sign in"
                            },
                        ),
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
                                .disabled(
                                    self.busy
                                        || self.snapshot.session_state
                                            != twayblade_core::auth::SessionState::Active,
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.options = None;
                                    this.password
                                        .update(cx, |state, cx| state.set_value("", window, cx));
                                    cx.notify();
                                })),
                        ),
                );
                if options.browser && self.oauth_cancel.is_none() {
                    form = form.child(
                        Button::new("browser-sign-in")
                            .primary()
                            .w_full()
                            .label("Continue in browser")
                            .disabled(self.busy || self.preview)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.browser_sign_in(window, cx)),
                            ),
                    );
                }
                if options.sso && self.oauth_cancel.is_none() {
                    if options.providers.is_empty() {
                        form = form.child(
                            Button::new("sso-sign-in")
                                .w_full()
                                .label("Continue with SSO")
                                .disabled(self.busy || self.preview)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.sso_sign_in(None, window, cx)
                                })),
                        );
                    }
                    for provider in &options.providers {
                        let id = provider.id.clone();
                        let mut button =
                            Button::new(SharedString::from(format!("sso-{}", provider.id)))
                                .w_full()
                                .label(format!("Continue with {}", provider.name))
                                .disabled(self.busy || self.preview)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.sso_sign_in(Some(id.clone()), window, cx)
                                }));
                        if let Some(icon) = provider
                            .icon_png
                            .as_ref()
                            .and_then(|bytes| crate::attachments::render_small_png(bytes, cx))
                        {
                            button = button.child(img(icon).size(px(20.)));
                        }
                        form = form.child(button);
                    }
                }
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
                                .child(Input::new(&self.username).disabled(
                                    self.busy
                                        || self.snapshot.session_state
                                            != twayblade_core::auth::SessionState::Active,
                                )),
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
        if self.oauth_cancel.is_some() {
            form = form.child(
                Button::new("cancel-browser-sign-in")
                    .w_full()
                    .label("Cancel")
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_browser_sign_in(cx))),
            );
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
                    "Twayblade keeps your session securely in the {}.",
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

    /// A toolbar row. On macOS it is also the window's title bar: dragging moves
    /// the window and double-clicking zooms it, as in native apps.
    pub(super) fn toolbar(&self, id: &'static str, cx: &mut Context<Self>) -> Stateful<Div> {
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
                                        crate::spaces_ui::ManageKind::Directory(browse.clone()),
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
                                        crate::spaces_ui::ManageKind::Leave(leave.clone()),
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
                    this.open_management(crate::spaces_ui::ManageKind::JoinSpace, window, cx)
                })),
        );
        rail.into_any_element()
    }

    fn room_row(&self, room: &Room, cx: &mut Context<Self>) -> AnyElement {
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
    fn sidebar_section(
        &self,
        key: &'static str,
        label: &'static str,
        rooms: Vec<&Room>,
        add: Option<(&'static str, crate::spaces_ui::ManageKind)>,
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
    fn own_menu(&self, user: String, cx: &mut Context<Self>) -> AnyElement {
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
                                this.open_settings(crate::settings::Section::Account, cx)
                            });
                        }),
                    )
            })
            .into_any_element()
    }

    pub(super) fn with_presence(
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

    fn search_messages_row(&self, query: &str, cx: &mut Context<Self>) -> AnyElement {
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

    fn sidebar_view(&self, cx: &mut Context<Self>) -> AnyElement {
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
                Some(("Create Room…", crate::spaces_ui::ManageKind::Create)),
                cx,
            ));
        }
        if !direct.is_empty() {
            list = list.child(self.sidebar_section(
                "direct",
                "Direct messages",
                direct,
                Some(("New Direct Message…", crate::spaces_ui::ManageKind::Dm)),
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
                                    crate::spaces_ui::ManageKind::Create,
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
                                this.open_management(crate::spaces_ui::ManageKind::Dm, window, cx)
                            })),
                    ),
            )
            .child(list)
            .into_any_element()
    }

    /// The window title bar: navigation on the left, search in the middle,
    /// status and account on the right. On macOS it is also the drag region.
    fn window_titlebar(&self, cx: &mut Context<Self>) -> AnyElement {
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
                                this.open_settings(crate::settings::Section::Notifications, cx)
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
                                this.open_settings(crate::settings::Section::Appearance, cx)
                            })),
                    )
                    .child(self.own_menu(user.clone(), cx)),
            )
            .into_any_element()
    }

    pub(super) fn message_body(
        &self,
        message: &crate::backend::Message,
        outgoing: bool,
        in_thread: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = theme::palette(cx);
        let bubbles = self.preferences.chat.style == ChatStyle::Bubbles;
        let outgoing = outgoing && bubbles;
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
                .with_variant(if !bubbles {
                    BubbleVariant::Ghost
                } else if outgoing {
                    BubbleVariant::Filled
                } else {
                    BubbleVariant::Secondary
                })
                .content(BubbleContent::new().child({
                    let mut content = v_flex().min_w_0().gap(px(3.));
                    if let Some(reply) = &message.reply {
                        let (sender, excerpt) = match reply {
                            ReplyTarget::Ready { sender, excerpt } => {
                                (sender.as_str(), excerpt.as_str())
                            }
                            ReplyTarget::Pending => ("", "Loading the original message…"),
                            // Usually history from before you joined; no fake sender name.
                            ReplyTarget::Unavailable => ("", "Replying to an earlier message"),
                        };
                        // Line styles quote on one line: "↩ Sender  excerpt…".
                        let one_line = self.preferences.chat.style != ChatStyle::Bubbles;
                        content = content.child(
                            div()
                                .id(SharedString::from(format!("reply-{}", message.id)))
                                .flex()
                                .when(one_line, |quote| {
                                    quote.flex_row().items_center().gap(px(6.))
                                })
                                .when(!one_line, |quote| quote.flex_col())
                                .min_w_0()
                                .when(one_line, |quote| {
                                    quote.child(
                                        Icon::new(IconName::CornerUpLeft)
                                            .size(px(12.))
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                })
                                .when(!one_line, |quote| {
                                    quote
                                        .border_l_2()
                                        .border_color(if outgoing {
                                            cx.theme().primary_foreground.opacity(0.6)
                                        } else {
                                            cx.theme().primary
                                        })
                                        .pl(px(7.))
                                })
                                .text_size(px(11.5))
                                .opacity(0.85)
                                .when(!sender.is_empty(), |quote| {
                                    quote.child(
                                        div()
                                            .flex_none()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .truncate()
                                            .child(sender.to_owned()),
                                    )
                                })
                                .child(
                                    div()
                                        .min_w_0()
                                        .when(one_line, |line| line.flex_1())
                                        .truncate()
                                        .when(sender.is_empty(), |line| line.italic())
                                        .child(excerpt.to_owned()),
                                )
                                .test_support(),
                        );
                    }
                    let message_font = theme::message_font(cx);
                    content = content.child(
                        div()
                            .font_family(message_font)
                            .text_size(px(f32::from(self.preferences.appearance.message_size)))
                            .child(
                                TextView::markdown(
                                    SharedString::from(format!("message-{}", message.id)),
                                    match &message.rich {
                                        Some(rich) => model::rich_markdown(rich),
                                        _ => model::plain_text(&model::message_text(message)),
                                    },
                                )
                                .selectable(true)
                                // Show the real destination; a label can disguise it.
                                .link_tooltip(|url, window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(url.clone())
                                        .build(window, cx)
                                })
                                .on_link_click(|url, _, _, cx| {
                                    if url::Url::parse(url).is_ok_and(|url| {
                                        matches!(url.scheme(), "https" | "http" | "mailto")
                                    }) {
                                        cx.open_url(url);
                                    }
                                })
                                .when(
                                    matches!(
                                        message.kind,
                                        MessageKind::Emote | MessageKind::Notice
                                    ),
                                    |text| text.italic(),
                                ),
                            ),
                    );
                    if message.edited {
                        content = content.child(
                            div()
                                .id(SharedString::from(format!("edited-{}", message.id)))
                                .text_size(px(10.5))
                                .child("Edited")
                                .test_support(),
                        );
                    }
                    if let Some(attachment) = &message.attachment {
                        let thumbnail = attachment
                            .preview_png
                            .as_ref()
                            .and_then(|bytes| crate::attachments::render_png(bytes, cx));
                        if message.kind == MessageKind::Image {
                            let id = message.id.clone();
                            let keyboard_id = id.clone();
                            let focus = self.image_focus(&id, cx);
                            let mut button = Button::new(SharedString::from(format!("image-{id}")))
                                .ghost()
                                .text_color(if outgoing {
                                    cx.theme().primary_foreground
                                } else {
                                    cx.theme().foreground
                                })
                                .p_0()
                                .track_focus(&focus)
                                .accessibility_label(format!("Open image {}", attachment.filename))
                                .tooltip("Open Image · Space")
                                .disabled(message.event_id.is_none())
                                .capture_key_down(cx.listener(
                                    move |this, event: &KeyDownEvent, window, cx| {
                                        if event.keystroke.key == "space"
                                            && !event.keystroke.modifiers.modified()
                                        {
                                            this.open_viewer(
                                                keyboard_id.clone(),
                                                in_thread,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }
                                    },
                                ))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_viewer(id.clone(), in_thread, window, cx)
                                }));
                            if let Some(image) = thumbnail {
                                button = button
                                    .w(px(280.))
                                    .h(px(180.))
                                    .child(img(image).size_full().object_fit(ObjectFit::Contain));
                            } else {
                                button = button.small().label("Open Image");
                            }
                            if self.reveal_image.as_ref() == Some(&message.id) {
                                let target = message.id.clone();
                                let weak = cx.weak_entity();
                                button = button.relative().child(
                                    canvas(
                                        move |bounds, window, cx| {
                                            window.request_autoscroll(bounds);
                                            window.defer(cx, move |_, cx| {
                                                let _ = weak.update(cx, |this, cx| {
                                                    if this.reveal_image.as_ref() == Some(&target) {
                                                        this.reveal_image = None;
                                                        cx.notify();
                                                    }
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
                            content = content.child(button);
                        }
                        let on_bubble = if outgoing {
                            cx.theme().primary_foreground
                        } else {
                            cx.theme().foreground
                        };
                        let id = message.id.clone();
                        let filename = attachment.filename.clone();
                        let open_id = id.clone();
                        let open_name = filename.clone();
                        let cancel_id = id.clone();
                        if message.delivery == DeliveryState::Queued {
                            content =
                                content.child(
                                    h_flex()
                                        .gap(px(8.))
                                        .child(
                                            message
                                                .upload_percent
                                                .map(|p| format!("Uploading · {p}%"))
                                                .unwrap_or_else(|| "Waiting to upload…".into()),
                                        )
                                        .child(
                                            Button::new(SharedString::from(format!(
                                                "cancel-upload-{id}"
                                            )))
                                            .small()
                                            .label("Cancel upload")
                                            .disabled(self.preview)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.cancel_upload(cancel_id.clone(), window, cx)
                                            })),
                                        ),
                                );
                        } else if message.delivery != DeliveryState::Failed {
                            content = content.child(
                                h_flex()
                                    .gap(px(2.))
                                    .child(
                                        Button::new(SharedString::from(format!("save-{id}")))
                                            .ghost()
                                            .size(px(24.))
                                            .w(px(28.))
                                            .h(px(28.))
                                            .p_0()
                                            .text_color(on_bubble)
                                            .icon(IconName::Download)
                                            .accessibility_label("Save attachment")
                                            .tooltip("Save…")
                                            .disabled(self.preview)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.save_attachment(
                                                    id.clone(),
                                                    filename.clone(),
                                                    false,
                                                    window,
                                                    cx,
                                                )
                                            })),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!("open-{open_id}")))
                                            .ghost()
                                            .size(px(24.))
                                            .w(px(28.))
                                            .h(px(28.))
                                            .p_0()
                                            .text_color(on_bubble)
                                            .icon(IconName::ExternalLink)
                                            .accessibility_label("Open attachment")
                                            .tooltip("Open…")
                                            .disabled(self.preview)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.save_attachment(
                                                    open_id.clone(),
                                                    open_name.clone(),
                                                    true,
                                                    window,
                                                    cx,
                                                )
                                            })),
                                    ),
                            );
                        }
                    }
                    content
                }))
                .into_any_element(),
            MessageKind::Undecryptable => h_flex()
                .gap(px(7.))
                .px(px(12.))
                .py(px(7.))
                .when(bubbles, |view| {
                    view.rounded(px(16.)).bg(palette.warning_background)
                })
                .text_color(palette.warning)
                .child(Icon::new(IconName::Lock).small())
                .child("Unable to decrypt this message.")
                .into_any_element(),
            MessageKind::Redacted => div()
                .px(px(12.))
                .py(px(7.))
                .when(bubbles, |view| {
                    view.rounded(px(16.))
                        .border_1()
                        .border_color(cx.theme().border)
                })
                .text_color(cx.theme().muted_foreground)
                .italic()
                .child("Message deleted")
                .into_any_element(),
        };
        let mut row = v_flex()
            .id(SharedString::from(format!("message-row-{}", message.id)))
            .min_w_0()
            .max_w_full()
            .gap(px(3.))
            .when(outgoing, |row| row.items_end())
            .when(!outgoing, |row| row.items_start())
            // The message a search result opened stays tinted until another is opened.
            .when(
                message.event_id.is_some() && message.event_id == self.revealed_event,
                |row| row.rounded(px(8.)).bg(palette.selection),
            );
        let pending = self.navigation.selected_room.as_ref().is_some_and(|room| {
            self.pending_actions
                .contains(&(room.clone(), message.id.clone()))
        });
        let disabled =
            self.preview || pending || self.backend.is_none() || !self.snapshot.timeline_ready;
        // Message actions sit beside the bubble and appear on hover, so the
        // timeline is not doubled in height by a control under every message.
        let mut line = h_flex()
            .group("message")
            .max_w_full()
            .min_w_0()
            .items_center()
            .gap(px(4.))
            .when(outgoing, |line| line.flex_row_reverse());
        line = line.child(div().min_w_0().child(bubble));
        // React and reply-in-thread share one small bar that appears on hover.
        let mut actions = h_flex()
            .flex_none()
            .p(px(2.))
            .gap(px(1.))
            .rounded(px(7.))
            .border_1()
            .border_color(gpui_kit::transparent_black())
            .group_hover("message", |style| {
                style
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
            });
        if message.event_id.is_some() || self.preview {
            let message_id = message.id.clone();
            let focus = self.image_focus(&format!("reaction-action-{}", message.id), cx);
            actions = actions.child(
                div()
                    .flex_none()
                    .track_focus(&focus)
                    .focus(|style| style.opacity(1.))
                    .opacity(0.)
                    .group_hover("message", |style| style.opacity(1.))
                    .child(
                        Button::new(SharedString::from(format!("add-reaction-{}", message.id)))
                            .track_focus(&focus)
                            .ghost()
                            .xsmall()
                            .icon(IconName::FaceSlightlySmiling)
                            .accessibility_label("Add reaction")
                            .tooltip("Add reaction")
                            .disabled(!self.preview && disabled)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_emoji(Some(message_id.clone()), window, cx)
                            })),
                    ),
            );
        }
        if !in_thread && message.event_id.is_some() {
            let id = message.id.clone();
            let focus = self.image_focus(&format!("thread-action-{id}"), cx);
            actions = actions.child(
                div()
                    .track_focus(&focus)
                    .focus(|style| style.opacity(1.))
                    .opacity(0.)
                    .group_hover("message", |style| style.opacity(1.))
                    .child(
                        Button::new(SharedString::from(format!("reply-thread-{id}")))
                            .track_focus(&focus)
                            .ghost()
                            .xsmall()
                            .icon(IconName::MessagesSquare)
                            .accessibility_label("Reply in Thread")
                            .tooltip("Reply in Thread")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_thread(id.clone(), window, cx)
                            })),
                    ),
            );
        }
        line = line.child(actions);
        if self.preferences.chat.style != ChatStyle::Irc {
            line = line.child(
                div()
                    .flex_none()
                    .text_size(px(10.5))
                    .text_color(cx.theme().muted_foreground)
                    .when(
                        self.preferences.chat.timestamps == Timestamps::OnHover,
                        |view| {
                            view.opacity(0.)
                                .group_hover("message", |style| style.opacity(1.))
                        },
                    )
                    .child(time_label(message.timestamp_ms)),
            );
        }
        row = row.child(line);
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
                        .h(px(20.))
                        .px(px(6.))
                        .rounded(px(10.))
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
        if !in_thread && message.thread_replies > 0 {
            let id = message.id.clone();
            row = row.child(
                Button::new(SharedString::from(format!("thread-{}", message.id)))
                    .text()
                    .p_0()
                    .h_auto()
                    .child(
                        h_flex()
                            .gap(px(5.))
                            .pl(px(4.))
                            .text_size(px(11.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().link)
                            .child(Icon::new(IconName::MessageCircle).size(px(12.)))
                            .child(format!(
                                "{} {}",
                                message.thread_replies,
                                if message.thread_replies == 1 {
                                    "reply"
                                } else {
                                    "replies"
                                }
                            ))
                            .when(message.thread_unread > 0, |line| {
                                line.child(
                                    div()
                                        .font_weight(FontWeight::NORMAL)
                                        .text_color(cx.theme().muted_foreground)
                                        .child(format!("{} unread", message.thread_unread)),
                                )
                            }),
                    )
                    .accessibility_label("Open thread")
                    .tooltip("Open thread")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_thread(id.clone(), window, cx)
                    })),
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
                                this.message_action(in_thread, id.clone(), action, window, cx);
                            },
                        )),
                    );
                }
            }
            row = row.child(failure);
        }
        let messages = if in_thread {
            self.thread
                .as_ref()
                .map(|t| t.messages.as_slice())
                .unwrap_or(&[])
        } else {
            &self.snapshot.messages
        };
        // The main timeline's latest event is precomputed in `apply`; threads are small.
        let latest = if in_thread {
            messages
                .iter()
                .rev()
                .find(|m| m.event_id.is_some())
                .map(|m| m.id.clone())
        } else {
            self.latest_event()
        };
        if message.event_id.is_some()
            && latest.as_deref() == Some(message.id.as_str())
            && let (Some(event_id), Some(room)) = (
                message.event_id.clone(),
                self.navigation.selected_room.clone(),
            )
        {
            let weak = cx.weak_entity();
            let account = self.snapshot.account_instance;
            let request = self.thread.as_ref().map(|t| t.request);
            row = row.relative().child(
                canvas(
                    move |bounds, window, cx| {
                        let visible = bounds.intersect(&window.content_mask().bounds);
                        let visible = visible.size.width > px(1.) && visible.size.height > px(1.);
                        window.defer(cx, move |window, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                if in_thread {
                                    this.thread_visibility(
                                        request, event_id, account, visible, window, cx,
                                    );
                                } else {
                                    this.report_latest_visibility(
                                        room, event_id, account, visible, window, cx,
                                    );
                                }
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
        row.test_support().into_any_element()
    }

    fn message_group(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        self.message_group_from(ix, &self.snapshot.messages, &self.groups, false, cx)
    }

    pub(super) fn message_group_from(
        &self,
        ix: usize,
        messages: &[crate::backend::Message],
        groups: &[std::ops::Range<usize>],
        in_thread: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(range) = groups.get(ix).cloned() else {
            return div().into_any_element();
        };
        let palette = theme::palette(cx);
        let muted = cx.theme().muted_foreground;
        let first = messages[range.start].clone();
        let outgoing = first.sender == self.snapshot.user_id;
        let previous = ix
            .checked_sub(1)
            .and_then(|previous| groups.get(previous))
            .map(|previous| &messages[previous.end - 1]);
        let new_day = previous
            .is_none_or(|previous| day_of(previous.timestamp_ms) != day_of(first.timestamp_ms));
        // Messages-style time markers: one centred label for a new day or after a
        // long silence, instead of a timestamp row above every group.
        let day_marker = new_day.then(|| day_label(first.timestamp_ms));
        let marker = if new_day {
            None
        } else {
            previous
                .filter(|previous| first.timestamp_ms - previous.timestamp_ms >= TIME_GAP_MS)
                .map(|_| time_label(first.timestamp_ms))
        };
        let mut group = v_flex()
            .id(SharedString::from(format!("group-{}", first.id)))
            .w_full()
            .px(px(16.))
            .pt(px(if previous.is_none() {
                10.
            } else if self.preferences.chat.density == Density::Compact {
                2.
            } else {
                6.
            }))
            .gap(px(2.));
        if let Some(day) = day_marker {
            // A new day is a ruled divider; a long pause is just a centred time.
            group = group.child(
                h_flex()
                    .w_full()
                    .py(px(8.))
                    .gap(px(10.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(muted)
                    .child(div().flex_1().h(px(1.)).bg(cx.theme().border))
                    .child(day)
                    .child(div().flex_1().h(px(1.)).bg(cx.theme().border)),
            );
        }
        if let Some(marker) = marker {
            group = group.child(
                div()
                    .w_full()
                    .py(px(8.))
                    .text_center()
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(muted)
                    .child(marker),
            );
        }
        if first.new_messages {
            group = group.child(
                h_flex()
                    .id(SharedString::from(format!("new-messages-{}", first.id)))
                    .py(px(6.))
                    .gap(px(8.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.error)
                    .child(div().flex_1().h(px(1.)).bg(palette.error.opacity(0.55)))
                    .child("New messages")
                    .child(div().flex_1().h(px(1.)).bg(palette.error.opacity(0.55)))
                    .test_support(),
            );
        }
        if self.preferences.chat.style != ChatStyle::Bubbles {
            let style = self.preferences.chat.style;
            let compact = self.preferences.chat.density == Density::Compact;
            let latest_own_id = if in_thread {
                messages
                    .iter()
                    .rev()
                    .find(|message| message.sender == self.snapshot.user_id)
                    .map(|message| message.id.clone())
            } else {
                self.latest_own()
            };
            let latest_own = latest_own_id.as_ref();
            let delivered = |message: &crate::backend::Message| {
                (latest_own == Some(&message.id) && message.delivery == DeliveryState::Sent)
                    .then_some(if message.read_by.is_empty() {
                        "Sent"
                    } else {
                        "Read"
                    })
            };
            if style == ChatStyle::Minimal {
                // Slack-style: avatar gutter, name and time on each new group (new
                // sender or a pause), then the group's lines in one text column.
                let id = first.sender.clone();
                let name = first.display_name.clone();
                let header = h_flex()
                    .gap(px(8.))
                    .items_baseline()
                    .child(
                        Button::new(SharedString::from(format!("sender-{}", first.id)))
                            .text()
                            .p_0()
                            .h_auto()
                            .accessibility_label(name.clone())
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().foreground)
                                    .child(name.clone()),
                            )
                            .tooltip(id.clone())
                            .on_click(cx.listener({
                                let id = id.clone();
                                let name = name.clone();
                                move |this, _, window, cx| {
                                    this.open_profile(id.clone(), name.clone(), window, cx)
                                }
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(time_label(first.timestamp_ms)),
                    );
                let mut column = v_flex().flex_1().min_w_0().child(header);
                for message in &messages[range] {
                    column = column.child(
                        div()
                            .id(SharedString::from(format!("style-row-{}", message.id)))
                            .w_full()
                            .min_w_0()
                            .py(px(if compact { 0. } else { 1. }))
                            .child(self.message_body(message, false, in_thread, cx))
                            .test_support(),
                    );
                    if let Some(label) = delivered(message) {
                        column =
                            column.child(div().text_size(px(10.5)).text_color(muted).child(label));
                    }
                }
                let mut row = h_flex().w_full().items_start().gap(px(10.));
                if self.preferences.chat.avatars {
                    row = row.child(
                        Button::new(SharedString::from(format!("avatar-{}", first.id)))
                            .ghost()
                            .size(px(32.))
                            .p_0()
                            .rounded(px(16.))
                            .accessibility_label(format!("Open profile of {name}"))
                            .tooltip(format!("Open profile of {name}"))
                            .child(self.avatar_image(
                                first.sender_avatar.as_deref(),
                                &first.sender,
                                &name,
                                32.,
                                16.,
                                cx,
                            ))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_profile(id.clone(), name.clone(), window, cx)
                            })),
                    );
                }
                return group
                    .child(row.child(column))
                    .test_support()
                    .into_any_element();
            }
            // IRC: one line per message. Sender at body size in its own colour; the
            // time column exists only when timestamps are always shown.
            for message in &messages[range] {
                let id = message.sender.clone();
                let name = message.display_name.clone();
                let mut line = h_flex()
                    .id(SharedString::from(format!("style-row-{}", message.id)))
                    .w_full()
                    .min_w_0()
                    .gap(px(10.))
                    .items_start()
                    .py(px(if compact { 0. } else { 2. }))
                    .rounded(px(4.))
                    .hover(|style| style.bg(cx.theme().secondary.opacity(0.4)));
                // IRC always shows the time: it fills the gutter and aids scanning.
                {
                    line = line.child(
                        div()
                            .w(px(38.))
                            .flex_none()
                            .pt(px(1.))
                            .text_color(muted)
                            .text_size(px(11.))
                            .child(time_label(message.timestamp_ms)),
                    );
                }
                line = line
                    .child(
                        div().w(px(120.)).flex_none().flex().justify_end().child(
                            Button::new(SharedString::from(format!("sender-{}", message.id)))
                                .text()
                                .p_0()
                                .h_auto()
                                .max_w(px(120.))
                                .accessibility_label(name.clone())
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme::avatar_text_color(&id, cx))
                                        .child(name.clone()),
                                )
                                .tooltip(format!("{id} · {}", time_label(message.timestamp_ms)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_profile(id.clone(), name.clone(), window, cx)
                                })),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.message_body(message, false, in_thread, cx)),
                    );
                group = group.child(line.test_support());
                if let Some(label) = delivered(message) {
                    group = group.child(
                        div()
                            .pl(px(178.))
                            .text_size(px(10.5))
                            .text_color(muted)
                            .child(label),
                    );
                }
            }
            return group.test_support().into_any_element();
        }
        let latest_own = if in_thread {
            messages
                .iter()
                .rev()
                .find(|message| message.sender == self.snapshot.user_id)
                .map(|message| message.id.clone())
        } else {
            self.latest_own()
        };
        let messages = &messages[range];
        if outgoing {
            let mut column = v_flex()
                .w_full()
                .min_w_0()
                .ml_auto()
                .max_w(px(BUBBLE_MAX_WIDTH))
                .items_end()
                .gap(px(2.));
            for message in messages {
                column = column.child(self.message_body(message, true, in_thread, cx));
                let label = match message.delivery {
                    DeliveryState::Queued => "Sending…",
                    _ if latest_own.as_ref() == Some(&message.id)
                        && !message.read_by.is_empty() =>
                    {
                        "Read"
                    }
                    DeliveryState::Sent if latest_own.as_ref() == Some(&message.id) => "Sent",
                    _ => "",
                };
                if !label.is_empty() {
                    column = column.child(
                        h_flex()
                            .pr(px(6.))
                            .gap(px(3.))
                            .text_size(px(10.5))
                            .text_color(muted)
                            .when(label == "Read", |line| {
                                line.child(Icon::new(IconName::CheckCheck).size(px(12.)))
                            })
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
            .size(px(26.))
            .p_0()
            .rounded(px(13.))
            .accessibility_label(format!("Open profile of {}", first.display_name))
            .tooltip(format!("Open profile of {}", first.display_name))
            .child(self.avatar_image(
                first.sender_avatar.as_deref(),
                &first.sender,
                &first.display_name,
                26.,
                13.,
                cx,
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
            .gap(px(2.))
            .child(
                h_flex().pl(px(10.)).gap(px(6.)).items_baseline().child(
                    Button::new(SharedString::from(format!("sender-{}", first.id)))
                        .text()
                        .p_0()
                        .h_auto()
                        .accessibility_label(name.clone())
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(cx.theme().foreground)
                                .child(name.clone()),
                        )
                        .tooltip(first.sender.clone())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_profile(id.clone(), name.clone(), window, cx)
                        })),
                ),
            );
        for message in messages {
            column = column.child(self.message_body(message, false, in_thread, cx));
        }
        group
            .child(
                h_flex()
                    .w_full()
                    .items_end()
                    .gap(px(8.))
                    .when(self.preferences.chat.avatars, |row| {
                        row.child(avatar_button)
                    })
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
                        .font_weight(FontWeight::SEMIBOLD)
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
        let details_shown = self.details_open || self.profile.is_some() || self.thread.is_some();

        let room_icon = if room.direct || room.avatar.is_some() {
            self.avatar_image(
                room.avatar.as_deref(),
                &room.id,
                &room.name,
                18.,
                if room.direct { 9. } else { 5. },
                cx,
            )
            .into_any_element()
        } else {
            Icon::new(if room.public {
                IconName::Hash
            } else {
                IconName::Lock
            })
            .size(px(14.))
            .text_color(muted)
            .into_any_element()
        };
        let favourite_room = room.id.clone();
        let favourite = room.favourite;
        let members = self.snapshot.members.len();
        let count_button = |id: &'static str,
                            icon: IconName,
                            count: usize,
                            label: String,
                            tab: crate::app::InfoTab| {
            Button::new(id)
                .ghost()
                .h(px(24.))
                .px(px(6.))
                .accessibility_label(label.clone())
                .tooltip(label)
                .child(
                    h_flex()
                        .gap(px(4.))
                        .text_size(px(12.))
                        .text_color(muted)
                        .child(Icon::new(icon).size(px(13.)))
                        .child(count.to_string()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.info_tab = tab;
                    this.close_profile(cx);
                    this.details_open = true;
                    cx.notify();
                }))
        };
        let header = h_flex()
            .h(px(48.))
            .flex_none()
            .pl(px(16.))
            .pr(px(10.))
            .gap(px(6.))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(room_icon)
            .child(
                div()
                    .flex_none()
                    .max_w(px(260.))
                    .truncate()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(room.name.clone()),
            )
            .child(
                Button::new("favourite-room")
                    .ghost()
                    .size(px(24.))
                    .icon(
                        Icon::new(IconName::Star)
                            .size(px(14.))
                            .when(favourite, |icon| icon.text_color(palette.star)),
                    )
                    .accessibility_label(if favourite {
                        "Remove from Favourites"
                    } else {
                        "Add to Favourites"
                    })
                    .tooltip(if favourite {
                        "Remove from Favourites"
                    } else {
                        "Add to Favourites"
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_favourite(favourite_room.clone(), !favourite, window, cx)
                    })),
            )
            .child(div().w(px(1.)).h(px(16.)).mx(px(4.)).bg(cx.theme().border))
            .when(members > 0, |header| {
                header.child(count_button(
                    "header-members",
                    IconName::Users,
                    members,
                    format!("{members} members"),
                    crate::app::InfoTab::Members,
                ))
            })
            .when(room.pinned > 0, |header| {
                header.child(count_button(
                    "header-pinned",
                    IconName::Pin,
                    room.pinned,
                    format!("{} pinned messages", room.pinned),
                    crate::app::InfoTab::About,
                ))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .pl(px(4.))
                    .truncate()
                    .text_size(px(12.))
                    .text_color(muted)
                    .children(room.topic.clone()),
            )
            .when(room.encrypted, |header| {
                header.child(
                    div()
                        .id("header-encrypted")
                        .child(Icon::new(IconName::Lock).size(px(13.)).text_color(muted))
                        .tooltip(|window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new("End-to-end encrypted")
                                .build(window, cx)
                        }),
                )
            })
            .child(
                Button::new("details")
                    .ghost()
                    .size(px(28.))
                    .icon(IconName::PanelRight)
                    .accessibility_label("Room info")
                    .tooltip("Room Info  ⌥⌘I")
                    .selected(details_shown)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_details(cx))),
            );

        let body = if self.groups.is_empty() {
            v_flex()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap(px(4.))
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
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
            // Kit defaults to 32px between rows; groups own their 6px spacing.
            .with_row_style(StyleRefinement::default().px_0().pb_0())
            .into_any_element()
        };

        let notices: Vec<String> = [&self.status, &self.snapshot.timeline_status]
            .into_iter()
            .filter(|text| !text.is_empty())
            .cloned()
            .collect();
        let typing = model::typing_text(&self.snapshot);
        let paste_target = cx.weak_entity();
        let note = |text: String| {
            div()
                .pl(px(12.))
                .text_size(px(13.))
                .text_color(muted)
                .child(text)
        };
        let composer = v_flex()
            .px(px(20.))
            .pt(px(2.))
            .pb(px(6.))
            .gap(px(4.))
            .children(notices.into_iter().map(note))
            .when(self.emoji_picker.is_some(), |view| {
                view.child(motion::rise(
                    div().child(self.emoji_panel(cx)),
                    "emoji-picker-motion",
                    self.snapshot.reduce_motion,
                ))
            })
            .when(
                self.attachment_draft.is_some() || self.attachment_sample,
                |view| view.child(self.attachment_confirmation(cx)),
            )
            .when(self.attachment_busy, |view| {
                view.child(note("Preparing attachment…".into()))
            })
            // A composer box: the message field, then a quiet toolbar row with
            // attach, emoji and send, as in Slack and Mail.
            .child(
                v_flex()
                    .rounded(px(9.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(palette.field)
                    .child(
                        div()
                            .id("composer-panel")
                            .min_w_0()
                            .px(px(12.))
                            .pt(px(8.))
                            .pb(px(2.))
                            .child(
                                Textarea::new(&self.composer)
                                    .accessibility_id("composer")
                                    .appearance(false)
                                    .w_full()
                                    .disabled(!editable)
                                    .aria_label("Message")
                                    .on_paste(move |clipboard, window, cx| {
                                        paste_target
                                            .update(cx, |this, cx| {
                                                this.paste_attachment(clipboard, window, cx)
                                            })
                                            .unwrap_or(false)
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .h(px(34.))
                            .px(px(6.))
                            .gap(px(2.))
                            .child(div().flex_1())
                            .child(
                                Button::new("attach-file")
                                    .ghost()
                                    .size(px(26.))
                                    .icon(IconName::Paperclip)
                                    .accessibility_label("Attach file")
                                    .tooltip("Attach File…")
                                    .disabled(!editable || self.preview || self.attachment_busy)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.choose_attachment(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("emoji-open")
                                    .ghost()
                                    .size(px(26.))
                                    .icon(IconName::FaceSlightlySmiling)
                                    .accessibility_label("Choose emoji")
                                    .tooltip("Emoji")
                                    .disabled(!editable && !self.preview)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_emoji(None, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("send")
                                    .primary()
                                    .h(px(26.))
                                    .w(px(30.))
                                    .ml(px(4.))
                                    .rounded(px(6.))
                                    .icon(IconName::Send)
                                    .accessibility_label("Send message")
                                    .tooltip("Send  ↩")
                                    .disabled(!ready || empty || self.sending.contains(&room.id))
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.send(window, cx)),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(18.))
                    .pl(px(4.))
                    .text_size(px(11.))
                    .text_color(muted)
                    .when(!typing.is_empty(), |line| {
                        // Who is typing (avatars), a pulse, then the accessible text.
                        let reduce = self.snapshot.reduce_motion;
                        let typers: Vec<_> = self.snapshot.typing.iter().take(3).cloned().collect();
                        line.child(motion::rise(
                            h_flex()
                                .gap(px(6.))
                                .child(h_flex().children(typers.iter().enumerate().map(
                                    |(index, user)| {
                                        div()
                                            .when(index > 0, |avatar| avatar.ml(px(-4.)))
                                            .rounded_full()
                                            .border_1()
                                            .border_color(cx.theme().background)
                                            .child(self.avatar_image(
                                                user.avatar.as_deref(),
                                                &user.user_id,
                                                &user.name,
                                                14.,
                                                7.,
                                                cx,
                                            ))
                                    },
                                )))
                                .child(motion::typing_dots(muted, reduce))
                                .child(
                                    div()
                                        .id("typing-notice")
                                        .child(typing.clone())
                                        .test_support(),
                                ),
                            "typing-row",
                            reduce,
                        ))
                    }),
            );

        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(cx.theme().background)
            .child(header)
            .when(self.show_security_notice(), |view| {
                view.child(self.security_notice(cx))
            })
            .child(body)
            .child(composer)
            .on_drop::<ExternalPaths>(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                if let Some(path) = paths.0.first() {
                    this.prepare_attachment(
                        crate::attachments::AttachmentSource::File(path.clone()),
                        window,
                        cx,
                    );
                }
            }))
            .into_any_element()
    }

    fn panel(&self, cx: &App) -> Stateful<Div> {
        v_flex()
            .id("details-panel")
            .w(px(DETAILS_WIDTH))
            .max_w_full()
            .h_full()
            .flex_none()
            .bg(cx.theme().background)
            .border_l_1()
            .border_color(cx.theme().border)
            .overflow_y_scroll()
    }

    pub(super) fn panel_header(
        &self,
        title: &'static str,
        close: &'static str,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.toolbar("panel-toolbar", cx)
            .px(px(14.))
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
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
        // One line per member, like Contacts; the Matrix ID is in the tooltip.
        Button::new(SharedString::from(format!("member-{id}")))
            .ghost()
            .w_full()
            .h(px(MEMBER_ROW_HEIGHT))
            .px(px(6.))
            .rounded(px(6.))
            .accessibility_label(format!("Open profile of {name} {role}"))
            .tooltip(id.clone())
            .child(
                h_flex()
                    .w_full()
                    .gap(px(8.))
                    .child(
                        self.with_presence(
                            self.avatar_image(member.avatar.as_deref(), &id, &name, 22., 11., cx)
                                .into_any_element(),
                            member.presence.as_ref(),
                            22.,
                            cx,
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_left()
                            .text_size(px(13.))
                            .child(name.clone()),
                    )
                    .when(!role.is_empty(), |row| {
                        row.child(
                            div()
                                .text_size(px(11.))
                                .text_color(cx.theme().muted_foreground)
                                .child(role),
                        )
                    }),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_profile(id.clone(), name.clone(), window, cx)
            }))
            .into_any_element()
    }

    fn section_label(&self, label: String, cx: &App) -> Div {
        div()
            .px(px(6.))
            .pb(px(4.))
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().muted_foreground)
            .child(label)
    }

    /// Message search results, in the trailing panel.
    fn search_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let palette = theme::palette(cx);
        let search = &self.snapshot.search;
        let in_room = !self.search_all_rooms;
        let room_name = self
            .snapshot
            .rooms
            .iter()
            .find(|room| room.id == self.snapshot.selected_room)
            .map(|room| room.name.clone());
        let scope = h_flex()
            .p(px(2.))
            .gap(px(2.))
            .rounded(px(7.))
            .bg(cx.theme().secondary)
            .children(
                [(true, "All Rooms"), (false, "This Room")].map(|(all, label)| {
                    let selected = self.search_all_rooms == all;
                    Button::new(SharedString::from(format!(
                        "search-scope-{}",
                        if all { "all" } else { "room" }
                    )))
                    .ghost()
                    .xsmall()
                    .flex_1()
                    .h(px(22.))
                    .rounded(px(5.))
                    .label(label)
                    .disabled(!all && room_name.is_none())
                    .selected(selected)
                    .when(selected, |button| button.bg(palette.pill).shadow_xs())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.search_all_rooms = all;
                        this.run_message_search(window, cx);
                    }))
                }),
            );
        let files = Button::new("search-has-file")
            .ghost()
            .xsmall()
            .icon(IconName::Paperclip)
            .label("Has Files")
            .selected(self.search_has_file)
            .on_click(cx.listener(|this, _, window, cx| {
                this.search_has_file = !this.search_has_file;
                this.run_message_search(window, cx);
            }));
        let mut list = v_flex().px(px(8.)).py(px(6.)).gap(px(2.));
        if self.preview {
            list = list.child(
                div()
                    .p(px(8.))
                    .text_color(muted)
                    .child("Search needs a connected account."),
            );
        }
        if search.limited {
            list =
                list.child(
                    h_flex()
                        .id("search-limited")
                        .items_start()
                        .gap(px(8.))
                        .mx(px(4.))
                        .mb(px(4.))
                        .px(px(8.))
                        .py(px(6.))
                        .rounded(px(6.))
                        .bg(cx.theme().secondary)
                        .text_size(px(11.5))
                        .text_color(muted)
                        .child(
                            Icon::new(IconName::Lock)
                                .size(px(12.))
                                .flex_none()
                                .mt(px(2.)),
                        )
                        .child(
                            div().flex_1().min_w_0().whitespace_normal().child(
                                "Encrypted rooms only include messages already on this device.",
                            ),
                        )
                        .test_support(),
                );
        }
        for result in &search.results {
            let room = self
                .snapshot
                .rooms
                .iter()
                .find(|room| room.id == result.room_id);
            let sender_name = if result.sender_name == result.sender {
                // Cached encrypted results carry only the ID; prefer a known name.
                self.snapshot
                    .members
                    .iter()
                    .find(|member| member.user_id == result.sender)
                    .map(|member| member.display_name.clone())
                    .unwrap_or_else(|| result.sender.clone())
            } else {
                result.sender_name.clone()
            };
            let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = result
                .match_ranges
                .iter()
                .filter(|range| result.snippet.get((*range).clone()).is_some())
                .map(|range| {
                    (
                        range.clone(),
                        HighlightStyle {
                            font_weight: Some(FontWeight::SEMIBOLD),
                            background_color: Some(palette.selection),
                            ..Default::default()
                        },
                    )
                })
                .collect();
            let room_id = result.room_id.clone();
            let event_id = result.event_id.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!(
                        "search-result-{}",
                        result.event_id
                    )))
                    .w_full()
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|style| style.bg(cx.theme().secondary.opacity(0.6)))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .items_start()
                            .child(self.avatar_image(
                                self.member_avatar(&result.sender),
                                &result.sender,
                                &sender_name,
                                22.,
                                11.,
                                cx,
                            ))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap(px(2.))
                                    .child(
                                        h_flex()
                                            .gap(px(6.))
                                            .text_size(px(11.5))
                                            .child(
                                                div()
                                                    .truncate()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(sender_name),
                                            )
                                            .when(!in_room, |line| {
                                                line.children(room.map(|room| {
                                                    div().truncate().text_color(muted).child(
                                                        if room.direct {
                                                            room.name.clone()
                                                        } else {
                                                            format!("# {}", room.name)
                                                        },
                                                    )
                                                }))
                                            })
                                            .child(div().flex_1())
                                            .child(div().flex_none().text_color(muted).child(
                                                format!(
                                                    "{} {}",
                                                    day_label(result.timestamp_ms),
                                                    time_label(result.timestamp_ms)
                                                ),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .whitespace_normal()
                                            .line_clamp(3)
                                            .child(
                                                StyledText::new(result.snippet.clone())
                                                    .with_highlights(highlights),
                                            ),
                                    ),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_event(room_id.clone(), event_id.clone(), window, cx)
                    })),
            );
        }
        if search.loading {
            list = list.children((0..4).map(|_| {
                div()
                    .mx(px(8.))
                    .my(px(4.))
                    .h(px(44.))
                    .rounded(px(6.))
                    .bg(cx.theme().secondary.opacity(0.6))
            }));
        } else if search.error.is_some() {
            list = list.child(
                v_flex()
                    .p(px(8.))
                    .gap(px(6.))
                    .text_color(muted)
                    .child("Search failed. Check your connection and try again.")
                    .child(
                        Button::new("search-retry")
                            .small()
                            .label("Try Again")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run_message_search(window, cx)
                            })),
                    ),
            );
        } else if search.query.is_some() && search.results.is_empty() && search.exhausted {
            list = list.child(
                div()
                    .p(px(8.))
                    .text_color(muted)
                    .child("No messages match. Try other words or search all rooms."),
            );
        } else if search.query.is_some() && !search.exhausted {
            list = list.child(
                Button::new("search-more")
                    .ghost()
                    .small()
                    .w_full()
                    .label("Load More Results")
                    .on_click(cx.listener(|this, _, window, cx| this.search_more(window, cx))),
            );
        }
        self.panel(cx)
            .child(self.panel_header("Search", "close-search", cx))
            .child(
                v_flex()
                    .px(px(12.))
                    .pt(px(10.))
                    .pb(px(6.))
                    .gap(px(8.))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(scope)
                    .child(h_flex().child(files)),
            )
            .child(
                div()
                    .id("search-results")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(list),
            )
            .into_any_element()
    }

    fn details_view(&self, cx: &mut Context<Self>) -> AnyElement {
        use crate::app::InfoTab;
        let palette = theme::palette(cx);
        let Some(room) = self
            .snapshot
            .rooms
            .iter()
            .find(|room| self.navigation.selected_room.as_ref() == Some(&room.id))
            .cloned()
        else {
            return div().into_any_element();
        };
        let tab = self.info_tab;
        let tabs = h_flex()
            .p(px(2.))
            .gap(px(2.))
            .rounded(px(7.))
            .bg(cx.theme().secondary)
            .children(
                [
                    (InfoTab::About, "About"),
                    (InfoTab::Members, "Members"),
                    (InfoTab::Files, "Files"),
                ]
                .map(|(value, label)| {
                    Button::new(SharedString::from(format!("info-tab-{label}")))
                        .ghost()
                        .xsmall()
                        .flex_1()
                        .h(px(22.))
                        .rounded(px(5.))
                        .label(label)
                        .selected(value == tab)
                        .when(value == tab, |button| button.bg(palette.pill).shadow_xs())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.info_tab = value;
                            cx.notify();
                        }))
                }),
            );
        // The conversation header already names the room; the panel stays "Info".
        let header = self.panel_header("Info", "close-details", cx);
        let body = match tab {
            InfoTab::About => self.info_about(&room, cx),
            InfoTab::Members => self.info_members(cx),
            InfoTab::Files => self.info_files(cx),
        };
        self.panel(cx)
            .child(header)
            .child(div().px(px(12.)).pt(px(10.)).child(tabs))
            .child(body)
            .into_any_element()
    }

    fn info_about(&self, room: &Room, cx: &mut Context<Self>) -> AnyElement {
        use twayblade_core::notifications::{RoomMode, RuleChange};
        let palette = theme::palette(cx);
        let muted = cx.theme().muted_foreground;
        let mode = self.snapshot.notifications.room_mode(&room.id);
        let muted_room = mode == RoomMode::Mute;
        let tile = |id: &'static str, icon: IconName, label: &'static str, active: bool| {
            Button::new(id)
                .ghost()
                .h(px(54.))
                .flex_1()
                .rounded(px(8.))
                .bg(cx.theme().secondary)
                .accessibility_label(label)
                .child(
                    v_flex()
                        .items_center()
                        .gap(px(4.))
                        .text_size(px(11.))
                        .child(Icon::new(icon).size(px(15.)).text_color(if active {
                            cx.theme().link
                        } else {
                            muted
                        }))
                        .child(label),
                )
        };
        let favourite_room = room.id.clone();
        let favourite = room.favourite;
        let mute_room = room.id.clone();
        let link_room = room.id.clone();
        let copy_id = room.id.clone();
        let files = self
            .snapshot
            .messages
            .iter()
            .filter(|message| message.attachment.is_some())
            .count();
        let info_row = |label: &'static str, value: String| {
            h_flex()
                .h(px(32.))
                .px(px(8.))
                .gap(px(10.))
                .child(div().flex_1().child(label))
                .child(div().text_size(px(12.)).text_color(muted).child(value))
        };
        v_flex()
            .id("info-about")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                v_flex()
                    .px(px(16.))
                    .pt(px(14.))
                    .pb(px(12.))
                    .gap(px(4.))
                    .child(match room.topic.clone() {
                        Some(topic) => div().whitespace_normal().child(topic),
                        None => div()
                            .text_color(muted)
                            .child("No topic. Room admins can add one in room settings."),
                    }),
            )
            .child(
                h_flex()
                    .px(px(12.))
                    .pb(px(14.))
                    .gap(px(6.))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        tile(
                            "info-favourite",
                            IconName::Star,
                            if favourite { "Favourited" } else { "Favourite" },
                            favourite,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.set_favourite(favourite_room.clone(), !favourite, window, cx)
                            },
                        )),
                    )
                    .child(
                        tile(
                            "info-mute",
                            if muted_room {
                                IconName::BellOff
                            } else {
                                IconName::Bell
                            },
                            if muted_room { "Unmute" } else { "Mute" },
                            muted_room,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.change_notification_rule(
                                    RuleChange::Room(
                                        mute_room.clone(),
                                        if muted_room {
                                            RoomMode::Default
                                        } else {
                                            RoomMode::Mute
                                        },
                                    ),
                                    window,
                                    cx,
                                )
                            },
                        )),
                    )
                    .child(
                        tile("info-copy-link", IconName::Link, "Copy link", false).on_click(
                            move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(format!(
                                    "https://matrix.to/#/{link_room}"
                                )))
                            },
                        ),
                    ),
            )
            .child(
                v_flex()
                    .px(px(8.))
                    .py(px(6.))
                    .child(info_row(
                        "Encryption",
                        if room.encrypted { "On" } else { "Off" }.into(),
                    ))
                    .when(!room.encrypted, |rows| {
                        rows.child(
                            h_flex()
                                .id("encryption-warning")
                                .items_start()
                                .gap(px(8.))
                                .mx(px(8.))
                                .my(px(4.))
                                .px(px(8.))
                                .py(px(6.))
                                .rounded(px(6.))
                                .bg(palette.warning_background)
                                .text_color(palette.warning)
                                .text_size(px(12.))
                                .child(Icon::new(IconName::TriangleAlert).xsmall().flex_none())
                                .child(
                                    div()
                                        .id("encryption-warning-text")
                                        .flex_1()
                                        .min_w_0()
                                        .whitespace_normal()
                                        .child(
                                            "Not encrypted. Your server can read these messages.",
                                        )
                                        .test_support(),
                                )
                                .test_support(),
                        )
                    })
                    .child(info_row("Pinned messages", room.pinned.to_string()))
                    .child(info_row("Files", files.to_string()))
                    .child(
                        div()
                            .px(px(-2.))
                            .child(self.room_notification_controls(&room.id, cx)),
                    ),
            )
            .child(
                h_flex()
                    .mt_auto()
                    .px(px(16.))
                    .py(px(12.))
                    .gap(px(6.))
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(room.id.clone()),
                    )
                    .child(
                        Button::new("copy-room-id")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Copy)
                            .accessibility_label("Copy room ID")
                            .tooltip("Copy Room ID")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy_id.clone()))
                            }),
                    ),
            )
            .into_any_element()
    }

    fn info_members(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let body = match self.snapshot.members_state {
            MembersState::Loading => div()
                .p(px(14.))
                .text_color(muted)
                .child("Loading members…")
                .into_any_element(),
            MembersState::Failed => div()
                .p(px(14.))
                .text_color(muted)
                .child("Members could not be loaded.")
                .into_any_element(),
            MembersState::Ready => {
                let members = self.snapshot.members.len();
                uniform_list(
                    "members",
                    members,
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .filter_map(|ix| this.snapshot.members.get(ix))
                            .map(|member| this.member_row(member, cx))
                            .collect::<Vec<_>>()
                    }),
                )
                .flex_1()
                .w_full()
                .into_any_element()
            }
        };
        v_flex()
            .flex_1()
            .min_h_0()
            .px(px(8.))
            .pt(px(10.))
            .child(self.section_label(format!("Members · {}", self.snapshot.members.len()), cx))
            .child(body)
            .into_any_element()
    }

    /// Info › Files: the room's paginated gallery (Images grid, Files list).
    /// Offline previews have no server, so they list the loaded timeline.
    fn info_files(&self, cx: &mut Context<Self>) -> AnyElement {
        use twayblade_core::media::MediaKind;
        if self.preview || self.backend.is_none() {
            return self.info_files_loaded(cx);
        }
        let palette = theme::palette(cx);
        let muted = cx.theme().muted_foreground;
        let media = &self.snapshot.media;
        let current = media.room == self.snapshot.selected_room;
        let kind = media.kind;
        let kinds = h_flex().px(px(12.)).pt(px(10.)).gap(px(6.)).children(
            [(MediaKind::Images, "Images"), (MediaKind::Files, "Files")].map(|(value, label)| {
                Button::new(SharedString::from(format!("gallery-kind-{label}")))
                    .ghost()
                    .xsmall()
                    .rounded(px(6.))
                    .label(label)
                    .selected(value == kind)
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.load_media(value, window, cx)),
                    )
            }),
        );
        let items: &[twayblade_core::media::MediaItem] = if current { &media.items } else { &[] };
        let mut body = v_flex().px(px(12.)).py(px(10.)).gap(px(8.));
        if kind == MediaKind::Images {
            // Thumbnails come from already-loaded timeline previews; others show
            // a plain tile until gallery thumbnails are fetched.
            let mut grid = h_flex().flex_wrap().gap(px(4.));
            for item in items {
                let preview = self
                    .snapshot
                    .messages
                    .iter()
                    .find(|message| message.event_id.as_ref() == Some(&item.event_id))
                    .and_then(|message| message.attachment.as_ref())
                    .and_then(|attachment| attachment.preview_png.as_ref())
                    .and_then(|png| crate::attachments::render_png(png, cx));
                let event = item.event_id.clone();
                grid = grid.child(
                    div()
                        .id(SharedString::from(format!(
                            "gallery-image-{}",
                            item.event_id
                        )))
                        .size(px(84.))
                        .rounded(px(6.))
                        .overflow_hidden()
                        .bg(cx.theme().secondary)
                        .border_1()
                        .border_color(cx.theme().border)
                        .cursor_pointer()
                        .hover(|style| style.border_color(cx.theme().muted_foreground))
                        .tooltip({
                            let label = format!(
                                "{} · {} · {}",
                                item.filename,
                                item.sender_name,
                                day_label(item.timestamp_ms)
                            );
                            move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(label.clone())
                                    .build(window, cx)
                            }
                        })
                        .child(match preview {
                            Some(image) => img(image)
                                .size_full()
                                .object_fit(ObjectFit::Cover)
                                .into_any_element(),
                            None => v_flex()
                                .size_full()
                                .items_center()
                                .justify_center()
                                .gap(px(4.))
                                .p(px(6.))
                                .text_color(muted)
                                .child(Icon::new(IconName::Image).size(px(18.)))
                                .child(
                                    div()
                                        .w_full()
                                        .truncate()
                                        .text_center()
                                        .text_size(px(10.))
                                        .child(item.filename.clone()),
                                )
                                .into_any_element(),
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_media_image(&event, window, cx)
                        })),
                );
            }
            body = body.child(grid);
        } else {
            for item in items {
                let extension = item
                    .filename
                    .rsplit('.')
                    .next()
                    .filter(|ext| ext.len() <= 4 && *ext != item.filename)
                    .unwrap_or("FILE")
                    .to_uppercase();
                let event = item.event_id.clone();
                let message = self
                    .snapshot
                    .messages
                    .iter()
                    .find(|message| message.event_id.as_ref() == Some(&item.event_id))
                    .map(|message| message.id.clone());
                let name = item.filename.clone();
                body = body.child(
                    Button::new(SharedString::from(format!("gallery-file-{event}")))
                        .ghost()
                        .w_full()
                        .h(px(44.))
                        .px(px(6.))
                        .accessibility_label(format!(
                            "{}, from {}",
                            item.filename, item.sender_name
                        ))
                        .tooltip(if message.is_some() {
                            "Save…"
                        } else {
                            "Show in conversation"
                        })
                        .child(
                            h_flex()
                                .w_full()
                                .gap(px(10.))
                                .child(
                                    div()
                                        .size(px(30.))
                                        .flex_none()
                                        .rounded(px(6.))
                                        .bg(theme::avatar_color(&item.filename))
                                        .text_color(palette.on_avatar)
                                        .text_size(px(9.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(extension),
                                )
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .truncate()
                                                .text_left()
                                                .text_size(px(12.5))
                                                .child(item.filename.clone()),
                                        )
                                        .child(
                                            div()
                                                .truncate()
                                                .text_left()
                                                .text_size(px(11.))
                                                .text_color(muted)
                                                .child(format!(
                                                    "{} · {} · {}",
                                                    item.sender_name,
                                                    day_label(item.timestamp_ms),
                                                    model::size_label(item.size)
                                                )),
                                        ),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| match &message {
                            // Loaded: save directly. Older: open it in context to save there.
                            Some(id) => {
                                this.save_attachment(id.clone(), name.clone(), false, window, cx)
                            }
                            None => {
                                let room = this.snapshot.selected_room.clone();
                                this.open_event(room, event.clone(), window, cx)
                            }
                        })),
                );
            }
        }
        if current && media.loading {
            body = body.child(h_flex().gap(px(4.)).flex_wrap().children((0..6).map(|_| {
                div()
                    .when(kind == MediaKind::Images, |tile| tile.size(px(84.)))
                    .when(kind == MediaKind::Files, |row| row.w_full().h(px(36.)))
                    .rounded(px(6.))
                    .bg(cx.theme().secondary.opacity(0.6))
            })));
        } else if current && media.error.is_some() {
            body = body.child(
                v_flex()
                    .gap(px(6.))
                    .text_color(muted)
                    .child("The room's files could not be loaded. Check your connection.")
                    .child(
                        Button::new("gallery-retry")
                            .small()
                            .label("Try Again")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.load_media(kind, window, cx)
                            })),
                    ),
            );
        } else if current && items.is_empty() && media.exhausted {
            body = body.child(div().text_color(muted).child(if kind == MediaKind::Images {
                "No images have been shared in this room."
            } else {
                "No files have been shared in this room."
            }));
        } else if current && !media.exhausted {
            body = body.child(
                Button::new("gallery-more")
                    .ghost()
                    .small()
                    .w_full()
                    .label("Load Older")
                    .on_click(cx.listener(|this, _, window, cx| this.load_more_media(window, cx))),
            );
        }
        v_flex()
            .flex_1()
            .min_h_0()
            .child(kinds)
            .child(
                div()
                    .id("info-gallery")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
            .into_any_element()
    }

    fn info_files_loaded(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let files: Vec<_> = self
            .snapshot
            .messages
            .iter()
            .rev()
            .filter_map(|message| message.attachment.as_ref().map(|file| (message, file)))
            .collect();
        let mut list = v_flex()
            .id("info-files")
            .flex_1()
            .min_h_0()
            .px(px(8.))
            .py(px(10.))
            .gap(px(1.))
            .overflow_y_scroll();
        if files.is_empty() {
            list = list.child(div().p(px(6.)).text_color(muted).whitespace_normal().child(
                "No files in the loaded history. Scroll up in the conversation to load more.",
            ));
        }
        for (message, file) in files {
            let extension = file
                .filename
                .rsplit('.')
                .next()
                .filter(|ext| ext.len() <= 4 && *ext != file.filename)
                .unwrap_or("FILE")
                .to_uppercase();
            let id = message.id.clone();
            let name = file.filename.clone();
            list = list.child(
                Button::new(SharedString::from(format!("info-file-{}", message.id)))
                    .ghost()
                    .w_full()
                    .h(px(44.))
                    .px(px(8.))
                    .accessibility_label(format!("Save {}", file.filename))
                    .tooltip("Save…")
                    .child(
                        h_flex()
                            .w_full()
                            .gap(px(10.))
                            .child(
                                div()
                                    .size(px(30.))
                                    .flex_none()
                                    .rounded(px(6.))
                                    .bg(theme::avatar_color(&file.filename))
                                    .text_color(theme::palette(cx).on_avatar)
                                    .text_size(px(9.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(extension),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .truncate()
                                            .text_left()
                                            .text_size(px(12.5))
                                            .child(file.filename.clone()),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_left()
                                            .text_size(px(11.))
                                            .text_color(muted)
                                            .child(format!(
                                                "{} · {}",
                                                message.display_name,
                                                time_label(message.timestamp_ms)
                                            )),
                                    ),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.save_attachment(id.clone(), name.clone(), false, window, cx)
                    })),
            );
        }
        list.into_any_element()
    }

    /// Message, Verify and Ignore for anyone; Mute, Kick and Ban when your power
    /// level allows. Kick, Ban and Ignore need a second click to confirm.
    fn member_actions(&self, user: &str, cx: &mut Context<Self>) -> AnyElement {
        use twayblade_core::room_state::MemberAction;
        let member = self
            .snapshot
            .members
            .iter()
            .find(|member| member.user_id == user)
            .cloned()
            .unwrap_or_default();
        let palette = theme::palette(cx);
        let action = |id: String, icon: IconName, label: String, danger: bool| {
            Button::new(SharedString::from(id))
                .ghost()
                .w_full()
                .h(px(32.))
                .px(px(10.))
                .justify_start()
                .accessibility_label(label.clone())
                .child(
                    h_flex()
                        .gap(px(10.))
                        .when(danger, |row| row.text_color(palette.error))
                        .child(Icon::new(icon).size(px(14.)))
                        .child(label),
                )
        };
        let confirming =
            |kind: MemberAction| self.member_confirm.as_ref() == Some(&(user.to_owned(), kind));
        let user_owned = user.to_owned();
        let mut list = v_flex()
            .w_full()
            .px(px(8.))
            .py(px(6.))
            .gap(px(1.))
            .border_t_1()
            .border_color(cx.theme().border);
        let target = user_owned.clone();
        list =
            list.child(
                action(
                    "member-message".into(),
                    IconName::MessageCircle,
                    "Message".into(),
                    false,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.message_user(target.clone(), window, cx)
                })),
            );
        let target = user_owned.clone();
        list =
            list.child(
                action(
                    "member-verify".into(),
                    IconName::ShieldCheck,
                    "Verify…".into(),
                    false,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.verify_user(target.clone(), window, cx)
                })),
            );
        if member.can_mute {
            let target = user_owned.clone();
            let mute = !member.muted;
            list = list.child(
                action(
                    "member-mute".into(),
                    if mute {
                        IconName::MicOff
                    } else {
                        IconName::Mic
                    },
                    if mute {
                        "Mute in This Room".into()
                    } else {
                        "Unmute in This Room".into()
                    },
                    false,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.member_action(target.clone(), MemberAction::Mute(mute), window, cx)
                })),
            );
        }
        let target = user_owned.clone();
        list = list.child(
            action(
                "member-ignore".into(),
                IconName::EyeOff,
                if confirming(MemberAction::Ignore) {
                    "Click again to ignore everywhere".into()
                } else {
                    "Ignore…".into()
                },
                true,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.member_action(target.clone(), MemberAction::Ignore, window, cx)
            })),
        );
        for (kind, id, icon, label, confirm) in [
            (
                MemberAction::Kick,
                "member-kick",
                IconName::UserX,
                "Remove from Room…",
                "Click again to remove",
            ),
            (
                MemberAction::Ban,
                "member-ban",
                IconName::Ban,
                "Ban…",
                "Click again to ban",
            ),
        ] {
            let allowed = match kind {
                MemberAction::Kick => member.can_kick,
                _ => member.can_ban,
            };
            if !allowed {
                continue;
            }
            let target = user_owned.clone();
            list = list.child(
                action(
                    id.into(),
                    icon,
                    if confirming(kind) {
                        confirm.into()
                    } else {
                        label.into()
                    },
                    true,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.member_action(target.clone(), kind, window, cx)
                })),
            );
        }
        list.into_any_element()
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
            .child(self.avatar_image(
                self.member_avatar(&profile_id),
                &profile_id,
                &name,
                64.,
                32.,
                cx,
            ))
            .child(
                div()
                    .text_size(px(17.))
                    .font_weight(FontWeight::SEMIBOLD)
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
        let own = profile_id == self.snapshot.user_id;
        self.panel(cx)
            .child(self.panel_header("Profile", "close-profile", cx))
            .child(body)
            .when(!own, |panel| {
                panel.child(self.member_actions(&profile_id, cx))
            })
            .into_any_element()
    }
}

impl Render for MatrixApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.notification_window_active = window.is_window_active();
        self.notification_config();
        if self.details_open
            && self.info_tab == crate::app::InfoTab::Files
            && self.backend.is_some()
            && !self.preview
            && !self.snapshot.selected_room.is_empty()
            && self.snapshot.media.room != self.snapshot.selected_room
            && self.gallery_requested != self.snapshot.selected_room
            && !self.snapshot.media.loading
        {
            self.gallery_requested = self.snapshot.selected_room.clone();
            let kind = self.snapshot.media.kind;
            self.load_media(kind, window, cx);
        }
        // Presence is observed only for what is on screen: DM partners in the
        // sidebar and, when the Members tab shows, the room's members (≤ 64).
        if self.snapshot.presence_enabled && self.backend.is_some() {
            let mut visible: Vec<String> = self
                .snapshot
                .rooms
                .iter()
                .filter_map(|room| room.partner.clone())
                .collect();
            if self.details_open && self.info_tab == crate::app::InfoTab::Members {
                visible.extend(
                    self.snapshot
                        .members
                        .iter()
                        .map(|member| member.user_id.clone()),
                );
            }
            visible.sort();
            visible.dedup();
            visible.truncate(64);
            if visible != self.presence_observed {
                self.presence_observed = visible.clone();
                self.observe_presence(visible);
            }
        }
        // A search result opened: scroll its message into view once.
        if let Some(event) = self.snapshot.focused_event.clone()
            && self.revealed_event.as_ref() != Some(&event)
            && let Some(ix) = self.groups.iter().position(|group| {
                self.snapshot.messages[group.clone()]
                    .iter()
                    .any(|message| message.event_id.as_ref() == Some(&event))
            })
        {
            self.revealed_event = Some(event);
            self.scroller
                .update(cx, |state, cx| state.scroll_to_item(ix, cx));
        }
        // "Write to # General" names the destination, as Slack does.
        let placeholder: SharedString = self
            .snapshot
            .rooms
            .iter()
            .find(|room| self.navigation.selected_room.as_ref() == Some(&room.id))
            .map(|room| {
                if room.direct {
                    format!("Message {}", room.name)
                } else {
                    format!("Write to # {}", room.name)
                }
            })
            .unwrap_or_else(|| "Message".into())
            .into();
        if self.composer_placeholder != placeholder {
            self.composer_placeholder = placeholder.clone();
            self.composer.update(cx, |state, cx| {
                state.set_placeholder(placeholder, window, cx)
            });
        }
        if let Some(viewer) = &mut self.viewer {
            viewer.clamp_pan(window);
        }
        if self.security_open {
            self.security_open = false;
            self.open_settings(crate::settings::Section::Security, cx);
        }
        let content = if self.snapshot.session_state != twayblade_core::auth::SessionState::Active {
            self.session_recovery_view(cx)
        } else if self.snapshot.user_id.is_empty() {
            self.sign_in_view(cx)
        } else {
            let reduce = self.snapshot.reduce_motion;
            let panel = if self.management.is_some() {
                None
            } else if self.search_open {
                Some(("panel-search", self.search_view(cx)))
            } else if self.thread.is_some() {
                Some(("panel-thread", self.thread_view(cx)))
            } else if self.profile.is_some() {
                Some(("panel-profile", self.profile_view(cx)))
            } else if self.details_open && self.navigation.selected_room.is_some() {
                Some(("panel-details", self.details_view(cx)))
            } else {
                None
            }
            // The side panel slides in from the trailing edge when it opens or changes.
            .map(|(key, panel)| {
                motion::slide_in(
                    // The window-relative width cap lives here, on the panel's direct row child.
                    h_flex()
                        .h_full()
                        .flex_none()
                        .max_w(relative(0.32))
                        .items_stretch()
                        .child(panel),
                    key,
                    reduce,
                )
            });
            let room_key = SharedString::from(format!(
                "conversation-{}",
                self.navigation.selected_room.as_deref().unwrap_or("none")
            ));
            let full_page = self
                .management
                .as_ref()
                .is_some_and(|page| !page.kind.is_dialog());
            let dialog = self
                .management
                .as_ref()
                .is_some_and(|page| page.kind.is_dialog());
            v_flex()
                .size_full()
                .child(self.window_titlebar(cx))
                .child(
                    h_flex()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .items_stretch()
                        .child(self.rail_view(cx))
                        .when(self.sidebar_open, |row| row.child(self.sidebar_view(cx)))
                        .child(if full_page {
                            self.management_view(cx)
                        } else {
                            // Switching rooms crossfades the conversation briefly.
                            motion::fade_in(
                                h_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .items_stretch()
                                    .child(self.conversation_view(cx)),
                                room_key,
                                reduce,
                            )
                        })
                        .children(panel)
                        // Create Room / New DM / Join Space: a small sheet over a dimmed window.
                        .when(dialog, |row| {
                            row.child(
                                div()
                                    .id("management-backdrop")
                                    .absolute()
                                    .inset_0()
                                    .bg(gpui_kit::black().opacity(0.35))
                                    .flex()
                                    .items_start()
                                    .justify_center()
                                    .pt(px(72.))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.management = None;
                                        cx.notify();
                                    }))
                                    .child(motion::rise(
                                        div()
                                            .id("management-dialog")
                                            .w(px(440.))
                                            .rounded(px(12.))
                                            .overflow_hidden()
                                            .border_1()
                                            .border_color(cx.theme().border)
                                            .shadow_lg()
                                            // Clicks inside the sheet must not close it.
                                            .on_click(|_, _, cx| cx.stop_propagation())
                                            .child(self.management_view(cx)),
                                        "management-dialog-motion",
                                        reduce,
                                    )),
                            )
                        }),
                )
                .into_any_element()
        };
        let overlay = self.viewer.as_ref().map(|_| self.viewer_view(window, cx));
        let workspace = !self.security_open
            && !self.snapshot.user_id.is_empty()
            && self.snapshot.session_state == twayblade_core::auth::SessionState::Active;
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(px(13.))
            .track_focus(&self.focus)
            .key_context("Twayblade")
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.viewer.is_none() {
                    this.emoji_key(event, window, cx);
                }
            }))
            .on_action(
                cx.listener(|this, _: &crate::app::IncreaseScale, window, cx| {
                    this.change_scale(5, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::app::DecreaseScale, window, cx| {
                    this.change_scale(-5, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &crate::app::ResetScale, window, cx| {
                this.reset_scale(window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::app::OpenSettings, _, cx| {
                this.open_settings(crate::settings::Section::Appearance, cx)
            }))
            .on_action(cx.listener(|this, _: &FindConversation, window, cx| {
                this.search.update(cx, |state, cx| state.focus(window, cx))
            }))
            .on_action(cx.listener(|this, _: &crate::app::ToggleSidebar, _, cx| {
                this.sidebar_open = !this.sidebar_open;
                cx.notify();
            }))
            .on_action(
                cx.listener(|this, _: &crate::app::NavigateBack, window, cx| {
                    this.navigate_history(true, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::app::NavigateForward, window, cx| {
                    this.navigate_history(false, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &CloseProfile, window, cx| {
                if this.viewer.is_some() {
                    this.close_viewer(window, cx);
                } else if this
                    .management
                    .as_ref()
                    .is_some_and(|page| page.kind.is_dialog())
                {
                    this.management = None;
                    cx.notify();
                } else {
                    this.dismiss_panel(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleDetails, _, cx| this.toggle_details(cx)))
            .on_action(cx.listener(|this, _: &NewRoom, window, cx| {
                if !this.snapshot.user_id.is_empty() {
                    this.open_management(crate::spaces_ui::ManageKind::Create, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NewDirectMessage, window, cx| {
                if !this.snapshot.user_id.is_empty() {
                    this.open_management(crate::spaces_ui::ManageKind::Dm, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &JoinSpace, window, cx| {
                if !this.snapshot.user_id.is_empty() {
                    this.open_management(crate::spaces_ui::ManageKind::JoinSpace, window, cx);
                }
            }))
            // Full-screen flows (sign-in, settings, recovery) still need a title-bar
            // row on macOS: it clears the traffic lights and moves the window.
            .when(!workspace && cfg!(target_os = "macos"), |view| {
                view.child(self.toolbar("window-toolbar", cx))
            })
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(content)
                    .children(overlay)
                    .when(self.notification_sample, |view| {
                        view.child(self.notification_sample_view(cx))
                    }),
            )
    }
}
