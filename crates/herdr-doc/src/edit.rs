//! One replacement in a text, by byte range.

use std::ops::Range;

/// Replaces `range` (bytes of the text it was made for) with `text`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

impl Edit {
    pub fn new(range: Range<usize>, text: impl Into<String>) -> Self {
        Self {
            range,
            text: text.into(),
        }
    }

    pub fn insert(at: usize, text: impl Into<String>) -> Self {
        Self::new(at..at, text)
    }

    /// Replaces the one place `old` occurs in `text` with `new`.
    pub fn replace_unique(text: &str, old: &str, new: &str) -> crate::Result<Self> {
        if old.is_empty() {
            return Err(crate::Error::EmptyOld);
        }
        let mut found = text.match_indices(old);
        let Some((at, _)) = found.next() else {
            return Err(crate::Error::NoText);
        };
        let more = found.count();
        if more > 0 {
            return Err(crate::Error::AmbiguousText { count: more + 1 });
        }
        Ok(Self::new(at..at + old.len(), new))
    }

    /// Applies `edits`, which must not overlap, to `text`. A plain string
    /// version of what [`crate::Document`] does, for tests and previews.
    pub fn apply_all(text: &str, edits: &[Edit]) -> String {
        let mut sorted: Vec<&Edit> = edits.iter().collect();
        sorted.sort_by_key(|edit| std::cmp::Reverse(edit.range.start));
        let mut out = text.to_owned();
        for edit in sorted {
            out.replace_range(edit.range.clone(), &edit.text);
        }
        out
    }

    /// Where byte `offset` of the text before `edits` (ascending, not
    /// overlapping) falls after them. Text inserted at the offset goes
    /// before it; an offset inside a replaced range moves to the end of
    /// the replacement.
    pub fn map_offset(offset: usize, edits: &[Edit]) -> usize {
        let mut shift: isize = 0;
        for edit in edits {
            if edit.range.start > offset {
                break;
            }
            if edit.range.end > offset {
                return edit.range.start.saturating_add_signed(shift) + edit.text.len();
            }
            shift += edit.text.len() as isize - edit.range.len() as isize;
        }
        offset.saturating_add_signed(shift)
    }
}
