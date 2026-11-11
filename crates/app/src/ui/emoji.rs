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
/// Columns in the full picker grid; arrow up/down move by one row.
const COLUMNS: usize = 8;
const CELL: f32 = 34.;
/// Category bar: CLDR groups with a representative glyph, in picker order.
/// Line icons, not emoji glyphs, so the bar matches the app's icon system.
const GROUPS: [(emojis::Group, IconName, &str); 9] = [
    (
        emojis::Group::SmileysAndEmotion,
        IconName::FaceSlightlySmiling,
        "Smileys & Emotion",
    ),
    (
        emojis::Group::PeopleAndBody,
        IconName::Hand,
        "People & Body",
    ),
    (
        emojis::Group::AnimalsAndNature,
        IconName::PawPrint,
        "Animals & Nature",
    ),
    (
        emojis::Group::FoodAndDrink,
        IconName::Utensils,
        "Food & Drink",
    ),
    (
        emojis::Group::TravelAndPlaces,
        IconName::Plane,
        "Travel & Places",
    ),
    (
        emojis::Group::Activities,
        IconName::Volleyball,
        "Activities",
    ),
    (emojis::Group::Objects, IconName::Lightbulb, "Objects"),
    (emojis::Group::Symbols, IconName::Heart, "Symbols"),
    (emojis::Group::Flags, IconName::Flag, "Flags"),
];
enum Row {
    Header(&'static str),
    /// A range into `Grid::emojis`.
    Cells(Range<usize>),
}
/// The full picker's content: one flat emoji list (for keyboard selection)
/// laid out as headed rows (for a virtualised, uniform-height list).
struct Grid {
    emojis: Vec<&'static emojis::Emoji>,
    rows: Vec<Row>,
    /// (group, first row) for the category bar.
    sections: Vec<(Option<emojis::Group>, usize)>,
}
fn toned(emoji: &'static emojis::Emoji, tone: usize) -> &'static emojis::Emoji {
    emoji.with_skin_tone(TONES[tone % 6]).unwrap_or(emoji)
}
/// The six single-tone variants of an emoji, or None when it has no tones.
fn tone_variants(emoji: &'static emojis::Emoji) -> Option<Vec<&'static emojis::Emoji>> {
    emoji.skin_tones()?.next()?;
    let variants: Vec<_> = TONES
        .iter()
        .filter_map(|tone| emoji.with_skin_tone(*tone))
        .collect();
    (variants.len() == 6).then_some(variants)
}
fn grid(query: &str, preferences: &Preferences) -> Grid {
    let mut grid = Grid {
        emojis: Vec::new(),
        rows: Vec::new(),
        sections: Vec::new(),
    };
    let section = |grid: &mut Grid, group, label, values: Vec<&'static emojis::Emoji>| {
        if values.is_empty() {
            return;
        }
        grid.sections.push((group, grid.rows.len()));
        grid.rows.push(Row::Header(label));
        let start = grid.emojis.len();
        grid.emojis.extend(values);
        let end = grid.emojis.len();
        let mut row = start;
        while row < end {
            grid.rows.push(Row::Cells(row..(row + COLUMNS).min(end)));
            row += COLUMNS;
        }
    };
    let query = query.trim();
    if !query.is_empty() {
        let found = results(query, preferences);
        section(&mut grid, None, "Search Results", found);
        return grid;
    }
    let recent = preferences
        .recents
        .iter()
        .filter_map(|value| emojis::get(value))
        .collect();
    section(&mut grid, None, "Frequently Used", recent);
    for (group, _, label) in GROUPS {
        let values = group
            .emojis()
            .filter(|emoji| emoji.unicode_version().major() <= 15)
            .map(|emoji| toned(emoji, preferences.tone))
            .collect();
        section(&mut grid, Some(group), label, values);
    }
    grid
}
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Preferences {
    pub recents: Vec<String>,
    pub tone: usize,
}
impl Preferences {
    fn path() -> Option<PathBuf> {
        Some(
            directories::ProjectDirs::from("", "Twayblade", "Twayblade")?
                .config_dir()
                .join("emoji.json"),
        )
    }
    pub(crate) fn load() -> Self {
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
pub(crate) struct Picker {
    pub reaction: Option<String>,
    pub selected: usize,
    /// The emoji (index into the grid) whose skin-tone popover is open.
    tones: Option<usize>,
    tone_selected: usize,
    scroll: UniformListScrollHandle,
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
    pub(crate) fn open_emoji(
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
            tones: None,
            tone_selected: 0,
            scroll: UniformListScrollHandle::new(),
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
    pub(crate) fn update_emoji_completion(&mut self, original: String, cursor: usize) {
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
            tones: None,
            tone_selected: 0,
            scroll: UniformListScrollHandle::new(),
            room: self.snapshot.selected_room.clone(),
            account: self.snapshot.account_instance,
            original,
            range,
            completion: Some(query),
        });
    }
    fn emoji_results(&self, cx: &App) -> Vec<&'static emojis::Emoji> {
        let picker = self.emoji_picker.as_ref();
        if let Some(query) = picker.and_then(|picker| picker.completion.as_deref()) {
            return results(query, &self.emoji_preferences);
        }
        self.emoji_grid(cx).emojis
    }
    fn emoji_grid(&self, cx: &App) -> Grid {
        grid(
            self.emoji_search.read(cx).value().as_ref(),
            &self.emoji_preferences,
        )
    }
    /// A click (or Return) on a grid emoji: tone-capable ones open the tone
    /// popover; the rest insert immediately.
    fn pick_emoji(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let values = self.emoji_results(cx);
        let Some(emoji) = values.get(index).copied() else {
            return;
        };
        let completion = self
            .emoji_picker
            .as_ref()
            .is_some_and(|picker| picker.completion.is_some());
        if !completion && tone_variants(emoji).is_some() {
            let tone = self.emoji_preferences.tone % 6;
            if let Some(picker) = &mut self.emoji_picker {
                picker.selected = index;
                picker.tones = Some(index);
                picker.tone_selected = tone;
            }
            cx.notify();
            return;
        }
        self.choose_emoji(emoji.as_str().into(), window, cx);
    }
    /// Insert one tone variant and remember the tone for the grid and completions.
    fn choose_tone(&mut self, tone: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.emoji_picker.as_ref().and_then(|picker| picker.tones) else {
            return;
        };
        let Some(variant) = self
            .emoji_results(cx)
            .get(index)
            .and_then(|emoji| tone_variants(emoji))
            .and_then(|variants| variants.get(tone % 6).copied())
        else {
            return;
        };
        self.emoji_preferences.tone = tone % 6;
        self.choose_emoji(variant.as_str().into(), window, cx);
    }
    pub(crate) fn choose_emoji(
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
    pub(crate) fn emoji_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &self.emoji_picker else {
            return;
        };
        if picker.tones.is_some() {
            let tone = picker.tone_selected;
            match event.keystroke.key.as_str() {
                "escape" => self.emoji_picker.as_mut().unwrap().tones = None,
                "enter" => self.choose_tone(tone, window, cx),
                "left" | "right" => {
                    let step = if event.keystroke.key == "left" { 5 } else { 1 };
                    self.emoji_picker.as_mut().unwrap().tone_selected = (tone + step) % 6;
                }
                _ => return,
            }
            cx.stop_propagation();
            window.prevent_default();
            cx.notify();
            return;
        }
        let values = self.emoji_results(cx);
        let selected = picker.selected;
        let step = if picker.completion.is_some() {
            1
        } else {
            COLUMNS
        };
        match event.keystroke.key.as_str() {
            "escape" => {
                self.emoji_picker = None;
                self.composer
                    .update(cx, |state, cx| state.focus(window, cx));
            }
            "enter" => {
                if selected < values.len() {
                    self.pick_emoji(selected, window, cx);
                }
            }
            "up" | "down" | "left" | "right" if !values.is_empty() => {
                let offset = match event.keystroke.key.as_str() {
                    "up" => -(step as isize),
                    "down" => step as isize,
                    "left" => -1,
                    _ => 1,
                };
                let next = (selected as isize + offset).rem_euclid(values.len() as isize) as usize;
                self.emoji_picker.as_mut().unwrap().selected = next;
                if self.emoji_picker.as_ref().unwrap().completion.is_none() {
                    let grid = self.emoji_grid(cx);
                    if let Some(row) = grid
                        .rows
                        .iter()
                        .position(|row| matches!(row, Row::Cells(range) if range.contains(&next)))
                    {
                        self.emoji_picker
                            .as_ref()
                            .unwrap()
                            .scroll
                            .scroll_to_item(row, ScrollStrategy::Nearest);
                    }
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
    }
    pub(crate) fn emoji_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(picker) = &self.emoji_picker else {
            return div().into_any_element();
        };
        if picker.completion.is_some() {
            return self.emoji_completion(picker, cx);
        }
        let grid = self.emoji_grid(cx);
        let empty = grid.emojis.is_empty();
        let selected = picker.selected;
        // The category bar highlights the section holding the selected emoji.
        let current = grid
            .sections
            .iter()
            .rev()
            .find(|(_, row)| {
                grid.rows[*row..]
                    .iter()
                    .find_map(|row| match row {
                        Row::Cells(range) => Some(range.start),
                        Row::Header(_) => None,
                    })
                    .is_some_and(|start| start <= selected)
            })
            .map(|(group, _)| *group);
        let mut bar = h_flex()
            .px(px(6.))
            .gap(px(1.))
            .border_b_1()
            .border_color(cx.theme().border);
        let recent_row = grid
            .sections
            .iter()
            .find(|(group, _)| group.is_none())
            .map(|(_, row)| *row);
        let searching = !self.emoji_search.read(cx).value().trim().is_empty();
        if !searching {
            let mut tabs: Vec<(Option<emojis::Group>, IconName, &str)> = Vec::new();
            if recent_row.is_some() {
                tabs.push((None, IconName::Clock, "Frequently Used"));
            }
            for (group, icon, label) in GROUPS {
                tabs.push((Some(group), icon, label));
            }
            for (index, (group, glyph, label)) in tabs.into_iter().enumerate() {
                let row = grid
                    .sections
                    .iter()
                    .find(|(section, _)| *section == group)
                    .map(|(_, row)| *row)
                    .unwrap_or(0);
                let first = grid.rows.get(row + 1).and_then(|row| match row {
                    Row::Cells(range) => Some(range.start),
                    Row::Header(_) => None,
                });
                let active = current == Some(group);
                bar = bar.child(
                    Button::new(SharedString::from(format!("emoji-group-{index}")))
                        .ghost()
                        .size(px(28.))
                        .rounded(px(6.))
                        .selected(active)
                        .accessibility_label(label)
                        .tooltip(label)
                        .child(Icon::new(glyph).size(px(15.)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(picker) = &mut this.emoji_picker {
                                picker.tones = None;
                                if let Some(first) = first {
                                    picker.selected = first;
                                }
                                picker.scroll.scroll_to_item(row, ScrollStrategy::Top);
                            }
                            cx.notify();
                        })),
                );
            }
        }
        let rows = grid.rows.len();
        let list = uniform_list(
            "emoji-grid",
            rows,
            cx.processor(|this, range: Range<usize>, _, cx| {
                let grid = this.emoji_grid(cx);
                let selected = this
                    .emoji_picker
                    .as_ref()
                    .map_or(0, |picker| picker.selected);
                let muted = cx.theme().muted_foreground;
                range
                    .filter_map(|ix| grid.rows.get(ix).map(|row| (ix, row)))
                    .map(|(ix, row)| match row {
                        Row::Header(label) => div()
                            .id(("emoji-header", ix))
                            .h(px(CELL))
                            .px(px(4.))
                            .pt(px(12.))
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(muted)
                            .child(*label)
                            .into_any_element(),
                        Row::Cells(cells) => h_flex()
                            .h(px(CELL))
                            .children(cells.clone().map(|index| {
                                let emoji = grid.emojis[index];
                                let tones = tone_variants(emoji).is_some();
                                Button::new(SharedString::from(format!("emoji-{index}")))
                                    .ghost()
                                    .size(px(CELL))
                                    .rounded(px(6.))
                                    .selected(selected == index)
                                    .accessibility_label(format!("Insert {}", emoji.name()))
                                    .tooltip(if tones {
                                        format!("{} · choose a skin tone", emoji.name())
                                    } else {
                                        emoji.name().to_owned()
                                    })
                                    .child(div().text_size(px(20.)).child(emoji.as_str()))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.pick_emoji(index, window, cx)
                                    }))
                            }))
                            .into_any_element(),
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&picker.scroll)
        .h(px(CELL * 8.))
        .w_full();
        let hovered = grid.emojis.get(selected).map(|emoji| {
            (
                emoji.as_str(),
                emoji.name(),
                emoji.shortcode().map(|code| format!(":{code}:")),
            )
        });
        let tone_popover = picker.tones.and_then(|index| {
            let variants = tone_variants(grid.emojis.get(index)?)?;
            let chosen = picker.tone_selected;
            Some(
                v_flex()
                    .id("emoji-tones")
                    .absolute()
                    .left(px(12.))
                    .right(px(12.))
                    .bottom(px(42.))
                    .p(px(8.))
                    .gap(px(6.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().popover)
                    .shadow_lg()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        if let Some(picker) = &mut this.emoji_picker {
                            picker.tones = None;
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().muted_foreground)
                            .child("Skin Tone"),
                    )
                    .child(h_flex().justify_between().children(
                        variants.into_iter().enumerate().map(|(tone, variant)| {
                            Button::new(SharedString::from(format!("emoji-tone-{tone}")))
                                .ghost()
                                .size(px(36.))
                                .rounded(px(7.))
                                .selected(tone == chosen)
                                .accessibility_label(format!(
                                    "{}, {}",
                                    variant.name(),
                                    TONE_NAMES[tone]
                                ))
                                .tooltip(TONE_NAMES[tone])
                                .child(div().text_size(px(22.)).child(variant.as_str()))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.choose_tone(tone, window, cx)
                                }))
                        }),
                    ))
                    .test_support(),
            )
        });
        v_flex()
            .id("emoji-picker")
            .relative()
            .w(px(CELL * COLUMNS as f32 + 24.))
            .rounded(px(12.))
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .shadow_lg()
            .child(
                h_flex()
                    .p(px(10.))
                    .gap(px(6.))
                    .child(
                        div()
                            .id("emoji-search-panel")
                            .flex_1()
                            .child(
                                Input::new(&self.emoji_search)
                                    .accessibility_id("emoji-search")
                                    .aria_label("Search emoji")
                                    .prefix(Icon::new(IconName::Search).size(px(13.)))
                                    .small(),
                            )
                            .test_support(),
                    )
                    .child(
                        Button::new("emoji-close")
                            .ghost()
                            .xsmall()
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
            .when(!searching, |view| view.child(bar))
            .child(div().px(px(12.)).py(px(4.)).child(if empty {
                div()
                    .h(px(CELL * 8.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(cx.theme().muted_foreground)
                    .child("No emoji found")
                    .into_any_element()
            } else {
                list.into_any_element()
            }))
            .child(
                h_flex()
                    .h(px(36.))
                    .px(px(12.))
                    .gap(px(8.))
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .text_size(px(11.5))
                    .when_some(hovered, |footer, (glyph, name, code)| {
                        footer
                            .child(div().text_size(px(18.)).child(glyph))
                            .child(div().flex_1().min_w_0().truncate().child(name))
                            .children(code.map(|code| {
                                div().text_color(cx.theme().muted_foreground).child(code)
                            }))
                    }),
            )
            .children(tone_popover)
            .test_support()
            .into_any_element()
    }

    /// Inline `:shortcode` completion above the composer: a short list.
    fn emoji_completion(&self, picker: &Picker, cx: &mut Context<Self>) -> AnyElement {
        let values = self.emoji_results(cx);
        let count = 6;
        let start = picker.selected / count * count;
        let mut choices = v_flex().gap(px(1.));
        for (index, emoji) in values.iter().enumerate().skip(start).take(count) {
            let value = emoji.as_str().to_owned();
            choices = choices.child(
                Button::new(SharedString::from(format!("emoji-{index}")))
                    .ghost()
                    .small()
                    .w_full()
                    .justify_start()
                    .selected(picker.selected == index)
                    .accessibility_label(format!("Insert {}", emoji.name()))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .child(div().text_size(px(16.)).child(value.clone()))
                            .child(format!(":{}:", emoji.shortcode().unwrap_or("")))
                            .child(
                                div()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(emoji.name().to_owned()),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_emoji(value.clone(), window, cx)
                    })),
            );
        }
        v_flex()
            .id("emoji-picker")
            .p(px(6.))
            .gap(px(4.))
            .w(px(380.))
            .rounded(px(10.))
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .shadow_lg()
            .when(values.is_empty(), |view| {
                view.child(div().p(px(6.)).child("No emoji found"))
            })
            .child(choices)
            .child(
                div()
                    .px(px(6.))
                    .text_size(px(10.5))
                    .text_color(cx.theme().muted_foreground)
                    .child("↑ ↓ to choose · Return to insert · Escape to close"),
            )
            .test_support()
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
