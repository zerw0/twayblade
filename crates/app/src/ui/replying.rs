//! Replying to a message in the main composer: Reply in the message actions or
//! menu, a bar above the field naming the message, Escape or × to cancel.
//! The SDK builds the quote and mention when the reply is sent.
use crate::app::MatrixApp;
use crate::backend::Message;
use crate::theme::scaled_px as px;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::*;

/// The message the next main-composer send replies to.
pub(crate) struct ReplyDraft {
    pub room: String,
    pub event_id: String,
    pub sender: String,
    pub excerpt: String,
}

/// One line of the quoted message: its text, or the file's name.
pub(crate) fn reply_excerpt(message: &Message) -> String {
    let text = match &message.attachment {
        Some(attachment) if message.body.is_empty() || message.body == attachment.filename => {
            attachment.filename.clone()
        }
        _ => message.body.clone(),
    };
    let line = text.lines().next().unwrap_or_default().trim();
    let mut excerpt: String = line.chars().take(80).collect();
    if line.chars().count() > 80 {
        excerpt.push('…');
    }
    excerpt
}

impl MatrixApp {
    pub(crate) fn start_reply(
        &mut self,
        message_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(room) = self.navigation.selected_room.clone() else {
            return;
        };
        let Some(message) = self
            .snapshot
            .messages
            .iter()
            .find(|message| message.id == message_id && message.event_id.is_some())
        else {
            return;
        };
        let draft = ReplyDraft {
            room,
            event_id: message.event_id.clone().unwrap_or_default(),
            sender: if message.sender == self.snapshot.user_id {
                "yourself".to_owned()
            } else {
                message.display_name.clone()
            },
            excerpt: reply_excerpt(message),
        };
        // Replying and editing share the composer: one at a time.
        self.end_edit(window, cx);
        self.reply_to = Some(draft);
        let composer = self.composer.clone();
        composer.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn cancel_reply(&mut self, cx: &mut Context<Self>) -> bool {
        let cancelled = self.reply_to.take().is_some();
        cx.notify();
        cancelled
    }

    /// The reply for the open room, if one is being written.
    pub(crate) fn open_reply(&self) -> Option<&ReplyDraft> {
        self.reply_to
            .as_ref()
            .filter(|reply| self.navigation.selected_room.as_ref() == Some(&reply.room))
    }

    /// The bar above the main composer while replying.
    pub(crate) fn reply_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use gpui_kit::component::button::{Button, ButtonVariants};
        use gpui_kit::component::*;
        let reply = self.open_reply()?;
        let muted = cx.theme().muted_foreground;
        Some(
            h_flex()
                .id("reply-bar")
                .px(px(12.))
                .pt(px(8.))
                .gap(px(8.))
                .items_center()
                .text_size(px(12.))
                .child(
                    Icon::new(gpui_kit::assets::IconName::CornerUpLeft)
                        .size(px(13.))
                        .text_color(cx.theme().primary),
                )
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(6.))
                        .child(
                            div()
                                .flex_none()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("Replying to {}", reply.sender)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(muted)
                                .child(reply.excerpt.clone()),
                        ),
                )
                .child(
                    Button::new("reply-cancel")
                        .ghost()
                        .xsmall()
                        .icon(gpui_kit::assets::IconName::X)
                        .accessibility_label("Cancel reply")
                        .tooltip("Cancel Reply  Esc")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cancel_reply(cx);
                        })),
                )
                .test_support()
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::reply_excerpt;
    use crate::backend::Message;
    #[test]
    fn the_excerpt_is_the_first_line_or_the_file_name() {
        let text = Message {
            body: "first line\nsecond line".into(),
            ..Default::default()
        };
        assert_eq!(reply_excerpt(&text), "first line");
        let long = Message {
            body: "x".repeat(90),
            ..Default::default()
        };
        assert_eq!(reply_excerpt(&long).chars().count(), 81);
        assert!(reply_excerpt(&long).ends_with('…'));
    }
}
