use super::{Kind, Line, Style, inline};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagSpan {
    pub range: Range<usize>,
    pub name: Range<usize>,
    pub kind: TagKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagKind {
    // keep-sorted start
    Cancelled,
    Critical,
    Done,
    Generic,
    High,
    Low,
    Today,
    // keep-sorted end
}

pub fn tags(text: &str, line: &Line) -> Vec<TagSpan> {
    if matches!(
        line.kind,
        Kind::Code | Kind::Fence | Kind::Blank | Kind::Rule
    ) {
        return Vec::new();
    }
    let content = line.content();
    let source = &text[content.clone()];
    let styles = inline(text, line);
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(relative) = source[at..].find('@') {
        let start = at + relative;
        let global_start = content.start + start;
        let preceded_by_whitespace = start > 0
            && source[..start]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        if start != 0 && !preceded_by_whitespace {
            at = start + 1;
            continue;
        }
        let name_start = start + 1;
        let Some(first) = source[name_start..].chars().next() else {
            break;
        };
        if !first.is_ascii_alphanumeric() && first != '_' {
            at = name_start;
            continue;
        }
        let mut name_end = name_start + first.len_utf8();
        for ch in source[name_end..].chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                name_end += ch.len_utf8();
            } else {
                break;
            }
        }
        let end = if source[name_end..].starts_with('(') {
            source[name_end..]
                .find(')')
                .map_or(name_end, |close| name_end + close + 1)
        } else {
            name_end
        };
        let range = global_start..content.start + end;
        if !styles.iter().any(|span| {
            matches!(&span.style, Style::Code | Style::Link { .. })
                && span.range.start < range.end
                && range.start < span.range.end
        }) {
            let name = global_start + 1..content.start + name_end;
            let kind = match text[name.clone()].to_ascii_lowercase().as_str() {
                "cancelled" => TagKind::Cancelled,
                "critical" => TagKind::Critical,
                "done" | "lasted" => TagKind::Done,
                "high" => TagKind::High,
                "low" => TagKind::Low,
                "today" => TagKind::Today,
                _ => TagKind::Generic,
            };
            out.push(TagSpan { range, name, kind });
        }
        at = end.max(name_end);
    }
    out
}
