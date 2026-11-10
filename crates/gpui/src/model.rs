use std::{collections::HashMap, ops::Range};

use chrono::{Local, TimeZone};

use crate::backend::{Message, MessageKind, Room, Snapshot, Space};

#[derive(Default)]
pub(super) struct Navigation {
    pub selected_space: Option<String>,
    pub selected_room: Option<String>,
    pub drafts: HashMap<String, String>,
}

impl Navigation {
    pub fn room_visible(&self, id: &str, snapshot: &Snapshot) -> bool {
        if let Some(selected) = &self.selected_space {
            return snapshot
                .spaces
                .iter()
                .find(|space| &space.id == selected)
                .is_some_and(|space| space.room_ids.iter().any(|room| room == id));
        }
        snapshot.spaces_loaded
            && !snapshot
                .spaces
                .iter()
                .any(|space| space.room_ids.iter().any(|room| room == id))
    }

    pub fn reconcile(&mut self, snapshot: &Snapshot) -> bool {
        if self
            .selected_space
            .as_ref()
            .is_some_and(|id| !snapshot.spaces.iter().any(|space| &space.id == id))
        {
            self.selected_space = None;
        }
        if self.selected_room.as_ref().is_some_and(|id| {
            !self.room_visible(id, snapshot) || !snapshot.rooms.iter().any(|room| &room.id == id)
        }) {
            self.selected_room = None;
            return true;
        }
        false
    }
}

pub(super) fn grouped(messages: &[Message]) -> Vec<Range<usize>> {
    let mut groups: Vec<Range<usize>> = Vec::new();
    for (ix, message) in messages.iter().enumerate() {
        let joins = ix > 0 && {
            let previous = &messages[ix - 1];
            previous.sender == message.sender
                && (0..300_000)
                    .contains(&message.timestamp_ms.saturating_sub(previous.timestamp_ms))
                && Local
                    .timestamp_millis_opt(previous.timestamp_ms)
                    .single()
                    .map(|date| date.date_naive())
                    == Local
                        .timestamp_millis_opt(message.timestamp_ms)
                        .single()
                        .map(|date| date.date_naive())
        };
        if joins {
            groups.last_mut().expect("preceding group").end = ix + 1;
        } else {
            groups.push(ix..ix + 1);
        }
    }
    groups
}

// Preserve plain text when using Kit's selectable text renderer. Escape markup
// so message content cannot become remote images or change formatting.
pub(super) fn plain_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii_punctuation() {
            result.push('\\');
        }
        if ch == '\n' {
            result.push_str("  ");
        }
        result.push(ch);
    }
    result
}

pub(super) fn preview(home: bool, count: usize) -> Snapshot {
    let mut snapshot = Snapshot {
        revision: 1,
        user_id: "@you:preview.invalid".into(),
        spaces_loaded: true,
        selected_room: if home { "notes" } else { "general" }.into(),
        timeline_ready: true,
        sync_status: "Offline preview · no account connected".into(),
        timeline_status: "Sample conversation · sending disabled".into(),
        spaces: vec![Space {
            id: "design".into(),
            name: "Design team".into(),
            level: 0,
            room_ids: vec!["general".into(), "research".into()],
        }],
        rooms: vec![
            Room {
                id: "general".into(),
                name: "General".into(),
                unread: 3,
                topic: Some("Day-to-day design talk".into()),
                encrypted: true,
                ..Default::default()
            },
            Room {
                id: "research".into(),
                name: "Research".into(),
                encrypted: true,
                ..Default::default()
            },
            Room {
                id: "notes".into(),
                name: "Personal notes".into(),
                ..Default::default()
            },
            Room {
                id: "alex".into(),
                name: "Alex".into(),
                unread: 2,
                encrypted: true,
                direct: true,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let bodies = [
        "A single Rust interface for macOS, Linux and Windows.",
        "Messages from the same person share their name and timestamp.",
        "Space rooms stay in their space. Home keeps everything else.",
        "The Matrix core is unchanged.",
        "Profiles, drafts and history are carried over.",
    ];
    snapshot.messages = (0..count)
        .map(|ix| {
            let outgoing = ix / 3 % 2 == 1;
            Message {
                id: format!("sample-{ix}"),
                sender: if outgoing {
                    snapshot.user_id.clone()
                } else {
                    "@alex:preview.invalid".into()
                },
                display_name: if outgoing { "You" } else { "Alex" }.into(),
                body: bodies[ix % bodies.len()].into(),
                kind: MessageKind::Text,
                timestamp_ms: 1_794_300_000_000 + ix as i64 * 30_000,
                delivery: String::new(),
            }
        })
        .collect();
    // Show the missing-key state in the sample conversation.
    if let Some(message) = snapshot.messages.get_mut(2) {
        message.kind = MessageKind::Undecryptable;
        message.body.clear();
    }
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_waits_for_membership_and_preserves_hidden_drafts() {
        let mut snapshot = preview(false, 6);
        let mut navigation = Navigation {
            selected_room: Some("general".into()),
            ..Default::default()
        };
        navigation.drafts.insert("general".into(), "Keep me".into());
        assert!(!navigation.room_visible("general", &snapshot));
        assert!(navigation.room_visible("notes", &snapshot));
        assert!(navigation.reconcile(&snapshot));
        assert_eq!(
            navigation.drafts.get("general").map(String::as_str),
            Some("Keep me")
        );
        snapshot.spaces_loaded = false;
        assert!(!navigation.room_visible("notes", &snapshot));
    }

    #[test]
    fn grouping_uses_identity_forward_time_and_keeps_stable_message_ids() {
        let mut snapshot = preview(false, 6);
        assert_eq!(grouped(&snapshot.messages), vec![0..3, 3..6]);
        snapshot.messages[1].sender = "@other:preview.invalid".into();
        snapshot.messages[1].display_name = snapshot.messages[0].display_name.clone();
        assert_eq!(grouped(&snapshot.messages)[0], 0..1);
        snapshot.messages[1].sender = snapshot.messages[0].sender.clone();
        snapshot.messages[1].timestamp_ms = snapshot.messages[0].timestamp_ms - 1;
        assert_eq!(grouped(&snapshot.messages)[0], 0..1);
        snapshot.messages[1].timestamp_ms = snapshot.messages[0].timestamp_ms + 300_000;
        assert_eq!(grouped(&snapshot.messages)[0], 0..1);
        assert_eq!(snapshot.messages[0].id, "sample-0");
    }

    #[test]
    fn plaintext_renderer_escapes_remote_image_markup_and_preserves_newlines() {
        assert_eq!(
            plain_text("![a](https://example.org/a)\nx"),
            "\\!\\[a\\]\\(https\\:\\/\\/example\\.org\\/a\\)  \nx"
        );
    }
}
