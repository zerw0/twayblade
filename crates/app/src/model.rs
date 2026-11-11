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
    pub fn room_visible(&self, id: &str, snapshot: &Snapshot) -> bool {
        if let Some(selected) = &self.selected_space {
            return snapshot
                .spaces
                .iter()
                .find(|space| &space.id == selected)
                .is_some_and(|space| space.room_ids.contains(id));
        }
        snapshot.spaces_loaded
            && !snapshot
                .spaces
                .iter()
                .any(|space| space.room_ids.contains(id))
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
            !message.new_messages
                && !boundaries.contains(&message.id)
                && previous.sender == message.sender
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
            avatar: None,
            can_add_rooms: true,
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
                filename: "sample-mockup.png".into(),
                size: Some(245_760),
            });
        }
        if let Some(message) = snapshot.messages.get_mut(5) {
            message.kind = MessageKind::File;
            message.attachment = Some(Attachment {
                preview_png: None,
                filename: "sample-notes.pdf".into(),
                size: Some(1_048_576),
            });
            message.delivery = DeliveryState::Failed;
            message.can_retry = true;
            message.event_id = None;
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
    snapshot
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

pub(super) fn size_label(size: Option<u64>) -> String {
    match size {
        None => "size unknown".into(),
        Some(size) if size < 1024 => format!("{size} B"),
        Some(size) if size < 1_048_576 => format!("{:.0} KB", size as f64 / 1024.),
        Some(size) => format!("{:.1} MB", size as f64 / 1_048_576.),
    }
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
