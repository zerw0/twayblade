//! The floating formatting bar above selected composer text (#89).
//!
//! A small view of its own that observes one composer, so moving the caret or
//! the selection re-renders only the bar, not the whole window. Its controls
//! never take focus: a press is handled on mouse down and stopped there, so
//! the composer keeps its focus and selection, and a second format can follow.
//! Keyboard users have the same formats as shortcuts and in the right-click
//! menu.
use super::*;
use crate::composer_format::{self, Format};
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::input::TextareaState;

pub(crate) struct SelectionBar {
    composer: Entity<TextareaState>,
    _observe: Subscription,
}

const ITEMS: [(Format, Lucide, &str, &str); 7] = [
    (Format::Bold, Lucide::Bold, "Bold", "Bold  ⌘B"),
    (Format::Italic, Lucide::Italic, "Italic", "Italic  ⌘I"),
    (Format::Code, Lucide::Code, "Code", "Code  ⌘E"),
    (Format::Link, Lucide::Link, "Link", "Link  ⇧⌘L"),
    (Format::Quote, Lucide::TextQuote, "Quote", "Quote  ⇧⌘>"),
    (
        Format::BulletList,
        Lucide::List,
        "Bulleted list",
        "Bulleted List",
    ),
    (
        Format::NumberedList,
        Lucide::ListOrdered,
        "Numbered list",
        "Numbered List",
    ),
];

impl SelectionBar {
    pub(crate) fn new(composer: Entity<TextareaState>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&composer, |_, _, cx| cx.notify());
        Self {
            composer,
            _observe: observe,
        }
    }

    /// Follows the composer the window currently shows (each room keeps its own).
    pub(crate) fn track(&mut self, composer: &Entity<TextareaState>, cx: &mut Context<Self>) {
        if self.composer.entity_id() != composer.entity_id() {
            self.composer = composer.clone();
            self._observe = cx.observe(composer, |_, _, cx| cx.notify());
        }
    }

    fn apply(&mut self, format: Format, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |state, cx| {
            composer_format::apply(format, state, window, cx);
        });
    }
}

impl Render for SelectionBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.composer.read(cx);
        let focused = gpui::Focusable::focus_handle(state, cx).is_focused(window);
        let range = state.selected_range();
        // Window coordinates; the origin is where the selection starts.
        let Some(selection) = (!range.is_empty() && focused)
            .then(|| state.range_to_bounds(&range))
            .flatten()
        else {
            return div().into_any_element();
        };
        let start = selection.origin;
        let reduce = twayblade_platform::reduce_motion();
        let mut bar = h_flex()
            .id("selection-format-bar")
            .test_support()
            .role(Role::Toolbar)
            .aria_label("Format selection")
            .p(px(3.))
            .gap(px(1.))
            .rounded(px(8.))
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .shadow_md();
        for (index, (format, icon, label, tooltip)) in ITEMS.into_iter().enumerate() {
            if index == 5 {
                bar = bar.child(div().w(px(1.)).h(px(16.)).mx(px(2.)).bg(cx.theme().border));
            }
            bar = bar.child(
                div()
                    .id(SharedString::from(format!("selection-format-{index}")))
                    .test_support()
                    .role(Role::Button)
                    .aria_label(label)
                    .size(px(26.))
                    .rounded(px(5.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|item| item.bg(cx.theme().secondary))
                    .active(|item| item.bg(cx.theme().secondary_active))
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(tooltip).build(window, cx)
                    })
                    .child(Icon::new(icon).size(px(15.)))
                    // Mouse down, not click: the composer must not lose focus.
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.apply(format, window, cx);
                        }),
                    ),
            );
        }
        deferred(
            anchored()
                .position(point(start.x - px(14.), start.y - px(6.)))
                .anchor(Anchor::BottomLeft)
                .snap_to_window_with_margin(px(8.))
                .child(motion::rise(bar, "selection-format-bar-motion", reduce)),
        )
        .with_priority(1)
        .into_any_element()
    }
}
