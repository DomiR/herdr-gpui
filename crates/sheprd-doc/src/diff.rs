//! The edits that turn one text into another: lines first, then characters
//! inside each changed run of lines, so a word an agent changed in a line
//! the user is typing in merges with the typing instead of replacing the
//! line. A run too large to compare by characters, or a comparison past its
//! deadline, is replaced whole.

use crate::Edit;
use similar::{Algorithm, DiffTag, capture_diff_slices_deadline};
use std::time::{Duration, Instant};

/// Character comparison runs only on hunks up to this many bytes a side.
const MAX_REFINED: usize = 16 * 1024;
/// How long one diff may take before it settles for coarser edits.
const DEADLINE: Duration = Duration::from_millis(250);

/// Edits, ascending and not overlapping, in `old`'s byte offsets, that turn
/// `old` into `new`.
pub fn edits(old: &str, new: &str) -> Vec<Edit> {
    if old == new {
        return Vec::new();
    }
    let deadline = Instant::now() + DEADLINE;
    let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = new.split_inclusive('\n').collect();
    let old_starts = starts(&old_lines);
    let new_starts = starts(&new_lines);
    let mut edits = Vec::new();
    for op in capture_diff_slices_deadline(Algorithm::Myers, &old_lines, &new_lines, Some(deadline))
    {
        let (tag, old_range, new_range) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            continue;
        }
        let old_bytes = old_starts[old_range.start]..old_starts[old_range.end];
        let new_bytes = new_starts[new_range.start]..new_starts[new_range.end];
        refine(
            old,
            old_bytes,
            &new[new_bytes],
            Instant::now() < deadline,
            &mut edits,
        );
    }
    edits
}

/// Each line's byte offset, and the end.
fn starts(lines: &[&str]) -> Vec<usize> {
    let mut starts = Vec::with_capacity(lines.len() + 1);
    let mut at = 0;
    starts.push(at);
    for line in lines {
        at += line.len();
        starts.push(at);
    }
    starts
}

/// Pushes the edits that turn `old[range]` into `new`: by character when
/// both are small and time remains, else one replacement.
fn refine(old: &str, range: std::ops::Range<usize>, new: &str, in_time: bool, out: &mut Vec<Edit>) {
    let before = &old[range.clone()];
    if !in_time || before.is_empty() || new.is_empty() || before.len().max(new.len()) > MAX_REFINED
    {
        out.push(Edit::new(range, new));
        return;
    }
    let old_chars: Vec<(usize, char)> = before.char_indices().collect();
    let new_chars: Vec<(usize, char)> = new.char_indices().collect();
    let old_plain: Vec<char> = old_chars.iter().map(|(_, ch)| *ch).collect();
    let new_plain: Vec<char> = new_chars.iter().map(|(_, ch)| *ch).collect();
    let byte = |chars: &[(usize, char)], len: usize, index: usize| {
        chars.get(index).map_or(len, |(at, _)| *at)
    };
    let deadline = Instant::now() + DEADLINE / 4;
    for op in capture_diff_slices_deadline(Algorithm::Myers, &old_plain, &new_plain, Some(deadline))
    {
        let (tag, old_range, new_range) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            continue;
        }
        let start = range.start + byte(&old_chars, before.len(), old_range.start);
        let end = range.start + byte(&old_chars, before.len(), old_range.end);
        let text = &new[byte(&new_chars, new.len(), new_range.start)
            ..byte(&new_chars, new.len(), new_range.end)];
        out.push(Edit::new(start..end, text));
    }
}

#[cfg(test)]
mod tests;
