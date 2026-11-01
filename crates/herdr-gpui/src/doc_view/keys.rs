//! The editor's keys: moving and selecting, deleting, and what Markdown
//! lists want from Enter, Tab, and Alt-arrows. Printable text is left to the
//! platform's input handler, so compositions and dead keys work.

use super::editor::{DocEditor, Selection};
use crate::{actions, controls::CTRL_SHORTCUTS, words};
use gpui::{ClipboardItem, Context, KeyDownEvent, Modifiers, Window, point};
use herdr_doc::{
    Edit,
    markdown::{self, Kind},
    todo,
};
use unicode_segmentation::UnicodeSegmentation;

/// Cmd on macOS, Ctrl elsewhere.
fn primary(modifiers: Modifiers) -> bool {
    if CTRL_SHORTCUTS {
        modifiers.control && !modifiers.platform
    } else {
        modifiers.platform && !modifiers.control
    }
}

impl DocEditor {
    pub(super) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The input method owns keys while it composes.
        if self.marked.is_some() {
            return;
        }
        let m = event.keystroke.modifiers;
        if primary(m) && m.shift && event.keystroke.key.eq_ignore_ascii_case("m") {
            self.start_comment(window, cx);
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        if self.handle_key(event, cx) {
            cx.stop_propagation();
            window.prevent_default();
            cx.notify();
        }
    }

    /// Runs `event`'s key; whether it was the editor's.
    pub(super) fn handle_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        self.prepare(cx);
        let key = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
        let shift = m.shift;
        if primary(m) {
            return self.primary_key(key, m, cx);
        }
        if actions::word_modifiers(m) && matches!(key, "left" | "right" | "backspace" | "delete") {
            self.word_key(key, shift, cx);
            return true;
        }
        if m.alt && !m.control && !m.platform && matches!(key, "up" | "down") {
            self.move_item(key == "up", cx);
            return true;
        }
        if m.control || m.alt || m.platform || m.function {
            return false;
        }
        match key {
            "left" | "right" => {
                let range = self.selection.range();
                let at = match (key, shift, self.selection.is_empty()) {
                    ("left", false, false) => range.start,
                    ("right", false, false) => range.end,
                    ("left", _, _) => self.previous_boundary(cx),
                    _ => self.next_boundary(cx),
                };
                self.move_to(at, shift, cx);
            }
            "up" | "down" => self.vertical(key == "down", shift, cx),
            "home" | "end" => {
                let line = &self.prepared.lines[self.caret_line()];
                let at = if key == "home" {
                    line.range.start
                } else {
                    line.range.end
                };
                self.move_to(at, shift, cx);
            }
            "pageup" | "pagedown" => {
                for _ in 0..20 {
                    self.vertical(key == "pagedown", shift, cx);
                }
            }
            "backspace" | "delete" => self.delete(key == "backspace", cx),
            "enter" if shift => self.insert("\n", cx),
            "enter" => self.enter(cx),
            "tab" => self.indent(shift, cx),
            "escape" if !self.selection.is_empty() => {
                let head = self.selection.head;
                self.select(Selection::caret(head), cx);
            }
            _ => return false,
        }
        true
    }

    fn primary_key(&mut self, key: &str, m: Modifiers, cx: &mut Context<Self>) -> bool {
        let shift = m.shift;
        match (key, m.alt) {
            ("z", false) => self.undo(shift, cx),
            ("y", false) if CTRL_SHORTCUTS => self.undo(true, cx),
            ("a", false) => {
                let len = self.text(cx).len();
                self.select(
                    Selection {
                        anchor: 0,
                        head: len,
                    },
                    cx,
                );
            }
            ("c" | "x", false) => self.copy(key == "x", cx),
            ("v", false) => self.paste(cx),
            ("s", false) => self.buffer.update(cx, |buffer, cx| buffer.save(cx)),
            ("enter", false) => self.cycle_task(cx),
            ("left", false) | ("right", false) => {
                let line = &self.prepared.lines[self.caret_line()];
                let at = if key == "left" {
                    line.range.start
                } else {
                    line.range.end
                };
                self.move_to(at, shift, cx);
            }
            ("up", false) => self.move_to(0, shift, cx),
            ("down", false) => {
                let len = self.text(cx).len();
                self.move_to(len, shift, cx);
            }
            ("backspace", false) => {
                let start = self.prepared.lines[self.caret_line()].range.start;
                if self.selection.is_empty() {
                    self.selection.anchor = start;
                }
                self.insert("", cx);
            }
            ("h", false) if shift => self.toggle_hide_done(cx),
            ("[" | "{", true) if shift => self.fold_all(true, cx),
            ("]" | "}", true) if shift => self.fold_all(false, cx),
            ("[", true) => self.fold_at_caret(true, cx),
            ("]", true) => self.fold_at_caret(false, cx),
            _ => return false,
        }
        true
    }

    fn word_key(&mut self, key: &str, shift: bool, cx: &mut Context<Self>) {
        let text = self.text(cx);
        let head = self.selection.head;
        let at = match key {
            "left" | "backspace" => words::previous_start(text, head),
            _ => words::next_end(text, head),
        };
        match key {
            "left" | "right" => self.move_to(at, shift, cx),
            _ => {
                if self.selection.is_empty() {
                    self.selection.anchor = at;
                }
                self.insert("", cx);
            }
        }
    }

    /// Moves the caret to `at`, extending the selection with Shift.
    pub(super) fn move_to(&mut self, at: usize, extend: bool, cx: &mut Context<Self>) {
        let anchor = if extend { self.selection.anchor } else { at };
        self.goal_x = None;
        self.select(Selection { anchor, head: at }, cx);
        self.reveal(cx);
    }

    /// Grapheme boundaries of the caret's line, so arrows never split an
    /// accented letter or an emoji sequence.
    fn boundaries(&self, cx: &Context<Self>) -> Vec<usize> {
        let line = &self.prepared.lines[self.caret_line()];
        let text = self.text(cx);
        let start = line.range.start;
        let mut out: Vec<usize> = text[line.range.clone()]
            .grapheme_indices(true)
            .map(|(at, _)| start + at)
            .collect();
        out.push(line.range.end);
        out
    }

    fn previous_boundary(&self, cx: &Context<Self>) -> usize {
        let head = self.selection.head;
        let line = &self.prepared.lines[self.caret_line()];
        if head == line.range.start {
            // Back over the previous line break, to the shown line before.
            return self
                .shown_neighbour(false)
                .map_or(head, |l| self.prepared.lines[l].range.end);
        }
        self.boundaries(cx)
            .into_iter()
            .rev()
            .find(|at| *at < head)
            .unwrap_or(line.range.start)
    }

    fn next_boundary(&self, cx: &Context<Self>) -> usize {
        let head = self.selection.head;
        let line = &self.prepared.lines[self.caret_line()];
        if head >= line.range.end {
            return self
                .shown_neighbour(true)
                .map_or(head, |l| self.prepared.lines[l].range.start);
        }
        self.boundaries(cx)
            .into_iter()
            .find(|at| *at > head)
            .unwrap_or(line.range.end)
    }

    /// The shown line after (or before) the caret's.
    fn shown_neighbour(&self, down: bool) -> Option<usize> {
        let row = self.prepared.row_of(self.caret_line());
        let row = if down { row + 1 } else { row.checked_sub(1)? };
        self.prepared.rows.get(row).copied()
    }

    /// Up or Down, by drawn line: within a wrapped line where it wraps,
    /// then to the shown line above or below, keeping the column it aimed at.
    fn vertical(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        let line = self.caret_line();
        let start = self.prepared.lines[line].range.start;
        let target = {
            let layouts = self.layouts.borrow();
            layouts.get(&line).and_then(|(layout, map)| {
                let pos = layout.position_for_index(map.to_display(self.selection.head - start))?;
                let x = self.goal_x.unwrap_or(pos.x);
                let height = layout.line_height();
                let y = if down {
                    pos.y + height * 1.5
                } else {
                    pos.y - height * 0.5
                };
                let bounds = layout.bounds();
                if y >= bounds.top() && y < bounds.bottom() {
                    let index = layout.index_for_position(point(x, y)).unwrap_or_else(|i| i);
                    return Some((start + map.to_source(index), x));
                }
                let next = self.shown_neighbour(down)?;
                let (other, other_map) = layouts.get(&next)?;
                let bounds = other.bounds();
                let half = other.line_height() * 0.5;
                let y = if down {
                    bounds.top() + half
                } else {
                    bounds.bottom() - half
                };
                let index = other.index_for_position(point(x, y)).unwrap_or_else(|i| i);
                Some((
                    self.prepared.lines[next].range.start + other_map.to_source(index),
                    x,
                ))
            })
        };
        let (at, x) = match target {
            Some(found) => (found.0, Some(found.1)),
            None => (self.by_column(down, cx), self.goal_x),
        };
        let anchor = if extend { self.selection.anchor } else { at };
        self.select(Selection { anchor, head: at }, cx);
        self.goal_x = x;
        self.reveal(cx);
    }

    /// Up or Down without a laid-out line: the same column of the shown
    /// line above or below, or the text's end.
    fn by_column(&self, down: bool, cx: &Context<Self>) -> usize {
        let line = &self.prepared.lines[self.caret_line()];
        let text = self.text(cx);
        let column = text[line.range.start..self.selection.head].chars().count();
        let Some(next) = self.shown_neighbour(down) else {
            return if down {
                line.range.end
            } else {
                line.range.start
            };
        };
        let next = &self.prepared.lines[next];
        text[next.range.clone()]
            .char_indices()
            .nth(column)
            .map_or(next.range.end, |(at, _)| next.range.start + at)
    }

    fn delete(&mut self, backward: bool, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            let head = self.selection.head;
            let line = self.prepared.lines[self.caret_line()].clone();
            // Backspace right after a list marker removes the marker.
            if backward && line.is_item() && head == line.marker.end && head > line.marker.start {
                self.apply(
                    vec![Edit::new(line.marker.clone(), "")],
                    line.marker.start,
                    cx,
                );
                return;
            }
            let other = if backward {
                self.previous_boundary(cx)
            } else {
                self.next_boundary(cx)
            };
            if other == head {
                return;
            }
            self.selection.anchor = other;
        }
        self.insert("", cx);
    }

    /// Enter: continues a list item or ends the list; a plain line break
    /// elsewhere.
    fn enter(&mut self, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            let text = self.text(cx);
            if let Some((edit, caret)) = todo::enter(text, self.selection.head) {
                self.apply(vec![edit], caret, cx);
                return;
            }
        }
        self.insert("\n", cx);
    }

    /// Tab and Shift-Tab: indents or outdents the list items selected, with
    /// their sub-items; Tab elsewhere types a tab.
    fn indent(&mut self, outdent: bool, cx: &mut Context<Self>) {
        let range = self.selection.range();
        let lines = &self.prepared.lines;
        let first = markdown::line_at(lines, range.start);
        let last = markdown::line_at(lines, range.end);
        let text = self.text(cx);
        let edits = todo::indent(text, first..last + 1, outdent);
        if edits.is_empty() {
            if !outdent {
                self.insert("\t", cx);
            }
            return;
        }
        let map = |at: usize| Edit::map_offset(at, &edits);
        let selection = Selection {
            anchor: map(self.selection.anchor),
            head: map(self.selection.head),
        };
        self.apply_selecting(edits, selection, cx);
    }

    /// Alt-Up and Alt-Down: moves the caret's item with its sub-items past
    /// its sibling, the caret going along.
    fn move_item(&mut self, up: bool, cx: &mut Context<Self>) {
        let line = self.caret_line();
        let text = self.text(cx);
        let Some((edit, moved_to)) = todo::move_item(text, line, up) else {
            return;
        };
        let column = self.selection.head - self.prepared.lines[line].range.start;
        let after = Edit::apply_all(text, std::slice::from_ref(&edit));
        let start = markdown::lines(&after)
            .get(moved_to)
            .map_or(0, |l| l.range.start);
        self.apply(vec![edit], start + column, cx);
    }

    /// Cmd-Enter on each selected line: a task is checked or unchecked, a
    /// list item becomes a task, and other text becomes a task item.
    fn cycle_task(&mut self, cx: &mut Context<Self>) {
        let range = self.selection.range();
        let lines = &self.prepared.lines;
        let first = markdown::line_at(lines, range.start);
        let last = markdown::line_at(lines, range.end);
        let mut edits = Vec::new();
        for line in &lines[first..=last] {
            match line.kind {
                Kind::Item {
                    task: Some(done), ..
                } => {
                    if let Some(checkbox) = line.checkbox.clone() {
                        edits.push(Edit::new(checkbox, if done { "[ ]" } else { "[x]" }));
                    }
                }
                Kind::Item { task: None, .. } => {
                    edits.push(Edit::insert(line.marker.end, "[ ] "));
                }
                Kind::Paragraph => {
                    edits.push(Edit::insert(line.range.start + line.indent, "- [ ] "));
                }
                Kind::Blank if first == last => {
                    edits.push(Edit::insert(line.range.start, "- [ ] "));
                }
                _ => {}
            }
        }
        let map = |at: usize| Edit::map_offset(at, &edits);
        let selection = Selection {
            anchor: map(self.selection.anchor),
            head: map(self.selection.head),
        };
        self.apply_selecting(edits, selection, cx);
    }

    fn copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        let range = self.selection.range();
        if range.is_empty() {
            return;
        }
        let text = self.text(cx)[range].to_owned();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        if cut {
            self.insert("", cx);
        }
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        // One undo step for the whole paste.
        self.buffer.update(cx, |buffer, _| buffer.break_undo());
        self.insert(&text.replace("\r\n", "\n"), cx);
        self.buffer.update(cx, |buffer, _| buffer.break_undo());
    }
}
