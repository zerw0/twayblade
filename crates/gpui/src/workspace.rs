//! Signed-in workspace and sign-in window, laid out after the approved mockup:
//! space rail → conversations sidebar → conversation → optional details panel.
//! Controls whose feature does not exist yet are omitted rather than faked.
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
        use matrix_client_core::auth::SessionState;
        if self.snapshot.session_state == SessionState::SoftLoggedOut && self.options.is_some() {
            return self.sign_in_view(cx);
        }
        let soft = self.snapshot.session_state == SessionState::SoftLoggedOut;
        let mut form = v_flex().w(px(360.)).gap(px(16.)).child(div().text_size(px(22.)).font_weight(FontWeight::BOLD).child("You were signed out"))
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
                        .font_weight(FontWeight::BOLD)
                        .child(
                            if self.snapshot.session_state
                                == matrix_client_core::auth::SessionState::SoftLoggedOut
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
                                            != matrix_client_core::auth::SessionState::Active,
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
                            .and_then(|bytes| crate::attachments::render_png(bytes, cx))
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
                                            != matrix_client_core::auth::SessionState::Active,
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
                    "Matrix Client keeps your session securely in the {}.",
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
            None => (palette.pressed, cx.theme().foreground),
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
        // Selection is an accent ring around a fixed rounded square, not a shape change.
        div()
            .relative()
            .p(px(2.))
            .rounded(px(10.))
            .border_2()
            .border_color(if selected {
                cx.theme().primary
            } else {
                gpui_kit::transparent_black()
            })
            .child(
                Button::new(element_id)
                    .ghost()
                    .size(px(32.))
                    .p_0()
                    .rounded(px(7.))
                    .accessibility_label(name)
                    .tooltip(tooltip)
                    .child(
                        div()
                            .size(px(32.))
                            .rounded(px(7.))
                            .bg(bg)
                            .text_color(fg)
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
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
                // A dot, like a Dock or Mail sidebar indicator; the count is in the tooltip.
                tile.child(
                    div()
                        .absolute()
                        .top(px(0.))
                        .right(px(0.))
                        .size(px(10.))
                        .rounded(px(5.))
                        .border_2()
                        .border_color(cx.theme().sidebar)
                        .bg(cx.theme().primary),
                )
            })
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
        let _ = &palette;
        let mut rail = v_flex()
            .id("space-rail")
            .w(px(RAIL_WIDTH))
            .h_full()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .pt(px(4.))
            .pb(px(10.))
            .overflow_y_scroll();
        rail = rail.child(self.rail_tile(
            RailTile {
                space: None,
                name: "Home".into(),
                content: Icon::new(IconName::House).small().into_any_element(),
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
            rail = rail.child(div().my(px(2.)).w(px(24.)).h(px(1.)).bg(cx.theme().border));
        }
        for space in self.snapshot.spaces.clone() {
            let unread = self
                .snapshot
                .rooms
                .iter()
                .filter(|room| !room.is_space && space.room_ids.contains(&room.id))
                .fold(0u64, |sum, room| sum.saturating_add(room.unread));
            let label = div().child(theme::initials(&space.name)).into_any_element();
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
                .size(px(24.))
                .w(px(28.))
                .h(px(28.))
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
        let selected = self.navigation.selected_room.as_ref() == Some(&room.id);
        let id = room.id.clone();
        let unread = room.unread > 0 && !selected;
        // Selected rows use the accent fill with its contrasting foreground, as in
        // Mail and Finder; everything inside follows that foreground.
        let (fg, secondary) = if selected {
            (cx.theme().primary_foreground, cx.theme().primary_foreground)
        } else {
            (cx.theme().foreground, cx.theme().muted_foreground)
        };
        let icon = if room.direct {
            avatar(
                &theme::initials(&room.name),
                theme::avatar_color(&room.id),
                18.,
                9.,
                &palette,
            )
            .text_size(px(9.))
            .into_any_element()
        } else {
            div()
                .w(px(18.))
                .flex()
                .justify_center()
                .child(
                    Icon::new(IconName::Hash)
                        .size(px(14.))
                        .text_color(secondary),
                )
                .into_any_element()
        };
        Button::new(SharedString::from(format!("room-{}", room.id)))
            .ghost()
            .w_full()
            .h(px(28.))
            .p_0()
            .rounded(px(6.))
            .accessibility_label(if room.unread > 0 {
                format!("{}, {} unread", room.name, room.unread)
            } else {
                room.name.clone()
            })
            .child(
                h_flex()
                    .size_full()
                    .px(px(6.))
                    .gap(px(7.))
                    .rounded(px(6.))
                    .when(selected, |row| row.bg(cx.theme().primary))
                    .text_color(fg)
                    .child(icon)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_left()
                            .text_size(px(13.))
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
            .gap(px(12.))
            .overflow_y_scroll();
        if !self.snapshot.invites.is_empty() {
            list = list.child(self.invite_section(cx));
        }
        for (heading, direct) in [("Rooms", false), ("Direct messages", true)] {
            let rooms: Vec<_> = visible
                .iter()
                .filter(|room| room.direct == direct)
                .collect();
            if rooms.is_empty() {
                continue;
            }
            let mut section = v_flex().child(
                div()
                    .px(px(6.))
                    .pb(px(3.))
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
        let localpart = user
            .strip_prefix('@')
            .and_then(|id| id.split(':').next())
            .unwrap_or(&user)
            .to_owned();
        let account = h_flex()
            .h(px(44.))
            .px(px(8.))
            .gap(px(2.))
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("my-profile")
                    .ghost()
                    .flex_1()
                    .min_w_0()
                    .h(px(34.))
                    .px(px(6.))
                    .rounded(px(6.))
                    .accessibility_label("My profile")
                    .tooltip(user.clone())
                    .child(
                        h_flex()
                            .w_full()
                            .gap(px(8.))
                            .child(avatar(
                                &theme::initials(&user),
                                theme::avatar_color(&user),
                                24.,
                                12.,
                                &palette,
                            ))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .truncate()
                                            .text_left()
                                            .text_size(px(12.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(localpart),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_left()
                                            .text_size(px(11.))
                                            .text_color(muted)
                                            .child(self.snapshot.sync_status.clone()),
                                    ),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_profile(user.clone(), "You".into(), window, cx)
                    })),
            )
            .child(
                Button::new("open-settings")
                    .ghost()
                    .size(px(24.))
                    .w(px(28.))
                    .h(px(28.))
                    .flex_none()
                    .p_0()
                    .icon(IconName::Settings)
                    .accessibility_label("Settings")
                    .tooltip("Settings")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.security_open = true;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("logout")
                    .ghost()
                    .size(px(24.))
                    .w(px(28.))
                    .h(px(28.))
                    .flex_none()
                    .p_0()
                    .icon(IconName::LogOut)
                    .accessibility_label("Sign out")
                    .tooltip("Sign out")
                    .disabled(self.preview || self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.logout(window, cx))),
            );
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                div().px(px(8.)).pb(px(8.)).child(
                    Input::new(&self.search)
                        .small()
                        .prefix(Icon::new(IconName::Search).xsmall().text_color(muted)),
                ),
            )
            .child(list)
            .child(account)
            .into_any_element()
    }

    /// Sidebar column: toolbar row under the traffic lights, then the space rail
    /// and the room list sharing one sidebar surface.
    fn navigation_column(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.navigation.selected_space.as_ref();
        let title = self
            .snapshot
            .spaces
            .iter()
            .find(|space| Some(&space.id) == selected)
            .map(|space| space.name.clone())
            .unwrap_or_else(|| "Home".into());
        v_flex()
            .w(px(RAIL_WIDTH + SIDEBAR_WIDTH))
            .max_w(relative(0.30))
            .h_full()
            .flex_none()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                self.toolbar("sidebar-toolbar", cx)
                    .pl(px(if cfg!(target_os = "macos") {
                        TRAFFIC_LIGHTS_WIDTH + 8.
                    } else {
                        14.
                    }))
                    .pr(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(FontWeight::BOLD)
                            .child(title),
                    )
                    .child(
                        Button::new("new-dm")
                            .ghost()
                            .size(px(24.))
                            .w(px(28.))
                            .h(px(28.))
                            .icon(IconName::SquarePen)
                            .accessibility_label("Start a direct message")
                            .tooltip("New Direct Message  ⌘N")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_management(crate::spaces_ui::ManageKind::Dm, window, cx)
                            })),
                    )
                    .child(
                        Button::new("new-room")
                            .ghost()
                            .size(px(24.))
                            .w(px(28.))
                            .h(px(28.))
                            .icon(IconName::Plus)
                            .accessibility_label("Create a room")
                            .tooltip("Create Room  ⇧⌘N")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_management(
                                    crate::spaces_ui::ManageKind::Create,
                                    window,
                                    cx,
                                )
                            })),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .child(self.rail_view(cx))
                    .child(self.sidebar_view(cx)),
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
                            ReplyTarget::Pending => ("Reply", "Loading original message…"),
                            ReplyTarget::Unavailable => ("Reply", "Original message unavailable"),
                        };
                        content = content.child(
                            v_flex()
                                .id(SharedString::from(format!("reply-{}", message.id)))
                                .border_l_2()
                                .border_color(if outgoing {
                                    cx.theme().primary_foreground.opacity(0.6)
                                } else {
                                    cx.theme().primary
                                })
                                .pl(px(7.))
                                .text_size(px(11.5))
                                .opacity(0.85)
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .truncate()
                                        .child(sender.to_owned()),
                                )
                                .child(div().truncate().child(excerpt.to_owned()))
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
                                    model::plain_text(&model::message_text(message)),
                                )
                                .selectable(true)
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
            .when(!outgoing, |row| row.items_start());
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
        if message.event_id.is_some() || self.preview {
            let message_id = message.id.clone();
            let focus = self.image_focus(&format!("reaction-action-{}", message.id), cx);
            line = line.child(
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
            line = line.child(
                div()
                    .track_focus(&focus)
                    .focus(|style| style.opacity(1.))
                    .opacity(0.)
                    .group_hover("message", |style| style.opacity(1.))
                    .child(
                        Button::new(SharedString::from(format!("reply-thread-{id}")))
                            .track_focus(&focus)
                            .ghost()
                            .small()
                            .icon(IconName::MessagesSquare)
                            .accessibility_label("Reply in Thread")
                            .tooltip("Reply in Thread")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_thread(id.clone(), window, cx)
                            })),
                    ),
            );
        }
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
                    .xsmall()
                    .text_color(cx.theme().link)
                    .label(format!(
                        "{} replies{}",
                        message.thread_replies,
                        if message.thread_unread > 0 {
                            format!(" · {} unread", message.thread_unread)
                        } else {
                            String::new()
                        }
                    ))
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
        if message.event_id.is_some()
            && messages
                .iter()
                .rev()
                .find(|m| m.event_id.is_some())
                .map(|m| &m.id)
                == Some(&message.id)
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
        let marker = if new_day {
            Some(format!(
                "{} {}",
                day_label(first.timestamp_ms),
                time_label(first.timestamp_ms)
            ))
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
                    .text_color(cx.theme().link)
                    .child(div().flex_1().h(px(1.)).bg(cx.theme().link.opacity(0.5)))
                    .child("New")
                    .child(div().flex_1().h(px(1.)).bg(cx.theme().link.opacity(0.5)))
                    .test_support(),
            );
        }
        if self.preferences.chat.style != ChatStyle::Bubbles {
            let style = self.preferences.chat.style;
            if style == ChatStyle::Minimal
                && previous.is_none_or(|message| message.sender != first.sender)
            {
                let id = first.sender.clone();
                let name = first.display_name.clone();
                let mut header = h_flex().gap(px(8.)).items_center();
                if self.preferences.chat.avatars {
                    header = header.child(avatar(
                        &theme::initials(&name),
                        theme::avatar_color(&id),
                        22.,
                        11.,
                        &palette,
                    ));
                }
                header = header.child(
                    Button::new(SharedString::from(format!("sender-{}", first.id)))
                        .text()
                        .xsmall()
                        .font_weight(FontWeight::SEMIBOLD)
                        .label(name.clone())
                        .tooltip(id.clone())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_profile(id.clone(), name.clone(), window, cx)
                        })),
                );
                group = group.child(header);
            }
            let latest_own = messages
                .iter()
                .rev()
                .find(|message| message.sender == self.snapshot.user_id)
                .map(|message| &message.id);
            for message in &messages[range] {
                let mut line = h_flex()
                    .id(SharedString::from(format!("style-row-{}", message.id)))
                    .group("chat-row")
                    .w_full()
                    .min_w_0()
                    .gap(px(8.))
                    .items_start()
                    .py(px(if self.preferences.chat.density == Density::Compact {
                        1.
                    } else {
                        3.
                    }))
                    .hover(|style| style.bg(cx.theme().secondary.opacity(0.4)));
                if style == ChatStyle::Irc {
                    let id = message.sender.clone();
                    let name = message.display_name.clone();
                    line = line
                        .child(
                            div()
                                .w(px(42.))
                                .flex_none()
                                .text_color(muted)
                                .text_size(px(11.))
                                .when(
                                    self.preferences.chat.timestamps == Timestamps::OnHover,
                                    |view| {
                                        view.opacity(0.)
                                            .group_hover("chat-row", |style| style.opacity(1.))
                                    },
                                )
                                .child(time_label(message.timestamp_ms)),
                        )
                        .child(
                            Button::new(SharedString::from(format!("sender-{}", message.id)))
                                .text()
                                .xsmall()
                                .w(px(90.))
                                .flex_none()
                                .text_color(theme::avatar_text_color(&id, cx))
                                .label(name.clone())
                                .tooltip(id.clone())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_profile(id.clone(), name.clone(), window, cx)
                                })),
                        );
                }
                line = line.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(self.message_body(message, false, in_thread, cx)),
                );
                group = group.child(line.test_support());
                if latest_own == Some(&message.id) && message.delivery == DeliveryState::Sent {
                    group = group.child(div().text_size(px(10.5)).text_color(muted).child(
                        if message.read_by.is_empty() {
                            "Sent"
                        } else {
                            "Read"
                        },
                    ));
                }
            }
            return group.test_support().into_any_element();
        }
        let latest_own = messages
            .iter()
            .rev()
            .find(|message| message.sender == self.snapshot.user_id)
            .map(|message| message.id.clone());
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
                        div()
                            .pr(px(6.))
                            .text_size(px(10.5))
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
            .size(px(26.))
            .p_0()
            .rounded(px(13.))
            .accessibility_label(format!("Open profile of {}", first.display_name))
            .tooltip(format!("Open profile of {}", first.display_name))
            .child(avatar(
                &theme::initials(&first.display_name),
                theme::avatar_color(&first.sender),
                26.,
                13.,
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
            .gap(px(2.))
            .child(
                h_flex().pl(px(10.)).gap(px(6.)).items_baseline().child(
                    Button::new(SharedString::from(format!("sender-{}", first.id)))
                        .text()
                        .xsmall()
                        .text_size(px(11.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(muted)
                        .label(name.clone())
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
        let details_shown = self.details_open || self.profile.is_some() || self.thread.is_some();

        let mut title = h_flex().gap(px(5.)).child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::BOLD)
                .truncate()
                .child(room.name.clone()),
        );
        if room.encrypted {
            title = title.child(Icon::new(IconName::Lock).size(px(11.)).text_color(muted));
        }
        // The conversation header is the window toolbar: title and subtitle on the
        // left, a single panel toggle on the right. History loads on scroll.
        let header = self
            .toolbar("conversation-toolbar", cx)
            .pl(px(16.))
            .pr(px(10.))
            .gap(px(8.))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(v_flex().flex_1().min_w_0().child(title).when_some(
                room.topic.clone(),
                |column, topic| {
                    column.child(
                        div()
                            .text_size(px(11.))
                            .text_color(muted)
                            .truncate()
                            .child(topic),
                    )
                },
            ))
            .child(
                Button::new("details")
                    .ghost()
                    .small()
                    .icon(IconName::PanelRight)
                    .accessibility_label("Room details")
                    .tooltip("Room Details  ⌘I")
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
            .px(px(16.))
            .pt(px(4.))
            .pb(px(12.))
            .gap(px(4.))
            .when(!typing.is_empty(), |composer| {
                composer.child(note(typing).id("typing-notice").test_support())
            })
            .children(notices.into_iter().map(note))
            .when(self.emoji_picker.is_some(), |view| {
                view.child(self.emoji_panel(cx))
            })
            .when(
                self.attachment_draft.is_some() || self.attachment_sample,
                |view| view.child(self.attachment_confirmation(cx)),
            )
            .when(self.attachment_busy, |view| {
                view.child(note("Preparing attachment…".into()))
            })
            // Messages-style field: attach on the left, emoji inside on the right,
            // and a send button that appears only once there is something to send.
            .child(
                h_flex()
                    .items_center()
                    .gap(px(4.))
                    .min_h(px(36.))
                    .p(px(3.))
                    .rounded(px(16.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(palette.field)
                    .child(
                        Button::new("attach-file")
                            .ghost()
                            .size(px(24.))
                            .w(px(28.))
                            .h(px(28.))
                            .flex_none()
                            .p_0()
                            .rounded(px(12.))
                            .icon(IconName::Paperclip)
                            .accessibility_label("Attach file")
                            .tooltip("Attach File…")
                            .disabled(!editable || self.preview || self.attachment_busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_attachment(window, cx)
                            })),
                    )
                    .child(
                        div()
                            .id("composer-panel")
                            .flex_1()
                            .min_w_0()
                            .py(px(1.))
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
                        Button::new("emoji-open")
                            .ghost()
                            .size(px(24.))
                            .w(px(28.))
                            .h(px(28.))
                            .flex_none()
                            .p_0()
                            .rounded(px(12.))
                            .icon(IconName::FaceSlightlySmiling)
                            .accessibility_label("Choose emoji")
                            .tooltip("Emoji")
                            .disabled(!editable && !self.preview)
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.open_emoji(None, window, cx)
                                }),
                            ),
                    )
                    .when(!empty, |field| {
                        field.child(
                            Button::new("send")
                                .primary()
                                .size(px(24.))
                                .w(px(28.))
                                .h(px(28.))
                                .flex_none()
                                .p_0()
                                .rounded(px(12.))
                                .icon(IconName::ArrowUp)
                                .accessibility_label("Send message")
                                .tooltip("Send  ↩")
                                .disabled(!ready || self.sending.contains(&room.id))
                                .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
                        )
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
            .max_w(relative(0.32))
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
                    .font_weight(FontWeight::BOLD)
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
                    .child(avatar(
                        &theme::initials(&name),
                        theme::avatar_color(&id),
                        22.,
                        11.,
                        &theme::palette(cx),
                    ))
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
                .h(px(28.))
                .px(px(6.))
                .gap(px(8.))
                .child(Icon::new(IconName::Lock).xsmall().text_color(muted))
                .child(div().flex_1().child("Encrypted"))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(muted)
                        .child("Messages and files"),
                )
                .into_any_element()
        } else {
            h_flex()
                .id("encryption-warning")
                .items_start()
                .gap(px(8.))
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
                        .child("Not encrypted. Your server can read these messages.")
                        .test_support(),
                )
                .test_support()
                .into_any_element()
        };
        let id = room.id.clone();
        let members = self.snapshot.members.len();
        self.panel(cx)
            .child(self.panel_header("Details", "close-details", cx))
            .child(
                v_flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(16.))
                    .pt(px(18.))
                    .pb(px(14.))
                    .child(room_avatar(&room, 48., &palette))
                    .child(
                        div()
                            .pt(px(4.))
                            .text_size(px(14.))
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
                    .px(px(8.))
                    .pb(px(10.))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(encryption)
                    .child(
                        Button::new("copy-room-id")
                            .ghost()
                            .xsmall()
                            .h(px(28.))
                            .justify_start()
                            .icon(IconName::Copy)
                            .label("Copy Room ID")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(id.clone()))
                            }),
                    ),
            )
            .child(
                v_flex()
                    .px(px(8.))
                    .pt(px(12.))
                    .child(self.section_label(format!("Members · {members}"), cx))
                    .child(match self.snapshot.members_state {
                        MembersState::Loading => div()
                            .px(px(6.))
                            .text_color(muted)
                            .child("Loading members…")
                            .into_any_element(),
                        MembersState::Failed => div()
                            .px(px(6.))
                            .text_color(muted)
                            .child("Members could not be loaded.")
                            .into_any_element(),
                        MembersState::Ready => uniform_list(
                            "members",
                            members,
                            cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .filter_map(|ix| this.snapshot.members.get(ix))
                                    .map(|member| this.member_row(member, cx))
                                    .collect::<Vec<_>>()
                            }),
                        )
                        // Virtualized: grows with the room, capped so large rooms scroll.
                        .h(px(
                            (members as f32 * MEMBER_ROW_HEIGHT).clamp(MEMBER_ROW_HEIGHT, 480.)
                        ))
                        .w_full()
                        .into_any_element(),
                    }),
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(viewer) = &mut self.viewer {
            viewer.clamp_pan(window);
        }
        if self.security_open {
            self.security_open = false;
            self.open_settings(crate::settings::Section::Security, cx);
        }
        let content =
            if self.snapshot.session_state != matrix_client_core::auth::SessionState::Active {
                self.session_recovery_view(cx)
            } else if self.snapshot.user_id.is_empty() {
                self.sign_in_view(cx)
            } else {
                let panel = if self.management.is_some() {
                    None
                } else if self.thread.is_some() {
                    Some(self.thread_view(cx))
                } else if self.profile.is_some() {
                    Some(self.profile_view(cx))
                } else if self.details_open && self.navigation.selected_room.is_some() {
                    Some(self.details_view(cx))
                } else {
                    None
                };
                h_flex()
                    .size_full()
                    .items_stretch()
                    .child(self.navigation_column(cx))
                    .child(if self.management.is_some() {
                        self.management_view(cx)
                    } else {
                        self.conversation_view(cx)
                    })
                    .children(panel)
                    .into_any_element()
            };
        let overlay = self.viewer.as_ref().map(|_| self.viewer_view(window, cx));
        let workspace = !self.security_open
            && !self.snapshot.user_id.is_empty()
            && self.snapshot.session_state == matrix_client_core::auth::SessionState::Active;
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(px(13.))
            .track_focus(&self.focus)
            .key_context("MatrixClient")
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
            .on_action(cx.listener(|this, _: &CloseProfile, window, cx| {
                if this.viewer.is_some() {
                    this.close_viewer(window, cx);
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
                    .children(overlay),
            )
    }
}
