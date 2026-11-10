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
        message_scroller::{MessageScroller, MessageScrollerState},
        *,
    },
    *,
};
use std::ops::Range;

pub(super) struct ThreadPanel {
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

    pub(super) fn open_thread_event(
        &mut self,
        root: String,
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
        self.open_thread_root(row, window, cx);
    }

    fn open_thread_root(&mut self, root: Message, window: &mut Window, cx: &mut Context<Self>) {
        let Some(room) = self.navigation.selected_room.clone() else {
            return;
        };
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
                move |this, _, event: &InputEvent, window, cx| {
                    if this.thread.as_ref().is_some_and(|t| {
                        t.room == target.0 && t.root.event_id.as_ref() == Some(&target.1)
                    }) {
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
        });
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
        if self.preview {
            return;
        }
        let Some(t) = &mut self.thread else {
            return;
        };
        let body = t.composer.read(cx).value().to_string();
        if !t.ready || t.sending || body.trim().is_empty() {
            return;
        }
        t.sending = true;
        let input = t.composer.clone();
        let sent = body.clone();
        let room = t.room.clone();
        let root = t.root.event_id.clone().expect("remote root");
        let request = t.request;
        let account = self.snapshot.account_instance;
        self.job(
            window,
            cx,
            move |b| b.thread_action(room, root, account, request, ThreadAction::Send(body)),
            move |this, result, window, cx| {
                if this.snapshot.account_instance != account {
                    return;
                }
                if result.is_ok() && input.read(cx).value().as_str() == sent {
                    input.update(cx, |s, cx| s.set_value("", window, cx));
                }
                if let Some(t) = &mut this.thread
                    && t.request == request
                {
                    t.sending = false;
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
            .w(px(360.))
            .max_w(relative(0.36))
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
                MessageScroller::new("thread-messages", t.scroller.clone(), move |ix, _, cx| {
                    weak.update(cx, |this, cx| {
                        let t = this.thread.as_ref().expect("thread scroller");
                        this.message_group_from(ix, &t.messages, &t.groups, true, cx)
                    })
                    .unwrap_or_else(|_| div().into_any_element())
                })
                .flex_1()
                .min_h_0()
                .with_row_style(StyleRefinement::default().px_0().pb_0()),
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
                div().px(px(12.)).pt(px(4.)).pb(px(12.)).child(
                    h_flex()
                        .items_end()
                        .gap(px(2.))
                        .min_h(px(32.))
                        .py(px(3.))
                        .pl(px(10.))
                        .pr(px(3.))
                        .rounded(px(16.))
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(crate::theme::palette(cx).field)
                        .child(
                            div()
                                .id("thread-composer-panel")
                                .flex_1()
                                .min_w_0()
                                .py(px(1.))
                                .child(
                                    Textarea::new(&t.composer)
                                        .accessibility_id("thread-composer")
                                        .aria_label("Reply in thread")
                                        .appearance(false)
                                        .w_full()
                                        .disabled(!t.ready),
                                ),
                        )
                        .child(
                            Button::new("thread-send")
                                .primary()
                                .xsmall()
                                .rounded(px(12.))
                                .icon(assets::IconName::ArrowUp)
                                .accessibility_label("Send thread reply")
                                .tooltip("Send Reply  ↩")
                                .disabled(self.preview || !t.ready || empty || t.sending)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.send_thread(window, cx)),
                                ),
                        ),
                ),
            )
            .into_any_element()
    }
}
