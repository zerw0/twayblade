//! Unicode picker shared by the composer and SDK reactions. Only UI preferences persist.
use crate::theme::scaled_px as px;
use crate::{app::MatrixApp, backend::TimelineAction};
use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        input::Input,
        *,
    },
    *,
};
use serde::{Deserialize, Serialize};
use std::{ops::Range, path::PathBuf};

const TONES: [emojis::SkinTone; 6] = [
    emojis::SkinTone::Default,
    emojis::SkinTone::Light,
    emojis::SkinTone::MediumLight,
    emojis::SkinTone::Medium,
    emojis::SkinTone::MediumDark,
    emojis::SkinTone::Dark,
];
const TONE_NAMES: [&str; 6] = [
    "Default",
    "Light",
    "Medium light",
    "Medium",
    "Medium dark",
    "Dark",
];
#[derive(Default, Serialize, Deserialize)]
pub(super) struct Preferences {
    pub recents: Vec<String>,
    pub tone: usize,
}
impl Preferences {
    fn path() -> Option<PathBuf> {
        Some(
            directories::ProjectDirs::from("", "MatrixClient", "MatrixClient")?
                .config_dir()
                .join("emoji.json"),
        )
    }
    pub(super) fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(metadata) = std::fs::metadata(&path) else {
            return Self::default();
        };
        if metadata.len() > 16384 {
            return Self::default();
        }
        let mut preferences: Self = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        preferences.tone %= 6;
        preferences
            .recents
            .retain(|value| emojis::get(value).is_some());
        preferences.recents.truncate(24);
        preferences
    }
    fn save(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        let Ok(bytes) = serde_json::to_vec(self) else {
            return;
        };
        if std::fs::create_dir_all(parent).is_ok() {
            let temporary = path.with_extension("json.new");
            if std::fs::write(&temporary, bytes).is_ok() {
                let _ = std::fs::rename(temporary, path);
            }
        }
    }
    fn remember(&mut self, value: &str) {
        self.recents.retain(|recent| recent != value);
        self.recents.insert(0, value.into());
        self.recents.truncate(24);
    }
}
pub(super) struct Picker {
    pub reaction: Option<String>,
    pub selected: usize,
    room: String,
    account: u128,
    original: String,
    range: Range<usize>,
    completion: Option<String>,
}
/// Byte ranges are converted to UTF-16 only at the input boundary.
fn shortcode_at(text: &str, cursor: usize) -> Option<(Range<usize>, String)> {
    let before = text.get(..cursor)?;
    let start = before.rfind(':')?;
    if start > 0 && !before[..start].chars().next_back()?.is_whitespace() {
        return None;
    }
    let query = &before[start + 1..];
    if query.len() < 2
        || query.len() > 64
        || !query
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || b"_+-".contains(&ch))
    {
        return None;
    }
    Some((start..cursor, query.to_ascii_lowercase()))
}
fn results(query: &str, preferences: &Preferences) -> Vec<&'static emojis::Emoji> {
    let query = query.trim_matches(':').to_ascii_lowercase();
    let mut values = Vec::new();
    if query.is_empty() {
        for recent in &preferences.recents {
            if let Some(emoji) = emojis::get(recent) {
                values.push(emoji);
            }
        }
    }
    for emoji in emojis::iter()
        .filter(|emoji| emoji.unicode_version().major() <= 15)
        .filter(|emoji| {
            query.split_whitespace().all(|word| {
                emoji.name().to_ascii_lowercase().contains(word)
                    || emoji.shortcodes().any(|code| code.contains(word))
            })
        })
    {
        let toned = emoji
            .with_skin_tone(TONES[preferences.tone % 6])
            .unwrap_or(emoji);
        if !values.iter().any(|value| value.as_str() == toned.as_str()) {
            values.push(toned);
        }
    }
    values
}
impl MatrixApp {
    pub(super) fn open_emoji(
        &mut self,
        reaction: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let original = self.composer.read(cx).value().to_string();
        let range = self.composer.read(cx).selected_range();
        self.emoji_picker = Some(Picker {
            reaction,
            selected: 0,
            room: self.snapshot.selected_room.clone(),
            account: self.snapshot.account_instance,
            original,
            range,
            completion: None,
        });
        self.emoji_search.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.focus(window, cx);
        });
        cx.notify();
    }
    pub(super) fn update_emoji_completion(&mut self, original: String, cursor: usize) {
        if self
            .emoji_picker
            .as_ref()
            .is_some_and(|picker| picker.completion.is_none())
        {
            return;
        }
        self.emoji_picker = shortcode_at(&original, cursor).map(|(range, query)| Picker {
            reaction: None,
            selected: 0,
            room: self.snapshot.selected_room.clone(),
            account: self.snapshot.account_instance,
            original,
            range,
            completion: Some(query),
        });
    }
    fn emoji_results(&self, cx: &App) -> Vec<&'static emojis::Emoji> {
        let query = self
            .emoji_picker
            .as_ref()
            .and_then(|picker| picker.completion.as_deref())
            .map(str::to_owned)
            .unwrap_or_else(|| self.emoji_search.read(cx).value().to_string());
        results(&query, &self.emoji_preferences)
    }
    pub(super) fn choose_emoji(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = self.emoji_picker.take() else {
            return;
        };
        if picker.room != self.snapshot.selected_room
            || self.navigation.selected_room.as_deref() != Some(picker.room.as_str())
            || picker.account != self.snapshot.account_instance
            || emojis::get(&value).is_none()
        {
            return;
        }
        if let Some(message) = picker.reaction {
            if !self.preview {
                self.timeline_action(
                    message.clone(),
                    TimelineAction::React {
                        message,
                        key: value.clone(),
                    },
                    window,
                    cx,
                );
            }
        } else {
            if picker.completion.is_some() && self.composer.read(cx).cursor() != picker.range.end {
                return;
            }
            if self.composer.read(cx).value().as_ref() != picker.original {
                return;
            }
            let Some(before) = picker.original.get(..picker.range.start) else {
                return;
            };
            let Some(through) = picker.original.get(..picker.range.end) else {
                return;
            };
            let range = before.encode_utf16().count()..through.encode_utf16().count();
            self.composer.update(cx, |state, cx| {
                state.replace_text_in_range(Some(range), &value, window, cx);
                state.focus(window, cx);
            });
        }
        self.emoji_preferences.remember(&value);
        if !self.preview {
            self.emoji_preferences.save();
        }
        cx.notify();
    }
    pub(super) fn emoji_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &self.emoji_picker else {
            return;
        };
        let values = self.emoji_results(cx);
        let selected = picker.selected;
        let step = if picker.completion.is_some() { 1 } else { 6 };
        match event.keystroke.key.as_str() {
            "escape" => {
                self.emoji_picker = None;
                self.composer
                    .update(cx, |state, cx| state.focus(window, cx));
            }
            "enter" => {
                if let Some(emoji) = values.get(selected) {
                    self.choose_emoji(emoji.as_str().into(), window, cx);
                }
            }
            "up" | "down" | "left" | "right" if !values.is_empty() => {
                let offset = match event.keystroke.key.as_str() {
                    "up" => -(step as isize),
                    "down" => step as isize,
                    "left" => -1,
                    _ => 1,
                };
                self.emoji_picker.as_mut().unwrap().selected =
                    (selected as isize + offset).rem_euclid(values.len() as isize) as usize;
            }
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
    }
    pub(super) fn emoji_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(picker) = &self.emoji_picker else {
            return div().into_any_element();
        };
        let values = self.emoji_results(cx);
        let completion = picker.completion.is_some();
        let count = if completion { 6 } else { 18 };
        let start = picker.selected / count * count;
        let mut choices = if completion {
            v_flex()
        } else {
            h_flex().flex_wrap()
        };
        choices = choices.gap(px(4.));
        for (index, emoji) in values.iter().enumerate().skip(start).take(count) {
            let value = emoji.as_str().to_owned();
            let label = if completion {
                format!(
                    "{}  :{}: · {}",
                    value,
                    emoji.shortcode().unwrap_or(""),
                    emoji.name()
                )
            } else {
                value.clone()
            };
            choices = choices.child(
                Button::new(SharedString::from(format!("emoji-{index}")))
                    .ghost()
                    .small()
                    .label(label)
                    .selected(picker.selected == index)
                    .accessibility_label(format!("Insert {}", emoji.name()))
                    .tooltip(emoji.name().to_owned())
                    .when(!completion, |button| button.w(px(38.)).h(px(34.)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_emoji(value.clone(), window, cx)
                    })),
            );
        }
        v_flex()
            .id("emoji-picker")
            .p(px(10.))
            .gap(px(6.))
            .w(if completion { px(440.) } else { px(288.) })
            .rounded(px(10.))
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .justify_between()
                    .child(if picker.reaction.is_some() {
                        "Add reaction"
                    } else {
                        "Emoji"
                    })
                    .child(
                        Button::new("emoji-close")
                            .ghost()
                            .small()
                            .icon(IconName::X)
                            .accessibility_label("Close emoji picker")
                            .tooltip("Close · Escape")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.emoji_picker = None;
                                this.composer
                                    .update(cx, |state, cx| state.focus(window, cx));
                                cx.notify();
                            })),
                    ),
            )
            .when(!completion, |view| {
                view.child(
                    div()
                        .id("emoji-search-panel")
                        .child(
                            Input::new(&self.emoji_search)
                                .accessibility_id("emoji-search")
                                .aria_label("Search emoji")
                                .small(),
                        )
                        .test_support(),
                )
            })
            .child(
                Button::new("emoji-tone")
                    .ghost()
                    .small()
                    .label(format!(
                        "Skin tone: {}",
                        TONE_NAMES[self.emoji_preferences.tone % 6]
                    ))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.emoji_preferences.tone = (this.emoji_preferences.tone + 1) % 6;
                        if !this.preview {
                            this.emoji_preferences.save();
                        }
                        cx.notify();
                    })),
            )
            .when(values.is_empty(), |view| view.child("No emoji found."))
            .child(choices)
            .child(
                h_flex()
                    .gap(px(6.))
                    .child(
                        Button::new("emoji-prev")
                            .small()
                            .label("Previous")
                            .disabled(start == 0)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(picker) = &mut this.emoji_picker {
                                    picker.selected = picker.selected.saturating_sub(count);
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("emoji-next")
                            .small()
                            .label("Next")
                            .disabled(start + count >= values.len())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(picker) = &mut this.emoji_picker {
                                    picker.selected += count;
                                }
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(cx.theme().muted_foreground)
                    .child(if completion {
                        "↑ ↓ to choose · Return to insert · Escape to close"
                    } else if self.emoji_preferences.recents.is_empty() {
                        "Arrow keys to choose · Return to insert"
                    } else {
                        "Recent emoji first · Arrows to choose · Return to insert"
                    }),
            )
            .into_any_element()
    }
}
#[cfg(test)]
mod tests {
    use super::{Preferences, results, shortcode_at};
    #[test]
    fn shortcode_ranges_preserve_unicode_and_reject_urls_or_punctuation() {
        let text = "🦀 hello :thu";
        let (range, query) = shortcode_at(text, text.len()).unwrap();
        assert_eq!(&text[range], ":thu");
        assert_eq!(query, "thu");
        for text in ["https://example", "10:30", ":x", ":hello!", "word:smile"] {
            assert!(shortcode_at(text, text.len()).is_none());
        }
    }
    #[test]
    fn search_skin_tones_and_bounded_recents_use_unicode_data() {
        let mut preferences = Preferences {
            tone: 5,
            ..Default::default()
        };
        assert!(
            results("thumbsup", &preferences)
                .iter()
                .any(|emoji| emoji.as_str() == "👍🏿")
        );
        assert!(!results("crab", &preferences).is_empty());
        for emoji in emojis::iter().take(40) {
            preferences.remember(emoji.as_str());
        }
        assert_eq!(preferences.recents.len(), 24);
        preferences.remember("🦀");
        preferences.remember("🦀");
        assert_eq!(
            preferences
                .recents
                .iter()
                .filter(|value| value.as_str() == "🦀")
                .count(),
            1
        );
        assert_eq!(results("", &preferences)[0].as_str(), "🦀");
    }
}
