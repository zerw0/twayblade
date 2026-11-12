//! The interface. This module renders the window (space rail → sidebar →
//! conversation → optional panel) and holds shared layout helpers; each area
//! lives in its own submodule. Controls whose feature does not exist yet are
//! omitted rather than faked.

// Areas of the main window.
mod chrome;
pub(crate) mod conversation;
mod panels;
mod sign_in;
mod timeline;

// Secondary windows, overlays and shared pieces.
pub(crate) mod attachments;
pub(crate) mod avatars;
pub(crate) mod calls;
pub(crate) mod editing;
pub(crate) mod emoji;
pub(crate) mod keyboard;
pub(crate) mod motion;
pub(crate) mod security;
pub(crate) mod settings;
pub(crate) mod spaces;
pub(crate) mod threads;
pub(crate) mod viewer;

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

pub(crate) fn avatar(label: &str, fill: Hsla, size: f32, radius: f32, palette: &Palette) -> Div {
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
        self.sync_call_views(cx);
        self.sync_room_loading(cx);
        if self.security_open {
            self.security_open = false;
            self.open_settings(crate::ui::settings::Section::Security, cx);
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
                        .children(self.incoming_call_card(cx))
                        // Create Room / New DM / Join Space: a small sheet over a dimmed window.
                        .when(dialog, |row| {
                            row.child(
                                div()
                                    .id("management-backdrop")
                                    .absolute()
                                    .inset_0()
                                    .occlude()
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
                this.open_settings(crate::ui::settings::Section::Appearance, cx)
            }))
            .on_action(
                cx.listener(|this, _: &crate::shortcuts::ShowKeyboardShortcuts, _, cx| {
                    this.open_settings(crate::ui::settings::Section::Keyboard, cx)
                }),
            )
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
                } else if this.emoji_picker.is_some() {
                    this.close_emoji(window, cx);
                } else if this.edit_session.is_some() {
                    this.end_edit(window, cx);
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
                    this.open_management(crate::ui::spaces::ManageKind::Create, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NewDirectMessage, window, cx| {
                if !this.snapshot.user_id.is_empty() {
                    this.open_management(crate::ui::spaces::ManageKind::Dm, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &JoinSpace, window, cx| {
                if !this.snapshot.user_id.is_empty() {
                    this.open_management(crate::ui::spaces::ManageKind::JoinSpace, window, cx);
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
