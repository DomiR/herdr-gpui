//! Task lists as edits: what the editor's keys and the `doc todo` commands
//! do to a Markdown text. Every function reads the text and returns edits
//! in its byte offsets; the caller applies them to a [`crate::Document`] as
//! the user's or an agent's change.

use crate::{
    Edit, Error, Result,
    markdown::{self, Kind, Line, List},
    section,
};
use std::ops::Range;

mod changes;

pub use changes::{ChangeKind, TodoChange, changes};

/// The most characters a todo added by command may have.
pub const MAX_ITEM: usize = 4096;

/// One task, as `doc todo list` reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Todo {
    /// 1-based, in document order.
    pub number: usize,
    /// 0-based line.
    pub line: usize,
    pub done: bool,
    pub text: String,
    /// Leading whitespace in columns: 0 for a top-level item.
    pub depth: usize,
    /// The heading the task is under, if any.
    pub section: Option<String>,
}

/// Every task of `text`.
pub fn todos(text: &str) -> Vec<Todo> {
    let lines = markdown::lines(text);
    let mut section = None;
    let mut todos = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.is_heading() {
            section = Some(text[line.content()].trim().to_owned());
        }
        let Some(done) = line.task() else {
            continue;
        };
        todos.push(Todo {
            number: todos.len() + 1,
            line: index,
            done,
            text: text[line.content()].trim().to_owned(),
            depth: line.depth,
            section: section.clone(),
        });
    }
    todos
}

/// The task `query` names: its number, or text it alone contains (an
/// exact match wins over several partial ones), ignoring case.
pub fn find(text: &str, query: &str) -> Result<Todo> {
    find_in(text, query, None)
}

/// [`find`] among the tasks of section `section` (by ID) alone.
pub fn find_in(text: &str, query: &str, section: Option<&str>) -> Result<Todo> {
    let lines = match section {
        Some(id) => Some(section::find(text, id)?.lines),
        None => None,
    };
    let todos: Vec<Todo> = todos(text)
        .into_iter()
        .filter(|todo| {
            lines
                .as_ref()
                .is_none_or(|lines| lines.contains(&todo.line))
        })
        .collect();
    let query = query.trim();
    if let Ok(number) = query.parse::<usize>() {
        return todos
            .into_iter()
            .find(|todo| todo.number == number)
            .ok_or_else(|| Error::NoTodo {
                query: query.to_owned(),
            });
    }
    let needle = query.to_lowercase();
    let exact: Vec<&Todo> = todos
        .iter()
        .filter(|todo| todo.text.to_lowercase() == needle)
        .collect();
    let mut matching: Vec<Todo> = match exact.as_slice() {
        [] => todos
            .iter()
            .filter(|todo| !needle.is_empty() && todo.text.to_lowercase().contains(&needle))
            .cloned()
            .collect(),
        _ => exact.into_iter().cloned().collect(),
    };
    match matching.len() {
        0 => Err(Error::NoTodo {
            query: query.to_owned(),
        }),
        1 => Ok(matching.remove(0)),
        count => Err(Error::AmbiguousTodo {
            query: query.to_owned(),
            count,
        }),
    }
}

/// Checks or unchecks the task on `line`; `None` when it is not a task or
/// already so.
pub fn set_done(text: &str, line: usize, done: bool) -> Option<Edit> {
    let lines = markdown::lines(text);
    let line = lines.get(line)?;
    let checkbox = line.checkbox.clone()?;
    (line.task()? != done).then(|| Edit::new(checkbox, if done { "[x]" } else { "[ ]" }))
}

/// Flips the task on `line`.
pub fn toggle(text: &str, line: usize) -> Option<Edit> {
    let done = markdown::lines(text).get(line)?.task()?;
    set_done(text, line, !done)
}

/// Adds the task `item`: after the last task under the heading `under`
/// (a section ID or a heading's text; a heading of that text is added at
/// the end when neither is found), else after the document's last task,
/// else at the end.
pub fn add(text: &str, item: &str, under: Option<&str>) -> Result<Edit> {
    let item = one_line(item)?;
    let lines = markdown::lines(text);
    let task = format!("- [ ] {item}");
    let Some(heading) = under.map(str::trim).filter(|heading| !heading.is_empty()) else {
        let last = lines.iter().rposition(|line| line.task().is_some());
        return Ok(match last {
            Some(index) => after_item(text, &lines, index, &task),
            None => append_block(text, &task),
        });
    };
    let by_id = section::find(text, heading)
        .ok()
        .map(|found| found.lines.start);
    let found = by_id.or_else(|| {
        lines.iter().position(|line| {
            line.is_heading() && text[line.content()].trim().eq_ignore_ascii_case(heading)
        })
    });
    let Some(index) = found else {
        return Ok(append_block(text, &format!("## {heading}\n\n{task}")));
    };
    // The heading's own lines, before any sub-heading.
    let end = lines[index + 1..]
        .iter()
        .position(Line::is_heading)
        .map_or(lines.len(), |at| index + 1 + at);
    let last = lines[index + 1..end]
        .iter()
        .rposition(|line| line.task().is_some())
        .map(|at| index + 1 + at);
    Ok(match last {
        Some(at) => after_item(text, &lines, at, &task),
        None => {
            // Right under the heading, after a blank line.
            let at = lines[index].range.end;
            Edit::insert(at, format!("\n\n{task}"))
        }
    })
}

/// Appends `addition` as a block of its own at the end of the text, or of
/// section `section` (by ID).
pub fn append(text: &str, addition: &str, section: Option<&str>) -> Result<Edit> {
    let addition = addition.trim_end_matches(['\n', '\r']);
    if addition.trim().is_empty() {
        return Err(Error::EmptyText);
    }
    let Some(id) = section else {
        return Ok(append_block(text, addition));
    };
    let range = section::find(text, id)?.range;
    let body = &text[range.clone()];
    let content = body.trim_end_matches(['\n', '\r']).len();
    if content == body.len() {
        // The section ends the text without a line break.
        let edit = append_block(body, addition);
        return Ok(Edit::new(
            range.start + edit.range.start..range.start + edit.range.end,
            edit.text,
        ));
    }
    // Before the section's trailing line breaks, which keep its gap.
    let edit = append_block(&body[..content], addition);
    let text = edit.text.trim_end_matches('\n').to_owned();
    Ok(Edit::insert(range.start + content, text))
}

/// Replaces the one place `old` occurs, in the whole text or in section
/// `section` (by ID), with `new`.
pub fn replace(text: &str, old: &str, new: &str, section: Option<&str>) -> Result<Edit> {
    let range = match section {
        Some(id) => section::find(text, id)?.range,
        None => 0..text.len(),
    };
    let edit = Edit::replace_unique(&text[range.clone()], old, new)?;
    Ok(Edit::new(
        range.start + edit.range.start..range.start + edit.range.end,
        edit.text,
    ))
}

fn one_line(item: &str) -> Result<&str> {
    let item = item.trim();
    if item.is_empty() || item.contains('\n') || item.chars().count() > MAX_ITEM {
        return Err(Error::BadItem);
    }
    Ok(item)
}

/// `block` at the end, separated from the text by a blank line when the
/// last line is not a list item, ending in a line break.
fn append_block(text: &str, block: &str) -> Edit {
    let trimmed = text.trim_end_matches(['\n', '\r']);
    let last_is_item = markdown::lines(trimmed)
        .last()
        .is_some_and(|line| line.is_item() && block.starts_with("- "));
    let separator = match (trimmed.is_empty(), last_is_item) {
        (true, _) => "",
        (false, true) => "\n",
        (false, false) => "\n\n",
    };
    Edit::new(trimmed.len()..text.len(), format!("{separator}{block}\n"))
}

/// `task` as a sibling after the item on `index` and its sub-items.
fn after_item(text: &str, lines: &[Line], index: usize, task: &str) -> Edit {
    let end = block_end(lines, index);
    let last = &lines[end - 1];
    let indent = &text[lines[index].range.start..lines[index].range.start + lines[index].indent];
    Edit::insert(last.range.end, format!("\n{indent}{task}"))
}

/// The line after the item on `index` and everything nested under it.
pub fn block_end(lines: &[Line], index: usize) -> usize {
    let depth = lines[index].depth;
    let mut end = index + 1;
    let mut probe = end;
    while probe < lines.len() {
        let line = &lines[probe];
        if line.kind == Kind::Blank {
            probe += 1;
            continue;
        }
        if line.depth <= depth || line.is_heading() {
            break;
        }
        probe += 1;
        end = probe;
    }
    end
}

/// What Enter does at byte `caret` in a list item: continue the list with a
/// new item (an unchecked task after a task, the next number after a
/// numbered one), carrying the text after the caret; or, on an item with
/// nothing after its marker, end the list by removing the marker. `None`
/// outside a list item, or with the caret inside the marker.
pub fn enter(text: &str, caret: usize) -> Option<(Edit, usize)> {
    let lines = markdown::lines(text);
    let line = &lines[markdown::line_at(&lines, caret)];
    let Kind::Item { list, task } = line.kind else {
        return None;
    };
    if caret < line.marker.end {
        return None;
    }
    if text[line.content()].trim().is_empty() {
        let edit = Edit::new(line.range.start + line.indent..line.range.end, "");
        return Some((edit, line.range.start + line.indent));
    }
    let indent = &text[line.range.start..line.range.start + line.indent];
    let marker = match list {
        List::Bullet(ch) => format!("{ch} "),
        List::Ordered { number, delimiter } => format!("{}{delimiter} ", number + 1),
    };
    let checkbox = if task.is_some() { "[ ] " } else { "" };
    let insert = format!("\n{indent}{marker}{checkbox}");
    let caret_after = caret + insert.len();
    // Trailing spaces before the caret do not carry over.
    Some((Edit::insert(caret, insert), caret_after))
}

/// Indents (or outdents) every list item whose line meets `lines_range`,
/// with its sub-items, by two spaces. Lines that are not items are left
/// alone; outdenting stops at the margin.
pub fn indent(text: &str, lines_range: Range<usize>, outdent: bool) -> Vec<Edit> {
    let lines = markdown::lines(text);
    let mut touched = vec![false; lines.len()];
    for index in lines_range.start..lines_range.end.min(lines.len()) {
        if lines[index].is_item() {
            for flag in &mut touched[index..block_end(&lines, index)] {
                *flag = true;
            }
        }
    }
    let mut edits = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !touched[index] || line.kind == Kind::Blank {
            continue;
        }
        let start = line.range.start;
        if !outdent {
            edits.push(Edit::insert(start, "  "));
            continue;
        }
        let lead = &text[start..start + line.indent];
        let remove = if lead.starts_with('\t') {
            1
        } else {
            lead.chars().take_while(|ch| *ch == ' ').count().min(2)
        };
        if remove > 0 {
            edits.push(Edit::new(start..start + remove, ""));
        }
    }
    edits
}

/// Moves the item on `line` with its sub-items above the previous sibling
/// (or below the next one). The edits and the line the item starts on
/// after them; `None` when there is no sibling that way.
///
/// A CRDT text has no move, only delete and insert, so an agent's edit
/// inside the text that moves is applied to the deleted copy. Of the two
/// blocks trading places, the shorter one is the one moved, so as little
/// text as possible is exposed to that.
pub fn move_item(text: &str, line: usize, up: bool) -> Option<(Vec<Edit>, usize)> {
    let lines = markdown::lines(text);
    let item = lines.get(line).filter(|item| item.is_item())?;
    let end = block_end(&lines, line);
    let (first, second) = if up {
        let previous = (0..line).rev().find(|at| {
            let other = &lines[*at];
            other.kind != Kind::Blank && other.depth <= item.depth
        })?;
        let other = &lines[previous];
        if !other.is_item() || other.depth != item.depth || block_end(&lines, previous) != line {
            return None;
        }
        (previous..line, line..end)
    } else {
        let next = lines
            .get(end)
            .filter(|next| next.is_item() && next.depth == item.depth)?;
        let _ = next;
        (line..end, end..block_end(&lines, end))
    };
    let span =
        |range: &Range<usize>| lines[range.start].range.start..lines[range.end - 1].range.end;
    let a = span(&first);
    let b = span(&second);
    // `a` is directly followed by its line break and then `b`.
    let edits = if b.len() <= a.len() {
        vec![
            Edit::insert(a.start, format!("{}\n", &text[b.clone()])),
            Edit::new(a.end..b.end, ""),
        ]
    } else {
        vec![
            Edit::new(a.start..b.start, ""),
            Edit::insert(b.end, format!("\n{}", &text[a.clone()])),
        ]
    };
    let moved_to = if up {
        first.start
    } else {
        first.start + second.len()
    };
    Some((edits, moved_to))
}

/// Done and total tasks under each heading, nested sections included, by
/// the heading's line.
pub fn section_counts(lines: &[Line]) -> Vec<(usize, usize, usize)> {
    let mut counts = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(level) = line.level() else {
            continue;
        };
        let (mut done, mut total) = (0, 0);
        for other in &lines[index + 1..] {
            if other.level().is_some_and(|other| other <= level) {
                break;
            }
            if let Some(checked) = other.task() {
                total += 1;
                done += usize::from(checked);
            }
        }
        if total > 0 {
            counts.push((index, done, total));
        }
    }
    counts
}

/// Which lines "hide done" hides: each done task with its sub-items.
pub fn done_lines(lines: &[Line]) -> Vec<bool> {
    let mut hidden = vec![false; lines.len()];
    let mut index = 0;
    while index < lines.len() {
        if lines[index].task() == Some(true) {
            let end = block_end(lines, index);
            for flag in &mut hidden[index..end] {
                *flag = true;
            }
            index = end;
        } else {
            index += 1;
        }
    }
    hidden
}

#[cfg(test)]
mod tests;
