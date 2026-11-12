//! Pure Markdown edits. Selections are UTF-8 byte ranges; malformed ranges are
//! clipped/expanded to whole extended grapheme clusters before any slicing.
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Bold,
    Italic,
    Code,
    Quote,
    Link,
}

/// A caret inside a grapheme snaps left; a selection expands to contain whole
/// graphemes. Reversed/out-of-bounds inputs are accepted without panicking.
pub fn selection(text: &str, range: Range<usize>) -> Range<usize> {
    let start = range.start.min(range.end).min(text.len());
    let end = range.start.max(range.end).min(text.len());
    let bounds: Vec<_> = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain([text.len()])
        .collect();
    let left = bounds
        .iter()
        .copied()
        .take_while(|&i| i <= start)
        .last()
        .unwrap_or(0);
    if start == end {
        return left..left;
    }
    left..bounds.into_iter().find(|&i| i >= end).unwrap_or(text.len())
}

fn replace(
    text: &str,
    range: Range<usize>,
    value: &str,
    selected: Range<usize>,
) -> (String, Range<usize>) {
    let mut output = String::with_capacity(text.len() - range.len() + value.len());
    output.push_str(&text[..range.start]);
    output.push_str(value);
    output.push_str(&text[range.end..]);
    (
        output,
        range.start + selected.start..range.start + selected.end,
    )
}

fn wrap(text: &str, range: Range<usize>, open: &str, close: &str) -> (String, Range<usize>) {
    let inner = &text[range.clone()];
    // The caller may select either the contents or the complete marked text.
    if range.start >= open.len()
        && text[..range.start].ends_with(open)
        && text[range.end..].starts_with(close)
    {
        let start = range.start - open.len();
        return replace(text, start..range.end + close.len(), inner, 0..inner.len());
    }
    if inner.len() >= open.len() + close.len() && inner.starts_with(open) && inner.ends_with(close)
    {
        let body = &inner[open.len()..inner.len() - close.len()];
        return replace(text, range, body, 0..body.len());
    }
    let value = format!("{open}{inner}{close}");
    replace(text, range, &value, open.len()..open.len() + inner.len())
}

fn lines(text: &str, range: Range<usize>) -> Range<usize> {
    let start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
    // An endpoint immediately after a newline does not select the next line.
    let end = if range.end > start && text[..range.end].ends_with('\n') {
        range.end - 1
    } else {
        text[range.end..]
            .find('\n')
            .map_or(text.len(), |i| range.end + i)
    };
    start..end
}

fn quote(text: &str, range: Range<usize>) -> (String, Range<usize>) {
    let caret = range.is_empty().then_some(range.start);
    let range = lines(text, range);
    let body = &text[range.clone()];
    let remove = body.split('\n').all(|l| l.starts_with("> "));
    let output = body
        .split('\n')
        .map(|l| {
            if remove {
                l[2..].to_string()
            } else {
                format!("> {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let selected = if let Some(caret) = caret {
        let relative = caret - range.start;
        let offset = if remove {
            relative.saturating_sub(2)
        } else {
            relative + 2
        };
        offset..offset
    } else {
        0..output.len()
    };
    replace(text, range, &output, selected)
}

fn code(text: &str, range: Range<usize>) -> (String, Range<usize>) {
    let body = &text[range.clone()];
    // Recognise our generated delimiters, including longer backtick runs and
    // the spaces needed to preserve literal edge backticks in inline code.
    // Only delimiter runs adjoining this selection can wrap it. Avoid scanning
    // every possible width (quadratic allocations for a long backtick run).
    let before = &text[..range.start];
    let leading = body.bytes().take_while(|&c| c == b'`').count();
    let mut widths = vec![leading];
    // Selecting both markers of an empty inline span gives one joined run.
    if leading == body.len() && leading.is_multiple_of(2) {
        widths.push(leading / 2);
    }
    for prefix in [
        before,
        before.strip_suffix('\n').unwrap_or(before),
        before.strip_suffix(' ').unwrap_or(before),
    ] {
        widths.push(prefix.bytes().rev().take_while(|&c| c == b'`').count());
    }
    widths.sort_unstable();
    widths.dedup();
    for width in widths.into_iter().rev().filter(|&w| w > 0) {
        let fence = "`".repeat(width);
        let open = format!("{fence}\n");
        let close = format!("\n{fence}");
        for (open, close) in [
            (&*open, &*close),
            (&*format!("{fence} "), &*format!(" {fence}")),
            (&*fence, &*fence),
        ] {
            if (range.start >= open.len()
                && text[..range.start].ends_with(open)
                && text[range.end..].starts_with(close))
                || (body.len() >= open.len() + close.len()
                    && body.starts_with(open)
                    && body.ends_with(close))
            {
                return wrap(text, range, open, close);
            }
        }
    }
    let width = body.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1;
    if body.contains('\n') {
        // Fences must occupy their own lines; include complete selected lines.
        let range = lines(text, range);
        let body = &text[range.clone()];
        let width = body
            .split(|c| c != '`')
            .map(str::len)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(3);
        let fence = "`".repeat(width);
        wrap(text, range, &format!("{fence}\n"), &format!("\n{fence}"))
    } else {
        let fence = "`".repeat(width);
        if body.starts_with('`') || body.ends_with('`') {
            wrap(text, range, &format!("{fence} "), &format!(" {fence}"))
        } else {
            wrap(text, range, &fence, &fence)
        }
    }
}

fn link(text: &str, range: Range<usize>) -> (String, Range<usize>) {
    let body = &text[range.clone()];
    // Toggle when the complete link or its label is selected.
    if body.starts_with('[')
        && body.ends_with(')')
        && let Some(mid) = body.rfind("](")
    {
        let label = &body[1..mid];
        return replace(text, range, label, 0..label.len());
    }
    if range.start > 0
        && text[..range.start].ends_with('[')
        && text[range.end..].starts_with("](")
        && let Some(end) = text[range.end + 2..].find(')')
    {
        return replace(
            text,
            range.start - 1..range.end + 3 + end,
            body,
            0..body.len(),
        );
    }
    // Immediately after insertion the URL is selected. A second invocation
    // removes that generated link and selects its label.
    if text[..range.start].ends_with("](")
        && text[range.end..].starts_with(')')
        && let Some(start) = text[..range.start - 2].rfind('[')
    {
        let label = &text[start + 1..range.start - 2];
        return replace(text, start..range.end + 1, label, 0..label.len());
    }
    let value = format!("[{body}](url)");
    if range.is_empty() {
        replace(text, range, &value, 1..1)
    } else {
        let start = body.len() + 3;
        replace(text, range, &value, start..start + 3)
    }
}

/// The core send/edit path uses `twayblade_core::rich_text::compose`: Ruma
/// Markdown -> sanitised HTML plus an exact Markdown plain-body fallback.
pub fn transform(format: Format, text: &str, range: Range<usize>) -> (String, Range<usize>) {
    let range = selection(text, range);
    match format {
        Format::Bold => wrap(text, range, "**", "**"),
        Format::Italic => wrap(text, range, "_", "_"),
        Format::Code => code(text, range),
        Format::Quote => quote(text, range),
        Format::Link => link(text, range),
    }
}

/// Public Kit 0.9.1 APIs, no fork or UTF-16 conversion. This is an atomic undoable
/// edit. Claude calls this from each composer's action handler.
pub fn apply(
    format: Format,
    state: &mut gpui_kit::component::input::TextareaState,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::Context<gpui_kit::component::input::TextareaState>,
) {
    let (text, range) = transform(format, &state.value(), state.selected_range());
    state.replace_all(text, window, cx);
    state.set_selected_range(range, cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wraps_and_toggles_contents_or_markers() {
        for (format, expected, selected) in [
            (Format::Bold, "**hello**", 2..7),
            (Format::Italic, "_hello_", 1..6),
            (Format::Code, "`hello`", 1..6),
        ] {
            let result = transform(format, "hello", 0..5);
            assert_eq!(result, (expected.into(), selected));
            assert_eq!(
                transform(format, &result.0, result.1),
                ("hello".into(), 0..5)
            );
            assert_eq!(
                transform(format, expected, 0..expected.len()),
                ("hello".into(), 0..5)
            );
        }
    }
    #[test]
    fn empty_selections_place_caret_inside_markers() {
        for (format, expected, caret) in [
            (Format::Bold, "****", 2),
            (Format::Italic, "__", 1),
            (Format::Code, "``", 1),
            (Format::Quote, "> ", 2),
            (Format::Link, "[](url)", 1),
        ] {
            assert_eq!(transform(format, "", 0..0), (expected.into(), caret..caret));
            assert_eq!(transform(format, expected, caret..caret), ("".into(), 0..0));
            assert_eq!(
                transform(format, expected, 0..expected.len()),
                ("".into(), 0..0)
            );
        }
    }
    #[test]
    fn quotes_whole_lines_and_toggles() {
        let (s, r) = transform(Format::Quote, "first\nsecond\nlast", 2..13);
        assert_eq!(s, "> first\n> second\nlast");
        assert_eq!(transform(Format::Quote, &s, r).0, "first\nsecond\nlast");
        assert_eq!(transform(Format::Quote, "a\nb", 0..2).0, "> a\nb");
        assert_eq!(transform(Format::Quote, "> a\nb", 0..5).0, "> > a\n> b");
    }
    #[test]
    fn fenced_code_and_literal_backticks() {
        let original = "before\nα\n🦀\nafter";
        let (s, r) = transform(Format::Code, original, 7..14);
        assert_eq!(s, "before\n```\nα\n🦀\n```\nafter");
        assert_eq!(transform(Format::Code, &s, r).0, original);
        let (s, r) = transform(Format::Code, "a`b", 0..3);
        assert_eq!(s, "``a`b``");
        assert_eq!(transform(Format::Code, &s, r).0, "a`b");
        let (s, r) = transform(Format::Code, "`x`", 1..2); // existing inline code toggles off
        assert_eq!((s, r), ("x".into(), 0..1));
    }
    #[test]
    fn links_select_url_and_toggle() {
        let (s, r) = transform(Format::Link, "héllo", 0..6);
        assert_eq!(s, "[héllo](url)");
        assert_eq!(&s[r.clone()], "url");
        assert_eq!(transform(Format::Link, &s, r), ("héllo".into(), 0..6));
        assert_eq!(transform(Format::Link, &s, 1..7), ("héllo".into(), 0..6));
        assert_eq!(
            transform(Format::Link, &s, 0..s.len()),
            ("héllo".into(), 0..6)
        );
    }
    #[test]
    fn clips_to_extended_graphemes_including_invalid_bytes() {
        let text = "a👩🏽‍💻e\u{301}я";
        let emoji_end = 1 + "👩🏽‍💻".len();
        assert_eq!(selection(text, 2..4), 1..emoji_end);
        assert_eq!(selection(text, 2..2), 1..1);
        assert_eq!(
            selection(text, emoji_end + 1..emoji_end + 2),
            emoji_end..emoji_end + 3
        );
        for format in [
            Format::Bold,
            Format::Italic,
            Format::Code,
            Format::Quote,
            Format::Link,
        ] {
            for start in 0..text.len() + 3 {
                for end in start..text.len() + 3 {
                    let (s, r) = transform(format, text, start..end);
                    assert!(s.is_char_boundary(r.start) && s.is_char_boundary(r.end));
                    assert!(s.contains("👩🏽‍💻") && s.contains("e\u{301}"));
                }
            }
        }
    }
    #[test]
    fn unicode_and_multiline_round_trips() {
        for format in [
            Format::Bold,
            Format::Italic,
            Format::Code,
            Format::Quote,
            Format::Link,
        ] {
            for original in ["Български 👩🏽‍💻 e\u{301}", "α\n🦀\ne\u{301}"] {
                let (text, range) = transform(format, original, 0..original.len());
                assert_eq!(transform(format, &text, range).0, original);
            }
        }
        let (text, range) = transform(Format::Code, "a`b\n```", 0..8);
        assert!(text.starts_with("````\n"));
        assert_eq!(transform(Format::Code, &text, range).0, "a`b\n```");
        assert_eq!(
            transform(Format::Quote, "hello", 3..3),
            ("> hello".into(), 5..5)
        );
    }

    #[test]
    fn markdown_goes_through_existing_core_path() {
        for format in [
            Format::Bold,
            Format::Italic,
            Format::Code,
            Format::Quote,
            Format::Link,
        ] {
            let (s, _) = transform(format, "hello", 0..5);
            let content = twayblade_core::rich_text::compose(s.clone()).unwrap();
            assert_eq!(content.body(), s);
            // `url` is intentionally a placeholder; a real URL is supplied by the user.
            if format != Format::Link {
                assert!(
                    serde_json::to_value(content)
                        .unwrap()
                        .get("formatted_body")
                        .is_some()
                );
            }
        }
    }
}
