//! Two-finger swipe right on a message to reply, as in Messages and WhatsApp.
//! The message follows the fingers and a reply arrow appears in the gap; past
//! `TRIGGER` the arrow takes the accent and letting go starts the reply. The
//! message then slides back. Only precise (trackpad) scrolls count, the
//! direction is decided once per gesture so vertical scrolling is untouched,
//! and momentum after the fingers lift is ignored.
use super::*;
use std::time::{Duration, Instant};

/// Distance after which letting go replies.
const TRIGGER: f32 = 64.;
/// The message stops following the fingers here.
const MAX_OFFSET: f32 = 96.;
/// Movement before the gesture's direction is decided.
const DECIDE: f32 = 6.;
/// A gesture with no events for this long (cancelled by the system) is let go
/// without replying.
const STALE: Duration = Duration::from_millis(350);
/// Slide back after letting go.
const RETURN: Duration = Duration::from_millis(180);

#[derive(Default)]
pub(crate) struct SwipeReply {
    /// The message under an ongoing trackpad gesture.
    tracking: Option<Tracking>,
    /// The message sliding back, from where, and a key for its animation.
    returning: Option<(String, f32, u64)>,
    returns: u64,
    /// The swipeable message under the pointer (its full-width row).
    pub(crate) hovered: Option<String>,
}

struct Tracking {
    message: String,
    /// Total movement while undecided.
    moved: Point<f32>,
    /// `Some(true)` once the gesture is a horizontal swipe, `Some(false)`
    /// when it is a scroll and is left alone.
    horizontal: Option<bool>,
    offset: f32,
    last: Instant,
}

impl MatrixApp {
    /// A scroll event over the main timeline, seen in the capture phase so a
    /// horizontal swipe can be taken before the timeline's own scrolling
    /// (which claims every wheel event) sees it. The message is the one under
    /// the pointer when the gesture starts.
    pub(crate) fn swipe_scroll(
        &mut self,
        event: &ScrollEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ScrollDelta::Pixels(delta) = event.delta else {
            return;
        };
        let delta = point(f32::from(delta.x), f32::from(delta.y));
        let id = match (&self.swipe.tracking, event.touch_phase) {
            (_, TouchPhase::Started) => self.swipe.hovered.clone().filter(|id| {
                self.snapshot.messages.iter().any(|message| {
                    &message.id == id
                        && message.event_id.is_some()
                        && message.kind != MessageKind::Membership
                })
            }),
            (Some(tracking), _) => Some(tracking.message.clone()),
            (None, _) => None,
        };
        let Some(id) = id else {
            return;
        };
        let id = id.as_str();
        match event.touch_phase {
            TouchPhase::Started => {
                self.swipe.tracking = Some(Tracking {
                    message: id.to_owned(),
                    moved: delta,
                    horizontal: None,
                    offset: 0.,
                    last: Instant::now(),
                });
                self.watch_stale_swipe(cx);
            }
            TouchPhase::Moved => {
                // Momentum after the fingers lift has no gesture to belong to.
                let Some(tracking) = self
                    .swipe
                    .tracking
                    .as_mut()
                    .filter(|tracking| tracking.message == id)
                else {
                    return;
                };
                tracking.last = Instant::now();
                match tracking.horizontal {
                    None => {
                        tracking.moved += delta;
                        let moved = tracking.moved;
                        if moved.x.abs().max(moved.y.abs()) >= DECIDE {
                            let horizontal = moved.x > 0. && moved.x.abs() > moved.y.abs() * 1.2;
                            tracking.horizontal = Some(horizontal);
                            if horizontal {
                                tracking.offset = moved.x.min(MAX_OFFSET);
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }
                    }
                    Some(true) => {
                        let before = tracking.offset;
                        tracking.offset = (tracking.offset + delta.x).clamp(0., MAX_OFFSET);
                        cx.stop_propagation();
                        if before != tracking.offset {
                            cx.notify();
                        }
                    }
                    Some(false) => {}
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let reply = event.touch_phase == TouchPhase::Ended;
                self.let_go_swipe(id, reply, window, cx);
            }
        }
    }

    /// The fingers lifted (or the system took the gesture): reply when far
    /// enough, then slide the message back.
    fn let_go_swipe(&mut self, id: &str, reply: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tracking) = self
            .swipe
            .tracking
            .take_if(|tracking| tracking.message == id)
        else {
            return;
        };
        if tracking.horizontal != Some(true) {
            return;
        }
        cx.stop_propagation();
        if tracking.offset > 0. {
            self.swipe.returns += 1;
            self.swipe.returning = Some((id.to_owned(), tracking.offset, self.swipe.returns));
        }
        if reply && tracking.offset >= TRIGGER {
            self.start_reply(id.to_owned(), window, cx);
        }
        cx.notify();
    }

    /// Lets go of a gesture that stopped sending events without ending.
    fn watch_stale_swipe(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(STALE).await;
                let stale = this.update(cx, |this, cx| {
                    let Some(tracking) = &this.swipe.tracking else {
                        return true;
                    };
                    if tracking.last.elapsed() < STALE {
                        return false;
                    }
                    let tracking = this.swipe.tracking.take();
                    if let Some(tracking) = tracking.filter(|t| t.horizontal == Some(true)) {
                        this.swipe.returns += 1;
                        this.swipe.returning =
                            Some((tracking.message, tracking.offset, this.swipe.returns));
                        cx.notify();
                    }
                    true
                });
                if !matches!(stale, Ok(false)) {
                    break;
                }
            }
        })
        .detach();
    }

    /// Wraps a message row: moves it with the gesture and shows the reply
    /// arrow in the gap it leaves.
    pub(crate) fn swipe_row(
        &self,
        id: &str,
        row: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let offset = self
            .swipe
            .tracking
            .as_ref()
            .filter(|tracking| tracking.message == id && tracking.horizontal == Some(true))
            .map(|tracking| tracking.offset);
        if let Some(offset) = offset {
            let armed = offset >= TRIGGER;
            let arrow = h_flex()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(offset / 2. - 11.))
                .items_center()
                .child(
                    div()
                        .size(px(22.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(if armed {
                            cx.theme().primary
                        } else {
                            cx.theme().secondary
                        })
                        .opacity((offset / TRIGGER).clamp(0.2, 1.))
                        .child(Icon::new(IconName::CornerUpLeft).size(px(13.)).text_color(
                            if armed {
                                cx.theme().primary_foreground
                            } else {
                                cx.theme().muted_foreground
                            },
                        )),
                );
            return div()
                .relative()
                .overflow_hidden()
                .child(arrow)
                .child(row.relative().left(px(offset)).test_support())
                .into_any_element();
        }
        if let Some((_, from, key)) = self
            .swipe
            .returning
            .as_ref()
            .filter(|(message, ..)| message == id)
            && !self.snapshot.reduce_motion
            && !cfg!(test)
        {
            let from = *from;
            return div()
                .relative()
                .overflow_hidden()
                .child(row.relative().with_animation(
                    SharedString::from(format!("swipe-return-{key}")),
                    Animation::new(RETURN).with_easing(crate::ui::motion::ease_out),
                    move |row, t| row.left(px(from * (1. - t))),
                ))
                .into_any_element();
        }
        row.test_support().into_any_element()
    }
}

/// An invisible layer under the timeline that sees scroll events first
/// (capture phase, registered before the timeline paints) and hands them to
/// `swipe_scroll`. Must be the timeline area's first child.
pub(crate) fn swipe_capture(view: WeakEntity<MatrixApp>) -> impl IntoElement {
    canvas(
        |bounds, _, _| bounds,
        move |_, bounds, window, _| {
            window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || !bounds.contains(&event.position) {
                    return;
                }
                let _ = view.update(cx, |this, cx| this.swipe_scroll(event, window, cx));
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
