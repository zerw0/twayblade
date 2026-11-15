//! The notifications popover under the title-bar bell: invitations, missed
//! calls, rooms with mentions, then other unread conversations, each room with
//! its latest message (#99).
use super::*;
use crate::ui::chrome::ToolbarControl;

/// Unread rooms listed below the mentions; the room list has the rest.
const UNREAD_LIMIT: usize = 20;
/// A click on the bell right after a click outside closed the popover must not
/// reopen it.
const REOPEN_GUARD: std::time::Duration = std::time::Duration::from_millis(250);

pub(crate) struct Activity<'a> {
    pub invites: &'a [twayblade_core::space_actions::Invite],
    pub missed: &'a [crate::backend::MissedCall],
    pub mentions: Vec<&'a crate::backend::Room>,
    pub unread: Vec<&'a crate::backend::Room>,
}

impl Activity<'_> {
    /// What the bell's badge counts: things addressed to you.
    pub fn attention(&self) -> u64 {
        self.invites.len() as u64
            + self.missed.len() as u64
            + self
                .mentions
                .iter()
                .fold(0u64, |sum, room| sum.saturating_add(room.mentions))
    }

    pub fn is_empty(&self) -> bool {
        self.invites.is_empty()
            && self.missed.is_empty()
            && self.mentions.is_empty()
            && self.unread.is_empty()
    }

    /// Something Mark All as Read would clear.
    pub fn has_unread(&self) -> bool {
        !(self.missed.is_empty() && self.mentions.is_empty() && self.unread.is_empty())
    }

    /// The rooms listed, for previews and Mark All as Read.
    pub fn room_ids(&self) -> Vec<String> {
        self.mentions
            .iter()
            .chain(&self.unread)
            .map(|room| room.id.clone())
            .collect()
    }
}

pub(crate) fn activity<'a>(
    rooms: &'a [crate::backend::Room],
    invites: &'a [twayblade_core::space_actions::Invite],
    missed: &'a [crate::backend::MissedCall],
) -> Activity<'a> {
    let listed = |room: &&crate::backend::Room| !room.is_space && !room.archived;
    let mut mentions: Vec<_> = rooms
        .iter()
        .filter(listed)
        .filter(|room| room.mentions > 0)
        .collect();
    let mut unread: Vec<_> = rooms
        .iter()
        .filter(listed)
        .filter(|room| room.mentions == 0 && room.unread > 0 && !room.muted)
        .collect();
    mentions.sort_by_key(|room| std::cmp::Reverse(room.recency));
    unread.sort_by_key(|room| std::cmp::Reverse(room.recency));
    unread.truncate(UNREAD_LIMIT);
    Activity {
        invites,
        missed,
        mentions,
        unread,
    }
}

/// One line under a room: who wrote what, or only the room's count when
/// notification previews are off.
pub(crate) fn preview_line(
    preview: &twayblade_core::activity::RoomPreview,
    direct: bool,
) -> String {
    use twayblade_core::activity::PreviewBody;
    let what = match &preview.body {
        PreviewBody::Text(text) => return prefix(preview, direct, text),
        PreviewBody::Image => "Image",
        PreviewBody::Video => "Video",
        PreviewBody::Audio => "Audio",
        PreviewBody::File => "File",
        PreviewBody::Location => "Location",
        PreviewBody::Sticker => "Sticker",
        PreviewBody::Poll => "Poll",
        PreviewBody::Encrypted => "Encrypted message",
        PreviewBody::Deleted => "Message deleted",
    };
    prefix(preview, direct, what)
}

/// "You: …" for your own message; the sender's name in a room; nothing in a
/// direct chat, where the room name is already the sender.
fn prefix(preview: &twayblade_core::activity::RoomPreview, direct: bool, text: &str) -> String {
    if preview.own {
        format!("You: {text}")
    } else if direct {
        text.to_owned()
    } else {
        format!("{}: {text}", preview.sender_name)
    }
}

/// "Missed video call" or "Missed call", with the room unless it is the caller's DM.
fn missed_label(call: &crate::backend::MissedCall) -> String {
    let kind = if call.video {
        "Missed video call"
    } else {
        "Missed call"
    };
    if call.room_name == call.caller_name || call.room_name.is_empty() {
        kind.to_owned()
    } else {
        format!("{kind} · {}", call.room_name)
    }
}

fn count_label(count: u64) -> String {
    if count > 99 {
        "99+".into()
    } else {
        count.to_string()
    }
}

impl MatrixApp {
    /// Opening moves keyboard focus into the popover, so Escape, Tab and
    /// screen readers land there; closing hands focus back.
    pub(crate) fn toggle_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.notifications_open {
            self.close_notifications(window, cx);
            return;
        }
        if self
            .notifications_closed_at
            .is_some_and(|at| at.elapsed() < REOPEN_GUARD)
        {
            return;
        }
        self.notifications_open = true;
        self.notifications_focus.focus(window, cx);
        self.activity_previews_key.clear();
        self.sync_activity_previews();
        cx.notify();
    }

    pub(crate) fn close_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.notifications_open {
            self.notifications_open = false;
            self.notifications_closed_at = Some(std::time::Instant::now());
            if self.notifications_focus.contains_focused(window, cx) {
                self.restore_focus(window, cx);
            }
            cx.notify();
        }
    }

    /// While the popover is open: ask for previews of the rooms it lists
    /// whenever that list or a room's latest activity changes.
    pub(crate) fn sync_activity_previews(&mut self) {
        if !self.notifications_open || self.preview {
            return;
        }
        let list = activity(&self.snapshot.rooms, &self.snapshot.invites, &[]);
        let key: Vec<(String, u64)> = list
            .mentions
            .iter()
            .chain(&list.unread)
            .map(|room| (room.id.clone(), room.recency))
            .collect();
        if key == self.activity_previews_key {
            return;
        }
        let rooms = list.room_ids();
        self.activity_previews_key = key;
        if let Some(backend) = &self.backend {
            backend.request_previews(rooms);
        }
    }

    fn mark_all_read(&mut self, rooms: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview {
            self.snapshot.missed_calls.clear();
            for room in &mut self.snapshot.rooms {
                room.unread = 0;
                room.mentions = 0;
            }
            cx.notify();
            return;
        }
        self.job(
            window,
            cx,
            move |backend| backend.mark_all_read(rooms),
            |this, result, _, _| {
                if let Err(error) = result {
                    this.status = error.to_string();
                }
            },
        );
    }

    /// The bell with its badge, and the popover anchored under it when open.
    pub(crate) fn notifications_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let activity = activity(
            &self.snapshot.rooms,
            &self.snapshot.invites,
            &self.snapshot.missed_calls,
        );
        let attention = activity.attention();
        let label = match attention {
            0 => "Notifications".to_owned(),
            1 => "Notifications, 1 new".to_owned(),
            n => format!("Notifications, {n} new"),
        };
        let mut button = div().relative().flex_none().child(
            Button::new("open-notifications")
                .toolbar_control()
                .ghost()
                .size(px(26.))
                .icon(IconName::Bell)
                .selected(self.notifications_open)
                .accessibility_label(label)
                .tooltip("Notifications")
                .on_click(cx.listener(|this, _, window, cx| this.toggle_notifications(window, cx))),
        );
        if attention > 0 {
            button = button.child(
                div()
                    .absolute()
                    .top(px(-2.))
                    .right(px(-3.))
                    .min_w(px(15.))
                    .h(px(15.))
                    .px(px(4.))
                    .rounded_full()
                    .bg(cx.theme().primary)
                    .text_color(cx.theme().primary_foreground)
                    .text_size(px(9.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(count_label(attention)),
            );
        }
        if self.notifications_open {
            button = button.child(
                div().absolute().top_full().right_0().child(
                    deferred(
                        anchored()
                            .anchor(Anchor::TopRight)
                            .offset(point(px(0.), px(6.)))
                            .snap_to_window_with_margin(px(8.))
                            .child(self.notifications_popover(&activity, cx)),
                    )
                    .with_priority(1),
                ),
            );
        }
        button.into_any_element()
    }

    fn notifications_popover(&self, activity: &Activity, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let section = |title: &'static str| {
            div()
                .px(px(14.))
                .pt(px(10.))
                .pb(px(4.))
                .text_size(px(11.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(muted)
                .child(title)
        };
        let mut list = v_flex()
            .id("notifications-list")
            .role(Role::List)
            .aria_label("Notifications")
            .max_h(px(420.))
            .overflow_y_scroll()
            .pb(px(6.));
        if activity.is_empty() {
            list = list.child(
                v_flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(20.))
                    .py(px(28.))
                    .child(Icon::new(IconName::BellOff).size(px(20.)).text_color(muted))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child("You’re all caught up"),
                    )
                    .child(
                        div()
                            .text_center()
                            .text_size(px(12.))
                            .text_color(muted)
                            .child(
                                "Invitations, missed calls, mentions and unread conversations appear here.",
                            ),
                    ),
            );
        }
        if !activity.invites.is_empty() {
            list = list.child(section("Invitations"));
            for invite in activity.invites {
                list = list.child(self.invite_row(invite, cx));
            }
        }
        if !activity.missed.is_empty() {
            list = list.child(section("Missed calls"));
            for call in activity.missed {
                list = list.child(self.missed_call_row(call, cx));
            }
        }
        if !activity.mentions.is_empty() {
            list = list.child(section("Mentions"));
            for room in &activity.mentions {
                list = list.child(self.activity_room_row(room, true, cx));
            }
        }
        if !activity.unread.is_empty() {
            list = list.child(section("Unread"));
            for room in &activity.unread {
                list = list.child(self.activity_room_row(room, false, cx));
            }
        }
        let popover = v_flex()
            .id("notifications-popover")
            .test_support()
            .role(Role::Dialog)
            .aria_label("Notifications")
            .w(px(340.))
            .rounded(px(10.))
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .shadow_lg()
            .overflow_hidden()
            .track_focus(&self.notifications_focus)
            .on_mouse_down_out(
                cx.listener(|this, _, window, cx| this.close_notifications(window, cx)),
            )
            .child(
                h_flex()
                    .px(px(14.))
                    .py(px(8.))
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .id("notifications-title")
                            .role(Role::Heading)
                            .aria_label("Notifications")
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Notifications"),
                    )
                    .child(
                        h_flex()
                            .gap(px(2.))
                            .when(activity.has_unread(), |header| {
                                let rooms = activity.room_ids();
                                header.child(
                                    Button::new("notifications-mark-read")
                                        .ghost()
                                        .xsmall()
                                        .icon(IconName::CheckCheck)
                                        .accessibility_label("Mark All as Read")
                                        .tooltip("Mark All as Read")
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.mark_all_read(rooms.clone(), window, cx)
                                        })),
                                )
                            })
                            .child(
                                Button::new("notifications-settings")
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::Settings)
                                    .accessibility_label("Notification Settings")
                                    .tooltip("Notification Settings")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_notifications(window, cx);
                                        this.open_settings(
                                            crate::ui::settings::Section::Notifications,
                                            cx,
                                        )
                                    })),
                            ),
                    ),
            )
            .child(list);
        motion::rise(
            popover,
            "notifications-popover-motion",
            self.snapshot.reduce_motion,
        )
    }

    fn activity_room_row(
        &self,
        room: &crate::backend::Room,
        mention: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = if mention { room.mentions } else { room.unread };
        let what = match (mention, count) {
            (true, 1) => "1 mention".to_owned(),
            (true, n) => format!("{n} mentions"),
            (false, 1) => "1 unread message".to_owned(),
            (false, n) => format!("{n} unread messages"),
        };
        let id = room.id.clone();
        let preview = self
            .preferences
            .notifications
            .preview
            .then(|| self.snapshot.activity_previews.get(&room.id))
            .flatten()
            .map(|preview| preview_line(preview, room.direct));
        let label = match &preview {
            Some(line) => format!("{}, {what}, {line}", room.name),
            None => format!("{}, {what}", room.name),
        };
        let palette_badge = if mention {
            (cx.theme().primary, cx.theme().primary_foreground)
        } else {
            (cx.theme().secondary, cx.theme().secondary_foreground)
        };
        Button::new(SharedString::from(format!("activity-room-{}", room.id)))
            .ghost()
            .w_full()
            .h(px(if preview.is_some() { 48. } else { 40. }))
            .px(px(10.))
            .rounded(px(0.))
            .accessibility_label(label)
            .child(
                h_flex()
                    .w_full()
                    .gap(px(10.))
                    .child(self.avatar_image(
                        room.avatar.as_deref(),
                        &room.id,
                        &room.name,
                        26.,
                        if room.direct { 13. } else { 7. },
                        cx,
                    ))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_left()
                                    .font_weight(if mention {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .child(room.name.clone()),
                            )
                            .children(preview.map(|line| {
                                div()
                                    .truncate()
                                    .text_left()
                                    .text_size(px(11.5))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(line)
                            })),
                    )
                    .child(
                        div()
                            .flex_none()
                            .min_w(px(20.))
                            .h(px(18.))
                            .px(px(6.))
                            .rounded_full()
                            .bg(palette_badge.0)
                            .text_color(palette_badge.1)
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(count_label(count)),
                    ),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.close_notifications(window, cx);
                this.select_room(id.clone(), window, cx);
            }))
            .into_any_element()
    }

    fn missed_call_row(
        &self,
        call: &crate::backend::MissedCall,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let room = call.room.clone();
        let detail = missed_label(call);
        let when = chrono::Local
            .timestamp_millis_opt(call.at as i64)
            .single()
            .map(|time| time.format("%H:%M").to_string())
            .unwrap_or_default();
        Button::new(SharedString::from(format!(
            "activity-missed-{}-{}",
            call.room, call.at
        )))
        .ghost()
        .w_full()
        .h(px(48.))
        .px(px(10.))
        .rounded(px(0.))
        .accessibility_label(format!("{}, {detail}, {when}", call.caller_name))
        .child(
            h_flex()
                .w_full()
                .gap(px(10.))
                .child(self.avatar_image(None, &call.caller, &call.caller_name, 26., 13., cx))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().truncate().text_left().child(call.caller_name.clone()))
                        .child(
                            h_flex()
                                .gap(px(4.))
                                .text_size(px(11.5))
                                .text_color(theme::palette(cx).error)
                                .child(Icon::new(IconName::PhoneMissed).size(px(12.)))
                                .child(div().truncate().child(detail)),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child(when),
                ),
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            this.close_notifications(window, cx);
            this.select_room(room.clone(), window, cx);
        }))
        .into_any_element()
    }

    fn invite_row(
        &self,
        invite: &twayblade_core::space_actions::Invite,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let kind = if invite.space {
            "Space invitation"
        } else if invite.direct {
            "Direct message invitation"
        } else {
            "Room invitation"
        };
        let disabled = self.preview || self.invite_pending.contains(&invite.id);
        let (accept_id, decline_id) = (invite.id.clone(), invite.id.clone());
        h_flex()
            .id(SharedString::from(format!("activity-invite-{}", invite.id)))
            .role(Role::Group)
            .aria_label(format!("{}, {kind}", invite.name))
            .px(px(14.))
            .py(px(6.))
            .gap(px(10.))
            .child(self.avatar_image(None, &invite.id, &invite.name, 26., 7., cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(invite.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(cx.theme().muted_foreground)
                            .child(kind),
                    ),
            )
            .child(
                Button::new(SharedString::from(format!(
                    "activity-decline-{}",
                    invite.id
                )))
                .ghost()
                .xsmall()
                .label("Decline")
                .accessibility_label(format!("Decline {}", invite.name))
                .disabled(disabled)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.respond_invite(decline_id.clone(), false, window, cx)
                })),
            )
            .child(
                Button::new(SharedString::from(format!("activity-accept-{}", invite.id)))
                    .primary()
                    .xsmall()
                    .label("Accept")
                    .accessibility_label(format!("Accept {}", invite.name))
                    .disabled(disabled)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_notifications(window, cx);
                        this.respond_invite(accept_id.clone(), true, window, cx)
                    })),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{activity, count_label, missed_label, preview_line};
    use crate::backend::{MissedCall, Room};
    use twayblade_core::activity::{PreviewBody, RoomPreview};

    #[test]
    fn previews_name_the_sender_only_where_it_helps() {
        let preview = |own, body| RoomPreview {
            room: "!r".into(),
            sender: "@sam:example.org".into(),
            sender_name: "Sam".into(),
            own,
            body,
            timestamp_ms: 0,
        };
        let text = || PreviewBody::Text("On my way".into());
        assert_eq!(
            preview_line(&preview(false, text()), false),
            "Sam: On my way"
        );
        assert_eq!(preview_line(&preview(false, text()), true), "On my way");
        assert_eq!(preview_line(&preview(true, text()), true), "You: On my way");
        assert_eq!(
            preview_line(&preview(false, PreviewBody::Image), false),
            "Sam: Image"
        );
        assert_eq!(
            preview_line(&preview(false, PreviewBody::Encrypted), true),
            "Encrypted message"
        );
    }

    #[test]
    fn missed_calls_count_and_can_be_cleared() {
        let call = MissedCall {
            room: "!dm".into(),
            room_name: "Alex".into(),
            caller: "@alex:example.org".into(),
            caller_name: "Alex".into(),
            video: true,
            at: 0,
        };
        assert_eq!(missed_label(&call), "Missed video call");
        let group = MissedCall {
            room_name: "Design".into(),
            video: false,
            ..call.clone()
        };
        assert_eq!(missed_label(&group), "Missed call · Design");
        let missed = [call];
        let list = activity(&[], &[], &missed);
        assert_eq!(list.attention(), 1, "a missed call counts on the bell");
        assert!(!list.is_empty() && list.has_unread());
        assert!(list.room_ids().is_empty());
    }

    fn room(id: &str, unread: u64, mentions: u64, recency: u64) -> Room {
        Room {
            id: id.into(),
            name: id.into(),
            unread,
            mentions,
            recency,
            ..Default::default()
        }
    }

    #[test]
    fn mentions_come_first_and_quiet_rooms_stay_out() {
        let mut muted = room("muted", 4, 0, 9);
        muted.muted = true;
        let mut archived = room("archived", 2, 1, 9);
        archived.archived = true;
        let mut space = room("space", 3, 0, 9);
        space.is_space = true;
        let rooms = vec![
            room("old-mention", 3, 1, 1),
            room("read", 0, 0, 8),
            room("new-unread", 2, 0, 7),
            room("new-mention", 5, 2, 6),
            room("old-unread", 1, 0, 2),
            muted,
            archived,
            space,
        ];
        let activity = activity(&rooms, &[], &[]);
        let ids = |rooms: &[&Room]| rooms.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&activity.mentions), ["new-mention", "old-mention"]);
        assert_eq!(ids(&activity.unread), ["new-unread", "old-unread"]);
        assert_eq!(
            activity.attention(),
            3,
            "mentions count, plain unread does not"
        );
    }

    #[test]
    fn badge_counts_invites_and_caps_at_99() {
        let invites = vec![twayblade_core::space_actions::Invite::default(); 2];
        let rooms = vec![room("busy", 500, 120, 1)];
        let busy = activity(&rooms, &invites, &[]);
        assert_eq!(busy.attention(), 122);
        assert_eq!(count_label(busy.attention()), "99+");
        assert!(activity(&[], &[], &[]).is_empty());
    }
}
