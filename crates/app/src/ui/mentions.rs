//! Mention completion: typing `@` in the room or thread composer lists the
//! room's members; picking one writes `@Name ` and remembers who it is, so
//! the sent message links them and lists them in `m.mentions`.
use std::ops::Range;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        input::TextareaState,
        *,
    },
    *,
};
use twayblade_core::rich_text::Mention;

use crate::app::MatrixApp;
use crate::theme::scaled_px as px;
use twayblade_core::room_state::Member;

/// People listed at once.
const VISIBLE: usize = 6;

pub(crate) struct MentionCompletion {
    pub in_thread: bool,
    /// The composer text the range refers to; a pick against changed text is
    /// dropped.
    pub original: String,
    /// Byte range of `@query` in `original`.
    pub range: Range<usize>,
    pub query: String,
    pub selected: usize,
}

/// `@query` ending at the caret: the `@` starts the text or follows
/// whitespace, and the query has no spaces (names are matched by any word).
pub(crate) fn mention_at(text: &str, cursor: usize) -> Option<(Range<usize>, String)> {
    let before = text.get(..cursor)?;
    let start = before.rfind('@')?;
    if start > 0 && !before[..start].chars().next_back()?.is_whitespace() {
        return None;
    }
    let query = &before[start + 1..];
    if query.chars().count() > 32 || query.chars().any(char::is_whitespace) {
        return None;
    }
    Some((start..cursor, query.to_lowercase()))
}

/// Members matching `query` by display name or Matrix ID, names starting
/// with it first, without `own`.
pub(crate) fn mention_candidates<'a>(
    members: &'a [Member],
    query: &str,
    own: &str,
) -> Vec<&'a Member> {
    let query = query.to_lowercase();
    let mut matched: Vec<(bool, &Member)> = members
        .iter()
        .filter(|member| member.user_id != own)
        .filter_map(|member| {
            let name = member.display_name.to_lowercase();
            let id = member.user_id.to_lowercase();
            let prefix = name.starts_with(&query)
                || name.split_whitespace().any(|word| word.starts_with(&query))
                || id.trim_start_matches('@').starts_with(&query);
            (prefix || name.contains(&query) || id.contains(&query)).then_some((prefix, member))
        })
        .collect();
    matched.sort_by(|a, b| {
        b.0.cmp(&a.0).then_with(|| {
            a.1.display_name
                .to_lowercase()
                .cmp(&b.1.display_name.to_lowercase())
        })
    });
    matched.into_iter().map(|(_, member)| member).collect()
}

impl MatrixApp {
    fn mention_composer(&self, in_thread: bool) -> Option<Entity<TextareaState>> {
        if in_thread {
            self.thread.as_ref().map(|t| t.composer.clone())
        } else {
            Some(self.composer.clone())
        }
    }

    /// After every composer change: open, update or close the list.
    pub(crate) fn update_mention_completion(
        &mut self,
        in_thread: bool,
        text: String,
        cursor: usize,
    ) {
        self.mention_completion = mention_at(&text, cursor).map(|(range, query)| {
            let selected = self
                .mention_completion
                .as_ref()
                .filter(|c| c.in_thread == in_thread && c.query == query)
                .map_or(0, |c| c.selected);
            MentionCompletion {
                in_thread,
                original: text,
                range,
                query,
                selected,
            }
        });
    }

    fn mention_results(&self) -> Vec<&Member> {
        match &self.mention_completion {
            Some(completion) => mention_candidates(
                &self.snapshot.members,
                &completion.query,
                &self.snapshot.user_id,
            ),
            None => Vec::new(),
        }
    }

    /// The people picked for the room (or thread) composer, sent with it.
    pub(crate) fn take_mentions(&mut self, in_thread: bool) -> Vec<Mention> {
        if in_thread {
            std::mem::take(&mut self.thread_mentions)
        } else {
            self.mentions
                .remove(&self.snapshot.selected_room)
                .unwrap_or_default()
        }
    }

    fn pick_mention(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(completion) = self.mention_completion.take() else {
            return;
        };
        let Some(member) = mention_candidates(
            &self.snapshot.members,
            &completion.query,
            &self.snapshot.user_id,
        )
        .get(index)
        .map(|member| (*member).clone()) else {
            return;
        };
        let Some(composer) = self.mention_composer(completion.in_thread) else {
            return;
        };
        // Only against the text the list was built for.
        if composer.read(cx).value().as_ref() != completion.original {
            return;
        }
        let (Some(before), Some(through)) = (
            completion.original.get(..completion.range.start),
            completion.original.get(..completion.range.end),
        ) else {
            return;
        };
        let range = before.encode_utf16().count()..through.encode_utf16().count();
        let label = member.display_name.trim().to_owned();
        let label = if label.is_empty() {
            member.user_id.clone()
        } else {
            label
        };
        composer.update(cx, |state, cx| {
            state.replace_text_in_range(Some(range), &format!("@{label} "), window, cx);
            state.focus(window, cx);
        });
        let list = if completion.in_thread {
            &mut self.thread_mentions
        } else {
            self.mentions
                .entry(self.snapshot.selected_room.clone())
                .or_default()
        };
        if !list
            .iter()
            .any(|m| m.user_id == member.user_id && m.label == label)
        {
            list.push(Mention {
                user_id: member.user_id.clone(),
                label,
            });
        }
        // The replacement itself fires a change; it ends after a space, so the
        // list stays closed.
        self.mention_completion = None;
        cx.notify();
    }

    /// Arrow keys, Return/Tab and Escape while the list is open.
    pub(crate) fn mention_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(completion) = &self.mention_completion else {
            return false;
        };
        let count = self.mention_results().len();
        if count == 0 {
            return false;
        }
        let selected = completion.selected.min(count - 1);
        match key {
            "escape" => self.mention_completion = None,
            "enter" | "tab" => self.pick_mention(selected, window, cx),
            "up" | "down" => {
                let step: isize = if key == "up" { -1 } else { 1 };
                let next = (selected as isize + step).rem_euclid(count as isize) as usize;
                if let Some(completion) = &mut self.mention_completion {
                    completion.selected = next;
                }
            }
            _ => return false,
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
        true
    }

    /// The list above the composer it belongs to, or nothing.
    pub(crate) fn mention_panel(
        &self,
        in_thread: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let completion = self
            .mention_completion
            .as_ref()
            .filter(|c| c.in_thread == in_thread)?;
        let results = self.mention_results();
        if results.is_empty() {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let selected = completion.selected.min(results.len() - 1);
        let start = selected / VISIBLE * VISIBLE;
        let rows =
            results
                .iter()
                .enumerate()
                .skip(start)
                .take(VISIBLE)
                .map(|(index, member)| {
                    Button::new(SharedString::from(format!("mention-{index}")))
                        .ghost()
                        .small()
                        .w_full()
                        .h(px(34.))
                        .justify_start()
                        .selected(index == selected)
                        .accessibility_label(format!("Mention {}", member.display_name))
                        .child(
                            h_flex()
                                .w_full()
                                .gap(px(8.))
                                .child(self.avatar_image(
                                    member.avatar.as_deref(),
                                    &member.user_id,
                                    &member.display_name,
                                    22.,
                                    11.,
                                    cx,
                                ))
                                .child(div().flex_none().child(member.display_name.clone()))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(11.))
                                        .text_color(muted)
                                        .child(member.user_id.clone()),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.pick_mention(index, window, cx)
                        }))
                })
                .collect::<Vec<_>>();
        Some(
            v_flex()
                .id(if in_thread {
                    "thread-mention-list"
                } else {
                    "mention-list"
                })
                .role(Role::ListBox)
                .aria_label("People to mention")
                .p(px(6.))
                .gap(px(2.))
                .w(px(380.))
                .rounded(px(10.))
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().popover)
                .shadow_lg()
                .children(rows)
                .child(
                    div()
                        .px(px(6.))
                        .text_size(px(10.5))
                        .text_color(muted)
                        .child("↑ ↓ to choose · Return to mention · Escape to close"),
                )
                .when(results.len() > VISIBLE, |list| {
                    list.child(
                        div()
                            .px(px(6.))
                            .text_size(px(10.5))
                            .text_color(muted)
                            .child(format!("{} people match", results.len())),
                    )
                })
                .test_support()
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Member, mention_at, mention_candidates};

    #[test]
    fn mention_queries_start_at_an_at_sign_after_space() {
        let text = "hi @sa";
        let (range, query) = mention_at(text, text.len()).unwrap();
        assert_eq!(&text[range], "@sa");
        assert_eq!(query, "sa");
        // Right after typing @, the whole list shows.
        assert_eq!(mention_at("@", 1).unwrap().1, "");
        for text in ["mail@example", "hi @sam lee", "@"] {
            if text == "@" {
                continue;
            }
            assert!(mention_at(text, text.len()).is_none(), "{text}");
        }
    }

    #[test]
    fn candidates_prefer_names_starting_with_the_query_and_skip_yourself() {
        let member = |id: &str, name: &str| Member {
            user_id: id.into(),
            display_name: name.into(),
            ..Default::default()
        };
        let members = vec![
            member("@me:example.org", "Me"),
            member("@robin:example.org", "Robin Sam"),
            member("@samantha:example.org", "Samantha"),
            member("@alex:example.org", "Alex Rosamund"),
        ];
        let names: Vec<_> = mention_candidates(&members, "sam", "@me:example.org")
            .into_iter()
            .map(|m| m.display_name.as_str())
            .collect();
        assert_eq!(names, ["Robin Sam", "Samantha", "Alex Rosamund"]);
        assert!(mention_candidates(&members, "me", "@me:example.org").is_empty());
    }
}
