//! Extra selections and carets for a Markdown editor with multiple cursors.
//! Each extra keeps Loro anchors so other writers move it with the document.
//! Only typing, Backspace, and Delete apply edits at every cursor.

use super::{
    editor::{DocEditor, Selection},
    layout::DisplayMap,
};
use crate::{controls::CTRL_SHORTCUTS, words};
use gpui::{App, Context, KeyBinding, actions};
use sheprd_doc::{Anchor, Edit, markdown};
use std::ops::Range;

actions!(doc_view, [AddNextOccurrence]);

/// This scoped binding wins over the global split command while a document
/// editor is focused, which is intentional for Cmd-D and Ctrl-D.
pub(crate) fn key_bindings() -> [KeyBinding; 1] {
    let key = if CTRL_SHORTCUTS { "ctrl-d" } else { "cmd-d" };
    [KeyBinding::new(key, AddNextOccurrence, Some("DocEditor"))]
}

/// Extra selections, their Loro anchors, and the last occurrence selected.
#[derive(Default)]
pub(super) struct Cursors {
    pub(super) extra: Vec<Selection>,
    anchors: Vec<(Anchor, Anchor)>,
    /// Index of the last added extra cursor, used as the Cmd-D search start.
    last: Option<usize>,
}

/// The portions of one selection or caret that a row paints.
pub(super) struct RowParts {
    pub(super) selection: Option<Range<usize>>,
    pub(super) selects_break: bool,
    pub(super) caret: Option<usize>,
}

fn overlaps_or_contains_caret(a: &Range<usize>, b: &Range<usize>) -> bool {
    if a.is_empty() && b.is_empty() {
        return a.start == b.start;
    }
    if a.is_empty() {
        return b.start <= a.start && a.start <= b.end;
    }
    if b.is_empty() {
        return a.start <= b.start && b.start <= a.end;
    }
    a.start < b.end && b.start < a.end
}

fn union(a: &Range<usize>, b: &Range<usize>) -> Selection {
    Selection {
        anchor: a.start.min(b.start),
        head: a.end.max(b.end),
    }
}

fn contains_selection(container: &Range<usize>, contained: &Range<usize>) -> bool {
    container.start <= contained.start && contained.end <= container.end
}

impl DocEditor {
    pub(super) fn normalize_cursors(&mut self, len: usize, cx: &App) {
        self.normalize_cursors_to(len);
        self.remember_cursors(cx);
    }

    fn normalize_cursors_to(&mut self, len: usize) {
        let main = self.selection.range();
        let old = std::mem::take(&mut self.cursors.extra);
        let target = self
            .cursors
            .last
            .and_then(|index| old.get(index).copied())
            .map(|selection| Selection {
                anchor: selection.anchor.min(len),
                head: selection.head.min(len),
            });
        let mut extra: Vec<Selection> = old
            .into_iter()
            .map(|selection| Selection {
                anchor: selection.anchor.min(len),
                head: selection.head.min(len),
            })
            .collect();
        extra.sort_by_key(|selection| {
            let range = selection.range();
            (range.start, range.end)
        });
        let mut normalized: Vec<Selection> = Vec::with_capacity(extra.len());
        for selection in extra {
            let range = selection.range();
            if overlaps_or_contains_caret(&main, &range) {
                continue;
            }
            let merge = normalized
                .last()
                .is_some_and(|previous| overlaps_or_contains_caret(&previous.range(), &range));
            if merge {
                if let Some(previous) = normalized.pop() {
                    normalized.push(union(&previous.range(), &range));
                }
            } else {
                normalized.push(selection);
            }
        }
        self.cursors.extra = normalized;
        self.cursors.anchors.clear();
        self.cursors.last = target.and_then(|target| {
            self.cursors
                .extra
                .iter()
                .position(|selection| contains_selection(&selection.range(), &target.range()))
        });
    }

    pub(super) fn add_cursor(&mut self, selection: Selection, cx: &mut Context<Self>) {
        let len = self.text(cx).len();
        self.cursors.extra.push(Selection {
            anchor: selection.anchor.min(len),
            head: selection.head.min(len),
        });
        self.normalize_cursors_to(len);
        self.remember_cursors(cx);
    }

    pub(super) fn clear_cursors(&mut self) {
        self.cursors.extra.clear();
        self.cursors.anchors.clear();
        self.cursors.last = None;
    }

    pub(super) fn has_cursors(&self) -> bool {
        !self.cursors.extra.is_empty()
    }

    pub(super) fn map_cursors(&mut self, edits: &[Edit]) {
        let mut edits = edits.to_vec();
        edits.sort_by_key(|edit| edit.range.start);
        for selection in &mut self.cursors.extra {
            selection.anchor = Edit::map_offset(selection.anchor, &edits);
            selection.head = Edit::map_offset(selection.head, &edits);
        }
    }

    fn remember_cursors(&mut self, cx: &App) {
        let target = self
            .cursors
            .last
            .and_then(|index| self.cursors.extra.get(index).copied());
        let Some(doc) = self.buffer.read(cx).document() else {
            self.cursors.anchors.clear();
            self.cursors.last = None;
            return;
        };
        let mut extra = Vec::with_capacity(self.cursors.extra.len());
        let mut anchors = Vec::with_capacity(self.cursors.extra.len());
        for selection in self.cursors.extra.iter().copied() {
            let (Some(anchor), Some(head)) = (
                doc.anchor(selection.anchor, false),
                doc.anchor(selection.head, false),
            ) else {
                continue;
            };
            extra.push(selection);
            anchors.push((anchor, head));
        }
        self.cursors.extra = extra;
        self.cursors.anchors = anchors;
        self.cursors.last = target.and_then(|target| {
            self.cursors
                .extra
                .iter()
                .position(|selection| contains_selection(&selection.range(), &target.range()))
        });
    }

    pub(super) fn cursors_changed(&mut self, cx: &mut Context<Self>) {
        let last = self.cursors.last;
        let Some((resolved, len, last)) = self.buffer.read(cx).document().map(|doc| {
            let mut resolved: Vec<Selection> = Vec::with_capacity(self.cursors.extra.len());
            let mut resolved_last = None;
            for (index, (_, (anchor, head))) in self
                .cursors
                .extra
                .iter()
                .zip(&self.cursors.anchors)
                .enumerate()
            {
                let (Some(anchor), Some(head)) = (doc.resolve(anchor), doc.resolve(head)) else {
                    continue;
                };
                if last == Some(index) {
                    resolved_last = Some(resolved.len());
                }
                resolved.push(Selection { anchor, head });
            }
            (resolved, doc.text().len(), resolved_last)
        }) else {
            return;
        };
        self.cursors.extra = resolved;
        self.cursors.last = last;
        self.normalize_cursors_to(len);
        self.remember_cursors(cx);
    }

    pub(super) fn cursors_rebase(&mut self, cx: &mut Context<Self>) {
        let len = self.text(cx).len();
        self.normalize_cursors_to(len);
        self.remember_cursors(cx);
    }

    pub(super) fn drop_hidden_cursors(&mut self) {
        let has_anchors = self.cursors.extra.len() == self.cursors.anchors.len();
        let target = self
            .cursors
            .last
            .and_then(|index| self.cursors.extra.get(index).copied());
        let old_extra = std::mem::take(&mut self.cursors.extra);
        let old_anchors = std::mem::take(&mut self.cursors.anchors);
        let mut extra = Vec::with_capacity(old_extra.len());
        let mut anchors = Vec::with_capacity(old_anchors.len());
        for (index, selection) in old_extra.into_iter().enumerate() {
            let line = markdown::line_at(&self.prepared.lines, selection.head);
            if self.prepared.rows.binary_search(&line).is_err() {
                continue;
            }
            extra.push(selection);
            if has_anchors && let Some(anchor) = old_anchors.get(index) {
                anchors.push(anchor.clone());
            }
        }
        self.cursors.extra = extra;
        self.cursors.anchors = if has_anchors { anchors } else { Vec::new() };
        self.cursors.last = target.and_then(|target| {
            self.cursors
                .extra
                .iter()
                .position(|selection| contains_selection(&selection.range(), &target.range()))
        });
    }

    pub(super) fn insert_all(&mut self, text: &str, cx: &mut Context<Self>) {
        let extras = self.cursors.extra.clone();
        let mut selections = Vec::with_capacity(extras.len() + 1);
        selections.push(self.selection);
        selections.extend(extras.iter().copied());
        selections.sort_by_key(|selection| {
            let range = selection.range();
            (range.start, range.end)
        });
        let edits: Vec<Edit> = selections
            .iter()
            .map(|selection| Edit::new(selection.range(), text))
            .collect();
        let mapped_main = Edit::map_offset(self.selection.range().start, &edits);
        let original_extras = self.cursors.extra.clone();
        self.cursors.extra = extras
            .iter()
            .map(|selection| Selection::caret(selection.range().start))
            .collect();
        if !self.apply_selecting(edits, Selection::caret(mapped_main), cx) {
            self.cursors.extra = original_extras;
        }
    }

    pub(super) fn delete_all(&mut self, backward: bool, cx: &mut Context<Self>) {
        let extras = self.cursors.extra.clone();
        let mut selections = Vec::with_capacity(extras.len() + 1);
        selections.push(self.selection);
        selections.extend(extras.iter().copied());
        let mut ranges: Vec<Range<usize>> = Vec::new();
        for selection in &selections {
            let selection_range = selection.range();
            if !selection_range.is_empty() {
                ranges.push(selection_range);
                continue;
            }
            let head = selection.head;
            let range = self.with_head(head, |this| {
                let line = this.prepared.lines[this.caret_line()].clone();
                if backward && line.is_item() && head == line.marker.end && head > line.marker.start
                {
                    Some(line.marker)
                } else {
                    let other = if backward {
                        this.previous_boundary(cx)
                    } else {
                        this.next_boundary(cx)
                    };
                    (other != head).then(|| other.min(head)..other.max(head))
                }
            });
            if let Some(range) = range {
                ranges.push(range);
            }
        }
        ranges.sort_by_key(|range| (range.start, range.end));
        let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
        for range in ranges {
            if let Some(previous) = merged.last_mut()
                && range.start <= previous.end
            {
                previous.end = previous.end.max(range.end);
            } else {
                merged.push(range);
            }
        }
        if merged.is_empty() {
            return;
        }
        let edits: Vec<Edit> = merged
            .into_iter()
            .map(|range| Edit::new(range, ""))
            .collect();
        let mapped_main = Edit::map_offset(self.selection.range().start, &edits);
        let original_extras = self.cursors.extra.clone();
        self.cursors.extra = extras
            .iter()
            .map(|selection| Selection::caret(selection.range().start))
            .collect();
        if !self.apply_selecting(edits, Selection::caret(mapped_main), cx) {
            self.cursors.extra = original_extras;
        }
    }

    fn with_head<T>(&mut self, head: usize, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved = self.selection;
        self.selection = Selection::caret(head);
        let result = f(self);
        self.selection = saved;
        result
    }

    pub(super) fn add_next_occurrence(&mut self, cx: &mut Context<Self>) {
        self.prepare(cx);
        let text = self.text(cx).to_owned();
        if self.selection.is_empty() {
            let offset = self.selection.head;
            let start = words::previous_start(&text, (offset + 1).min(text.len()));
            let end = words::next_end(&text, offset);
            self.select(
                Selection {
                    anchor: start.min(offset),
                    head: end.max(offset),
                },
                cx,
            );
            return;
        }
        let needle = text[self.selection.range()].to_owned();
        if needle.is_empty() {
            return;
        }
        let occurrences: Vec<Range<usize>> = text
            .match_indices(needle.as_str())
            .map(|(start, found)| start..start + found.len())
            .collect();
        let start = self
            .cursors
            .last
            .and_then(|index| self.cursors.extra.get(index))
            .map_or(self.selection.range().end, |selection| {
                selection.range().end
            });
        let covered = |range: &Range<usize>| {
            self.prepared
                .rows
                .binary_search(&markdown::line_at(&self.prepared.lines, range.start))
                .is_err()
                || contains_selection(&self.selection.range(), range)
                || self
                    .cursors
                    .extra
                    .iter()
                    .any(|selection| contains_selection(&selection.range(), range))
        };
        let mut found = None;
        for range in occurrences.iter().filter(|range| range.start >= start) {
            if !covered(range) {
                found = Some(range.clone());
                break;
            }
        }
        if found.is_none() {
            for range in occurrences.iter().filter(|range| range.start < start) {
                if !covered(range) {
                    found = Some(range.clone());
                    break;
                }
            }
        }
        let Some(found) = found else {
            return;
        };
        let selection = Selection {
            anchor: found.start,
            head: found.end,
        };
        self.add_cursor(selection, cx);
        self.cursors.last = self
            .cursors
            .extra
            .iter()
            .position(|extra| contains_selection(&extra.range(), &found));
        self.prepare(cx);
        let line = markdown::line_at(&self.prepared.lines, found.start);
        let row = self.prepared.row_of(line);
        self.list.scroll_to_reveal_item(row);
        cx.notify();
    }

    #[cfg(test)]
    pub(super) fn extra_cursors(&self) -> Vec<Selection> {
        self.cursors.extra.clone()
    }
}

pub(super) fn row_parts(
    selection: Selection,
    line: Range<usize>,
    map: &DisplayMap,
    focused: bool,
) -> RowParts {
    let range = selection.range();
    let local = |range: &Range<usize>| -> Option<Range<usize>> {
        let start = range.start.max(line.start);
        let end = range.end.min(line.end);
        (start < end || (start == end && range.start <= line.end && range.end > line.end))
            .then(|| start - line.start..end - line.start)
    };
    let selects_break = !selection.is_empty() && range.start <= line.end && range.end > line.end;
    let selected = (!selection.is_empty())
        .then(|| local(&range))
        .flatten()
        .or_else(|| selects_break.then(|| line.len()..line.len()))
        .map(|range| map.to_display(range.start)..map.to_display(range.end));
    let caret = (focused && (line.start..=line.end).contains(&selection.head))
        .then(|| map.to_display(selection.head - line.start));
    RowParts {
        selection: selected,
        selects_break,
        caret,
    }
}
