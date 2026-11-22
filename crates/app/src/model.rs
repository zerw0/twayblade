use std::{collections::HashMap, ops::Range};

use crate::backend::{Member, MemberRole, MembersState};
use chrono::{Local, TimeZone};

use crate::backend::{
    Attachment, DeliveryState, Message, MessageKind, Reaction, ReplyTarget, Room, Snapshot, Space,
};

#[derive(Default)]
pub(super) struct Navigation {
    pub selected_space: Option<String>,
    pub selected_room: Option<String>,
    pub drafts: HashMap<String, String>,
}

impl Navigation {
    /// Direct joined subspaces, in the SDK's stable navigation order.
    pub fn child_spaces<'a>(space: &Space, snapshot: &'a Snapshot) -> Vec<&'a Space> {
        snapshot
            .spaces
            .iter()
            .filter(|child| child.id != space.id && space.child_ids.contains(&child.id))
            .collect()
    }

    pub fn rail_space(space: &Space, snapshot: &Snapshot) -> bool {
        space.level == 0
            || !snapshot
                .spaces
                .iter()
                .any(|parent| parent.id != space.id && parent.child_ids.contains(&space.id))
    }

    /// Shared direct rooms stay visible in each parent. Other descendants
    /// belong inside their immediate joined subspace's disclosure group.
    pub fn room_in_space_group(space: &Space, id: &str, snapshot: &Snapshot) -> bool {
        space.room_ids.contains(id)
            && (space.child_ids.contains(id)
                || !Self::child_spaces(space, snapshot)
                    .iter()
                    .any(|child| child.room_ids.contains(id)))
    }

    pub fn room_visible(&self, id: &str, snapshot: &Snapshot) -> bool {
        Self::room_visible_in(self.selected_space.as_deref(), id, snapshot)
    }

    pub fn room_visible_in(space: Option<&str>, id: &str, snapshot: &Snapshot) -> bool {
        if space.is_none()
            && snapshot.rooms.iter().any(|room| {
                room.id == id
                    && !room.archived
                    && room.channel_kind == twayblade_core::channels::ChannelKind::Voice
            })
            && snapshot
                .spaces
                .iter()
                .any(|space| space.room_ids.contains(id))
        {
            return false;
        }
        if snapshot
            .rooms
            .iter()
            .any(|room| room.id == id && room.direct)
        {
            return space.is_none();
        }
        if let Some(selected) = space {
            return snapshot
                .spaces
                .iter()
                .find(|space| space.id == selected)
                .is_some_and(|space| space.room_ids.contains(id));
        }
        // Home holds rooms outside any space, plus every archived
        // conversation so the Archive has one place that lists them all.
        snapshot.spaces_loaded
            && (snapshot
                .rooms
                .iter()
                .any(|room| room.id == id && room.archived)
                || !snapshot
                    .spaces
                    .iter()
                    .any(|space| space.room_ids.contains(id)))
    }

    pub fn route_room(&mut self, id: &str, snapshot: &Snapshot) {
        if !snapshot.rooms.iter().any(|room| room.id == id) {
            return;
        }
        if self.room_visible(id, snapshot) {
            return;
        }
        self.selected_space = if snapshot
            .rooms
            .iter()
            .any(|room| room.id == id && room.direct)
        {
            None
        } else {
            snapshot
                .spaces
                .iter()
                .find(|space| space.room_ids.contains(id))
                .map(|space| space.id.clone())
        };
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
    grouped_with_boundaries(messages, &Default::default())
}

pub(super) fn grouped_with_boundaries(
    messages: &[Message],
    boundaries: &std::collections::HashSet<String>,
) -> Vec<Range<usize>> {
    let mut groups: Vec<Range<usize>> = Vec::new();
    for (ix, message) in messages.iter().enumerate() {
        let joins = ix > 0 && {
            let previous = &messages[ix - 1];
            // Membership changes group with each other, whoever made them,
            // and never with messages.
            let membership = |m: &Message| m.kind == MessageKind::Membership;
            !message.new_messages
                && !boundaries.contains(&message.id)
                && membership(previous) == membership(message)
                && (membership(message) || previous.sender == message.sender)
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

/// `--preview-showcase`: the offline preview with a believable team
/// conversation and without the "Sample data" labels, for the README and
/// website screenshots. Set once at start-up, before the window opens.
static SHOWCASE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn set_showcase(on: bool) {
    SHOWCASE.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn showcase() -> bool {
    SHOWCASE.load(std::sync::atomic::Ordering::Relaxed)
}

/// The PNG shown in the showcase's image message (`--preview-showcase-image`).
static SHOWCASE_IMAGE: std::sync::OnceLock<std::sync::Arc<Vec<u8>>> = std::sync::OnceLock::new();

pub(crate) fn set_showcase_image(png: Vec<u8>) {
    let _ = SHOWCASE_IMAGE.set(std::sync::Arc::new(png));
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
            avatar: None,
            can_add_rooms: true,
            child_ids: ["general", "research", "critique", "announcements"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            room_ids: [
                "general".into(),
                "research".into(),
                "critique".into(),
                "announcements".into(),
            ]
            .into_iter()
            .collect(),
        }],
        notifications: twayblade_core::notifications::RulesSnapshot {
            ready: true,
            defaults: [twayblade_core::notifications::DefaultLevel::All; 4],
            keywords: vec!["Matrix".into(), "release".into()],
            ..Default::default()
        },
        rooms: vec![
            Room {
                id: "general".into(),
                name: "General".into(),
                unread: 3,
                topic: Some("Day-to-day design talk".into()),
                encrypted: true,
                public: true,
                favourite: true,
                pinned: 2,
                ..Default::default()
            },
            Room {
                id: "critique".into(),
                name: "Critique".into(),
                unread: 4,
                mentions: 1,
                encrypted: true,
                ..Default::default()
            },
            Room {
                id: "announcements".into(),
                name: "Announcements".into(),
                unread: 6,
                muted: true,
                public: true,
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
                partner: Some("@alex:example.org".into()),
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
                event_id: Some(format!("$sample-{ix}:preview.invalid")),
                sender: if outgoing {
                    snapshot.user_id.clone()
                } else {
                    "@alex:preview.invalid".into()
                },
                display_name: if outgoing { "You" } else { "Alex" }.into(),
                body: bodies[ix % bodies.len()].into(),
                kind: MessageKind::Text,
                timestamp_ms: 1_794_300_000_000 + ix as i64 * 30_000,
                delivery: DeliveryState::Remote,
                ..Default::default()
            }
        })
        .collect();
    // Show the missing-key state in the sample conversation.
    if let Some(message) = snapshot.messages.get_mut(2) {
        message.kind = MessageKind::Undecryptable;
        message.body.clear();
    }
    {
        if let Some(message) = snapshot.messages.get_mut(0) {
            message.reactions = vec![
                Reaction {
                    key: "👍".into(),
                    count: 3,
                    mine: true,
                },
                Reaction {
                    key: "🦀".into(),
                    count: 2,
                    mine: false,
                },
            ];
            message.thread_replies = 4;
            // A formatted (HTML) sample, as sent by Element: bold, code and a link.
            use twayblade_core::rich_text::{RichNode, RichText, SpanKind};
            let span = |kind, text: &str| RichNode::Span {
                kind,
                children: vec![RichNode::Text(text.into())],
            };
            message.rich = Some(std::sync::Arc::new(RichText {
                nodes: vec![
                    RichNode::Text("A ".into()),
                    span(SpanKind::Strong, "single"),
                    RichNode::Text(" Rust interface for ".into()),
                    span(SpanKind::Code, "macOS"),
                    RichNode::Text(", Linux and Windows. See ".into()),
                    span(
                        SpanKind::Link {
                            target: "https://matrix.org/".into(),
                        },
                        "matrix.org",
                    ),
                    RichNode::Text(".".into()),
                ],
            }));
        }
        if let Some(message) = snapshot.messages.get_mut(1) {
            message.reply = Some(ReplyTarget::Ready {
                sender: "Alex".into(),
                excerpt: "A single Rust interface for every desktop.".into(),
            });
            message.reply_to = Some("$sample-0:preview.invalid".into());
            message.kind = MessageKind::Notice;
            message.edited = true;
        }
        if let Some(message) = snapshot.messages.get_mut(3) {
            message.kind = MessageKind::Emote;
            message.body = "say hello".into();
            message.new_messages = true;
        }
        if let Some(message) = snapshot.messages.get_mut(4) {
            message.kind = MessageKind::Image;
            message.attachment = Some(Attachment {
                preview_png: None,
                download_needed: false,
                filename: "sample-mockup.png".into(),
                size: Some(245_760),
            });
        }
        if let Some(message) = snapshot.messages.get_mut(5) {
            message.kind = MessageKind::File;
            message.attachment = Some(Attachment {
                preview_png: None,
                download_needed: false,
                filename: "sample-notes.pdf".into(),
                size: Some(1_048_576),
            });
            message.delivery = DeliveryState::Failed;
            message.can_retry = true;
            message.event_id = None;
        }
        // A membership change between messages, shown as a quiet line.
        if snapshot.messages.len() > 3 {
            snapshot.messages.insert(
                3,
                Message {
                    id: "sample-joined".into(),
                    event_id: Some("$sample-joined:preview.invalid".into()),
                    sender: "@robin:preview.invalid".into(),
                    display_name: "Robin".into(),
                    kind: MessageKind::Membership,
                    timestamp_ms: 1_794_300_000_000 + 70_000,
                    membership: Some(twayblade_core::conversation::Membership {
                        kind: twayblade_core::conversation::MembershipKind::Joined,
                        target: "@robin:preview.invalid".into(),
                        target_name: "Robin".into(),
                    }),
                    ..Default::default()
                },
            );
        }
    }
    snapshot.members = vec![
        Member {
            user_id: "@alex:preview.invalid".into(),
            display_name: "Alex".into(),
            role: MemberRole::Admin,
            avatar: None,
            ..Default::default()
        },
        Member {
            user_id: snapshot.user_id.clone(),
            display_name: "You".into(),
            role: MemberRole::Moderator,
            avatar: None,
            ..Default::default()
        },
        Member {
            user_id: "@sam:preview.invalid".into(),
            display_name: "Sam".into(),
            role: MemberRole::Member,
            avatar: None,
            can_kick: true,
            can_ban: true,
            can_mute: true,
            ..Default::default()
        },
    ];
    snapshot.members_state = MembersState::Ready;
    snapshot.typing = vec!["@alex:preview.invalid".into()];
    if showcase() && count < 100 {
        showcase_conversation(&mut snapshot);
    }
    snapshot
}

/// The showcase's own display name.
pub(crate) const OWN_NAME: &str = "Jordan";

/// The showcase's team conversation in #General (see [`showcase`]).
fn showcase_conversation(snapshot: &mut Snapshot) {
    use twayblade_core::rich_text::{RichNode, RichText, SpanKind};
    // An ordinary-looking own account rather than the preview's "You".
    snapshot.user_id = "@jordan:example.org".into();
    let own = snapshot.user_id.clone();
    let person = |name: &str| format!("@{}:preview.invalid", name.to_lowercase());
    let start = 1_794_300_000_000_i64;
    let message = |ix: usize, sender: &str, minute: i64, body: &str| Message {
        id: format!("sample-{ix}"),
        event_id: Some(format!("$sample-{ix}:preview.invalid")),
        sender: if sender == OWN_NAME {
            own.clone()
        } else {
            person(sender)
        },
        display_name: sender.into(),
        body: body.into(),
        kind: MessageKind::Text,
        timestamp_ms: start + minute * 60_000 + ix as i64 * 1_000,
        delivery: DeliveryState::Remote,
        ..Default::default()
    };
    let mut messages = vec![
        message(
            0,
            "Mira",
            0,
            "Morning! Onboarding v3 is ready for review. Feedback in the thread, please.",
        ),
        message(
            1,
            "Kai",
            8,
            "Looks great. The second step feels a bit crowded on smaller windows though.",
        ),
        message(
            2,
            "Sam",
            12,
            "Agreed. We could move the sync options into Settings.",
        ),
        message(
            3,
            OWN_NAME,
            19,
            "I’ll try that this afternoon. Anything else before Thursday’s review?",
        ),
        message(
            4,
            "Alex",
            28,
            "Release notes for 2.4 are in Announcements. Please skim them before Friday.",
        ),
        message(5, "Robin", 32, ""),
        message(6, "Kai", 40, ""),
        message(
            7,
            "Kai",
            40,
            "Pushed the icon fixes, they’ll be in tonight’s build.",
        ),
        message(8, "Mira", 46, "Sign-in with the tighter spacing:"),
        message(9, "Mira", 46, ""),
        message(
            10,
            OWN_NAME,
            51,
            "Much better. I’ll check the icons first thing tomorrow.",
        ),
    ];
    messages[0].thread_replies = 4;
    messages[0].reactions = vec![
        Reaction {
            key: "👍".into(),
            count: 3,
            mine: true,
        },
        Reaction {
            key: "🎉".into(),
            count: 2,
            mine: false,
        },
    ];
    messages[2].reply = Some(ReplyTarget::Ready {
        sender: "Kai".into(),
        excerpt: "Looks great. The second step feels a bit crowded on smaller windows though."
            .into(),
    });
    messages[2].reply_to = messages[1].event_id.clone();
    messages[3].delivery = DeliveryState::Sent;
    messages[4].rich = Some(std::sync::Arc::new(RichText {
        nodes: vec![
            RichNode::Text("Release notes for ".into()),
            RichNode::Span {
                kind: SpanKind::Strong,
                children: vec![RichNode::Text("2.4".into())],
            },
            RichNode::Text(" are in Announcements. Please skim them before Friday.".into()),
        ],
    }));
    messages[4].reactions = vec![Reaction {
        key: "👀".into(),
        count: 2,
        mine: false,
    }];
    messages[5].kind = MessageKind::Membership;
    messages[5].membership = Some(twayblade_core::conversation::Membership {
        kind: twayblade_core::conversation::MembershipKind::Joined,
        target: person("Robin"),
        target_name: "Robin".into(),
    });
    messages[6].kind = MessageKind::File;
    messages[6].attachment = Some(Attachment {
        preview_png: None,
        download_needed: false,
        filename: "icon-set-v2.zip".into(),
        size: Some(4_404_019),
    });
    // The picture comes from `--preview-showcase-image`.
    messages[9].kind = MessageKind::Image;
    messages[9].attachment = Some(Attachment {
        preview_png: SHOWCASE_IMAGE.get().cloned(),
        download_needed: false,
        filename: "sign-in-v3.png".into(),
        size: Some(486_400),
    });
    messages[10].delivery = DeliveryState::Sent;
    messages[10].read_by = vec![person("Mira"), person("Kai"), person("Alex")];
    snapshot.messages = messages;
    let member = |name: &str, role| Member {
        user_id: if name == OWN_NAME {
            own.clone()
        } else {
            person(name)
        },
        display_name: name.into(),
        role,
        avatar: None,
        ..Default::default()
    };
    snapshot.members = vec![
        member("Alex", MemberRole::Admin),
        member("Mira", MemberRole::Moderator),
        member(OWN_NAME, MemberRole::Moderator),
        member("Kai", MemberRole::Member),
        member("Sam", MemberRole::Member),
        member("Robin", MemberRole::Member),
    ];
    snapshot.typing.clear();
    snapshot.sync_status = "Connected".into();
    snapshot.timeline_status.clear();
    for (id, name, unread) in [("mira", "Mira", 1), ("kai", "Kai", 0)] {
        snapshot.rooms.push(Room {
            id: id.into(),
            name: name.into(),
            unread,
            encrypted: true,
            direct: true,
            partner: Some(person(name)),
            ..Default::default()
        });
    }
}

pub(super) fn typing_text(snapshot: &Snapshot) -> String {
    let names: Vec<_> = snapshot
        .typing
        .iter()
        .filter(|user| user.user_id != snapshot.user_id)
        .map(|user| {
            snapshot
                .members
                .iter()
                .find(|member| member.user_id == user.user_id)
                .map_or(user.name.as_str(), |member| member.display_name.as_str())
        })
        .collect();
    match names.as_slice() {
        [] => String::new(),
        [name] if snapshot.typing_others == 0 => format!("{name} is typing…"),
        [first, second] if snapshot.typing_others == 0 => {
            format!("{first} and {second} are typing…")
        }
        [first, ..] => format!(
            "{first} and {} others are typing…",
            names.len() - 1 + snapshot.typing_others
        ),
    }
}

/// Who has read up to the latest message: "Seen by Ana", "Seen by Ana and
/// Bo", "Seen by Ana, Bo and Cy", "Seen by Ana, Bo, Cy and 2 others".
pub(super) fn seen_by_line(names: &[String]) -> Option<String> {
    let shown = names.len().min(3);
    let rest = names.len() - shown;
    let list = match (&names[..shown], rest) {
        ([], _) => return None,
        ([one], 0) => one.clone(),
        ([first, second], 0) => format!("{first} and {second}"),
        ([first, second, third], 0) => format!("{first}, {second} and {third}"),
        (named, rest) => format!(
            "{} and {rest} {}",
            named.join(", "),
            if rest == 1 { "other" } else { "others" }
        ),
    };
    Some(format!("Seen by {list}"))
}

#[cfg(test)]
mod seen_tests {
    use super::seen_by_line;
    #[test]
    fn seen_by_names_up_to_three_and_counts_the_rest() {
        let names = |n: usize| -> Vec<String> {
            ["Ana", "Bo", "Cy", "Di", "Ed"][..n]
                .iter()
                .map(|s| s.to_string())
                .collect()
        };
        assert_eq!(seen_by_line(&names(0)), None);
        assert_eq!(seen_by_line(&names(1)).unwrap(), "Seen by Ana");
        assert_eq!(seen_by_line(&names(2)).unwrap(), "Seen by Ana and Bo");
        assert_eq!(seen_by_line(&names(3)).unwrap(), "Seen by Ana, Bo and Cy");
        assert_eq!(
            seen_by_line(&names(4)).unwrap(),
            "Seen by Ana, Bo, Cy and 1 other"
        );
        assert_eq!(
            seen_by_line(&names(5)).unwrap(),
            "Seen by Ana, Bo, Cy and 2 others"
        );
    }
}

pub(super) fn size_label(size: Option<u64>) -> String {
    match size {
        None => "size unknown".into(),
        Some(size) if size < 1024 => format!("{size} B"),
        Some(size) if size < 1_048_576 => format!("{:.0} KB", size as f64 / 1024.),
        Some(size) => format!("{:.1} MB", size as f64 / 1_048_576.),
    }
}

/// For a message made only of emoji (spaces allowed), how many there are, so
/// the timeline can show it larger. Limited to a short run; longer ones read as text.
pub(super) fn emoji_only_count(text: &str) -> Option<usize> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut count = 0;
    for grapheme in text.trim().graphemes(true) {
        if grapheme.trim().is_empty() {
            continue;
        }
        // Text-style hearts and similar arrive without the emoji selector.
        let is_emoji = emojis::get(grapheme).is_some()
            || emojis::get(&format!(
                "{}\u{FE0F}",
                grapheme.trim_end_matches('\u{FE0F}')
            ))
            .is_some();
        if !is_emoji || grapheme.is_ascii() {
            return None;
        }
        count += 1;
        if count > 10 {
            return None;
        }
    }
    (count > 0).then_some(count)
}

pub(super) fn message_text(message: &Message) -> String {
    if let Some(attachment) = &message.attachment {
        let size = match attachment.size {
            None => "size unknown".into(),
            Some(size) if size < 1024 => format!("{size} B"),
            Some(size) if size < 1_048_576 => format!("{:.0} KB", size as f64 / 1024.),
            Some(size) => format!("{:.1} MB", size as f64 / 1_048_576.),
        };
        format!("{} · {size}", attachment.filename)
    } else if message.kind == MessageKind::Emote {
        format!("{} {}", message.display_name, message.body)
    } else {
        message.body.clone()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn emoji_only_messages_are_recognised_and_text_is_not() {
        use super::emoji_only_count as count;
        assert_eq!(count("😀"), Some(1));
        assert_eq!(count(" 😀 👍🏽 "), Some(2));
        assert_eq!(count("👨‍👩‍👧"), Some(1));
        assert_eq!(count("❤️"), Some(1));
        assert_eq!(count("hi 😀"), None);
        assert_eq!(count("1"), None);
        assert_eq!(count("#"), None);
        assert_eq!(count(""), None);
        assert_eq!(count("😀😀😀😀😀😀😀😀😀😀😀"), None);
    }

    use super::*;

    #[test]
    fn subspaces_group_rooms_and_keep_shared_direct_links_and_orphans_visible() {
        let mut snapshot = preview(false, 3);
        snapshot.spaces[0].child_ids = ["child", "general"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        snapshot.spaces.push(Space {
            id: "child".into(),
            name: "Projects".into(),
            level: 1,
            child_ids: ["general", "research"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            room_ids: ["general", "research"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            ..Default::default()
        });
        let root = &snapshot.spaces[0];
        let child = &snapshot.spaces[1];
        assert!(Navigation::rail_space(root, &snapshot));
        assert!(!Navigation::rail_space(child, &snapshot));
        assert_eq!(Navigation::child_spaces(root, &snapshot)[0].id, "child");
        assert!(Navigation::room_in_space_group(root, "general", &snapshot));
        assert!(!Navigation::room_in_space_group(
            root, "research", &snapshot
        ));
        assert!(Navigation::room_in_space_group(
            child, "research", &snapshot
        ));
        snapshot.spaces.remove(0);
        assert!(Navigation::rail_space(&snapshot.spaces[0], &snapshot));
    }

    #[test]
    fn direct_messages_live_in_home_even_when_linked_to_a_space() {
        let mut snapshot = preview(false, 6);
        snapshot.rooms.push(Room {
            id: "dm".into(),
            direct: true,
            ..Default::default()
        });
        snapshot.spaces[0].room_ids.insert("dm".into());
        let mut navigation = Navigation {
            selected_space: Some("design".into()),
            ..Default::default()
        };
        assert!(!navigation.room_visible("dm", &snapshot));
        navigation.route_room("dm", &snapshot);
        assert!(navigation.selected_space.is_none());
        assert!(navigation.room_visible("dm", &snapshot));
        snapshot.spaces_loaded = false;
        assert!(navigation.room_visible("dm", &snapshot));
    }

    #[test]
    fn external_rooms_route_home_and_voice_channels_stay_inside_spaces() {
        let mut snapshot = preview(false, 6);
        snapshot.rooms.push(Room {
            id: "external".into(),
            ..Default::default()
        });
        snapshot.rooms.push(Room {
            id: "voice".into(),
            channel_kind: twayblade_core::channels::ChannelKind::Voice,
            ..Default::default()
        });
        snapshot.spaces[0].room_ids.insert("voice".into());
        let mut navigation = Navigation {
            selected_space: Some("design".into()),
            ..Default::default()
        };
        navigation.route_room("external", &snapshot);
        assert!(navigation.selected_space.is_none());
        assert!(navigation.room_visible("external", &snapshot));
        assert!(!navigation.room_visible("voice", &snapshot));
        navigation.route_room("voice", &snapshot);
        assert_eq!(navigation.selected_space.as_deref(), Some("design"));
        assert!(navigation.room_visible("voice", &snapshot));
        navigation.route_room("not-yet-synced", &snapshot);
        assert_eq!(navigation.selected_space.as_deref(), Some("design"));
    }

    #[test]
    fn joined_voice_without_a_joined_parent_remains_reachable_in_home() {
        let mut snapshot = preview(false, 6);
        snapshot.rooms.push(Room {
            id: "voice".into(),
            channel_kind: twayblade_core::channels::ChannelKind::Voice,
            ..Default::default()
        });
        snapshot.spaces[0].room_ids.insert("voice".into());
        let mut navigation = Navigation::default();
        assert!(!navigation.room_visible("voice", &snapshot));
        snapshot.spaces.clear();
        navigation.route_room("voice", &snapshot);
        assert!(navigation.selected_space.is_none());
        assert!(navigation.room_visible("voice", &snapshot));
        navigation.selected_room = Some("voice".into());
        assert!(!navigation.reconcile(&snapshot));
    }

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
        // The sample's membership line (index 3) is a group of its own.
        assert_eq!(grouped(&snapshot.messages), vec![0..3, 3..4, 4..7]);
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

#[cfg(test)]
mod room_state_tests {
    use super::*;
    #[test]
    fn typing_labels_resolve_members_and_do_not_include_our_own_user() {
        let mut snapshot = preview(false, 6);
        assert!(typing_text(&snapshot) == "Alex is typing…");
        snapshot.typing = vec![snapshot.user_id.clone().into()];
        assert!(typing_text(&snapshot).is_empty());
        snapshot.typing = vec![
            "@alex:preview.invalid".into(),
            "@sam:preview.invalid".into(),
        ];
        assert!(typing_text(&snapshot) == "Alex and Sam are typing…");
    }
    #[test]
    fn read_boundary_splits_a_sender_group_without_changing_message_ids() {
        let mut snapshot = preview(false, 6);
        snapshot.messages[1].new_messages = true;
        assert!(grouped(&snapshot.messages)[0] == (0..1));
        assert!(snapshot.messages[1].id == "sample-1");
    }
}

/// A fingerprint of everything that can change a message group's rendered
/// height. Only groups whose fingerprint changes are re-measured, so typing
/// notices, receipts and sync ticks don't re-layout the whole history.
pub(super) fn group_signature(messages: &[Message], range: std::ops::Range<usize>) -> u64 {
    use std::hash::{Hash, Hasher};
    use twayblade_core::conversation::ReplyTarget;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // The previous message decides time markers above the group.
    range
        .start
        .checked_sub(1)
        .and_then(|ix| messages.get(ix))
        .map(|previous| (previous.timestamp_ms, &previous.sender))
        .hash(&mut hasher);
    for message in &messages[range] {
        message.id.hash(&mut hasher);
        message.display_name.hash(&mut hasher);
        message.body.hash(&mut hasher);
        message.rich.hash(&mut hasher);
        std::mem::discriminant(&message.kind).hash(&mut hasher);
        std::mem::discriminant(&message.delivery).hash(&mut hasher);
        message.edited.hash(&mut hasher);
        message.new_messages.hash(&mut hasher);
        message.thread_replies.hash(&mut hasher);
        message.thread_unread.hash(&mut hasher);
        message.read_by.len().hash(&mut hasher);
        message.can_retry.hash(&mut hasher);
        message.upload_percent.hash(&mut hasher);
        message.sender_avatar.is_some().hash(&mut hasher);
        for reaction in &message.reactions {
            (&reaction.key, reaction.count, reaction.mine).hash(&mut hasher);
        }
        message.reply_to.hash(&mut hasher);
        match &message.reply {
            Some(ReplyTarget::Ready { sender, excerpt }) => {
                (1u8, sender, excerpt).hash(&mut hasher)
            }
            Some(ReplyTarget::Pending) => 2u8.hash(&mut hasher),
            Some(ReplyTarget::Unavailable) => 3u8.hash(&mut hasher),
            None => 0u8.hash(&mut hasher),
        }
        if let Some(attachment) = &message.attachment {
            (
                &attachment.filename,
                attachment.size,
                attachment.preview_png.is_some(),
            )
                .hash(&mut hasher);
        }
    }
    hasher.finish()
}

#[cfg(test)]
mod rich_tests {
    use super::*;
    #[test]
    fn formatting_only_edits_invalidate_the_message_group_fingerprint() {
        use twayblade_core::rich_text::{RichNode, RichText, SpanKind};
        let mut message = Message {
            id: "$same".into(),
            body: "same plain text".into(),
            ..Default::default()
        };
        let plain = group_signature(&[message.clone()], 0..1);
        message.rich = Some(std::sync::Arc::new(RichText {
            nodes: vec![RichNode::Span {
                kind: SpanKind::Strong,
                children: vec![RichNode::Text(message.body.clone())],
            }],
        }));
        assert_ne!(plain, group_signature(&[message], 0..1));
    }
}

/// The web link a message previews: the first http(s) link, from its
/// formatting or its text, outside code. Notices (usually bots) and media
/// never preview, nor do Matrix links.
pub(crate) fn preview_link(message: &crate::backend::Message) -> Option<String> {
    use crate::backend::MessageKind;
    use twayblade_core::rich_text::{BlockKind, RichNode, SpanKind};
    if !matches!(message.kind, MessageKind::Text | MessageKind::Emote)
        || message.attachment.is_some()
    {
        return None;
    }
    fn web(link: &str) -> Option<String> {
        let url = url::Url::parse(link).ok()?;
        (matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some_and(|host| host != "matrix.to"))
        .then(|| url.to_string())
    }
    fn in_rich(nodes: &[RichNode]) -> Option<String> {
        nodes.iter().find_map(|node| match node {
            RichNode::Text(_) => None,
            RichNode::Span {
                kind: SpanKind::Code,
                ..
            } => None,
            RichNode::Block {
                kind: BlockKind::Code { .. } | BlockKind::Quote,
                ..
            } => None,
            RichNode::Span {
                kind: SpanKind::Link { target },
                ..
            } => web(target),
            RichNode::Span { children, .. } | RichNode::Block { children, .. } => in_rich(children),
        })
    }
    if let Some(rich) = &message.rich
        && let Some(link) = in_rich(&rich.nodes)
    {
        return Some(link);
    }
    // Plain text: the first bare link, without the punctuation around it.
    let mut in_code = false;
    for word in message.body.split_whitespace() {
        if word.contains('`') {
            in_code ^= word.matches('`').count() % 2 == 1;
            continue;
        }
        if in_code {
            continue;
        }
        let candidate = word
            .trim_start_matches(['(', '<', '[', '"', '\''])
            .trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '>', ']', '"', '\'']);
        if (candidate.starts_with("https://") || candidate.starts_with("http://"))
            && let Some(link) = web(candidate)
        {
            return Some(link);
        }
    }
    None
}

/// The user a mention (pill) links to: `https://matrix.to/#/@user:server`,
/// percent-encoded or not and with optional `?via=`, or `matrix:u/user:server`.
/// None for every other link, which opens in the browser.
pub(super) fn mentioned_user(link: &str) -> Option<String> {
    let url = url::Url::parse(link).ok()?;
    let id = match url.scheme() {
        "https" if url.host_str() == Some("matrix.to") => {
            let fragment = url.fragment()?.strip_prefix('/')?;
            percent_decode(fragment.split(['?', '/']).next()?)?
        }
        "matrix" => {
            let user = url.path().strip_prefix("u/")?.split(['?', '/']).next()?;
            format!("@{}", percent_decode(user)?)
        }
        _ => return None,
    };
    let (local, server) = id.strip_prefix('@')?.split_once(':')?;
    (!local.is_empty() && !server.is_empty()).then_some(id)
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut ix = 0;
    while ix < bytes.len() {
        if bytes[ix] == b'%' {
            let hex = text.get(ix + 1..ix + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            ix += 3;
        } else {
            out.push(bytes[ix]);
            ix += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod preview_link_tests {
    use super::preview_link;
    use crate::backend::{Message, MessageKind};

    fn text(body: &str) -> Message {
        Message {
            id: "$m".into(),
            body: body.into(),
            kind: MessageKind::Text,
            ..Default::default()
        }
    }

    #[test]
    fn the_first_web_link_outside_code_previews() {
        assert_eq!(
            preview_link(&text("see (https://example.org/a?b=1).")).as_deref(),
            Some("https://example.org/a?b=1")
        );
        assert_eq!(
            preview_link(&text("`https://code.example` then http://example.org")).as_deref(),
            Some("http://example.org/")
        );
        assert_eq!(preview_link(&text("``` https://code.example ```")), None);
        assert_eq!(preview_link(&text("https://matrix.to/#/@a:b.c")), None);
        assert_eq!(preview_link(&text("mailto:a@example.org ftp://x")), None);
        let mut notice = text("https://example.org");
        notice.kind = MessageKind::Notice;
        assert_eq!(preview_link(&notice), None);
    }
}

#[cfg(test)]
mod mention_tests {
    use super::mentioned_user;
    #[test]
    fn mention_links_name_the_user_and_other_links_do_not() {
        for link in [
            "https://matrix.to/#/@hristo:example.org",
            "https://matrix.to/#/%40hristo%3Aexample.org",
            "https://matrix.to/#/@hristo:example.org?via=example.org",
            "matrix:u/hristo:example.org",
        ] {
            assert_eq!(
                mentioned_user(link).as_deref(),
                Some("@hristo:example.org"),
                "{link}"
            );
        }
        for link in [
            "https://matrix.to/#/#room:example.org",
            "https://matrix.to/#/!room:example.org/$event",
            "https://example.org/#/@hristo:example.org",
            "matrix:r/room:example.org",
            "https://matrix.to/#/@:example.org",
        ] {
            assert_eq!(mentioned_user(link), None, "{link}");
        }
    }
}

/// Sanitized rich text (from core) to Markdown for Kit's selectable text view.
/// Every text run is escaped; only structure produces Markdown syntax, so
/// message content can never inject formatting or links of its own.
pub(super) fn rich_markdown(rich: &twayblade_core::rich_text::RichText) -> String {
    let mut blocks = Vec::new();
    rich_blocks(&rich.nodes, &mut blocks);
    blocks.join("\n\n")
}

fn escape_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            // HTML text collapses newlines to spaces.
            '\n' | '\r' => out.push(' '),
            ch if ch.is_ascii_punctuation() => {
                out.push('\\');
                out.push(ch);
            }
            ch => out.push(ch),
        }
    }
    out
}

fn raw_text(nodes: &[twayblade_core::rich_text::RichNode], out: &mut String) {
    use twayblade_core::rich_text::{RichNode, SpanKind};
    for node in nodes {
        match node {
            RichNode::Text(text) => out.push_str(text),
            RichNode::Span {
                kind: SpanKind::LineBreak,
                ..
            } => out.push('\n'),
            RichNode::Span { children, .. } | RichNode::Block { children, .. } => {
                raw_text(children, out)
            }
        }
    }
}

/// The shortest backtick run longer than any run inside `text`.
fn fence(text: &str, minimum: usize) -> String {
    let longest = text.split(|ch| ch != '`').map(str::len).max().unwrap_or(0);
    "`".repeat((longest + 1).max(minimum))
}

fn rich_inline(nodes: &[twayblade_core::rich_text::RichNode]) -> String {
    use twayblade_core::rich_text::{RichNode, SpanKind};
    let mut out = String::new();
    for node in nodes {
        match node {
            RichNode::Text(text) => out.push_str(&escape_inline(text)),
            RichNode::Block { children, .. } => {
                if !out.is_empty() && !out.ends_with(' ') {
                    out.push(' ');
                }
                out.push_str(&rich_inline(children));
            }
            RichNode::Span { kind, children } => {
                let inner = rich_inline(children);
                match kind {
                    SpanKind::Strong if !inner.trim().is_empty() => {
                        out.push_str(&format!("**{inner}**"))
                    }
                    SpanKind::Emphasis if !inner.trim().is_empty() => {
                        out.push_str(&format!("*{inner}*"))
                    }
                    SpanKind::Strike if !inner.trim().is_empty() => {
                        out.push_str(&format!("~~{inner}~~"))
                    }
                    SpanKind::Code => {
                        let mut code = String::new();
                        raw_text(children, &mut code);
                        let code = code.replace('\n', " ");
                        if !code.is_empty() {
                            let ticks = fence(&code, 1);
                            let pad = if code.starts_with('`') || code.ends_with('`') {
                                " "
                            } else {
                                ""
                            };
                            out.push_str(&format!("{ticks}{pad}{code}{pad}{ticks}"));
                        }
                    }
                    SpanKind::Link { target } => match url::Url::parse(target) {
                        // A mention reads as one, "@Name" in bold, not as a web link.
                        Ok(url) if mentioned_user(target).is_some() && !inner.trim().is_empty() => {
                            let at = if inner.starts_with("\\@") { "" } else { "\\@" };
                            out.push_str(&format!("**[{at}{inner}](<{url}>)**"))
                        }
                        // Re-serialised so spaces and angle brackets are percent-encoded.
                        Ok(url) if !inner.trim().is_empty() => {
                            out.push_str(&format!("[{inner}](<{url}>)"))
                        }
                        Ok(url) => out.push_str(&format!("<{url}>")),
                        Err(_) => out.push_str(&inner),
                    },
                    // Hidden until the spoiler UI exists; never shown by accident.
                    SpanKind::Spoiler { .. } => out.push_str("*\\[spoiler\\]*"),
                    SpanKind::LineBreak => out.push_str("\\\n"),
                    SpanKind::Image { alt, .. } => {
                        out.push_str(&escape_inline(if alt.is_empty() { "[image]" } else { alt }))
                    }
                    _ => out.push_str(&inner),
                }
            }
        }
    }
    out
}

fn rich_blocks(nodes: &[twayblade_core::rich_text::RichNode], out: &mut Vec<String>) {
    use twayblade_core::rich_text::{BlockKind, RichNode};
    let mut inline: Vec<RichNode> = Vec::new();
    let flush = |inline: &mut Vec<RichNode>, out: &mut Vec<String>| {
        let text = rich_inline(inline);
        if !text.trim().is_empty() {
            out.push(text.trim().to_owned());
        }
        inline.clear();
    };
    for node in nodes {
        let RichNode::Block { kind, children } = node else {
            inline.push(node.clone());
            continue;
        };
        flush(&mut inline, out);
        match kind {
            BlockKind::Heading(level) => {
                let text = rich_inline(children);
                if !text.trim().is_empty() {
                    out.push(format!(
                        "{} {}",
                        "#".repeat((*level).clamp(1, 6) as usize),
                        text.trim()
                    ));
                }
            }
            BlockKind::Quote => {
                let mut inner = Vec::new();
                rich_blocks(children, &mut inner);
                let quoted = inner
                    .join("\n\n")
                    .lines()
                    .map(|line| {
                        if line.is_empty() {
                            ">".to_owned()
                        } else {
                            format!("> {line}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if !quoted.is_empty() {
                    out.push(quoted);
                }
            }
            BlockKind::Code { language } => {
                let mut code = String::new();
                raw_text(children, &mut code);
                let code = code.trim_end_matches('\n');
                let ticks = fence(code, 3);
                let language: String = language
                    .as_deref()
                    .unwrap_or("")
                    .chars()
                    .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '_'))
                    .take(32)
                    .collect();
                out.push(format!("{ticks}{language}\n{code}\n{ticks}"));
            }
            BlockKind::List { ordered, start } => {
                let mut items = Vec::new();
                for (number, item) in (*start..).zip(children.iter()) {
                    let item_children = match item {
                        RichNode::Block {
                            kind: BlockKind::ListItem,
                            children,
                        } => children.as_slice(),
                        other => std::slice::from_ref(other),
                    };
                    let mut inner = Vec::new();
                    rich_blocks(item_children, &mut inner);
                    let marker = if *ordered {
                        format!("{number}. ")
                    } else {
                        "- ".to_owned()
                    };
                    let indent = " ".repeat(marker.len());
                    let body = inner.join("\n\n");
                    let mut lines = body.lines();
                    let mut text = format!("{marker}{}", lines.next().unwrap_or(""));
                    for line in lines {
                        text.push('\n');
                        if !line.is_empty() {
                            text.push_str(&indent);
                            text.push_str(line);
                        }
                    }
                    items.push(text);
                }
                if !items.is_empty() {
                    out.push(items.join("\n"));
                }
            }
            BlockKind::Divider => out.push("---".into()),
            BlockKind::Table => {
                let mut rows = Vec::new();
                collect_rows(children, &mut rows);
                let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
                if columns > 0 {
                    let line = |cells: &[String]| {
                        let mut cells = cells.to_vec();
                        cells.resize(columns, String::new());
                        format!("| {} |", cells.join(" | "))
                    };
                    let mut table = vec![line(&rows[0]), format!("|{}", " --- |".repeat(columns))];
                    table.extend(rows[1..].iter().map(|row| line(row)));
                    out.push(table.join("\n"));
                }
            }
            _ => rich_blocks(children, out),
        }
    }
    flush(&mut inline, out);
}

fn collect_rows(nodes: &[twayblade_core::rich_text::RichNode], rows: &mut Vec<Vec<String>>) {
    use twayblade_core::rich_text::{BlockKind, RichNode};
    for node in nodes {
        if let RichNode::Block { kind, children } = node {
            match kind {
                BlockKind::TableRow => rows.push(
                    children
                        .iter()
                        .filter_map(|cell| match cell {
                            RichNode::Block {
                                kind: BlockKind::TableCell { .. },
                                children,
                            } => Some(rich_inline(children).trim().to_owned()),
                            _ => None,
                        })
                        .collect(),
                ),
                _ => collect_rows(children, rows),
            }
        }
    }
}

#[cfg(test)]
mod rich_markdown_tests {
    use super::rich_markdown;
    use matrix_sdk::ruma::events::room::message::FormattedBody;
    use twayblade_core::rich_text::incoming;

    fn render(html: &str) -> String {
        rich_markdown(&incoming(&FormattedBody::html(html)).unwrap())
    }

    #[test]
    fn structure_becomes_markdown_and_text_stays_escaped() {
        assert_eq!(render("<b>bold</b> and <em>it</em>"), "**bold** and *it*");
        assert_eq!(render("<code>a`b</code>"), "``a`b``");
        assert_eq!(
            render("<a href='https://example.org/a b'>site</a>"),
            "[site](<https://example.org/a%20b>)"
        );
        // Mentions read as "@Name", with or without the sender's own "@".
        assert_eq!(
            render("<a href='https://matrix.to/#/@hd:voltized.de'>hristo</a>"),
            "**[\\@hristo](<https://matrix.to/#/@hd:voltized.de>)**"
        );
        assert_eq!(
            render("<a href='https://matrix.to/#/@hd:voltized.de'>@hristo</a>"),
            "**[\\@hristo](<https://matrix.to/#/@hd:voltized.de>)**"
        );
        assert_eq!(render("<ul><li>one</li><li>two</li></ul>"), "- one\n- two");
        assert_eq!(render("<ol start='3'><li>x</li></ol>"), "3. x");
        assert_eq!(render("<blockquote><p>q</p></blockquote>"), "> q");
        assert_eq!(
            render("<pre><code class='language-rust'>let a = 1;\n</code></pre>"),
            "```rust\nlet a = 1;\n```"
        );
        // Text that looks like Markdown is shown literally.
        assert_eq!(
            render("<p>**not bold** [x](y)</p>"),
            "\\*\\*not bold\\*\\* \\[x\\]\\(y\\)"
        );
        // Spoilers stay hidden.
        assert!(!render("<span data-mx-spoiler>secret</span>").contains("secret"));
    }
}

#[cfg(test)]
mod membership_tests {
    use super::{Message, MessageKind, grouped};

    fn message(id: &str, sender: &str, kind: MessageKind, at: i64) -> Message {
        Message {
            id: id.into(),
            sender: sender.into(),
            kind,
            timestamp_ms: 1_794_300_000_000 + at,
            ..Default::default()
        }
    }

    #[test]
    fn membership_changes_group_together_and_apart_from_messages() {
        let messages = [
            message("a", "@alex", MessageKind::Text, 0),
            message("b", "@robin", MessageKind::Membership, 1_000),
            message("c", "@sam", MessageKind::Membership, 2_000),
            message("d", "@alex", MessageKind::Text, 3_000),
        ];
        assert_eq!(grouped(&messages), vec![0..1, 1..3, 3..4]);
    }
}
