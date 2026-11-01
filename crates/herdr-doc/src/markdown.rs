//! Markdown as the editor draws it: each line's block kind, where its
//! marker (`# `, `- [ ] `, `1. `, `> `) is, and the inline styles in it. The
//! text itself is never rewritten; ranges are byte offsets into it, so the
//! caret and selection map one to one onto what is drawn.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

mod decorate;
mod folds;

pub use decorate::{Decoration, decorations};
pub use folds::{Fold, FoldKind, folds};

/// What a list item's marker is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum List {
    /// `-`, `*`, or `+`.
    Bullet(char),
    /// `1.` or `1)`.
    Ordered { number: u64, delimiter: char },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Blank,
    Paragraph,
    Heading {
        level: u8,
    },
    /// A list item; `task` is `Some(done)` for `- [ ]` and `- [x]`.
    Item {
        list: List,
        task: Option<bool>,
    },
    Quote,
    /// A ```` ``` ```` or `~~~` line opening or closing a code block.
    Fence,
    /// A line inside a fenced code block.
    Code,
    /// `---`, `***`, or `___`.
    Rule,
}

/// One line of the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The line's bytes, without its line break.
    pub range: Range<usize>,
    /// Its leading whitespace, in columns (a tab is four).
    pub depth: usize,
    /// Its leading whitespace, in bytes.
    pub indent: usize,
    /// The block marker with the space after it, such as `- [ ] `; empty
    /// for a paragraph or code.
    pub marker: Range<usize>,
    /// A task's `[ ]` or `[x]`.
    pub checkbox: Option<Range<usize>>,
    pub kind: Kind,
}

impl Line {
    /// The text after the marker.
    pub fn content(&self) -> Range<usize> {
        self.marker.end.max(self.range.start + self.indent)..self.range.end
    }

    pub fn is_heading(&self) -> bool {
        matches!(self.kind, Kind::Heading { .. })
    }

    pub fn level(&self) -> Option<u8> {
        match self.kind {
            Kind::Heading { level } => Some(level),
            _ => None,
        }
    }

    pub fn task(&self) -> Option<bool> {
        match self.kind {
            Kind::Item { task, .. } => task,
            _ => None,
        }
    }

    pub fn is_item(&self) -> bool {
        matches!(self.kind, Kind::Item { .. })
    }
}

/// Every line of `text`, with fenced code blocks recognized across lines.
/// A text ending in a line break ends in an empty line, where the caret
/// goes after it.
pub fn lines(text: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut start = 0;
    loop {
        let end = text[start..].find('\n').map_or(text.len(), |at| start + at);
        let mut content_end = end;
        if text[start..end].ends_with('\r') {
            content_end -= 1;
        }
        lines.push(parse(text, start..content_end, &mut fence));
        if end == text.len() {
            break;
        }
        start = end + 1;
    }
    lines
}

fn parse(text: &str, range: Range<usize>, fence: &mut Option<(char, usize)>) -> Line {
    let line = &text[range.clone()];
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let depth = line[..indent]
        .chars()
        .map(|ch| if ch == '\t' { 4 } else { 1 })
        .sum();
    let rest = &line[indent..];
    let at = range.start + indent;
    let plain = |kind| Line {
        range: range.clone(),
        depth,
        indent,
        marker: at..at,
        checkbox: None,
        kind,
    };
    if let Some((ch, count)) = *fence {
        let run = rest.chars().take_while(|c| *c == ch).count();
        if run >= count && rest[run..].trim().is_empty() {
            *fence = None;
            return plain(Kind::Fence);
        }
        return plain(Kind::Code);
    }
    if rest.is_empty() {
        return plain(Kind::Blank);
    }
    for ch in ['`', '~'] {
        let run = rest.chars().take_while(|c| *c == ch).count();
        if run >= 3 && (ch == '~' || !rest[run..].contains('`')) {
            *fence = Some((ch, run));
            return plain(Kind::Fence);
        }
    }
    let marked = |len: usize, kind| Line {
        marker: at..at + len,
        ..plain(kind)
    };
    let hashes = rest.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && (rest.len() == hashes || rest[hashes..].starts_with(' ')) {
        let len = (hashes + 1).min(rest.len());
        return marked(
            len,
            Kind::Heading {
                level: hashes as u8,
            },
        );
    }
    if is_rule(rest) {
        return marked(rest.len(), Kind::Rule);
    }
    if let Some(after) = rest.strip_prefix('>') {
        let len = 1 + usize::from(after.starts_with(' '));
        return marked(len, Kind::Quote);
    }
    let Some((list, len)) = list_marker(rest) else {
        return plain(Kind::Paragraph);
    };
    let after = &rest[len..];
    let task = ["[ ]", "[x]", "[X]"]
        .iter()
        .find(|box_| {
            after.starts_with(*box_) && (after.len() == 3 || after[3..].starts_with([' ', '\t']))
        })
        .map(|box_| *box_ != "[ ]");
    match task {
        Some(done) => {
            let checkbox = at + len..at + len + 3;
            let marker_len = len + 3 + usize::from(after.len() > 3);
            Line {
                checkbox: Some(checkbox),
                ..marked(
                    marker_len,
                    Kind::Item {
                        list,
                        task: Some(done),
                    },
                )
            }
        }
        None => marked(len, Kind::Item { list, task: None }),
    }
}

/// A list marker at the start of `rest` and its length with the space after.
fn list_marker(rest: &str) -> Option<(List, usize)> {
    let mut chars = rest.chars();
    let first = chars.next()?;
    if matches!(first, '-' | '*' | '+') {
        return match chars.next() {
            Some(' ' | '\t') => Some((List::Bullet(first), 2)),
            None => Some((List::Bullet(first), 1)),
            _ => None,
        };
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 9 {
        return None;
    }
    let delimiter = rest[digits..].chars().next()?;
    if !matches!(delimiter, '.' | ')') {
        return None;
    }
    let number = rest[..digits].parse().ok()?;
    let len = match rest[digits + 1..].chars().next() {
        Some(' ' | '\t') => digits + 2,
        None => digits + 1,
        _ => return None,
    };
    Some((List::Ordered { number, delimiter }, len))
}

fn is_rule(rest: &str) -> bool {
    let trimmed = rest.trim_end();
    let Some(first) = trimmed.chars().next() else {
        return false;
    };
    matches!(first, '-' | '*' | '_')
        && trimmed.chars().filter(|ch| *ch == first).count() >= 3
        && trimmed.chars().all(|ch| ch == first || ch == ' ')
}

/// An inline style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Style {
    Strong,
    Emphasis,
    Strike,
    Code,
    /// A link; `url` is where its address is in the text, if written there.
    Link {
        url: Option<Range<usize>>,
    },
    /// Syntax the reader does not read: `**`, backticks, `[`, `](url)`.
    Marker,
}

/// A style over a range of the text. Spans may nest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub style: Style,
}

/// The inline styles of `line`'s content, in `text`'s offsets. Code lines
/// have none.
pub fn inline(text: &str, line: &Line) -> Vec<Span> {
    if matches!(
        line.kind,
        Kind::Code | Kind::Fence | Kind::Blank | Kind::Rule
    ) {
        return Vec::new();
    }
    let content = line.content();
    let base = content.start;
    let source = &text[content];
    let mut spans = Vec::new();
    let mut open: Vec<(usize, Tag<'_>)> = Vec::new();
    let parser = Parser::new_ext(source, Options::ENABLE_STRIKETHROUGH).into_offset_iter();
    for (event, range) in parser {
        let range = base + range.start..base + range.end;
        match event {
            Event::Start(tag) => open.push((range.start, tag)),
            Event::End(end) => {
                let Some((start, tag)) = open.pop() else {
                    continue;
                };
                let whole = start..range.end;
                push_tag(text, &tag, end, whole, &mut spans);
            }
            Event::Code(_) => {
                let ticks = text[range.clone()]
                    .chars()
                    .take_while(|c| *c == '`')
                    .count();
                if range.len() > ticks * 2 {
                    spans.push(Span {
                        range: range.start..range.start + ticks,
                        style: Style::Marker,
                    });
                    spans.push(Span {
                        range: range.start + ticks..range.end - ticks,
                        style: Style::Code,
                    });
                    spans.push(Span {
                        range: range.end - ticks..range.end,
                        style: Style::Marker,
                    });
                }
            }
            _ => {}
        }
    }
    spans.sort_by_key(|span| (span.range.start, std::cmp::Reverse(span.range.end)));
    spans
}

fn push_tag(text: &str, tag: &Tag<'_>, end: TagEnd, whole: Range<usize>, spans: &mut Vec<Span>) {
    let delimited = |len: usize, style: Style, spans: &mut Vec<Span>| {
        if whole.len() <= len * 2 {
            return;
        }
        spans.push(Span {
            range: whole.start..whole.start + len,
            style: Style::Marker,
        });
        spans.push(Span {
            range: whole.start + len..whole.end - len,
            style,
        });
        spans.push(Span {
            range: whole.end - len..whole.end,
            style: Style::Marker,
        });
    };
    match (tag, end) {
        (Tag::Strong, TagEnd::Strong) => delimited(2, Style::Strong, spans),
        (Tag::Emphasis, TagEnd::Emphasis) => delimited(1, Style::Emphasis, spans),
        (Tag::Strikethrough, TagEnd::Strikethrough) => {
            let len = text[whole.clone()]
                .chars()
                .take_while(|c| *c == '~')
                .count();
            delimited(len.clamp(1, 2), Style::Strike, spans);
        }
        (Tag::Link { .. }, TagEnd::Link) => {
            let source = &text[whole.clone()];
            // An inline link `[text](url)`; an autolink `<url>` is all link.
            let Some(close) = source
                .starts_with('[')
                .then(|| source.rfind("]("))
                .flatten()
            else {
                spans.push(Span {
                    range: whole,
                    style: Style::Link { url: None },
                });
                return;
            };
            let label = whole.start + 1..whole.start + close;
            let url_end = whole.end - usize::from(source.ends_with(')'));
            let url = whole.start + close + 2..url_end;
            spans.push(Span {
                range: whole.start..label.start,
                style: Style::Marker,
            });
            spans.push(Span {
                range: label,
                style: Style::Link {
                    url: Some(url.clone()),
                },
            });
            spans.push(Span {
                range: whole.start + close..whole.end,
                style: Style::Marker,
            });
        }
        _ => {}
    }
}

/// The URL a link span points to, if written in the text.
pub fn link_url<'a>(text: &'a str, span: &Span) -> Option<&'a str> {
    match &span.style {
        Style::Link { url: Some(url) } => text
            .get(url.clone())
            .map(|url| url.split_whitespace().next().unwrap_or(url)),
        Style::Link { url: None } => text
            .get(span.range.clone())
            .map(|url| url.trim_start_matches('<').trim_end_matches('>')),
        _ => None,
    }
}

/// Which line holds byte `offset`.
pub fn line_at(lines: &[Line], offset: usize) -> usize {
    lines
        .partition_point(|line| line.range.start <= offset)
        .saturating_sub(1)
}

#[cfg(test)]
mod tests;
