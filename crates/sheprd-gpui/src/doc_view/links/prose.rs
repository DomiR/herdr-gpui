use sheprd_doc::markdown::{self, Kind, Line, Style};
use std::ops::Range;

use super::reference;

const MAX_TOKENS: usize = 64;

/// Finds qualifying bare file paths in one Markdown line.
pub(in crate::doc_view) fn bare_paths(text: &str, line: &Line) -> Vec<Range<usize>> {
    if matches!(
        line.kind,
        Kind::Code | Kind::Fence | Kind::Blank | Kind::Rule
    ) {
        return Vec::new();
    }
    let content = line.content();
    let source = &text[content.clone()];
    let mut covered = vec![false; source.len()];
    for span in markdown::inline(text, line) {
        if matches!(span.style, Style::Code | Style::Link { .. } | Style::Marker) {
            let start = span.range.start.max(content.start) - content.start;
            let end = span.range.end.min(content.end) - content.start;
            if start < end {
                covered[start..end].fill(true);
            }
        }
    }

    let mut out = Vec::new();
    let mut start = None;
    for (offset, ch) in source.char_indices() {
        if covered[offset] || ch.is_whitespace() {
            finish(source, content.start, &mut start, offset, &mut out);
        } else if start.is_none() {
            start = Some(offset);
        }
    }
    finish(source, content.start, &mut start, source.len(), &mut out);
    out
}

fn finish(
    source: &str,
    base: usize,
    start: &mut Option<usize>,
    end: usize,
    out: &mut Vec<Range<usize>>,
) {
    let Some(start) = start.take() else {
        return;
    };
    let range = trim_token(source, start..end);
    if range.start < range.end && qualifies(&source[range.clone()]) && out.len() < MAX_TOKENS {
        out.push(base + range.start..base + range.end);
    }
}

fn trim_token(text: &str, mut range: Range<usize>) -> Range<usize> {
    while range.start < range.end {
        let ch = text[range.clone()].chars().next();
        if ch.is_some_and(|ch| "([<\"'*".contains(ch)) {
            range.start += ch.map_or(0, char::len_utf8);
        } else {
            break;
        }
    }
    while range.start < range.end {
        let Some((offset, ch)) = text[range.clone()].char_indices().next_back() else {
            break;
        };
        if ".,;)]>\"'!?*".contains(ch) || ch == ':' {
            range.end = range.start + offset;
        } else {
            break;
        }
    }
    range
}

fn qualifies(text: &str) -> bool {
    let Some(reference) = reference::parse(text, false) else {
        return false;
    };
    if text.starts_with('-')
        || text.contains("://")
        || text.contains('@')
        || matches!(text, "e.g" | "i.e")
    {
        return false;
    }
    if reference.home
        || reference.path.contains('/')
        || (cfg!(windows) && reference.path.contains('\\'))
    {
        return true;
    }
    let path = &reference.path;
    let Some(dot) = path.rfind('.') else {
        return false;
    };
    let extension = &path[dot + 1..];
    !path[..dot].is_empty()
        && (1..=8).contains(&extension.len())
        && extension
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
        && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
}
