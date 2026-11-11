//! Sanitized native rich-text data; no web runtime, raw markup or disk index.
use matrix_sdk::ruma::events::room::message::{
    FormattedBody, MessageFormat, MessageType, RoomMessageEventContent,
};
use ruma_html::{Html, NodeData, NodeRef, SanitizerConfig};
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RichText {
    pub nodes: Vec<RichNode>,
}
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum RichNode {
    Text(String),
    Span {
        kind: SpanKind,
        children: Vec<RichNode>,
    },
    Block {
        kind: BlockKind,
        children: Vec<RichNode>,
    },
}
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum SpanKind {
    Strong,
    Emphasis,
    Code,
    Strike,
    Underline,
    Superscript,
    Subscript,
    Link {
        target: String,
    },
    Spoiler {
        /// Zero-based preorder index within this message, after sanitization.
        index: u32,
        reason: Option<String>,
    },
    Color {
        foreground: Option<String>,
        background: Option<String>,
    },
    LineBreak,
    Image {
        source: String,
        alt: String,
        width: Option<u32>,
        height: Option<u32>,
    },
}
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum BlockKind {
    Paragraph,
    Container,
    Heading(u8),
    Quote,
    Code { language: Option<String> },
    List { ordered: bool, start: u64 },
    ListItem,
    Divider,
    Table,
    TableHead,
    TableBody,
    TableRow,
    TableCell { header: bool },
    Details,
    Summary,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FormattingError {
    #[error("formatting_limit")]
    Limit,
    #[error("formatting_empty")]
    Empty,
}
const MAX_HTML: usize = 256 * 1024;
fn sanitized(html: &str) -> Result<Html, FormattingError> {
    if html.len() > MAX_HTML {
        return Err(FormattingError::Limit);
    }
    // html5ever may include malformed tag/attribute data in parser diagnostics.
    // Parsing/sanitizing is synchronous; silence only this scope, not SDK tasks.
    Ok(tracing::subscriber::with_default(
        tracing::subscriber::NoSubscriber::default(),
        || {
            let html = Html::parse(html);
            html.sanitize_with(
                &SanitizerConfig::strict()
                    .remove_reply_fallback()
                    .remove_elements(["script", "iframe", "style", "object", "embed", "template"])
                    .max_depth(32),
            );
            html
        },
    ))
}
/// Ruma owns Markdown parsing. The plain fallback stays exactly as composed,
/// including its Markdown syntax, as in Ruma/Element-compatible message content.
pub fn compose(body: String) -> Result<RoomMessageEventContent, FormattingError> {
    if body.trim().is_empty() {
        return Err(FormattingError::Empty);
    }
    if body.len() > 64 * 1024 {
        return Err(FormattingError::Limit);
    }
    let Some(formatted) = FormattedBody::markdown(&body) else {
        return Ok(RoomMessageEventContent::text_plain(body));
    };
    let html = sanitized(&formatted.body)?.to_string();
    Ok(RoomMessageEventContent::text_html(body, html))
}
fn attr(node: &NodeRef, key: &str) -> Option<String> {
    node.as_element()?
        .attrs
        .borrow()
        .iter()
        .find(|a| a.name.local.as_ref() == key)
        .map(|a| a.value.to_string())
}
fn project(
    node: &NodeRef,
    budget: &mut usize,
    depth: usize,
) -> Result<Vec<RichNode>, FormattingError> {
    if *budget == 0 || depth > 40 {
        return Err(FormattingError::Limit);
    }
    *budget -= 1;
    match node.data() {
        NodeData::Text(text) => return Ok(vec![RichNode::Text(text.borrow().to_string())]),
        NodeData::Element(_) => {}
        _ => return Ok(Vec::new()),
    }
    let Some(element) = node.as_element() else {
        return Ok(Vec::new());
    };
    let tag = element.name.local.as_ref();
    let mut children = Vec::new();
    for child in node.children() {
        children.extend(project(&child, budget, depth + 1)?);
    }
    let block = match tag {
        "p" => Some(BlockKind::Paragraph),
        "div" => Some(BlockKind::Container),
        "blockquote" => Some(BlockKind::Quote),
        "h1" => Some(BlockKind::Heading(1)),
        "h2" => Some(BlockKind::Heading(2)),
        "h3" => Some(BlockKind::Heading(3)),
        "h4" => Some(BlockKind::Heading(4)),
        "h5" => Some(BlockKind::Heading(5)),
        "h6" => Some(BlockKind::Heading(6)),
        "pre" => Some(BlockKind::Code {
            language: node
                .children()
                .find(|n| {
                    n.as_element()
                        .is_some_and(|e| e.name.local.as_ref() == "code")
                })
                .and_then(|code| attr(&code, "class"))
                .and_then(|class| {
                    class
                        .split_whitespace()
                        .find_map(|c| c.strip_prefix("language-").map(str::to_owned))
                }),
        }),
        "ul" => Some(BlockKind::List {
            ordered: false,
            start: 1,
        }),
        "ol" => Some(BlockKind::List {
            ordered: true,
            start: attr(node, "start")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1),
        }),
        "li" => Some(BlockKind::ListItem),
        "hr" => Some(BlockKind::Divider),
        "table" => Some(BlockKind::Table),
        "thead" => Some(BlockKind::TableHead),
        "tbody" => Some(BlockKind::TableBody),
        "tr" => Some(BlockKind::TableRow),
        "td" | "th" => Some(BlockKind::TableCell {
            header: tag == "th",
        }),
        "details" => Some(BlockKind::Details),
        "summary" => Some(BlockKind::Summary),
        _ => None,
    };
    if let Some(kind) = block {
        return Ok(vec![RichNode::Block { kind, children }]);
    }
    let span = match tag {
        "strong" | "b" => Some(SpanKind::Strong),
        "em" | "i" => Some(SpanKind::Emphasis),
        "code" => Some(SpanKind::Code),
        "del" | "s" | "strike" => Some(SpanKind::Strike),
        "u" => Some(SpanKind::Underline),
        "sup" => Some(SpanKind::Superscript),
        "sub" => Some(SpanKind::Subscript),
        "br" => Some(SpanKind::LineBreak),
        "a" => attr(node, "href").and_then(|target| {
            // Defense in depth: only absolute Matrix-recommended URL schemes.
            let url = url::Url::parse(&target).ok()?;
            matches!(url.scheme(), "https" | "http" | "mailto" | "ftp" | "magnet")
                .then_some(SpanKind::Link { target })
        }),
        "span" if attr(node, "data-mx-spoiler").is_some() => Some(SpanKind::Spoiler {
            index: 0,
            reason: attr(node, "data-mx-spoiler").filter(|s| !s.is_empty()),
        }),
        "span" | "font" => Some(SpanKind::Color {
            foreground: attr(node, "data-mx-color").or_else(|| attr(node, "color")),
            background: attr(node, "data-mx-bg-color"),
        }),
        "img" => attr(node, "src")
            .filter(|src| {
                matrix_sdk::ruma::OwnedMxcUri::from(src.as_str())
                    .validate()
                    .is_ok()
            })
            .map(|source| SpanKind::Image {
                source,
                alt: attr(node, "alt").unwrap_or_default(),
                width: attr(node, "width").and_then(|v| v.parse().ok()),
                height: attr(node, "height").and_then(|v| v.parse().ok()),
            }),
        _ => None,
    };
    Ok(match span {
        Some(kind) => vec![RichNode::Span { kind, children }],
        None => children,
    })
}
fn index_spoilers(nodes: &mut [RichNode], next: &mut u32) {
    for node in nodes {
        match node {
            RichNode::Span { kind, children } => {
                if let SpanKind::Spoiler { index, .. } = kind {
                    *index = *next;
                    *next += 1;
                }
                index_spoilers(children, next);
            }
            RichNode::Block { children, .. } => index_spoilers(children, next),
            RichNode::Text(_) => {}
        }
    }
}
pub fn incoming(formatted: &FormattedBody) -> Option<RichText> {
    if formatted.format != MessageFormat::Html {
        return None;
    }
    let html = sanitized(&formatted.body).ok()?;
    let mut budget = 4096;
    let mut nodes = Vec::new();
    for node in html.children() {
        nodes.extend(project(&node, &mut budget, 0).ok()?);
    }
    index_spoilers(&mut nodes, &mut 0);
    Some(RichText { nodes })
}
pub(crate) fn message(msgtype: &MessageType) -> Option<std::sync::Arc<RichText>> {
    let formatted = match msgtype {
        MessageType::Text(m) => m.formatted.as_ref(),
        MessageType::Notice(m) => m.formatted.as_ref(),
        MessageType::Emote(m) => m.formatted.as_ref(),
        _ => None,
    }?;
    incoming(formatted).map(std::sync::Arc::new)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn any(nodes: &[RichNode], predicate: &impl Fn(&RichNode) -> bool) -> bool {
        nodes.iter().any(|n| {
            predicate(n)
                || match n {
                    RichNode::Span { children, .. } | RichNode::Block { children, .. } => {
                        any(children, predicate)
                    }
                    _ => false,
                }
        })
    }
    #[test]
    fn spoiler_indices_are_unique_stable_preorder_and_message_local() {
        fn indices(nodes: &[RichNode], out: &mut Vec<u32>) {
            for node in nodes {
                match node {
                    RichNode::Span { kind, children } => {
                        if let SpanKind::Spoiler { index, .. } = kind {
                            out.push(*index);
                        }
                        indices(children, out);
                    }
                    RichNode::Block { children, .. } => indices(children, out),
                    _ => {}
                }
            }
        }
        let html = FormattedBody::html(
            "<p><span data-mx-spoiler>first <span data-mx-spoiler='nested'>second</span></span></p><span data-mx-spoiler>third</span>",
        );
        let first = incoming(&html).unwrap();
        assert!(first == incoming(&html).unwrap());
        let mut result = Vec::new();
        indices(&first.nodes, &mut result);
        assert_eq!(result, vec![0, 1, 2]);
        let other = incoming(&FormattedBody::html(
            "<span data-mx-spoiler>other message</span>",
        ))
        .unwrap();
        let mut result = Vec::new();
        indices(&other.nodes, &mut result);
        assert_eq!(result, vec![0]);
    }
    #[test]
    fn markdown_round_trip_has_html_and_exact_plain_fallback() {
        let body = "**bold** *italic* `code`\n\n```rust\nlet a = 1;\n```\n\n[link](https://example.org)\n\n- item\n\n> quote";
        let content = compose(body.into()).unwrap();
        let MessageType::Text(text) = &content.msgtype else {
            panic!("expected text");
        };
        assert!(text.body == body);
        let formatted = text.formatted.as_ref().unwrap();
        assert!(formatted.format == MessageFormat::Html);
        let rich = incoming(formatted).unwrap();
        assert!(any(&rich.nodes, &|n| matches!(
            n,
            RichNode::Span {
                kind: SpanKind::Strong,
                ..
            }
        )));
        assert!(any(&rich.nodes, &|n| matches!(
            n,
            RichNode::Span {
                kind: SpanKind::Emphasis,
                ..
            }
        )));
        assert!(any(&rich.nodes, &|n| matches!(
            n,
            RichNode::Block {
                kind: BlockKind::Code { .. },
                ..
            }
        )));
        assert!(any(&rich.nodes, &|n| matches!(
            n,
            RichNode::Block {
                kind: BlockKind::List { .. },
                ..
            }
        )));
        assert!(any(&rich.nodes, &|n| matches!(
            n,
            RichNode::Block {
                kind: BlockKind::Quote,
                ..
            }
        )));
        assert!(any(
            &rich.nodes,
            &|n| matches!(n,RichNode::Span {kind:SpanKind::Link {target},..} if target=="https://example.org")
        ));
        let serialized = serde_json::to_value(content).unwrap();
        assert_eq!(serialized["format"], "org.matrix.custom.html");
        assert_eq!(serialized["body"], body);
        let round_trip: RoomMessageEventContent = serde_json::from_value(serialized).unwrap();
        assert!(message(&round_trip.msgtype).is_some());
    }
    #[test]
    fn element_formatting_is_structured_and_active_content_is_rejected() {
        let formatted = FormattedBody::html(
            "<mx-reply><blockquote>earlier secret</blockquote></mx-reply><p><b>bold</b><em>italic</em><span data-mx-spoiler='reason'>spoiler</span></p><script>script payload</script><iframe src='https://evil.invalid'>iframe payload</iframe><a href='jAvAsCrIpT:alert(1)' onclick='bad()'>label</a><img src='https://evil.invalid/tracker'><a href='https://example.org'>safe</a>",
        );
        let html = sanitized(&formatted.body).unwrap().to_string();
        for unsafe_text in [
            "<script",
            "<iframe",
            "javascript:",
            "jAvAsCrIpT:",
            "onclick",
            "earlier secret",
            "script payload",
            "iframe payload",
            "tracker",
        ] {
            assert!(!html.contains(unsafe_text));
        }
        let rich = incoming(&formatted).unwrap();
        assert!(any(&rich.nodes, &|n| matches!(
            n,
            RichNode::Span {
                kind: SpanKind::Spoiler { .. },
                ..
            }
        )));
        assert!(any(
            &rich.nodes,
            &|n| matches!(n,RichNode::Span {kind:SpanKind::Link {target},..} if target=="https://example.org")
        ));
    }
    #[test]
    fn plain_and_oversized_inputs_keep_a_safe_fallback() {
        let content = compose("plain & text".into()).unwrap();
        let MessageType::Text(text) = content.msgtype else {
            panic!("text");
        };
        assert_eq!(text.body, "plain & text");
        assert!(text.formatted.is_none());
        for body in ["Hello", "plain & text", "Здравей", "two\nlines"] {
            let serialized = serde_json::to_value(compose(body.into()).unwrap()).unwrap();
            assert_eq!(serialized["msgtype"], "m.text");
            assert_eq!(serialized["body"], body);
            assert!(serialized.get("formatted_body").is_none());
            assert!(serialized.get("format").is_none());
        }
        assert!(matches!(
            compose("x".repeat(65537)),
            Err(FormattingError::Limit)
        ));
        assert!(incoming(&FormattedBody::html("x".repeat(MAX_HTML + 1))).is_none());
    }
}
