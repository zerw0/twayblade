//! Message rows and groups in every chat style.
use super::*;

impl MatrixApp {
    pub(crate) fn message_body(
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
                            .and_then(|bytes| crate::ui::attachments::render_png(bytes, cx));
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

    pub(crate) fn message_group(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        self.message_group_from(ix, &self.snapshot.messages, &self.groups, false, cx)
    }

    pub(crate) fn message_group_from(
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
}
