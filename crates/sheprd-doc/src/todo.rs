//! Task lists as edits: what the editor's keys and the `doc todo` commands
//! do to a Markdown text. Every function reads the text and returns edits
//! in its byte offsets; the caller applies them to a [`crate::Document`] as
//! the user's or an agent's change.

use crate::{
    Edit, Error, Result,
    markdown::{self, Kind, Line, List},
    section,
};
use chrono::NaiveDateTime;
use std::ops::Range;

mod changes;
mod moves;
pub mod stamp;

pub use crate::markdown::TaskState;
pub use changes::{ChangeKind, TodoChange, changes};
pub use moves::move_item;

/// The most characters a todo added by command may have.
pub const MAX_ITEM: usize = 4096;

/// One task, as `doc todo list` reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Todo {
    /// 1-based, in document order.
    pub number: usize,
    /// 0-based line.
    pub line: usize,
    /// The task's checkbox state.
    pub state: TaskState,
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
        let Some(state) = line.task() else {
            continue;
        };
        todos.push(Todo {
            number: todos.len() + 1,
            line: index,
            state,
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
        .filter(|todo| stamp::without_stamps(&todo.text).to_lowercase() == needle)
        .collect();
    let mut matching: Vec<Todo> = match exact.as_slice() {
        [] => todos
            .iter()
            .filter(|todo| {
                !needle.is_empty()
                    && stamp::without_stamps(&todo.text)
                        .to_lowercase()
                        .contains(&needle)
            })
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

/// Finds `line` in `text` and writes `state`; an empty vector when it is not a
/// task or already in that state.
pub fn set_state(
    text: &str,
    line: usize,
    state: TaskState,
    at: Option<NaiveDateTime>,
) -> Vec<Edit> {
    let lines = markdown::lines(text);
    let Some(line) = lines.get(line) else {
        return Vec::new();
    };
    state_edits(text, line, state, at)
}

/// Writes `state` to a parsed task line; `line` must be a line of
/// [`markdown::lines`] from `text`.
pub fn state_edits(
    text: &str,
    line: &Line,
    state: TaskState,
    at: Option<NaiveDateTime>,
) -> Vec<Edit> {
    let Some(checkbox) = line.checkbox.clone() else {
        return Vec::new();
    };
    let Some(current) = line.task() else {
        return Vec::new();
    };
    if current == state {
        return Vec::new();
    }
    let mut edits = vec![Edit::new(checkbox, state.checkbox())];
    if let Some(at) = at {
        let label = line.content();
        let label_text = &text[label.clone()];
        let (remove, insert) = match state {
            TaskState::Done => {
                let mut stamp = stamp::stamp(stamp::Tag::Done, at);
                if let Some(started) = stamp::started_at(label_text) {
                    stamp.push(' ');
                    stamp.push_str(&stamp::lasted_stamp(&stamp::lasted(started, at)));
                }
                (
                    vec![stamp::Tag::Cancelled, stamp::Tag::Done, stamp::Tag::Lasted],
                    Some(stamp),
                )
            }
            TaskState::Cancelled => (
                vec![stamp::Tag::Cancelled, stamp::Tag::Done, stamp::Tag::Lasted],
                Some(stamp::stamp(stamp::Tag::Cancelled, at)),
            ),
            TaskState::Open => (
                vec![stamp::Tag::Cancelled, stamp::Tag::Done, stamp::Tag::Lasted],
                None,
            ),
        };
        edits.extend(label_edits(&label, label_text, &remove, insert));
    }
    edits.sort_by(|a, b| {
        a.range
            .start
            .cmp(&b.range.start)
            .then(a.range.end.cmp(&b.range.end))
    });
    edits
}

/// Checks or unchecks the task on `line`.
pub fn set_done(text: &str, line: usize, done: bool, at: Option<NaiveDateTime>) -> Vec<Edit> {
    set_state(
        text,
        line,
        if done {
            TaskState::Done
        } else {
            TaskState::Open
        },
        at,
    )
}

/// Flips the task on `line`.
pub fn toggle(text: &str, line: usize, at: Option<NaiveDateTime>) -> Vec<Edit> {
    let Some(state) = markdown::lines(text).get(line).and_then(Line::task) else {
        return Vec::new();
    };
    set_state(text, line, state.toggled(), at)
}

/// Toggles cancellation on `line`, reopening a cancelled task and cancelling
/// every other task state.
pub fn toggle_cancel(text: &str, line: usize, at: Option<NaiveDateTime>) -> Vec<Edit> {
    let Some(state) = markdown::lines(text).get(line).and_then(Line::task) else {
        return Vec::new();
    };
    set_state(text, line, state.cancel_toggled(), at)
}

/// Adds a dated @started stamp without changing the task state.
pub fn start(text: &str, line: usize, at: NaiveDateTime) -> Vec<Edit> {
    let lines = markdown::lines(text);
    let Some(line) = lines.get(line) else {
        return Vec::new();
    };
    if line.task().is_none() {
        return Vec::new();
    }
    let label = line.content();
    let label_text = &text[label.clone()];
    if stamp::started_at(label_text).is_some() {
        return Vec::new();
    }
    label_edits(
        &label,
        label_text,
        &[],
        Some(stamp::stamp(stamp::Tag::Started, at)),
    )
}

fn label_edits(
    label: &Range<usize>,
    text: &str,
    remove: &[stamp::Tag],
    insert: Option<String>,
) -> Vec<Edit> {
    let mut spans: Vec<Range<usize>> = remove
        .iter()
        .flat_map(|tag| stamp::spans(text, *tag))
        .collect();
    spans.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
    let insert_at = text.trim_end_matches([' ', '\t']).len();
    let prefix = remove_ranges(&text[..insert_at], &spans);
    let mut edits: Vec<Edit> = spans
        .into_iter()
        .map(|span| Edit::new(label.start + span.start..label.start + span.end, ""))
        .collect();
    if let Some(insert) = insert {
        let separator = if prefix.is_empty() { "" } else { " " };
        edits.push(Edit::insert(
            label.start + insert_at,
            format!("{separator}{insert}"),
        ));
    }
    edits
}

fn remove_ranges(text: &str, spans: &[Range<usize>]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for span in spans {
        out.push_str(&text[at..span.start]);
        at = span.end;
    }
    out.push_str(&text[at..]);
    out
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
            Some(index) => after_item(text, &lines, index, block_end(&lines, index), &task),
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
        Some(at) => after_item(text, &lines, at, block_end(&lines, at), &task),
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
fn after_item(text: &str, lines: &[Line], index: usize, end: usize, task: &str) -> Edit {
    let last = &lines[end - 1];
    let indent = &text[lines[index].range.start..lines[index].range.start + lines[index].indent];
    Edit::insert(last.range.end, format!("\n{indent}{task}"))
}

fn next_marker(list: List) -> String {
    match list {
        List::Bullet(ch) => format!("{ch} "),
        List::Ordered { number, delimiter } => format!("{}{delimiter} ", number + 1),
    }
}

/// A new open task (`{marker}[ ] `) as a sibling of the item on `line`, after
/// line `end - 1` (the last line of its block as the caller sees it, such as a
/// fold's hidden lines), and the caret after its `[ ] `. `None` when `line`
/// is not a list item or `end` is not past it.
pub fn task_after(text: &str, line: usize, end: usize) -> Option<(Edit, usize)> {
    let lines = markdown::lines(text);
    let item = lines.get(line)?;
    if end <= line || end > lines.len() {
        return None;
    }
    let Kind::Item { list, .. } = item.kind else {
        return None;
    };
    let task = format!("{}[ ] ", next_marker(list));
    let indent = item.indent;
    let edit = after_item(text, &lines, line, end, &task);
    let caret = lines[end - 1].range.end + 1 + indent + task.len();
    Some((edit, caret))
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
    let marker = next_marker(list);
    let checkbox = if task.is_some() { "[ ] " } else { "" };
    let insert = format!("\n{indent}{marker}{checkbox}");
    let caret_after = caret + insert.len();
    // Trailing spaces before the caret do not carry over.
    Some((Edit::insert(caret, insert), caret_after))
}

/// What Shift-Enter does at byte `caret`: a line break without a new list
/// marker. On a list item with text after its marker, the new line is indented
/// under that text; elsewhere it keeps the line's own leading whitespace up
/// to the caret.
pub fn line_break(text: &str, caret: usize) -> (Edit, usize) {
    let lines = markdown::lines(text);
    let line = &lines[markdown::line_at(&lines, caret)];
    let indent = if matches!(line.kind, Kind::Item { .. })
        && caret >= line.marker.end
        && !text[line.content()].trim().is_empty()
    {
        let leading = &text[line.range.start..line.range.start + line.indent];
        let marker = text[line.marker.clone()]
            .bytes()
            .map(|byte| if byte == b'\t' { '\t' } else { ' ' })
            .collect::<String>();
        format!("{leading}{marker}")
    } else {
        text[line.range.start..caret.min(line.range.start + line.indent)].to_owned()
    };
    let insert = format!("\n{indent}");
    let caret_after = caret + insert.len();
    (Edit::insert(caret, insert), caret_after)
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

/// Done and total tasks under each heading, nested sections included and
/// cancelled tasks left out, by the heading's line.
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
            if let Some(state) = other.task()
                && state != TaskState::Cancelled
            {
                total += 1;
                done += usize::from(state.is_done());
            }
        }
        if total > 0 {
            counts.push((index, done, total));
        }
    }
    counts
}

/// Which lines "hide done" hides: each resolved task with its sub-items.
pub fn resolved_lines(lines: &[Line]) -> Vec<bool> {
    let mut hidden = vec![false; lines.len()];
    let mut index = 0;
    while index < lines.len() {
        if lines[index].task().is_some_and(TaskState::is_resolved) {
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
