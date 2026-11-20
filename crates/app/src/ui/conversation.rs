//! The conversation column: header, timeline, typing row and composer.
use super::*;
use gpui_kit::component::spinner::Spinner;
use twayblade_core::conversation::TimelineStatus;

/// How long a room may load before the view offers to try again.
const LOAD_LIMIT: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoomLoad {
    Ready,
    Loading,
    /// Past the time limit, or the backend is retrying a timed-out open.
    Slow,
    Failed,
    Unavailable,
}

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
        let voice_channel = self.is_voice_channel(&room.id);
        let load = if voice_channel {
            RoomLoad::Ready
        } else {
            self.room_load(&room.id)
        };
        let ready = self.snapshot.timeline_ready && !self.preview && !self.busy;
        let editable = !self.busy && (self.preview || self.snapshot.timeline_ready);
        let empty = self.composer.read(cx).value().trim().is_empty();
        let editing = self.edit_session.as_ref().is_some_and(|s| !s.in_thread);
        let stacked = !self.attachments.is_empty() || (self.preview && self.attachment_sample);
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
                    .id("room-title")
                    .role(Role::Heading)
                    .aria_label(room.name.clone())
                    .flex_none()
                    .max_w(px(260.))
                    .truncate()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(room.name.clone()),
            )
            .when(load == RoomLoad::Loading, |header| {
                header.child(
                    div()
                        .id("room-loading")
                        .flex_none()
                        .child(if self.snapshot.reduce_motion {
                            Icon::new(IconName::Loader)
                                .size(px(14.))
                                .text_color(muted)
                                .into_any_element()
                        } else {
                            Spinner::new().small().color(muted).into_any_element()
                        })
                        .tooltip(|window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new("Loading messages")
                                .build(window, cx)
                        }),
                )
            })
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
            .children(self.call_buttons(&room.id, room.encrypted, cx))
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
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_details(window, cx))),
            );

        let call_banner = self.call_banner(&room.id, cx);
        let body = if self.groups.is_empty() && load == RoomLoad::Loading {
            div().flex_1().min_h_0().into_any_element()
        } else if self.groups.is_empty() && load != RoomLoad::Ready {
            self.room_load_error(load, cx)
        } else if voice_channel || self.groups.is_empty() {
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
                // Minimal placeholder reusing the existing empty-state layout.
                .child(div().text_color(muted).child(if voice_channel {
                    if self.live_call().is_some_and(|call| call.room_id == room.id) {
                        "Audio only."
                    } else {
                        "Join voice to start talking."
                    }
                } else {
                    "No messages yet."
                }))
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
            .with_label(format!("Messages in {}", room.name))
            // Kit defaults to 32px between rows; groups own their 6px spacing.
            .with_row_style(StyleRefinement::default().px_0().pb_0())
            .into_any_element()
        };
        // The messages area F6 lands in; never a Tab stop itself.
        let body = v_flex()
            .id("timeline-area")
            .track_focus(&self.timeline_area)
            .relative()
            .flex_1()
            .min_h_0()
            // First, so it sees scroll events before the timeline does.
            .child(crate::ui::swipe_reply::swipe_capture(cx.weak_entity()))
            .child(body)
            .into_any_element();

        let timeline_notice =
            if load == RoomLoad::Loading || (self.groups.is_empty() && load != RoomLoad::Ready) {
                None
            } else {
                Some(&self.snapshot.timeline_status)
            };
        let notices: Vec<String> = std::iter::once(&self.status)
            .chain(timeline_notice)
            .filter(|text| !text.is_empty())
            .cloned()
            .collect();
        let typing = model::typing_text(&self.snapshot);
        let paste_target = cx.weak_entity();
        let note = |(index, text): (usize, String)| {
            div()
                .id(("composer-note", index))
                .role(Role::Status)
                .aria_label(text.clone())
                .pl(px(12.))
                .text_size(px(13.))
                .text_color(muted)
                .child(text)
        };
        let composer =
            v_flex()
                .px(px(20.))
                .pt(px(2.))
                .pb(px(6.))
                .gap(px(4.))
                .children(notices.into_iter().enumerate().map(note))
                .when(self.emoji_picker.is_some(), |view| {
                    view.child(motion::rise(
                        // A click anywhere else closes the picker, as menus do.
                        div()
                            .on_mouse_down_out(cx.listener(
                                |this, event: &MouseDownEvent, _, cx| {
                                    this.dismiss_emoji(event.position, cx)
                                },
                            ))
                            .child(self.emoji_panel(cx)),
                        "emoji-picker-motion",
                        self.snapshot.reduce_motion,
                    ))
                })
                .children(self.mention_panel(false, cx).map(|panel| {
                    motion::rise(
                        div().child(panel),
                        "mention-list-motion",
                        self.snapshot.reduce_motion,
                    )
                }))
                .child(
                    div()
                        .h(px(18.))
                        .pl(px(4.))
                        .text_size(px(11.))
                        .text_color(muted)
                        // Nobody typing: who has read up to the latest message.
                        .when(typing.is_empty(), |line| {
                            line.children(self.seen_by_row(cx))
                        })
                        .when(!typing.is_empty(), |line| {
                            // Who is typing (avatars), a pulse, then the accessible text.
                            let reduce = self.snapshot.reduce_motion;
                            let typers: Vec<_> =
                                self.snapshot.typing.iter().take(3).cloned().collect();
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
                )
                // While a voice message records, its bar takes the composer's place.
                .when(self.voice_recorder.is_some(), |view| {
                    view.child(self.voice_recording_bar(cx))
                })
                // Reserve action width beside the growing field so text wraps
                // before the buttons rather than underneath them.
                .when(self.voice_recorder.is_none(), |view| {
                    view.child(
                v_flex()
                    .rounded(px(9.))
                    .border_1()
                    .border_color(cx.theme().input)
                    .bg(palette.field)
                    .when(stacked, |view| view.child(self.attachment_tray(false, cx)))
                    .children(self.edit_bar(false, cx))
                    .children(self.reply_bar(cx))
                    .child(
                        h_flex()
                            .items_end()
                            .min_w_0()
                            .child(
                                self.composer_keys(div().id("composer-panel"), false, cx)
                                    // The text field binds ↑/↓ itself; take them first only when they
                                    // start or step an edit.
                                    // Escape inside the field cancels an edit; the field would
                                    // otherwise take the key.
                                    .capture_action(cx.listener(
                                        |this,
                                         _: &gpui_kit::component::input::Escape,
                                         window,
                                         cx| {
                                            if this
                                                .edit_session
                                                .as_ref()
                                                .is_some_and(|session| !session.in_thread)
                                            {
                                                this.end_edit(window, cx);
                                                cx.stop_propagation();
                                            } else if this.open_reply().is_some() {
                                                this.cancel_reply(cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                                    .capture_action(cx.listener(
                                        |this,
                                         _: &gpui_kit::component::input::MoveUp,
                                         window,
                                         cx| {
                                            if this.edit_navigation_key("up", false, window, cx) {
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                                    .capture_action(cx.listener(
                                        |this,
                                         _: &gpui_kit::component::input::MoveDown,
                                         window,
                                         cx| {
                                            if this.edit_navigation_key("down", false, window, cx) {
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                                    .min_w_0()
                                    .flex_1()
                                    .px(px(12.))
                                    .py(px(8.))
                                    .child({
                                        let composer = self.composer.clone();
                                        self.selection_bar
                                            .update(cx, |bar, cx| bar.track(&composer, cx));
                                        self.selection_bar.clone()
                                    })
                                    .child(
                                        Textarea::new(&self.composer)
                                            .context_menu(
                                                crate::ui::keyboard::composer_context_menu(
                                                    self.composer.clone(),
                                                ),
                                            )
                                            .accessibility_id("composer")
                                            .appearance(false)
                                            .w_full()
                                            .disabled(!editable)
                                            .aria_label("Message")
                                            .on_paste(move |clipboard, window, cx| {
                                                paste_target
                                                    .update(cx, |this, cx| {
                                                        this.paste_attachment(
                                                            false, clipboard, window, cx,
                                                        )
                                                    })
                                                    .unwrap_or(false)
                                            }),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .flex_none()
                                    .h(px(42.))
                                    .pr(px(6.))
                                    .gap(px(2.))
                                    .child(self.composer_more_menu(editable, cx))
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
                                                    || self.attachments.full(),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.choose_attachment(false, window, cx)
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
                                            .on_click(cx.listener(
                                                |this, event: &ClickEvent, window, cx| {
                                                    this.toggle_emoji(event.position(), window, cx)
                                                },
                                            )),
                                    )
                                    .child(
                                        Button::new("send")
                                            .primary()
                                            .h(px(26.))
                                            .w(px(30.))
                                            .ml(px(4.))
                                            .rounded(px(6.))
                                            .icon(IconName::Send)
                                            .accessibility_label(if editing {
                                                "Save edit"
                                            } else {
                                                "Send message"
                                            })
                                            .tooltip(if editing {
                                                "Save  ↩"
                                            } else {
                                                "Send  ↩"
                                            })
                                            .disabled(
                                                !ready
                                                    || (empty
                                                        && self.attachments.drafts.is_empty())
                                                    || self.attachments.pending > 0
                                                    || self.sending.contains(&room.id),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.send(window, cx)
                                            })),
                                    ),
                            ),
                    ),
            )
                });

        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(cx.theme().background)
            .child(header)
            .children(call_banner)
            .children(self.offline_banner(cx))
            .when(self.show_security_notice(), |view| {
                view.child(self.security_notice(cx))
            })
            .child(body)
            .when(!voice_channel, |view| view.child(composer))
            .on_drop::<ExternalPaths>(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                if this
                    .navigation
                    .selected_room
                    .as_ref()
                    .is_some_and(|room| this.is_voice_channel(room))
                {
                    return;
                }
                this.add_attachments(false, paths.0.iter().cloned(), window, cx);
            }))
            .into_any_element()
    }

    /// The people whose read receipt is on the latest message, in groups and
    /// direct messages alike. None until someone besides you has read it.
    fn seen_by_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.snapshot.timeline_ready {
            return None;
        }
        let latest = self
            .snapshot
            .messages
            .iter()
            .rev()
            .find(|message| message.event_id.is_some())?;
        let names: Vec<String> = latest
            .read_by
            .iter()
            .map(|user| self.reader_name(user))
            .collect();
        let text = model::seen_by_line(&names)?;
        // Clicking the line lists everyone who has read it.
        Some(
            // Left-aligned under the composer's edge, like the typing line.
            h_flex()
                .id("seen-by")
                .w_full()
                .justify_start()
                .min_w_0()
                .child(
                    self.read_by_popover(
                        "latest",
                        &latest.read_by,
                        h_flex()
                            .gap(px(6.))
                            .items_center()
                            .min_w_0()
                            .child(Icon::new(IconName::CheckCheck).size(px(12.)))
                            .child(div().min_w_0().truncate().child(text.clone())),
                        format!("{text}. Show everyone who has read it"),
                        cx,
                    ),
                )
                .test_support()
                .into_any_element(),
        )
    }

    /// A slim strip under the header while there is no connection. The messages
    /// below are the saved ones; anything you send is queued and goes out when
    /// the connection returns.
    fn offline_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.snapshot.offline && !self.snapshot.no_network {
            return None;
        }
        let palette = theme::palette(cx);
        let text = "No internet connection. Showing saved messages.";
        Some(motion::rise(
            h_flex()
                .id("offline-banner")
                .role(Role::Status)
                .aria_label(text)
                .test_support()
                .h(px(28.))
                .flex_none()
                .px(px(16.))
                .gap(px(8.))
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(palette.warning_background)
                .text_size(px(12.))
                .child(
                    Icon::new(IconName::WifiOff)
                        .size(px(13.))
                        .text_color(palette.warning),
                )
                .child(div().flex_1().min_w_0().truncate().child(text)),
            "offline-banner-motion",
            self.snapshot.reduce_motion,
        ))
    }

    /// Sample for `--preview-offline`. Never shown with a live account.
    pub(crate) fn preview_offline(&mut self) {
        self.snapshot.offline = true;
        self.snapshot.sync_status = "Offline · reconnecting…".into();
    }

    /// The shown room's loading state, as the view presents it.
    pub(crate) fn room_load(&self, room: &str) -> RoomLoad {
        if self.preview {
            return RoomLoad::Ready;
        }
        let current = self.snapshot.selected_room == room;
        match self.snapshot.timeline_phase {
            TimelineStatus::Failed if current => return RoomLoad::Failed,
            TimelineStatus::Unavailable if current => return RoomLoad::Unavailable,
            TimelineStatus::Retrying if current => return RoomLoad::Slow,
            _ => {}
        }
        if current && self.snapshot.timeline_ready {
            RoomLoad::Ready
        } else if self
            .room_loading
            .as_ref()
            .is_some_and(|(id, since, _)| id == room && since.elapsed() >= LOAD_LIMIT)
        {
            RoomLoad::Slow
        } else {
            RoomLoad::Loading
        }
    }

    /// Starts the load clock when a room begins loading, and wakes the view
    /// when the limit passes.
    pub(crate) fn sync_room_loading(&mut self, cx: &mut Context<Self>) {
        let loading = self.navigation.selected_room.clone().filter(|room| {
            !self.preview && (self.snapshot.selected_room != *room || !self.snapshot.timeline_ready)
        });
        let Some(room) = loading else {
            self.room_loading = None;
            return;
        };
        if self
            .room_loading
            .as_ref()
            .is_some_and(|(id, _, _)| *id == room)
        {
            return;
        }
        let wake = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LOAD_LIMIT).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        });
        self.room_loading = Some((room, std::time::Instant::now(), wake));
    }

    fn room_load_error(&self, load: RoomLoad, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let (icon, title, detail) = match load {
            RoomLoad::Unavailable => (
                IconName::CircleSlash,
                "This conversation isn't available",
                "You may have left it, or it no longer exists.",
            ),
            RoomLoad::Failed => (
                IconName::TriangleAlert,
                "Couldn't load messages",
                "Something went wrong while opening this conversation.",
            ),
            _ => (
                IconName::WifiOff,
                "Messages are taking longer than usual",
                "Twayblade keeps trying in the background. Check your connection, or try again now.",
            ),
        };
        v_flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .px(px(24.))
            .child(Icon::new(icon).size(px(28.)).text_color(muted))
            .child(
                div()
                    .pt(px(4.))
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .max_w(px(360.))
                    .text_center()
                    .text_color(muted)
                    .child(detail),
            )
            .child(
                Button::new("room-retry")
                    .icon(IconName::RefreshCw)
                    .outline()
                    .mt(px(8.))
                    .h(px(28.))
                    .px(px(12.))
                    .label("Try Again")
                    .on_click(cx.listener(|this, _, window, cx| this.retry_room(window, cx))),
            )
            .into_any_element()
    }
}
