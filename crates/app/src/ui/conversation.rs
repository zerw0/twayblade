//! The conversation column: header, timeline, composer and typing row.
use super::*;

impl MatrixApp {
    pub(crate) fn conversation_view(&self, cx: &mut Context<Self>) -> AnyElement {
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
        let stacked = !self.attachment_drafts.is_empty()
            || self.attachment_pending > 0
            || (self.preview && self.attachment_sample);
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
            // A composer box: the message field, then a quiet toolbar row with
            // attach, emoji and send, as in Slack and Mail.
            .child(
                v_flex()
                    .rounded(px(9.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(palette.field)
                    .when(stacked, |view| view.child(self.attachment_tray(cx)))
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
                                    .disabled(
                                        !editable
                                            || self.preview
                                            || self.attachment_drafts.len()
                                                + self.attachment_pending
                                                >= crate::ui::attachments::MAX_ATTACHMENTS,
                                    )
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
                                        this.toggle_emoji(window, cx)
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
                                    .disabled(
                                        !ready
                                            || (empty && self.attachment_drafts.is_empty())
                                            || self.attachment_pending > 0
                                            || self.sending.contains(&room.id),
                                    )
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
                this.add_attachments(paths.0.iter().cloned(), window, cx);
            }))
            .into_any_element()
    }
}
