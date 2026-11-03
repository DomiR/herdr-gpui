use super::block_end;
use crate::{
    Edit,
    markdown::{self, Kind, Line},
};
use std::ops::Range;

/// Moves the item or movable plain line on `line` with its sub-items above
/// the previous block or below the next block. Headings, blank lines, code,
/// and fence lines do not move, and moving a whole section is out of scope.
/// The edits and the line where the moved unit starts after them are
/// returned; `None` means that the unit cannot move that way.
///
/// A CRDT text has no move, only delete and insert, so an agent's edit
/// inside the text that moves is applied to the deleted copy. Of the two
/// blocks trading places, the shorter one is the one moved, so as little
/// text as possible is exposed to that.
pub fn move_item(text: &str, line: usize, up: bool) -> Option<(Vec<Edit>, usize)> {
    let lines = markdown::lines(text);
    let (end, depth) = movable_unit(&lines, line)?;
    let has_parent = has_enclosing_item(&lines, line, depth);

    if up {
        let previous = (0..line).rev().find(|at| {
            let other = &lines[*at];
            other.kind != Kind::Blank && other.depth <= depth
        })?;
        let other = &lines[previous];
        if other.is_heading() {
            if has_parent || contains_code(&lines, previous + 1..line) {
                return None;
            }
            return relocate_up(text, &lines, line, end, previous);
        }
        if other.depth < depth || matches!(other.kind, Kind::Code | Kind::Fence) {
            return None;
        }
        let other_end = unit_end(&lines, previous);
        if other_end > line || !all_blank(&lines, other_end..line) {
            return None;
        }
        swap(text, &lines, previous..other_end, line..end, true)
    } else {
        let next = (end..lines.len()).find(|at| lines[*at].kind != Kind::Blank)?;
        let other = &lines[next];
        if other.is_heading() {
            if has_parent {
                return None;
            }
            return relocate_down(text, &lines, line, end, next);
        }
        if other.depth != depth || matches!(other.kind, Kind::Code | Kind::Fence) {
            return None;
        }
        let next_end = unit_end(&lines, next);
        swap(text, &lines, line..end, next..next_end, false)
    }
}

fn movable_unit(lines: &[Line], line: usize) -> Option<(usize, usize)> {
    let current = lines.get(line)?;
    if matches!(
        current.kind,
        Kind::Blank | Kind::Heading { .. } | Kind::Code | Kind::Fence
    ) {
        return None;
    }
    Some((unit_end(lines, line), current.depth))
}

fn unit_end(lines: &[Line], line: usize) -> usize {
    if lines[line].is_item() {
        block_end(lines, line)
    } else {
        line + 1
    }
}

fn has_enclosing_item(lines: &[Line], line: usize, depth: usize) -> bool {
    for previous in (0..line).rev() {
        let other = &lines[previous];
        if other.kind == Kind::Blank {
            continue;
        }
        if other.is_heading() {
            return false;
        }
        if other.depth < depth {
            return other.is_item();
        }
    }
    false
}

fn contains_code(lines: &[Line], range: Range<usize>) -> bool {
    lines[range]
        .iter()
        .any(|line| matches!(line.kind, Kind::Code | Kind::Fence))
}

fn all_blank(lines: &[Line], range: Range<usize>) -> bool {
    lines[range].iter().all(|line| line.kind == Kind::Blank)
}

fn swap(
    text: &str,
    lines: &[Line],
    first: Range<usize>,
    second: Range<usize>,
    up: bool,
) -> Option<(Vec<Edit>, usize)> {
    let span =
        |range: &Range<usize>| lines[range.start].range.start..lines[range.end - 1].range.end;
    let a = span(&first);
    let b = span(&second);
    let gap = &text[a.end..b.start];
    let edits = if b.len() <= a.len() {
        vec![
            Edit::insert(a.start, format!("{}{}", &text[b.clone()], gap)),
            Edit::new(a.end..b.end, ""),
        ]
    } else {
        vec![
            Edit::new(a.start..b.start, ""),
            Edit::insert(b.end, format!("{}{}", gap, &text[a.clone()])),
        ]
    };
    let moved_to = if up {
        first.start
    } else {
        first.start + second.len() - 1 + gap.bytes().filter(|byte| *byte == b'\n').count()
    };
    Some((edits, moved_to))
}

fn relocate_up(
    text: &str,
    lines: &[Line],
    line: usize,
    end: usize,
    heading: usize,
) -> Option<(Vec<Edit>, usize)> {
    let unit = lines[line].range.start..lines[end - 1].range.end;
    let remove = if unit.end == text.len() {
        let start = line
            .checked_sub(1)
            .map_or(0, |previous| lines[previous].range.end);
        start..unit.end
    } else {
        unit.start..lines[end].range.start
    };
    let moved = text[unit].to_owned();
    let previous = (0..heading).rev().find(|at| lines[*at].kind != Kind::Blank);
    let (insert_at, insert_text, moved_to) = match previous {
        Some(previous) => {
            let separator = line_break_after(text, lines, previous);
            (
                lines[previous].range.end,
                format!("{}{}", separator, moved),
                previous + 1,
            )
        }
        None => {
            let separator = line_break_after(text, lines, heading);
            (0, format!("{}{}", moved, separator), 0)
        }
    };
    Some((
        vec![Edit::insert(insert_at, insert_text), Edit::new(remove, "")],
        moved_to,
    ))
}

fn relocate_down(
    text: &str,
    lines: &[Line],
    line: usize,
    end: usize,
    heading: usize,
) -> Option<(Vec<Edit>, usize)> {
    let unit = lines[line].range.start..lines[end - 1].range.end;
    let remove_end = lines.get(end).map_or(text.len(), |next| next.range.start);
    let separator = line_break_after(text, lines, heading);
    let moved_to = heading.checked_sub(end - line)? + 1;
    Some((
        vec![
            Edit::insert(
                lines[heading].range.end,
                format!("{}{}", separator, &text[unit.clone()]),
            ),
            Edit::new(unit.start..remove_end, ""),
        ],
        moved_to,
    ))
}

fn line_break_after<'a>(text: &'a str, lines: &[Line], line: usize) -> &'a str {
    lines
        .get(line + 1)
        .map(|next| &text[lines[line].range.end..next.range.start])
        .filter(|separator| !separator.is_empty())
        .unwrap_or("\n")
}
