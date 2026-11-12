//! Bounded, account-scoped, memory-only sent text. Deliberately no Debug/Serde.
use std::collections::VecDeque;

const ROOM_LIMIT: usize = 50;
const ENTRY_LIMIT: usize = 50;
// Match core's maximum composed body; bound saved drafts as well as entries.
const TEXT_LIMIT: usize = 64 * 1024;

#[derive(Default)]
struct Room {
    id: String,
    entries: VecDeque<String>,
    cursor: Option<usize>,
    draft: String,
}
#[derive(Default)]
pub struct ComposerHistory {
    rooms: VecDeque<Room>,
}
impl ComposerHistory {
    fn room(&mut self, room: &str) -> &mut Room {
        if let Some(index) = self.rooms.iter().position(|r| r.id == room) {
            let entry = self.rooms.remove(index).expect("known room index");
            self.rooms.push_back(entry);
        } else {
            if self.rooms.len() == ROOM_LIMIT {
                self.rooms.pop_front();
            }
            self.rooms.push_back(Room {
                id: room.to_string(),
                ..Default::default()
            });
        }
        self.rooms.back_mut().expect("room inserted")
    }
    /// Called only after the SDK accepts a send, not for remote/other-client messages.
    pub fn push(&mut self, room: &str, text: String) {
        self.reset_cursor(room);
        if text.trim().is_empty() || text.len() > TEXT_LIMIT {
            return;
        }
        let room = self.room(room);
        if room.entries.len() == ENTRY_LIMIT {
            room.entries.pop_front();
        }
        room.entries.push_back(text);
    }
    pub fn previous(&mut self, room: &str, current_draft: &str) -> Option<String> {
        // Do not overwrite a draft we cannot retain in full.
        if current_draft.len() > TEXT_LIMIT {
            return None;
        }
        let room = self.room(room);
        if room.entries.is_empty() {
            return None;
        }
        let index = match room.cursor {
            None => {
                room.draft = current_draft.to_string();
                room.entries.len() - 1
            }
            Some(0) => return None,
            Some(index) => index - 1,
        };
        room.cursor = Some(index);
        room.entries.get(index).cloned()
    }
    pub fn next(&mut self, room: &str) -> Option<String> {
        let room = self.room(room);
        let index = room.cursor? + 1;
        if index < room.entries.len() {
            room.cursor = Some(index);
            room.entries.get(index).cloned()
        } else {
            room.cursor = None;
            Some(std::mem::take(&mut room.draft))
        }
    }
    pub fn reset_cursor(&mut self, room: &str) {
        if let Some(room) = self.rooms.iter_mut().find(|r| r.id == room) {
            room.cursor = None;
            room.draft.clear();
        }
    }
    /// Room/selection changes reset navigation but preserve the sent entries.
    pub fn reset_cursors(&mut self) {
        for room in &mut self.rooms {
            room.cursor = None;
            room.draft.clear();
        }
    }
    /// Sign-out/account replacement drops every retained text and saved draft.
    pub fn clear(&mut self) {
        self.rooms.clear();
    }
}

use super::{Account, Backend, ServiceError, failure};
impl Backend {
    /// Claude calls these from the focused main/thread composer's history action.
    /// Both surfaces share the room's sent-text history; pass the current account
    /// ticket so stale handlers cannot read a different account's history.
    pub fn history_previous(
        &self,
        account: u128,
        room: &str,
        current_draft: &str,
    ) -> Result<Option<String>, ServiceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session unavailable."))?;
        if state.account.as_ref().map(Account::instance_id) != Some(account)
            || self.updates.borrow().selected_room != room
        {
            return Err(failure("Conversation changed."));
        }
        Ok(state.history.previous(room, current_draft))
    }
    pub fn history_next(&self, account: u128, room: &str) -> Result<Option<String>, ServiceError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure("Session unavailable."))?;
        if state.account.as_ref().map(Account::instance_id) != Some(account)
            || self.updates.borrow().selected_room != room
        {
            return Err(failure("Conversation changed."));
        }
        Ok(state.history.next(room))
    }
    pub(super) fn record_sent(&self, epoch: u64, room: &str, body: String) {
        if let Ok(mut state) = self.state.lock()
            && state.epoch == epoch
        {
            state.history.push(room, body);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restores_draft_and_stops_at_each_end() {
        let mut h = ComposerHistory::default();
        h.push("a", "one".into());
        h.push("a", "two".into());
        assert_eq!(h.previous("a", "draft"), Some("two".into()));
        assert_eq!(h.previous("a", "two"), Some("one".into()));
        assert_eq!(h.previous("a", "one"), None);
        assert_eq!(h.next("a"), Some("two".into()));
        assert_eq!(h.next("a"), Some("draft".into()));
        assert_eq!(h.next("a"), None);
    }
    #[test]
    fn room_isolation_reset_and_clear_on_session_stop() {
        let mut state = super::super::State::default();
        state.history.push("a", "secret α".into());
        state.history.push("b", "🦀".into());
        assert_eq!(
            state.history.previous("a", "draft"),
            Some("secret α".into())
        );
        assert_eq!(state.history.previous("b", "other"), Some("🦀".into()));
        state.history.reset_cursors();
        assert_eq!(state.history.next("a"), None);
        state.history.previous("a", "new draft");
        state.history.push("a", "new send".into());
        assert_eq!(state.history.next("a"), None);
        // logout and connect (account replacement) both use State::stop.
        state.stop();
        assert!(state.history.rooms.is_empty());
        assert_eq!(state.history.previous("a", ""), None);
    }
    #[test]
    fn entry_and_room_bounds_use_lru() {
        let mut h = ComposerHistory::default();
        for i in 0..60 {
            h.push("a", i.to_string());
        }
        assert_eq!(h.rooms[0].entries.len(), 50);
        assert_eq!(h.rooms[0].entries[0], "10");
        for i in 0..49 {
            h.push(&i.to_string(), "body".into());
        }
        h.previous("a", "");
        h.push("new", "body".into());
        assert_eq!(h.rooms.len(), 50);
        assert!(!h.rooms.iter().any(|r| r.id == "0"));
        assert!(h.rooms.iter().any(|r| r.id == "a"));
    }
    #[test]
    fn empty_and_oversized_text_is_not_retained() {
        let mut h = ComposerHistory::default();
        h.push("a", "  ".into());
        h.push("a", "x".repeat(TEXT_LIMIT + 1));
        assert!(h.rooms.is_empty());
        h.push("a", "ok".into());
        assert_eq!(h.previous("a", &"x".repeat(TEXT_LIMIT + 1)), None);
        assert_eq!(h.previous("a", ""), Some("ok".into()));
        assert_eq!(h.next("a"), Some("".into()));
    }
    #[test]
    fn late_send_cannot_repopulate_a_signed_out_or_replaced_session() {
        let Ok(backend) = Backend::with_lock(None) else {
            panic!("test runtime");
        };
        let epoch = backend.state.lock().unwrap().epoch;
        backend.state.lock().unwrap().stop();
        backend.record_sent(epoch, "a", "private body".into());
        assert!(backend.state.lock().unwrap().history.rooms.is_empty());
    }
}
