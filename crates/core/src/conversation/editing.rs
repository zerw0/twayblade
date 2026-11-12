//! Edit sources are the latest plain body (our composer preserves Markdown there).
//! Other clients' HTML-only formatting is lost when saving this source as Markdown.
use super::Conversation;
use matrix_sdk::{room::edit::EditedContent, ruma::events::room::message::MessageType};
use matrix_sdk_ui::timeline::{EventSendState, EventTimelineItem};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("not_editable")]
    NotEditable,
    #[error("unchanged")]
    Unchanged,
    #[error("too_long")]
    TooLong,
    /// An edit cannot remove all text; deleting is a separate action.
    #[error("empty")]
    Empty,
    #[error("edit_failed")]
    Failed,
}

/// Our own sent text, notice or emote. `Timeline::edit` only requires our own
/// event; the SDK's `is_editable` omits notices, so ownership is checked here
/// instead of patching the SDK.
pub(super) fn editable(event: &EventTimelineItem) -> bool {
    event.is_own()
        && event.event_id().is_some()
        && matches!(event.send_state(), None | Some(EventSendState::Sent { .. }))
        && event.content().as_message().is_some_and(|m| {
            matches!(
                m.msgtype(),
                MessageType::Text(_) | MessageType::Notice(_) | MessageType::Emote(_)
            )
        })
}

fn replacement(original: &MessageType, body: String) -> Result<MessageType, EditError> {
    // Compare the source before composing: re-saving imported HTML unchanged must
    // not strip its formatting or create an unnecessary replacement event.
    if original.body() == body {
        return Err(EditError::Unchanged);
    }
    let composed = crate::rich_text::compose(body).map_err(|e| match e {
        crate::rich_text::FormattingError::Limit => EditError::TooLong,
        crate::rich_text::FormattingError::Empty => EditError::Empty,
    })?;
    let MessageType::Text(text) = composed.msgtype else {
        return Err(EditError::Failed);
    };
    Ok(match original {
        MessageType::Text(_) => MessageType::Text(text),
        MessageType::Notice(_) => {
            let mut content =
                matrix_sdk::ruma::events::room::message::NoticeMessageEventContent::plain(
                    text.body,
                );
            content.formatted = text.formatted;
            MessageType::Notice(content)
        }
        MessageType::Emote(_) => {
            let mut content =
                matrix_sdk::ruma::events::room::message::EmoteMessageEventContent::plain(text.body);
            content.formatted = text.formatted;
            MessageType::Emote(content)
        }
        _ => return Err(EditError::NotEditable),
    })
}

impl Conversation {
    /// Latest plain body, preserving our original Markdown exactly. Imported HTML
    /// formatting not representable in this source is lost on a changed re-save.
    pub async fn edit_source(&self, event_id: &str) -> Result<String, EditError> {
        let items = self.timeline.items().await;
        let event = items
            .iter()
            .filter_map(|i| i.as_event())
            .find(|e| e.event_id().is_some_and(|id| id.as_str() == event_id) && editable(e))
            .ok_or(EditError::NotEditable)?;
        Ok(event
            .content()
            .as_message()
            .ok_or(EditError::NotEditable)?
            .body()
            .to_owned())
    }

    pub async fn edit(&self, event_id: &str, new_body: String) -> Result<(), EditError> {
        let items = self.timeline.items().await;
        let event = items
            .iter()
            .filter_map(|i| i.as_event())
            .find(|e| e.event_id().is_some_and(|id| id.as_str() == event_id) && editable(e))
            .ok_or(EditError::NotEditable)?;
        let original = event.content().as_message().ok_or(EditError::NotEditable)?;
        let content = matrix_sdk::ruma::events::room::message::RoomMessageEventContent::new(
            replacement(original.msgtype(), new_body)?,
        );
        self.timeline
            .edit(
                &event.identifier(),
                EditedContent::RoomMessage(content.into()),
            )
            .await
            .map_err(|_| EditError::Failed)
    }

    /// Loaded timeline only, in SDK timeline order. A thread conversation excludes
    /// its root; a main conversation excludes thread replies.
    async fn editable_ids(&self, root: Option<&str>) -> Vec<String> {
        if self.root.as_deref() != root {
            return Vec::new();
        }
        self.timeline
            .items()
            .await
            .iter()
            .filter_map(|i| i.as_event())
            .filter(|e| {
                editable(e) && e.content().thread_root().as_deref().map(|id| id.as_str()) == root
            })
            .filter_map(|e| e.event_id().map(ToString::to_string))
            .collect()
    }

    pub async fn latest_editable(&self, root: Option<&str>) -> Option<String> {
        self.editable_ids(root).await.pop()
    }
    pub async fn previous_editable(&self, from: &str) -> Option<String> {
        adjacent(&self.editable_ids(self.root.as_deref()).await, from, false)
    }
    pub async fn next_editable(&self, from: &str) -> Option<String> {
        adjacent(&self.editable_ids(self.root.as_deref()).await, from, true)
    }
}
fn adjacent(ids: &[String], from: &str, next: bool) -> Option<String> {
    let i = ids.iter().position(|id| id == from)?;
    ids.get(if next {
        i.checked_add(1)?
    } else {
        i.checked_sub(1)?
    })
    .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composition_preserves_sources_types_and_unchanged_imported_html() {
        use matrix_sdk::ruma::events::room::message::RoomMessageEventContent as Content;
        for source in ["plain source", "**bold** and `code`\n\n- item"] {
            let original = crate::rich_text::compose(source.into()).unwrap().msgtype;
            assert!(matches!(
                replacement(&original, source.into()),
                Err(EditError::Unchanged)
            ));
            let changed = replacement(&original, format!("{source}!")).unwrap();
            assert!(changed.body() == format!("{source}!"));
            assert!(
                serde_json::to_value(&changed).unwrap()
                    == serde_json::to_value(
                        crate::rich_text::compose(format!("{source}!"))
                            .unwrap()
                            .msgtype
                    )
                    .unwrap()
            );
        }
        let html = Content::text_html("plain", "<u>plain</u>").msgtype;
        assert!(matches!(
            replacement(&html, "plain".into()),
            Err(EditError::Unchanged)
        ));
        assert!(matches!(
            replacement(&html, "x".repeat(65537)),
            Err(EditError::TooLong)
        ));
        assert!(matches!(
            replacement(&html, "  ".into()),
            Err(EditError::Empty)
        ));
        assert!(matches!(
            replacement(&Content::notice_plain("notice").msgtype, "**new**".into()).unwrap(),
            MessageType::Notice(_)
        ));
        assert!(matches!(
            replacement(&Content::emote_plain("emote").msgtype, "new".into()).unwrap(),
            MessageType::Emote(_)
        ));
    }
    #[test]
    fn navigation_stops_at_boundaries_and_rejects_missing_events() {
        let ids = vec!["first".into(), "middle".into(), "last".into()];
        assert_eq!(adjacent(&ids, "middle", false).as_deref(), Some("first"));
        assert_eq!(adjacent(&ids, "middle", true).as_deref(), Some("last"));
        assert_eq!(adjacent(&ids, "first", false), None);
        assert_eq!(adjacent(&ids, "last", true), None);
        assert_eq!(adjacent(&ids, "missing", false), None);
    }
}
