//! Message rows and groups in every chat style.
use super::*;

/// Sticker size in the timeline, in points.
const STICKER: f32 = 160.;
/// Width of the leading time column ("23:59" at 11 pt) and the gap after it.
const TIME_COLUMN: f32 = 38.;
const TIME_COLUMN_GAP: f32 = 10.;

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
        // Only the cursor message's controls are Tab stops (see `ui::focus`);
        // its hover actions also show while it has keyboard focus.
        let stops = self.message_tab_stops(in_thread, &message.id);
        let show_actions = self.message_cursor_focused(in_thread, &message.id);
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
                    // A few emoji on their own read as a reaction: show them large.
                    let base = f32::from(self.preferences.appearance.message_size);
                    let emoji_size = (message.kind == MessageKind::Text
                        && message.attachment.is_none())
                    .then(|| model::emoji_only_count(&message.body))
                    .flatten()
                    .map(|count| if count <= 3 { base * 2.6 } else { base * 1.7 });
                    // An image shows the picture alone: its filename is in the
                    // accessible label and the viewer, and only a caption is text.
                    let image_text =
                        message.kind != MessageKind::Image || !message.body.trim().is_empty();
                    content = content.when(image_text, |content| {
                        content.child(
                            div()
                                .font_family(message_font)
                                .text_size(px(emoji_size.unwrap_or(base)))
                                .child(
                                    TextView::markdown(
                                        SharedString::from(format!("message-{}", message.id)),
                                        match &message.rich {
                                            Some(rich) => model::rich_markdown(rich),
                                            _ if message.kind == MessageKind::Image => {
                                                model::plain_text(&message.body)
                                            }
                                            _ => model::plain_text(&model::message_text(message)),
                                        },
                                    )
                                    .selectable(true)
                                    // Show the real destination; a label can disguise it.
                                    .link_tooltip(|url, window, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(url.clone())
                                            .build(window, cx)
                                    })
                                    .on_link_click({
                                        let weak = cx.weak_entity();
                                        move |url, _, window, cx| {
                                            // A mention opens the person's profile beside the chat.
                                            if let Some(user) = model::mentioned_user(url) {
                                                let _ = weak.update(cx, |this, cx| {
                                                    let name = this
                                                        .snapshot
                                                        .members
                                                        .iter()
                                                        .find(|member| member.user_id == user)
                                                        .map(|member| member.display_name.clone())
                                                        .filter(|name| !name.is_empty())
                                                        .unwrap_or_else(|| user.clone());
                                                    this.open_profile(user, name, window, cx);
                                                });
                                                return;
                                            }
                                            if url::Url::parse(url).is_ok_and(|url| {
                                                matches!(url.scheme(), "https" | "http" | "mailto")
                                            }) {
                                                cx.open_url(url);
                                            }
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
                        )
                    });
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
                            let focus = self.image_focus(&id, cx).tab_stop(stops);
                            let mut button = Button::new(SharedString::from(format!("image-{id}")))
                                .ghost()
                                .text_color(if outgoing {
                                    cx.theme().primary_foreground
                                } else {
                                    cx.theme().foreground
                                })
                                .p_0()
                                .track_focus(&focus)
                                .tab_stop(stops)
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
                                // Keep the picture's shape inside the box for the
                                // chosen size (Settings › Chat › Images).
                                let (max_w, max_h) = match self.preferences.chat.preview_size {
                                    twayblade_core::conversation::PreviewSize::Default => {
                                        (280., 180.)
                                    }
                                    twayblade_core::conversation::PreviewSize::Large => {
                                        (560., 420.)
                                    }
                                };
                                let size = image.size(0);
                                let (w, h) = (
                                    (size.width.0 as f32).max(1.),
                                    (size.height.0 as f32).max(1.),
                                );
                                let scale = (max_w / w).min(max_h / h);
                                button =
                                    button.w(px(w * scale)).h(px(h * scale)).max_w_full().child(
                                        img(image).size_full().object_fit(ObjectFit::Contain),
                                    );
                            } else if attachment.download_needed {
                                // Automatic download is off here (#174).
                                let load = message.id.clone();
                                button = button
                                    .small()
                                    .icon(IconName::Download)
                                    .label("Load Image")
                                    .accessibility_label(format!(
                                        "Load image {}",
                                        attachment.filename
                                    ))
                                    .tooltip("Load Image")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.load_image(load.clone(), cx)
                                    }));
                            } else {
                                // Still downloading or not previewable: hold the
                                // picture's place rather than naming the file.
                                let (w, h) = match self.preferences.chat.preview_size {
                                    twayblade_core::conversation::PreviewSize::Default => {
                                        (280., 180.)
                                    }
                                    twayblade_core::conversation::PreviewSize::Large => {
                                        (560., 420.)
                                    }
                                };
                                button = button
                                    .w(px(w))
                                    .h(px(h))
                                    .max_w_full()
                                    .rounded(px(8.))
                                    .bg(cx.theme().secondary)
                                    .child(
                                        Icon::new(IconName::Image)
                                            .size(px(24.))
                                            .text_color(cx.theme().muted_foreground),
                                    );
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
                                            .tab_stop(stops)
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
                                            .tab_stop(stops)
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
                                            .tab_stop(stops)
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
            MessageKind::Membership => {
                return self.membership_line(message, in_thread, cx);
            }
            // A sticker shows as itself, without a bubble.
            MessageKind::Sticker => {
                let picture = message
                    .attachment
                    .as_ref()
                    .and_then(|a| a.preview_png.as_ref())
                    .and_then(|bytes| crate::ui::attachments::render_png(bytes, cx));
                match picture {
                    Some(image) => div()
                        .size(px(STICKER))
                        .child(img(image).size_full().object_fit(ObjectFit::Contain))
                        .into_any_element(),
                    None => {
                        // Held back by data saving (#174): load on request.
                        let held = message
                            .attachment
                            .as_ref()
                            .is_some_and(|a| a.download_needed);
                        let load = message.id.clone();
                        div()
                            .size(px(STICKER))
                            .rounded(px(12.))
                            .bg(cx.theme().secondary)
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(held, |tile| {
                                tile.child(
                                    Button::new(SharedString::from(format!(
                                        "sticker-load-{}",
                                        message.id
                                    )))
                                    .small()
                                    .icon(IconName::Download)
                                    .label("Load Sticker")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.load_image(load.clone(), cx)
                                    })),
                                )
                            })
                            .into_any_element()
                    }
                }
            }
            MessageKind::Voice => match &message.voice {
                Some(voice) => {
                    let disabled =
                        self.preview || self.backend.is_none() || !self.snapshot.timeline_ready;
                    self.voice_bubble(message, voice, outgoing, disabled, cx)
                }
                None => div().into_any_element(),
            },
            MessageKind::Poll => match &message.poll {
                Some(poll) => {
                    let disabled =
                        self.preview || self.backend.is_none() || !self.snapshot.timeline_ready;
                    self.poll_card(message, poll, disabled, cx)
                }
                None => div().into_any_element(),
            },
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
        let row_focus = self
            .message_focus(in_thread, &message.id, cx)
            .tab_stop(self.message_cursor(in_thread) == Some(message.id.as_str()));
        let key_id = message.id.clone();
        let click_id = message.id.clone();
        let key_focus = row_focus.clone();
        let mut row = v_flex()
            .id(SharedString::from(format!("message-row-{}", message.id)))
            // Message text is drawn, not exposed; screen readers read this label.
            .role(Role::Group)
            .aria_label(accessible_message(message, &self.snapshot.user_id))
            .track_focus(&row_focus)
            .rounded(px(8.))
            .focus_visible(|style| style.bg(palette.selection))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.set_message_cursor(in_thread, click_id.clone(), cx)
                }),
            )
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                // Only on the row itself; its buttons keep their own keys.
                if !key_focus.is_focused(window) || event.keystroke.modifiers.modified() {
                    return;
                }
                let step = match event.keystroke.key.as_str() {
                    "up" => crate::ui::focus::Step::Previous,
                    "down" => crate::ui::focus::Step::Next,
                    "home" => crate::ui::focus::Step::First,
                    "end" => crate::ui::focus::Step::Last,
                    _ => return,
                };
                this.step_message(in_thread, &key_id, step, window, cx);
                cx.stop_propagation();
            }))
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
        // The action buttons exist only for the message under the pointer or
        // with keyboard focus, so screen readers do not step through four
        // buttons per message (#156); elsewhere a same-size gap keeps layout.
        let active = show_actions || self.hovered_message.as_deref() == Some(message.id.as_str());
        let hover_id = message.id.clone();
        let mut line = h_flex()
            .id(SharedString::from(format!("message-line-{}", message.id)))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    if this.hovered_message.as_deref() != Some(hover_id.as_str()) {
                        this.hovered_message = Some(hover_id.clone());
                        cx.notify();
                    }
                } else if this.hovered_message.as_deref() == Some(hover_id.as_str()) {
                    this.hovered_message = None;
                    cx.notify();
                }
            }))
            .group("message")
            .max_w_full()
            .min_w_0()
            .items_center()
            .gap(px(4.))
            .when(outgoing, |line| line.flex_row_reverse());
        // Not on the server yet (queued, uploading or failed): dimmed until sent.
        line = line.child(
            div()
                .min_w_0()
                .opacity(delivery_opacity(message.delivery))
                .child(bubble),
        );
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
            })
            .when(show_actions, |bar| {
                bar.border_color(cx.theme().border)
                    .bg(cx.theme().background)
            });
        if message.event_id.is_some() || self.preview {
            let message_id = message.id.clone();
            let focus = self
                .image_focus(&format!("reaction-action-{}", message.id), cx)
                .tab_stop(stops);
            actions = actions.child(if active {
                div()
                    .flex_none()
                    .track_focus(&focus)
                    .focus(|style| style.opacity(1.))
                    .opacity(0.)
                    .group_hover("message", |style| style.opacity(1.))
                    .when(show_actions, |view| view.opacity(1.))
                    .child(
                        Button::new(SharedString::from(format!("add-reaction-{}", message.id)))
                            .track_focus(&focus)
                            .tab_stop(stops)
                            .ghost()
                            .xsmall()
                            .icon(IconName::FaceSlightlySmiling)
                            .accessibility_label("Add reaction")
                            .tooltip("Add reaction")
                            .disabled(!self.preview && disabled)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_emoji(Some(message_id.clone()), window, cx)
                            })),
                    )
                    .into_any_element()
            } else {
                div().flex_none().size_5().into_any_element()
            });
        }
        if !in_thread && message.event_id.is_some() {
            let id = message.id.clone();
            let focus = self
                .image_focus(&format!("reply-action-{id}"), cx)
                .tab_stop(stops);
            actions = actions.child(if active {
                div()
                    .track_focus(&focus)
                    .focus(|style| style.opacity(1.))
                    .opacity(0.)
                    .group_hover("message", |style| style.opacity(1.))
                    .when(show_actions, |view| view.opacity(1.))
                    .child(
                        Button::new(SharedString::from(format!("reply-to-{id}")))
                            .track_focus(&focus)
                            .tab_stop(stops)
                            .ghost()
                            .xsmall()
                            .icon(IconName::CornerUpLeft)
                            .accessibility_label("Reply")
                            .tooltip("Reply")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.start_reply(id.clone(), window, cx)
                            })),
                    )
                    .into_any_element()
            } else {
                div().flex_none().size_5().into_any_element()
            });
        }
        if message.editable
            && let Some(event_id) = message.event_id.clone()
        {
            let focus = self
                .image_focus(&format!("edit-action-{}", message.id), cx)
                .tab_stop(stops);
            actions = actions.child(if active {
                div()
                    .track_focus(&focus)
                    .focus(|style| style.opacity(1.))
                    .opacity(0.)
                    .group_hover("message", |style| style.opacity(1.))
                    .when(show_actions, |view| view.opacity(1.))
                    .child(
                        Button::new(SharedString::from(format!("edit-{}", message.id)))
                            .track_focus(&focus)
                            .tab_stop(stops)
                            .ghost()
                            .xsmall()
                            .icon(IconName::Pencil)
                            .accessibility_label("Edit message")
                            .tooltip("Edit")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.start_edit(event_id.clone(), in_thread, window, cx)
                            })),
                    )
                    .into_any_element()
            } else {
                div().flex_none().size_5().into_any_element()
            });
        }
        if !in_thread && message.event_id.is_some() {
            let id = message.id.clone();
            let focus = self
                .image_focus(&format!("thread-action-{id}"), cx)
                .tab_stop(stops);
            actions = actions.child(if active {
                div()
                    .track_focus(&focus)
                    .focus(|style| style.opacity(1.))
                    .opacity(0.)
                    .group_hover("message", |style| style.opacity(1.))
                    .when(show_actions, |view| view.opacity(1.))
                    .child(
                        Button::new(SharedString::from(format!("reply-thread-{id}")))
                            .track_focus(&focus)
                            .tab_stop(stops)
                            .ghost()
                            .xsmall()
                            .icon(IconName::MessagesSquare)
                            .accessibility_label("Reply in Thread")
                            .tooltip("Reply in Thread")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_thread(id.clone(), window, cx)
                            })),
                    )
                    .into_any_element()
            } else {
                div().flex_none().size_5().into_any_element()
            });
        }
        line = line.child(actions);
        // IRC and always-on Minimal show the time in a left column instead.
        if self.preferences.chat.style != ChatStyle::Irc && !self.time_gutter() {
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
        row = row.child(line.test_support());
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
                        .tab_stop(stops)
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
                    .tab_stop(stops)
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
                        .tab_stop(stops)
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
        if let Some(canvas) = self.latest_visibility(message, in_thread, cx) {
            row = row.relative().child(canvas);
        }
        div()
            .id(SharedString::from(format!(
                "message-context-{}",
                message.id
            )))
            .w_full()
            .child(row.test_support())
            .context_menu(self.message_menu(message, in_thread, cx))
            .into_any_element()
    }

    /// Right-click actions for one message: the hover bar's actions plus
    /// copying, in the order Messages and Slack use.
    fn message_menu(
        &self,
        message: &crate::backend::Message,
        in_thread: bool,
        cx: &mut Context<Self>,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
    + 'static {
        let weak = cx.weak_entity();
        let id = message.id.clone();
        let event_id = message.event_id.clone();
        let editable = message.editable;
        let readable = !message.body.is_empty()
            && matches!(
                message.kind,
                MessageKind::Text | MessageKind::Emote | MessageKind::Notice
            );
        let body = message.body.clone();
        let link = event_id
            .as_ref()
            .zip(self.navigation.selected_room.as_ref())
            .map(|(event, room)| format!("https://matrix.to/#/{room}/{event}"));
        let can_react = event_id.is_some() || self.preview;
        move |mut menu, _, _| {
            if can_react {
                let (weak, id) = (weak.clone(), id.clone());
                menu = menu.item(PopupMenuItem::new("Add Reaction…").on_click(
                    move |_, window, cx| {
                        let _ = weak
                            .update(cx, |this, cx| this.open_emoji(Some(id.clone()), window, cx));
                    },
                ));
            }
            if !in_thread && event_id.is_some() {
                let (reply_view, reply_id) = (weak.clone(), id.clone());
                menu = menu.item(PopupMenuItem::new("Reply").on_click(move |_, window, cx| {
                    let _ = reply_view.update(cx, |this, cx| {
                        this.start_reply(reply_id.clone(), window, cx)
                    });
                }));
                let (weak, id) = (weak.clone(), id.clone());
                menu = menu.item(PopupMenuItem::new("Reply in Thread").on_click(
                    move |_, window, cx| {
                        let _ =
                            weak.update(cx, |this, cx| this.open_thread(id.clone(), window, cx));
                    },
                ));
            }
            menu = menu.separator();
            if readable {
                let body = body.clone();
                menu = menu.item(PopupMenuItem::new("Copy Text").on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(body.clone()))
                }));
            }
            if let Some(link) = link.clone() {
                menu = menu.item(PopupMenuItem::new("Copy Link").on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(link.clone()))
                }));
            }
            if editable && let Some(event_id) = event_id.clone() {
                let (edit_view, edit_event) = (weak.clone(), event_id.clone());
                let (delete_view, delete_event) = (weak.clone(), event_id);
                menu = menu
                    .separator()
                    .item(PopupMenuItem::new("Edit").on_click(move |_, window, cx| {
                        let _ = edit_view.update(cx, |this, cx| {
                            this.start_edit(edit_event.clone(), in_thread, window, cx)
                        });
                    }))
                    .item(
                        PopupMenuItem::new("Delete…").on_click(move |_, window, cx| {
                            let _ = delete_view.update(cx, |this, cx| {
                                this.request_delete(delete_event.clone(), in_thread, window, cx)
                            });
                        }),
                    );
            }
            menu
        }
    }

    /// For the latest event: a full-size canvas that reports whether it is on
    /// screen, which sends the read receipt. None for any other row.
    fn latest_visibility(
        &self,
        message: &crate::backend::Message,
        in_thread: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
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
        if !(message.event_id.is_some() && latest.as_deref() == Some(message.id.as_str())) {
            return None;
        }
        let (Some(event_id), Some(room)) = (
            message.event_id.clone(),
            self.navigation.selected_room.clone(),
        ) else {
            return None;
        };
        {
            let weak = cx.weak_entity();
            let account = self.snapshot.account_instance;
            let request = self.thread.as_ref().map(|t| t.request);
            Some(
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
                .size_full()
                .into_any_element(),
            )
        }
    }

    /// A membership change as a quiet line, not a message (#104). It starts
    /// where message text starts in the current chat style, so the eye reads
    /// it with the conversation rather than as a centred banner.
    fn membership_line(
        &self,
        message: &crate::backend::Message,
        in_thread: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = membership_text(message, &self.snapshot.user_id);
        // Minimal and IRC drop the centred pause markers (their rows carry a
        // time), so a membership line shows its own time in passing.
        // With the time column, the time leads the line like a message's.
        let gutter = self.time_gutter();
        let time = (self.preferences.chat.style != ChatStyle::Bubbles && !gutter)
            .then(|| time_label(message.timestamp_ms));
        h_flex()
            .id(SharedString::from(format!("message-row-{}", message.id)))
            .role(Role::Group)
            .aria_label(format!("{}, {text}", time_label(message.timestamp_ms)))
            .relative()
            .w_full()
            .when(!gutter, |line| line.pl(px(self.membership_indent())))
            .py(px(1.))
            .gap(px(6.))
            .text_size(px(11.5))
            .text_color(cx.theme().muted_foreground)
            .when(gutter, |line| {
                line.child(
                    self.time_column(message.timestamp_ms, cx)
                        .mr(px(self.membership_indent() + TIME_COLUMN_GAP - 6.)),
                )
            })
            .child(div().truncate().child(text))
            .children(time.map(|time| div().flex_none().text_size(px(10.5)).child(time)))
            .children(self.latest_visibility(message, in_thread, cx))
            .test_support()
            .into_any_element()
    }

    /// Left inset of message text: the avatar gutter in Minimal, the time and
    /// name columns in IRC, the avatar plus the sender-name inset in Bubbles.
    fn membership_indent(&self) -> f32 {
        let avatars = self.preferences.chat.avatars;
        match self.preferences.chat.style {
            ChatStyle::Minimal if avatars => 32. + 10.,
            ChatStyle::Minimal => 0.,
            ChatStyle::Irc => 38. + 10. + 120. + 10.,
            ChatStyle::Bubbles if avatars => 26. + 8. + 10.,
            ChatStyle::Bubbles => 10.,
        }
    }

    /// Minimal with always-on timestamps puts every time in a left column,
    /// so the times line up instead of trailing each message (#193).
    fn time_gutter(&self) -> bool {
        self.preferences.chat.style == ChatStyle::Minimal
            && self.preferences.chat.timestamps == Timestamps::Always
    }

    /// The fixed-width time that leads a line in IRC and the Minimal time column.
    fn time_column(&self, timestamp_ms: i64, cx: &Context<Self>) -> Div {
        div()
            .w(px(TIME_COLUMN))
            .flex_none()
            .pt(px(1.))
            .text_color(cx.theme().muted_foreground)
            .text_size(px(11.))
            .child(time_label(timestamp_ms))
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
        self.note_visible_images(&messages[range.clone()], in_thread, cx);
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
        // long silence, instead of a timestamp row above every group. Only
        // Bubbles needs the pause marker: Minimal and IRC rows show their time.
        let day_marker = new_day.then(|| day_label(first.timestamp_ms));
        let marker = if new_day || self.preferences.chat.style != ChatStyle::Bubbles {
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
                    .id(SharedString::from(format!("day-{}", first.id)))
                    .role(Role::Heading)
                    .aria_label(day.clone())
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
                    .role(Role::Heading)
                    .aria_label("New messages")
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
        // Membership changes: quiet lines, no avatar or name header.
        if first.kind == MessageKind::Membership {
            for message in &messages[range] {
                group = group.child(self.membership_line(message, in_thread, cx));
            }
            return group.into_any_element();
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
                let gutter = self.time_gutter();
                // From the time column's left edge to the text: the time, its
                // gap, then the avatar and its gap when avatars show.
                let lead = TIME_COLUMN
                    + TIME_COLUMN_GAP
                    + if self.preferences.chat.avatars {
                        32. + 10.
                    } else {
                        0.
                    };
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
                    // The time column already shows when the group starts.
                    .when(!gutter, |header| {
                        header.child(
                            div()
                                .text_size(px(11.))
                                .text_color(muted)
                                .child(time_label(first.timestamp_ms)),
                        )
                    });
                let mut column = v_flex().flex_1().min_w_0().child(header);
                for message in &messages[range] {
                    let body = div()
                        .id(SharedString::from(format!("style-row-{}", message.id)))
                        .w_full()
                        .min_w_0()
                        .py(px(if compact { 0. } else { 1. }))
                        .child(self.message_body(message, false, in_thread, cx))
                        .test_support();
                    column = column.child(if gutter {
                        // Each line leads with its own time at the group's left
                        // edge, reaching back past the avatar gutter.
                        h_flex()
                            .min_w_0()
                            .items_start()
                            .ml(px(-lead))
                            .child(
                                self.time_column(message.timestamp_ms, cx)
                                    .mt(px(if compact { 0. } else { 1. })),
                            )
                            .child(div().flex_none().w(px(lead - TIME_COLUMN)))
                            .child(body)
                            .into_any_element()
                    } else {
                        body.into_any_element()
                    });
                    if let Some(label) = delivered(message) {
                        column =
                            column.child(div().text_size(px(10.5)).text_color(muted).child(label));
                    }
                }
                let mut row = h_flex()
                    .w_full()
                    .items_start()
                    .gap(px(10.))
                    .when(gutter, |row| row.pl(px(TIME_COLUMN + TIME_COLUMN_GAP)));
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
                line = line.child(self.time_column(message.timestamp_ms, cx));
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
            .tooltip(format!("Open profile of {}", first.display_name))
            // The name beside it is the one control screen readers and Tab
            // reach; the avatar repeats it for the pointer only.
            .role(gpui_kit::base::RoleOverride::Presentational)
            .tab_stop(false)
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
                        .accessibility_label(format!("Open profile of {name}"))
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

/// Full strength once the server has a message; dimmed while it waits in the
/// send queue or after it failed (#107).
pub(crate) fn delivery_opacity(delivery: DeliveryState) -> f32 {
    match delivery {
        DeliveryState::Queued | DeliveryState::Failed => 0.6,
        DeliveryState::Remote | DeliveryState::Sent => 1.0,
    }
}

/// "Alex joined", "You invited Sam", … for a membership row.
pub(crate) fn membership_text(message: &crate::backend::Message, own_user: &str) -> String {
    use twayblade_core::conversation::MembershipKind::*;
    let Some(change) = &message.membership else {
        return String::new();
    };
    let actor = if message.sender == own_user {
        "You".to_owned()
    } else {
        message.display_name.clone()
    };
    let target = if change.target == own_user {
        "you".to_owned()
    } else {
        change.target_name.clone()
    };
    // The member is also the subject of their own changes.
    let subject = if change.target == own_user {
        "You".to_owned()
    } else {
        change.target_name.clone()
    };
    match change.kind {
        Joined | InvitationAccepted => format!("{subject} joined"),
        Left => format!("{subject} left"),
        Invited => format!("{actor} invited {target}"),
        InvitationRejected => format!("{subject} declined the invitation"),
        InvitationRevoked => format!("{actor} withdrew the invitation for {target}"),
        Kicked => format!("{actor} removed {target}"),
        Banned | KickedAndBanned => format!("{actor} banned {target}"),
        Unbanned => format!("{actor} unbanned {target}"),
        Knocked => format!("{subject} asked to join"),
        KnockAccepted => format!("{actor} let {target} in"),
        KnockRetracted => format!("{subject} withdrew the request to join"),
        KnockDenied => format!("{actor} declined the request from {target}"),
    }
}

/// What a screen reader announces for one message: who, when, what, and its
/// state. Uses the same text the row shows; nothing is logged.
pub(crate) fn accessible_message(message: &crate::backend::Message, own_user: &str) -> String {
    let who = if message.sender == own_user {
        "You".to_owned()
    } else {
        message.display_name.clone()
    };
    let what = match message.kind {
        MessageKind::Undecryptable => "Unable to decrypt this message.".to_owned(),
        MessageKind::Redacted => "Message deleted".to_owned(),
        MessageKind::Image => format!("Image, {}", model::message_text(message)),
        MessageKind::File => format!("File, {}", model::message_text(message)),
        MessageKind::Sticker => format!("Sticker, {}", model::message_text(message)),
        MessageKind::Voice => format!(
            "Voice message, {}",
            crate::ui::voice::clock(std::time::Duration::from_millis(
                message.voice.as_ref().map_or(0, |v| v.duration_ms)
            ))
        ),
        MessageKind::Poll => message
            .poll
            .as_ref()
            .map(crate::ui::polls::accessible_poll)
            .unwrap_or_else(|| model::message_text(message)),
        _ => model::message_text(message),
    };
    let mut label = if message.kind == MessageKind::Emote {
        // The emote text already starts with the sender's name.
        format!("{}, {what}", time_label(message.timestamp_ms))
    } else {
        format!("{who}, {}, {what}", time_label(message.timestamp_ms))
    };
    if let Some(ReplyTarget::Ready { sender, .. }) = &message.reply {
        label.push_str(&format!(", replying to {sender}"));
    }
    if message.edited {
        label.push_str(", edited");
    }
    match message.delivery {
        DeliveryState::Failed => label.push_str(", not sent"),
        DeliveryState::Queued => label.push_str(", sending"),
        _ => {}
    }
    label
}

#[cfg(test)]
mod tests {
    use super::{
        DeliveryState, MessageKind, accessible_message, delivery_opacity, membership_text,
    };

    #[test]
    fn unsent_messages_are_dimmed() {
        assert_eq!(delivery_opacity(DeliveryState::Queued), 0.6);
        assert_eq!(delivery_opacity(DeliveryState::Failed), 0.6);
        assert_eq!(delivery_opacity(DeliveryState::Sent), 1.0);
        assert_eq!(delivery_opacity(DeliveryState::Remote), 1.0);
    }
    #[test]
    fn membership_lines_name_who_did_what() {
        use twayblade_core::conversation::{Membership, MembershipKind};
        let row = |sender: &str, name: &str, kind, target: &str, target_name: &str| {
            crate::backend::Message {
                sender: sender.into(),
                display_name: name.into(),
                kind: MessageKind::Membership,
                membership: Some(Membership {
                    kind,
                    target: target.into(),
                    target_name: target_name.into(),
                }),
                ..Default::default()
            }
        };
        let me = "@me:example.org";
        assert_eq!(
            membership_text(
                &row("@robin", "Robin", MembershipKind::Joined, "@robin", "Robin"),
                me
            ),
            "Robin joined"
        );
        assert_eq!(
            membership_text(&row(me, "Me", MembershipKind::Invited, "@sam", "Sam"), me),
            "You invited Sam"
        );
        assert_eq!(
            membership_text(&row("@alex", "Alex", MembershipKind::Kicked, me, "Me"), me),
            "Alex removed you"
        );
        assert_eq!(
            membership_text(&row(me, "Me", MembershipKind::Left, me, "Me"), me),
            "You left"
        );
    }

    #[test]
    fn screen_readers_hear_sender_content_and_state() {
        let message = |sender: &str, kind, body: &str| crate::backend::Message {
            id: "m".into(),
            sender: sender.into(),
            display_name: "Alex".into(),
            kind,
            body: body.into(),
            ..Default::default()
        };
        let text = message("@alex:test", MessageKind::Text, "Hello");
        let label = accessible_message(&text, "@you:test");
        assert!(
            label.starts_with("Alex, ") && label.ends_with(", Hello"),
            "{label}"
        );

        let mut own = message("@you:test", MessageKind::Text, "Hi");
        own.edited = true;
        own.delivery = DeliveryState::Failed;
        let label = accessible_message(&own, "@you:test");
        assert!(
            label.starts_with("You, ") && label.ends_with("Hi, edited, not sent"),
            "{label}"
        );

        let emote = message("@alex:test", MessageKind::Emote, "waves");
        assert!(accessible_message(&emote, "@you:test").ends_with(", Alex waves"));
        let locked = message("@alex:test", MessageKind::Undecryptable, "");
        assert!(
            accessible_message(&locked, "@you:test").ends_with("Unable to decrypt this message.")
        );
    }
}
