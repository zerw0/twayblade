//! Selection-scoped thread presentation; Matrix state remains in core.
use crate::theme::scaled_px as px;
use crate::{
    app::MatrixApp,
    backend::{Message, ThreadAction, ThreadSnapshot, TimelineAction},
    model,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        input::{InputEvent, Textarea, TextareaState},
        *,
    },
    *,
};
use std::ops::Range;

use super::message_scroller::{MessageScroller, MessageScrollerState};

pub(crate) struct ThreadPanel {
    pub room: String,
    pub root: Message,
    pub request: u64,
    pub messages: Vec<Message>,
    pub groups: Vec<Range<usize>>,
    pub group_signatures: Vec<u64>,
    pub scroller: Entity<MessageScrollerState>,
    pub composer: Entity<TextareaState>,
    pub ready: bool,
    pub status: String,
    pub unread: u64,
    pub sending: bool,
    pub paginating: bool,
    pub exhausted: bool,
    pub receipt: Option<String>,
    pub receipt_retry_after: std::time::Instant,
    /// Files stacked in the reply field, sent with the next reply.
    pub attachments: crate::ui::attachments::AttachmentStack,
}

impl MatrixApp {
    pub fn open_thread(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self
            .snapshot
            .messages
            .iter()
            .find(|m| m.id == id && m.event_id.is_some())
            .cloned()
        else {
            return;
        };
        self.open_thread_root(root, window, cx);
    }

    pub(crate) fn open_thread_event(
        &mut self,
        root: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_thread_event_inner(root, true, window, cx);
    }

    pub(crate) fn open_thread_event_inner(
        &mut self,
        root: String,
        record_history: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let row = self
            .snapshot
            .messages
            .iter()
            .find(|message| message.event_id.as_ref() == Some(&root))
            .cloned()
            .unwrap_or_else(|| Message {
                id: root.clone(),
                event_id: Some(root),
                ..Default::default()
            });
        self.open_thread_root_inner(row, record_history, window, cx);
    }

    fn open_thread_root(&mut self, root: Message, window: &mut Window, cx: &mut Context<Self>) {
        self.open_thread_root_inner(root, true, window, cx);
    }

    fn open_thread_root_inner(
        &mut self,
        root: Message,
        record_history: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(room) = self.navigation.selected_room.clone() else {
            return;
        };
        let before = self.current_nav_page();
        self.pending_history_thread = None;
        self.management = None;
        self.search_open = false;
        self.close_profile(cx);
        self.details_open = false;
        self.thread_request = self.thread_request.wrapping_add(1);
        let request = self.thread_request;
        let root_id = root.event_id.clone().expect("remote root");
        let key = (room.clone(), root_id.clone());
        let composer = if let Some((input, _)) = self.thread_drafts.get(&key) {
            input.clone()
        } else {
            let input = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Reply in thread")
                    .auto_grow(1, 5)
                    .submit_on_enter(true)
            });
            let target = key.clone();
            let subscription = cx.subscribe_in(
                &input,
                window,
                move |this, state, event: &InputEvent, window, cx| {
                    if this.thread.as_ref().is_some_and(|t| {
                        t.room == target.0 && t.root.event_id.as_ref() == Some(&target.1)
                    }) {
                        if matches!(event, InputEvent::Change) {
                            let text = state.read(cx).value().to_string();
                            this.edit_text_changed(true, &text);
                        }
                        if matches!(event, InputEvent::PressEnter { .. }) {
                            this.send_thread(window, cx);
                        }
                        cx.notify();
                    }
                },
            );
            self.thread_drafts
                .insert(key, (input.clone(), subscription));
            input
        };
        let messages = if self.preview {
            self.snapshot
                .messages
                .iter()
                .skip(1)
                .take(4)
                .cloned()
                .enumerate()
                .map(|(ix, mut m)| {
                    m.body = [
                        "The native layout is ready to review.",
                        "The compact spacing works well here.",
                        "I’ll check keyboard navigation next.",
                        "Thanks, I’ll review the screenshots.",
                    ][ix]
                        .into();
                    m.reply = None;
                    m.thread_replies = 0;
                    m.new_messages = false;
                    m.kind = crate::backend::MessageKind::Text;
                    m.attachment = None;
                    m.delivery = crate::backend::DeliveryState::Remote;
                    m
                })
                .collect()
        } else {
            Vec::new()
        };
        let groups = model::grouped(&messages);
        let scroller = cx.new(|cx| MessageScrollerState::new(groups.len(), cx));
        let group_signatures = groups
            .iter()
            .map(|group| model::group_signature(&messages, group.clone()))
            .collect();
        self.thread = Some(ThreadPanel {
            room: room.clone(),
            root,
            request,
            messages,
            group_signatures,
            groups,
            scroller,
            composer,
            ready: self.preview,
            status: if self.preview {
                "Sample data · Thread preview".into()
            } else {
                "Loading thread…".into()
            },
            unread: 0,
            sending: false,
            paginating: false,
            exhausted: false,
            receipt: None,
            receipt_retry_after: std::time::Instant::now(),
            attachments: Default::default(),
        });
        if record_history {
            self.record_history(before);
        }
        if !self.preview {
            let account = self.snapshot.account_instance;
            self.job(
                window,
                cx,
                move |b| b.select_thread(room, Some(root_id), account, request),
                move |this, result, _, _| {
                    if let Some(t) = &mut this.thread
                        && t.request == request
                        && let Err(e) = result
                    {
                        t.status = e.to_string();
                    }
                },
            );
        }
        if let Some(t) = &self.thread {
            t.composer.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    pub fn close_thread(&mut self, cx: &mut Context<Self>) {
        if self
            .edit_session
            .as_ref()
            .is_some_and(|session| session.in_thread)
            && let Some(session) = self.edit_session.take()
        {
            if let Some(backend) = &self.backend {
                backend.cancel_edit();
            }
            if let Some(thread) = &self.thread {
                self.edit_restore = Some((thread.composer.clone(), session.draft));
            }
        }
        let Some(thread) = self.thread.take() else {
            return;
        };
        self.thread_request = self.thread_request.wrapping_add(1);
        if !self.preview
            && let Some(backend) = self.backend.clone()
        {
            let account = self.snapshot.account_instance;
            let request = self.thread_request;
            cx.background_spawn(async move {
                let _ = backend.select_thread(thread.room, None, account, request);
            })
            .detach();
        }
        cx.notify();
    }

    pub fn update_thread(&mut self, snapshot: Option<&ThreadSnapshot>, cx: &mut Context<Self>) {
        let Some(snapshot) = snapshot else {
            return;
        };
        let Some(thread) = &mut self.thread else {
            return;
        };
        if thread.request != snapshot.request
            || thread.room != snapshot.room
            || thread.root.event_id.as_ref() != Some(&snapshot.root)
        {
            return;
        }
        let mut messages = snapshot.messages.clone();
        if let Some(ix) = messages
            .iter()
            .position(|m| m.event_id.as_ref() == Some(&snapshot.root))
        {
            thread.root = messages.remove(ix);
        }
        let groups = model::grouped(&messages);
        let old: Vec<_> = thread
            .groups
            .iter()
            .map(|g| &thread.messages[g.start].id)
            .collect();
        let new: Vec<_> = groups.iter().map(|g| &messages[g.start].id).collect();
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let suffix = old[prefix..]
            .iter()
            .rev()
            .zip(new[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let signatures: Vec<_> = groups
            .iter()
            .map(|group| model::group_signature(&messages, group.clone()))
            .collect();
        thread.scroller.update(cx, |s, cx| {
            s.splice(prefix..old.len() - suffix, new.len() - prefix - suffix, cx);
            let mut remeasure = |new_index: usize, old_index: usize| {
                if thread.group_signatures.get(old_index) != signatures.get(new_index) {
                    s.remeasure_items(new_index..new_index + 1, cx);
                }
            };
            for index in 0..prefix {
                remeasure(index, index);
            }
            for index in 0..suffix {
                remeasure(new.len() - suffix + index, old.len() - suffix + index);
            }
        });
        thread.group_signatures = signatures;
        thread.messages = messages;
        thread.groups = groups;
        thread.ready = snapshot.ready;
        thread.unread = snapshot.unread;
        thread.status = crate::backend::timeline_status_text(snapshot.status).into();
    }

    pub fn message_action(
        &mut self,
        in_thread: bool,
        id: String,
        action: TimelineAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if in_thread {
            self.run_thread_action(ThreadAction::Timeline(action), window, cx);
        } else {
            self.timeline_action(id, action, window, cx);
        }
    }

    fn run_thread_action(
        &mut self,
        action: ThreadAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview {
            return;
        }
        let Some(t) = &self.thread else {
            return;
        };
        if !t.ready {
            return;
        }
        let room = t.room.clone();
        let root = t.root.event_id.clone().expect("remote root");
        let request = t.request;
        let account = self.snapshot.account_instance;
        let pagination = matches!(action, ThreadAction::Older);
        self.job(
            window,
            cx,
            move |b| b.thread_action(room, root, account, request, action),
            move |this, result, _, _| {
                if let Some(t) = &mut this.thread
                    && t.request == request
                {
                    if pagination {
                        t.paginating = false;
                    }
                    match result {
                        Ok(exhausted) => t.exhausted |= exhausted,
                        Err(e) => {
                            t.status = e.to_string();
                            t.receipt = None;
                            t.receipt_retry_after =
                                std::time::Instant::now() + std::time::Duration::from_secs(10);
                        }
                    }
                }
            },
        );
    }

    pub fn send_thread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.save_edit_from_composer(true, window, cx) {
            return;
        }
        if self.preview {
            return;
        }
        let Some(t) = &mut self.thread else {
            return;
        };
        let body = t.composer.read(cx).value().to_string();
        let has_text = !body.trim().is_empty();
        // Wait for every stacked attachment so none is left behind.
        if !t.ready
            || t.sending
            || (!has_text && t.attachments.drafts.is_empty())
            || t.attachments.pending > 0
        {
            return;
        }
        t.sending = true;
        let input = t.composer.clone();
        let sent = body.clone();
        let room = t.room.clone();
        let root = t.root.event_id.clone().expect("remote root");
        let request = t.request;
        let staged = std::mem::take(&mut t.attachments.drafts);
        let stack_request = t.attachments.request;
        let account = self.snapshot.account_instance;
        self.job(
            window,
            cx,
            move |b| {
                // Attachments first, in order, then the text; stop at the first failure.
                let mut staged = staged.into_iter();
                while let Some(item) = staged.next() {
                    if let Err(error) = b.thread_action(
                        room.clone(),
                        root.clone(),
                        account,
                        request,
                        ThreadAction::Attach(item.draft),
                    ) {
                        return (Err(error), staged.collect::<Vec<_>>());
                    }
                }
                let result = if has_text {
                    b.thread_action(room, root, account, request, ThreadAction::Send(body))
                } else {
                    Ok(false)
                };
                (result, Vec::new())
            },
            move |this, (result, unsent), window, cx| {
                if this.snapshot.account_instance != account {
                    return;
                }
                if has_text && result.is_ok() && input.read(cx).value().as_str() == sent {
                    input.update(cx, |s, cx| s.set_value("", window, cx));
                }
                if let Some(t) = &mut this.thread
                    && t.request == request
                {
                    t.sending = false;
                    // Put unsent attachments back in the same thread.
                    if !unsent.is_empty() && t.attachments.request == stack_request {
                        let mut restored = unsent;
                        restored.append(&mut t.attachments.drafts);
                        t.attachments.drafts = restored;
                    }
                    if let Err(e) = result {
                        t.status = e.to_string();
                    }
                }
            },
        );
        cx.notify();
    }

    pub fn thread_visibility(
        &mut self,
        request: Option<u64>,
        event: String,
        account: u128,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview
            || account != self.snapshot.account_instance
            || !visible
            || !window.is_window_active()
        {
            return;
        }
        let Some(t) = &mut self.thread else {
            return;
        };
        if Some(t.request) != request
            || t.receipt.as_ref() == Some(&event)
            || std::time::Instant::now() < t.receipt_retry_after
        {
            return;
        }
        t.receipt = Some(event.clone());
        self.run_thread_action(
            ThreadAction::Timeline(TimelineAction::Read { event_id: event }),
            window,
            cx,
        );
    }

    pub fn thread_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.thread.as_ref().expect("open thread");
        let weak = cx.weak_entity();
        let empty = t.composer.read(cx).value().trim().is_empty();
        let mut root = t.root.clone();
        root.thread_replies = 0;
        root.thread_unread = 0;
        v_flex()
            // 360 px, never wider than the panel region, which caps it against
            // the window. A percentage here would be of the region instead.
            .w(px(360.))
            .max_w_full()
            .h_full()
            .flex_none()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(self.panel_header("Thread", "close-thread", cx))
            .child(
                v_flex()
                    .id("thread-root")
                    .flex_none()
                    .max_h(px(180.))
                    .overflow_y_scroll()
                    .px(px(12.))
                    .py(px(10.))
                    .gap(px(2.))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .pl(px(10.))
                            .text_size(px(11.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().muted_foreground)
                            .child(root.display_name.clone()),
                    )
                    .child(self.message_body(
                        &root,
                        root.sender == self.snapshot.user_id,
                        true,
                        cx,
                    )),
            )
            .when(t.unread > 0, |v| {
                v.child(
                    div()
                        .px(px(12.))
                        .text_color(cx.theme().link)
                        .child(format!("{} unread", t.unread)),
                )
            })
            .when(!t.exhausted && !t.messages.is_empty(), |v| {
                v.child(
                    Button::new("thread-older")
                        .text()
                        .xsmall()
                        .mx_auto()
                        .my(px(4.))
                        .text_color(cx.theme().muted_foreground)
                        .label("Load earlier replies")
                        .disabled(t.paginating || self.preview)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(t) = &mut this.thread {
                                t.paginating = true;
                            }
                            this.run_thread_action(ThreadAction::Older, window, cx);
                        })),
                )
            })
            .child(
                v_flex()
                    .id("thread-area")
                    .track_focus(&self.thread_area)
                    .flex_1()
                    .min_h_0()
                    .child(
                        MessageScroller::new(
                            "thread-messages",
                            t.scroller.clone(),
                            move |ix, _, cx| {
                                weak.update(cx, |this, cx| {
                                    let t = this.thread.as_ref().expect("thread scroller");
                                    this.message_group_from(ix, &t.messages, &t.groups, true, cx)
                                })
                                .unwrap_or_else(|_| div().into_any_element())
                            },
                        )
                        .flex_1()
                        .min_h_0()
                        .with_row_style(StyleRefinement::default().px_0().pb_0()),
                    ),
            )
            .when(!t.status.is_empty(), |v| {
                v.child(
                    div()
                        .px(px(14.))
                        .pt(px(4.))
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child(t.status.clone()),
                )
            })
            // Same field as the main composer: rounded, bordered, send inside.
            .child(
                div()
                    .px(px(12.))
                    .pt(px(4.))
                    .pb(px(12.))
                    .children(self.edit_bar(true, cx))
                    .child(
                        // The main composer's box: the field, then a toolbar
                        // row with the send button at the same size.
                        v_flex()
                            .rounded(px(9.))
                            .border_1()
                            .border_color(cx.theme().input)
                            .bg(crate::theme::palette(cx).field)
                            .when(!t.attachments.is_empty(), |field| {
                                field.child(self.attachment_tray(true, cx))
                            })
                            .child(
                                self.composer_keys(div().id("thread-composer-panel"), true, cx)
                                    // ↑/↓ start or step an edit before the
                                    // text field moves its cursor.
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
                                                .is_some_and(|session| session.in_thread)
                                            {
                                                this.end_edit(window, cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                                    .capture_action(cx.listener(
                                        |this,
                                         _: &gpui_kit::component::input::MoveUp,
                                         window,
                                         cx| {
                                            if this.edit_navigation_key("up", true, window, cx) {
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                                    .capture_action(cx.listener(
                                        |this,
                                         _: &gpui_kit::component::input::MoveDown,
                                         window,
                                         cx| {
                                            if this.edit_navigation_key("down", true, window, cx) {
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                                    .min_w_0()
                                    .px(px(12.))
                                    .pt(px(8.))
                                    .pb(px(2.))
                                    .child({
                                        let composer = t.composer.clone();
                                        self.thread_selection_bar
                                            .update(cx, |bar, cx| bar.track(&composer, cx));
                                        self.thread_selection_bar.clone()
                                    })
                                    .child(
                                        Textarea::new(&t.composer)
                                            .context_menu(
                                                crate::ui::keyboard::composer_context_menu(
                                                    t.composer.clone(),
                                                ),
                                            )
                                            .accessibility_id("thread-composer")
                                            .aria_label("Reply in thread")
                                            .appearance(false)
                                            .w_full()
                                            .disabled(!t.ready)
                                            .on_paste({
                                                let paste_target = cx.weak_entity();
                                                move |clipboard, window, cx| {
                                                    paste_target
                                                        .update(cx, |this, cx| {
                                                            this.paste_attachment(
                                                                true, clipboard, window, cx,
                                                            )
                                                        })
                                                        .unwrap_or(false)
                                                }
                                            }),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .h(px(34.))
                                    .px(px(6.))
                                    .gap(px(2.))
                                    .items_center()
                                    .child(div().flex_1())
                                    .child(
                                        Button::new("thread-attach-file")
                                            .ghost()
                                            .size(px(26.))
                                            .icon(assets::IconName::Paperclip)
                                            .accessibility_label("Attach file")
                                            .tooltip("Attach File…")
                                            .disabled(
                                                self.preview || !t.ready || t.attachments.full(),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.choose_attachment(true, window, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("thread-send")
                                            .primary()
                                            .h(px(26.))
                                            .w(px(30.))
                                            .rounded(px(6.))
                                            .icon(assets::IconName::Send)
                                            .accessibility_label("Send thread reply")
                                            .tooltip("Send Reply  ↩")
                                            .disabled(
                                                self.preview
                                                    || !t.ready
                                                    || (empty && t.attachments.drafts.is_empty())
                                                    || t.attachments.pending > 0
                                                    || t.sending,
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.send_thread(window, cx)
                                            })),
                                    ),
                            ),
                    ),
            )
            // Files dropped on the thread go with the reply, not the main message.
            .on_drop::<ExternalPaths>(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.add_attachments(true, paths.0.iter().cloned(), window, cx);
                cx.stop_propagation();
            }))
            .into_any_element()
    }
}
