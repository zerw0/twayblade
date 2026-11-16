//! A message list whose rows rest against the composer.
//!
//! Adapted from GPUI Kit's `gpui_component::message_scroller` 0.9.1
//! (https://github.com/longbridge/gpui-kit), licensed under Apache-2.0; see
//! `vendor/LICENSE-APACHE`.
//!
//! Kit fixes its list to `ListAlignment::Top`, so a conversation shorter than
//! the window sits at the top with an empty band above the composer. Chat
//! clients keep the latest message beside the composer, so this copy aligns
//! the list to the bottom and keeps only the parts Twayblade uses.

use std::{ops::Range, time::Duration};

use gpui_kit::assets::IconName;
use gpui_kit::base::{
    ScrollBounce,
    motion::{Transition, transition},
};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    scroll::ScrollableElement as _,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

const LIST_OVERDRAW: Pixels = px(400.);
const JUMP_BUTTON_TRANSITION: Duration = Duration::from_millis(200);

type RowRenderer = Box<dyn FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static>;

/// The entity-owned scrolling state for a [`MessageScroller`]. Message data
/// stays with the caller and is read by the row renderer.
pub struct MessageScrollerState {
    list_state: ListState,
}

impl MessageScrollerState {
    /// Create a bottom-aligned state for `item_count` rows that follows its
    /// tail.
    pub fn new(item_count: usize, cx: &mut Context<Self>) -> Self {
        let list_state = ListState::new(item_count, ListAlignment::Bottom, LIST_OVERDRAW);
        list_state.set_follow_mode(FollowMode::Tail);

        let weak_state = cx.weak_entity();
        list_state.set_scroll_handler(move |_, _, cx| {
            let weak_state = weak_state.clone();
            // The list holds its own borrow while scrolling; notify afterwards.
            cx.defer(move |cx| {
                let _ = weak_state.update(cx, |_, cx| cx.notify());
            });
        });

        Self { list_state }
    }

    pub fn item_count(&self) -> usize {
        self.list_state.item_count()
    }

    /// Whether the reader has scrolled away from the latest content.
    pub fn is_scrolled_up(&self) -> bool {
        self.list_state.max_offset_for_scrollbar().y > px(0.)
            && !self.list_state.is_following_tail()
            && !self.list_state.is_scrolled_to_end().unwrap_or(false)
    }

    /// The row at the top of the viewport and how far into it the list is
    /// scrolled. At the live edge of a bottom-aligned list this is
    /// `item_count` until the reader scrolls.
    pub fn logical_scroll_top(&self) -> ListOffset {
        self.list_state.logical_scroll_top()
    }

    pub fn reset(&mut self, item_count: usize, cx: &mut Context<Self>) {
        self.list_state.reset(item_count);
        self.list_state.set_follow_mode(FollowMode::Tail);
        cx.notify();
    }

    /// Replace `old_range` with `count` new rows. Returns `false`, leaving the
    /// state unchanged, when the range is outside the list.
    pub fn splice(
        &mut self,
        old_range: Range<usize>,
        count: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.valid_range(&old_range) {
            return false;
        }

        let neighbor = old_range.start.checked_sub(1);
        let touches_start = old_range.start == 0;
        self.list_state.splice(old_range, count);

        // Every row but the last carries bottom padding, so a row whose "last"
        // status may have flipped has a stale height.
        if let Some(last) = self.list_state.item_count().checked_sub(1) {
            self.list_state.remeasure_items(last..last + 1);
            if let Some(neighbor) = neighbor.filter(|neighbor| *neighbor != last) {
                self.list_state.remeasure_items(neighbor..neighbor + 1);
            }
        }
        // The first row carries the top inset; it may have changed hands.
        if touches_start {
            for index in [0, count] {
                if index < self.list_state.item_count() {
                    self.list_state.remeasure_items(index..index + 1);
                }
            }
        }

        cx.notify();
        true
    }

    /// Mark all rows for remeasurement while preserving a proportional anchor.
    pub fn remeasure(&mut self, cx: &mut Context<Self>) {
        self.list_state.remeasure();
        cx.notify();
    }

    /// Mark rows in `range` for remeasurement. Returns `false` when the range
    /// is outside the list.
    pub fn remeasure_items(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> bool {
        if !self.valid_range(&range) {
            return false;
        }
        self.list_state.remeasure_items(range);
        cx.notify();
        true
    }

    /// Scroll to the row at `index`, if it exists.
    pub fn scroll_to_item(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        if index >= self.list_state.item_count() {
            return false;
        }
        self.list_state.scroll_to(ListOffset {
            item_ix: index,
            offset_in_item: px(0.),
        });
        cx.notify();
        true
    }

    /// Resume tail following and scroll to the latest row.
    pub fn scroll_to_end(&mut self, cx: &mut Context<Self>) {
        self.list_state.set_follow_mode(FollowMode::Tail);
        self.list_state.scroll_to_end();
        cx.notify();
    }

    fn valid_range(&self, range: &Range<usize>) -> bool {
        range.start <= range.end && range.end <= self.list_state.item_count()
    }
}

/// A virtualized, bottom-aligned message list with a scrollbar and a
/// jump-to-latest button.
#[derive(IntoElement)]
pub struct MessageScroller {
    id: ElementId,
    state: Entity<MessageScrollerState>,
    renderer: RowRenderer,
    style: StyleRefinement,
    row_style: StyleRefinement,
    jump_button_label: SharedString,
}

impl MessageScroller {
    pub fn new<E: IntoElement>(
        id: impl Into<ElementId>,
        state: Entity<MessageScrollerState>,
        mut renderer: impl FnMut(usize, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            state,
            renderer: Box::new(move |index, window, cx| {
                renderer(index, window, cx).into_any_element()
            }),
            style: StyleRefinement::default(),
            row_style: StyleRefinement::default(),
            jump_button_label: "Jump to latest".into(),
        }
    }

    pub fn with_jump_button_label(mut self, label: impl Into<SharedString>) -> Self {
        self.jump_button_label = label.into();
        self
    }

    /// Refine the full-width wrapper around every rendered row.
    pub fn with_row_style(mut self, style: StyleRefinement) -> Self {
        self.row_style = style;
        self
    }
}

impl Styled for MessageScroller {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MessageScroller {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let root_id = self.id.clone();
        let (list_state, scrolled_up) = {
            let state = self.state.read(cx);
            (state.list_state.clone(), state.is_scrolled_up())
        };
        let jump_button_visibility = transition(
            (root_id.clone(), "jump-button-visibility"),
            if scrolled_up { 1. } else { 0. },
            Transition::new(JUMP_BUTTON_TRANSITION),
            window,
            cx,
        );
        let tokens = cx.theme().semantic_tokens();
        let row_style = self.row_style;
        let mut renderer = self.renderer;

        // Padding rides on the rows rather than the list: a list measures its
        // scroll extent from its items alone, so list padding would leave the
        // last row unreachable.
        let item_count = list_state.item_count();
        let list = list(list_state.clone(), move |index, window, cx| {
            div()
                .w_full()
                .when(index == 0, |this| this.pt_2())
                .when(index + 1 == item_count, |this| this.pb_2())
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .px_3()
                        .when(index + 1 < item_count, |this| this.pb_8())
                        .refine_style(&row_style)
                        .child(renderer(index, window, cx)),
                )
                .into_any_element()
        })
        .size_full()
        .min_h_0();

        let viewport = div()
            .id((root_id.clone(), "viewport"))
            // Appended rows are announced as a log region.
            .role(Role::Log)
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(
                // The bounce scrolls the list past its own scroll handler;
                // tell the state, as that handler would.
                ScrollBounce::new((root_id.clone(), "bounce"), &list_state, list).on_scroll({
                    let state = self.state.downgrade();
                    move |_, cx| {
                        let _ = state.update(cx, |_, cx| cx.notify());
                    }
                }),
            )
            .vertical_scrollbar(&list_state);

        div()
            .id(root_id.clone())
            .relative()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .child(viewport)
            .when(jump_button_visibility > 0., |this| {
                let state = self.state.clone();
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom(rems(0.5 + jump_button_visibility * 0.5))
                        .flex()
                        .justify_center()
                        .opacity(jump_button_visibility)
                        .child(
                            Button::new((root_id, "jump-to-latest"))
                                .secondary()
                                .icon(IconName::ArrowDown)
                                .tooltip(self.jump_button_label)
                                .rounded(cx.theme().radius_full())
                                .border_1()
                                .border_color(tokens.colors.border)
                                .bg(tokens.colors.background)
                                .text_color(tokens.colors.foreground)
                                .on_click(move |_, _, cx| {
                                    state.update(cx, |state, cx| state.scroll_to_end(cx));
                                })
                                .when(!scrolled_up, |button| button.disabled(true)),
                        ),
                )
            })
            .refine_style(&self.style)
    }
}
