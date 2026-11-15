//! Keyboard movement between the window's areas (F6) and through messages.
//!
//! Each timeline is a single tab stop, its cursor message: ↑/↓, Home and End
//! move the cursor, and Tab reaches that message's own controls before it
//! leaves the timeline. Other messages' controls stay reachable by pointer and
//! by screen readers, but not by Tab.
use super::*;
use crate::backend::Message;
use std::ops::Range;

/// Main timeline and thread panel, as indexes into the per-timeline arrays.
const MAIN: usize = 0;
const THREAD: usize = 1;

/// A part of the window F6 moves between, in Tab order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Area {
    Sidebar,
    Timeline,
    Composer,
    ThreadTimeline,
    ThreadComposer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Previous,
    Next,
    First,
    Last,
}

/// The message `step` lands on from `from`, or None at either end.
pub(crate) fn step_index(from: usize, len: usize, step: Step) -> Option<usize> {
    let target = match step {
        Step::Previous => from.checked_sub(1)?,
        Step::Next => from + 1,
        Step::First => 0,
        Step::Last => len.checked_sub(1)?,
    };
    (target < len && target != from).then_some(target)
}

/// The group (scroller row) that holds message `ix`.
pub(crate) fn group_of(groups: &[Range<usize>], ix: usize) -> usize {
    groups.partition_point(|group| group.end <= ix)
}

/// The next area after (or before) `current` among `available`, wrapping.
/// From outside every area, forward starts at the first and backward at the last.
pub(crate) fn next_area(available: &[Area], current: Option<Area>, forward: bool) -> Option<Area> {
    let len = available.len();
    if len == 0 {
        return None;
    }
    let ix = match current.and_then(|area| available.iter().position(|a| *a == area)) {
        Some(ix) if forward => (ix + 1) % len,
        Some(ix) => (ix + len - 1) % len,
        None if forward => 0,
        None => len - 1,
    };
    Some(available[ix])
}

impl MatrixApp {
    fn timeline_messages(&self, in_thread: bool) -> &[Message] {
        if in_thread {
            self.thread.as_ref().map_or(&[], |t| t.messages.as_slice())
        } else {
            &self.snapshot.messages
        }
    }

    /// The focus handle of a message row; thread rows are keyed apart, since a
    /// message can show in both.
    pub(crate) fn message_focus(
        &self,
        in_thread: bool,
        id: &str,
        cx: &mut Context<Self>,
    ) -> FocusHandle {
        let prefix = if in_thread { "t" } else { "m" };
        self.image_focus(&format!("message-{prefix}-{id}"), cx)
    }

    /// The message holding a timeline's tab stop: the one last moved to, else
    /// the latest.
    pub(crate) fn message_cursor(&self, in_thread: bool) -> Option<&str> {
        let slot = if in_thread { THREAD } else { MAIN };
        self.message_cursor[slot].as_deref().or_else(|| {
            self.timeline_messages(in_thread)
                .last()
                .map(|m| m.id.as_str())
        })
    }

    /// Whether `message`'s controls are in the Tab order: the cursor's are, and
    /// so are the thread root's above the thread timeline.
    pub(crate) fn message_tab_stops(&self, in_thread: bool, message: &str) -> bool {
        self.message_cursor(in_thread) == Some(message)
            || in_thread && self.thread.as_ref().is_some_and(|t| t.root.id == message)
    }

    /// Whether keyboard focus is on the cursor message or one of its controls;
    /// its hover actions then stay visible.
    pub(crate) fn message_cursor_focused(&self, in_thread: bool, message: &str) -> bool {
        self.message_cursor_focused[if in_thread { THREAD } else { MAIN }]
            && self.message_cursor(in_thread) == Some(message)
    }

    /// Once per frame: drops a cursor whose message is gone (back to the latest)
    /// and notes where focus is.
    pub(crate) fn sync_message_cursors(&mut self, window: &Window, cx: &mut Context<Self>) {
        for in_thread in [false, true] {
            let slot = if in_thread { THREAD } else { MAIN };
            if let Some(id) = &self.message_cursor[slot]
                && !self
                    .timeline_messages(in_thread)
                    .iter()
                    .rev()
                    .any(|m| &m.id == id)
            {
                self.message_cursor[slot] = None;
            }
            self.message_cursor_focused[slot] = match self.message_cursor(in_thread) {
                Some(id) => {
                    let id = id.to_owned();
                    self.message_focus(in_thread, &id, cx)
                        .contains_focused(window, cx)
                }
                None => false,
            };
        }
    }

    /// A message row took focus (click or Tab): it becomes the cursor.
    pub(crate) fn set_message_cursor(
        &mut self,
        in_thread: bool,
        id: String,
        cx: &mut Context<Self>,
    ) {
        let slot = if in_thread { THREAD } else { MAIN };
        if self.message_cursor[slot].as_ref() != Some(&id) {
            self.message_cursor[slot] = Some(id);
            cx.notify();
        }
    }

    /// ↑/↓/Home/End on a focused message row.
    pub(crate) fn step_message(
        &mut self,
        in_thread: bool,
        from: &str,
        step: Step,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let messages = self.timeline_messages(in_thread);
        let Some(ix) = messages.iter().position(|m| m.id == from) else {
            return false;
        };
        let Some(target) = step_index(ix, messages.len(), step) else {
            return false;
        };
        let id = messages[target].id.clone();
        self.focus_message(in_thread, id, window, cx);
        true
    }

    /// Scrolls a message into view and focuses its row.
    fn focus_message(
        &mut self,
        in_thread: bool,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let messages = self.timeline_messages(in_thread);
        let Some(ix) = messages.iter().position(|m| m.id == id) else {
            return;
        };
        let scroller = if in_thread {
            self.thread
                .as_ref()
                .map(|t| (t.scroller.clone(), group_of(&t.groups, ix)))
        } else {
            Some((self.scroller.clone(), group_of(&self.groups, ix)))
        };
        if let Some((scroller, group)) = scroller {
            scroller.update(cx, |state, cx| {
                state.scroll_to_item(group, cx);
            });
        }
        let handle = self.message_focus(in_thread, &id, cx);
        self.set_message_cursor(in_thread, id, cx);
        window.focus(&handle, cx);
        cx.notify();
    }

    /// The areas F6 can reach now, in window order.
    fn focus_areas(&self) -> Vec<Area> {
        let workspace = !self.snapshot.user_id.is_empty()
            && self.snapshot.session_state == twayblade_core::auth::SessionState::Active
            && !self.call_fills_window()
            && self.viewer.is_none()
            && self.management.is_none();
        if !workspace {
            return Vec::new();
        }
        let mut areas = Vec::new();
        if self.sidebar_open {
            areas.push(Area::Sidebar);
        }
        if self.navigation.selected_room.is_some() {
            if !self.snapshot.messages.is_empty() {
                areas.push(Area::Timeline);
            }
            areas.push(Area::Composer);
        }
        // The thread shows only when no other side panel replaces it.
        if let Some(t) = &self.thread
            && !self.search_open
        {
            if !t.messages.is_empty() {
                areas.push(Area::ThreadTimeline);
            }
            areas.push(Area::ThreadComposer);
        }
        areas
    }

    fn focused_area(&self, window: &Window, cx: &mut Context<Self>) -> Option<Area> {
        if self.composer.focus_handle(cx).contains_focused(window, cx) {
            return Some(Area::Composer);
        }
        if let Some(t) = &self.thread {
            if t.composer.focus_handle(cx).contains_focused(window, cx) {
                return Some(Area::ThreadComposer);
            }
            if self.thread_area.contains_focused(window, cx) {
                return Some(Area::ThreadTimeline);
            }
        }
        if self.timeline_area.contains_focused(window, cx) {
            return Some(Area::Timeline);
        }
        // The search field heads the sidebar, though it sits in the title bar.
        if self.sidebar_area.contains_focused(window, cx)
            || self.search.focus_handle(cx).contains_focused(window, cx)
        {
            return Some(Area::Sidebar);
        }
        None
    }

    /// F6 / ⇧F6: sidebar → messages → message field → thread, wrapping.
    pub(crate) fn focus_next_area(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.focused_area(window, cx);
        let Some(area) = next_area(&self.focus_areas(), current, forward) else {
            return;
        };
        match area {
            Area::Sidebar => {
                // The open conversation's row, else the conversation search.
                let row = self.navigation.selected_room.clone().filter(|room| {
                    self.search.read(cx).value().is_empty()
                        && self.navigation.room_visible(room, &self.snapshot)
                });
                match row {
                    Some(room) => {
                        let handle = self.room_focus(&room, cx);
                        window.focus(&handle, cx);
                    }
                    None => self.search.update(cx, |state, cx| state.focus(window, cx)),
                }
            }
            Area::Timeline | Area::ThreadTimeline => {
                let in_thread = area == Area::ThreadTimeline;
                if let Some(id) = self.message_cursor(in_thread).map(str::to_owned) {
                    self.focus_message(in_thread, id, window, cx);
                }
            }
            Area::Composer => self
                .composer
                .update(cx, |state, cx| state.focus(window, cx)),
            Area::ThreadComposer => {
                if let Some(t) = &self.thread {
                    t.composer.update(cx, |state, cx| state.focus(window, cx));
                }
            }
        }
        cx.notify();
    }

    /// The focus handle of a sidebar room row.
    pub(crate) fn room_focus(&self, room: &str, cx: &mut Context<Self>) -> FocusHandle {
        self.image_focus(&format!("room-row-{room}"), cx)
    }
}

#[cfg(test)]
mod tests {
    use super::{Area, Step, group_of, next_area, step_index};

    #[test]
    fn arrows_stop_at_the_ends() {
        assert_eq!(step_index(0, 3, Step::Previous), None);
        assert_eq!(step_index(1, 3, Step::Previous), Some(0));
        assert_eq!(step_index(2, 3, Step::Next), None);
        assert_eq!(step_index(0, 3, Step::Last), Some(2));
        assert_eq!(step_index(2, 3, Step::Last), None);
        assert_eq!(step_index(2, 3, Step::First), Some(0));
        assert_eq!(step_index(0, 1, Step::First), None);
    }

    #[test]
    fn messages_map_to_their_group() {
        let groups = [0..2, 2..3, 3..6];
        assert_eq!(group_of(&groups, 0), 0);
        assert_eq!(group_of(&groups, 1), 0);
        assert_eq!(group_of(&groups, 2), 1);
        assert_eq!(group_of(&groups, 5), 2);
    }

    #[test]
    fn f6_cycles_and_wraps() {
        use Area::*;
        let areas = [Sidebar, Timeline, Composer];
        assert_eq!(next_area(&areas, Some(Sidebar), true), Some(Timeline));
        assert_eq!(next_area(&areas, Some(Composer), true), Some(Sidebar));
        assert_eq!(next_area(&areas, Some(Sidebar), false), Some(Composer));
        // From the title bar or nowhere: forward starts at the sidebar.
        assert_eq!(next_area(&areas, None, true), Some(Sidebar));
        assert_eq!(next_area(&areas, None, false), Some(Composer));
        // An area that is gone (thread closed) counts as outside.
        assert_eq!(next_area(&areas, Some(ThreadComposer), true), Some(Sidebar));
        assert_eq!(next_area(&[], None, true), None);
    }
}
