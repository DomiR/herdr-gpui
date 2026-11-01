//! The text area's text and selection, and every edit to them, without
//! drawing: byte offsets on char boundaries, an undo history, and the
//! Markdown formatting herdr-dog's composer offers. Only the platform input
//! boundary converts to UTF-16, in the text area itself.

use std::ops::Range;

/// The most text a field holds.
pub(super) const MAX_TEXT: usize = 64 * 1024;
/// Undo steps kept.
const MAX_UNDO: usize = 200;

/// A Markdown edit from the toolbar or a shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    Heading,
    Bold,
    Italic,
    Code,
    Link,
    Quote,
    Bullets,
    Tasks,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    text: String,
    cursor: usize,
    anchor: usize,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Editing {
    pub(super) text: String,
    pub(super) cursor: usize,
    pub(super) anchor: usize,
    /// Text being composed by an input method.
    pub(super) marked: Option<Range<usize>>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

/// `text` without control characters other than newlines and tabs.
fn clean(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}

impl Editing {
    pub(super) fn selection(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }

    pub(super) fn select_to(&mut self, offset: usize, extend: bool) {
        self.cursor = offset.min(self.text.len());
        if !extend {
            self.anchor = self.cursor;
        }
    }

    pub(super) fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
    }

    pub(super) fn previous(&self, from: usize) -> usize {
        self.text[..from]
            .char_indices()
            .next_back()
            .map_or(0, |(at, _)| at)
    }

    pub(super) fn next(&self, from: usize) -> usize {
        self.text[from..]
            .chars()
            .next()
            .map_or(from, |c| from + c.len_utf8())
    }

    /// Where the logical line holding `at` starts.
    pub(super) fn line_start(&self, at: usize) -> usize {
        self.text[..at].rfind('\n').map_or(0, |newline| newline + 1)
    }

    /// Where the logical line holding `at` ends, before its newline.
    pub(super) fn line_end(&self, at: usize) -> usize {
        self.text[at..]
            .find('\n')
            .map_or(self.text.len(), |newline| at + newline)
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
            anchor: self.anchor,
        }
    }

    /// Remembers the text before an edit, for Undo.
    fn remember(&mut self) {
        if self.undo.last().is_some_and(|last| last.text == self.text) {
            return;
        }
        self.undo.push(self.snapshot());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.text = snapshot.text;
        self.cursor = snapshot.cursor.min(self.text.len());
        self.anchor = snapshot.anchor.min(self.text.len());
        self.marked = None;
    }

    pub(super) fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(snapshot);
        true
    }

    pub(super) fn redo(&mut self) -> bool {
        let Some(snapshot) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(snapshot);
        true
    }

    /// Replaces `range` (byte offsets; else the marked text, else the
    /// selection) with `text`, marking it while `composing`. `selected` is
    /// the selection within `text`, as an input method names it. Whether
    /// the text changed.
    pub(super) fn replace(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        composing: bool,
        selected: Option<Range<usize>>,
    ) -> bool {
        let range = range
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection());
        let range = range.start.min(self.text.len())..range.end.min(self.text.len());
        let inserted = clean(text);
        if self.text.len() - range.len() + inserted.len() > MAX_TEXT {
            return false;
        }
        let changed = self.text[range.clone()] != inserted;
        if changed && !composing && self.marked.is_none() {
            self.remember();
        }
        self.text.replace_range(range.clone(), &inserted);
        self.marked = (composing && !inserted.is_empty())
            .then_some(range.start..range.start + inserted.len());
        self.cursor = range.start + inserted.len();
        self.anchor = self.cursor;
        if composing && let Some(selected) = selected {
            let start = range.start + selected.start.min(inserted.len());
            let end = range.start + selected.end.min(inserted.len());
            if self.text.is_char_boundary(start) && self.text.is_char_boundary(end) {
                self.anchor = start;
                self.cursor = end;
            }
        }
        changed
    }

    /// Inserts a newline, continuing a list item's marker on the new line,
    /// or ending the list when the item is empty.
    pub(super) fn newline(&mut self) {
        let start = self.line_start(self.cursor);
        let line = &self.text[start..self.cursor];
        let indent: String = line.chars().take_while(|c| *c == ' ').collect();
        let rest = &line[indent.len()..];
        let marker = ["- [ ] ", "- [x] ", "- ", "* ", "> "]
            .into_iter()
            .find(|marker| rest.starts_with(marker))
            .map(|marker| if marker == "- [x] " { "- [ ] " } else { marker });
        match marker {
            // An empty item ends the list.
            Some(marker) if rest.len() == marker.len() && self.selection().is_empty() => {
                self.anchor = start;
                self.replace(None, "", false, None);
            }
            Some(marker) => {
                self.replace(None, &format!("\n{indent}{marker}"), false, None);
            }
            None => {
                self.replace(None, &format!("\n{indent}"), false, None);
            }
        }
    }

    /// Applies `format` to the selection, or at the caret.
    pub(super) fn format(&mut self, format: Format) {
        match format {
            Format::Bold => self.surround("**", "**"),
            Format::Italic => self.surround("_", "_"),
            Format::Code => self.surround("`", "`"),
            Format::Link => self.link(),
            Format::Heading => self.prefix("### "),
            Format::Quote => self.prefix("> "),
            Format::Bullets => self.prefix("- "),
            Format::Tasks => self.prefix("- [ ] "),
        }
    }

    /// Wraps the selection in `open` and `close`, or unwraps it when it
    /// already is; at a bare caret, inserts both with the caret between.
    fn surround(&mut self, open: &str, close: &str) {
        let selection = self.selection();
        let before = selection.start.checked_sub(open.len());
        let wrapped = before.is_some_and(|before| {
            self.text.get(before..selection.start) == Some(open)
                && self.text.get(selection.end..selection.end + close.len()) == Some(close)
        });
        if let (true, Some(before)) = (wrapped, before) {
            let inner = self.text[selection.clone()].to_owned();
            self.anchor = before;
            self.cursor = selection.end + close.len();
            self.replace(None, &inner, false, None);
            self.anchor = before;
            self.cursor = before + inner.len();
            return;
        }
        let inner = self.text[selection.clone()].to_owned();
        self.replace(None, &format!("{open}{inner}{close}"), false, None);
        self.anchor = selection.start + open.len();
        self.cursor = self.anchor + inner.len();
    }

    /// `[selection](url)`, with `url` selected to type over.
    fn link(&mut self) {
        let selection = self.selection();
        let inner = self.text[selection.clone()].to_owned();
        self.replace(None, &format!("[{inner}](url)"), false, None);
        if inner.is_empty() {
            self.anchor = selection.start + 1;
            self.cursor = self.anchor;
        } else {
            self.anchor = selection.start + inner.len() + 3;
            self.cursor = self.anchor + "url".len();
        }
    }

    /// Adds `marker` to the start of every line the selection touches, or
    /// removes it when they all have it.
    fn prefix(&mut self, marker: &str) {
        let selection = self.selection();
        let start = self.line_start(selection.start);
        let end = self.line_end(selection.end);
        let lines: Vec<&str> = self.text[start..end].split('\n').collect();
        let all = lines.iter().all(|line| line.starts_with(marker));
        let changed: Vec<String> = lines
            .iter()
            .map(|line| match (all, line.strip_prefix(marker)) {
                (true, Some(rest)) => rest.to_owned(),
                _ => format!("{marker}{line}"),
            })
            .collect();
        let replaced = changed.join("\n");
        self.anchor = start;
        self.cursor = end;
        self.replace(None, &replaced, false, None);
        if selection.is_empty() {
            let moved = if all {
                selection.start.saturating_sub(marker.len()).max(start)
            } else {
                selection.start + marker.len()
            };
            self.anchor = moved;
            self.cursor = moved;
        } else {
            self.anchor = start;
            self.cursor = start + replaced.len();
        }
    }
}
