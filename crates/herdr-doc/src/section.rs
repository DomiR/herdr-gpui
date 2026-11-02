//! Sections: each `#`, `##`, or `###` heading starts one, which runs to the
//! next such heading. Deeper headings stay inside. Agents name a section by
//! its slug, the anchor ID GitHub and most Markdown renderers give its
//! heading, to scope a change and to change it while others edit elsewhere.

use crate::{Error, Result, markdown};
use std::{collections::HashMap, ops::Range};

/// The deepest heading that starts a section.
pub const MAX_LEVEL: u8 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// The heading's slug, unique in the document.
    pub id: String,
    pub title: String,
    pub level: u8,
    /// The heading line and the lines after it, by index.
    pub lines: Range<usize>,
    /// The same, in bytes, including the last line's break.
    pub range: Range<usize>,
}

/// Every section of `text`, in order.
pub fn sections(text: &str) -> Vec<Section> {
    let lines = markdown::lines(text);
    let starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.level().is_some_and(|level| level <= MAX_LEVEL))
        .map(|(index, _)| index)
        .collect();
    let mut seen: HashMap<String, usize> = HashMap::new();
    starts
        .iter()
        .enumerate()
        .map(|(k, &start)| {
            let end = starts.get(k + 1).copied().unwrap_or(lines.len());
            let heading = &lines[start];
            let title = text[heading.content()].trim().trim_end_matches('#').trim();
            let base = slug(title);
            let count = seen.entry(base.clone()).or_insert(0);
            let id = match *count {
                0 => base.clone(),
                n => format!("{base}-{n}"),
            };
            *count += 1;
            let byte_end = lines.get(end).map_or(text.len(), |next| next.range.start);
            Section {
                id,
                title: title.to_owned(),
                level: heading.level().unwrap_or(1),
                lines: start..end,
                range: heading.range.start..byte_end,
            }
        })
        .collect()
}

/// The section `id` names in `text`.
pub fn find(text: &str, id: &str) -> Result<Section> {
    let id = id.trim().trim_start_matches('#');
    sections(text)
        .into_iter()
        .find(|section| section.id == id)
        .ok_or_else(|| Error::NoSection { id: id.to_owned() })
}

/// A heading's anchor ID as GitHub makes it: lowercase, letters, digits,
/// `-` and `_` kept, spaces as hyphens, everything else dropped.
pub fn slug(title: &str) -> String {
    title
        .chars()
        .flat_map(char::to_lowercase)
        .filter_map(|ch| match ch {
            ' ' => Some('-'),
            '-' | '_' => Some(ch),
            ch if ch.is_alphanumeric() => Some(ch),
            _ => None,
        })
        .collect()
}
