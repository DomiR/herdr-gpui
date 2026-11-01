//! The platform's text input for a document: typed text, compositions, and
//! where the input method's window goes. The platform speaks UTF-16 offsets
//! into the whole text; the editor keeps bytes.

use super::editor::{DocEditor, Selection};
use gpui::{
    Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window, point, px, size,
};
use herdr_doc::markdown;
use std::ops::Range;

fn to_utf16(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].encode_utf16().count()
}

fn from_utf16(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (at, ch) in text.char_indices() {
        if units >= offset {
            return at;
        }
        units += ch.len_utf16();
    }
    text.len()
}

fn bytes(text: &str, range: Range<usize>) -> Range<usize> {
    from_utf16(text, range.start.min(range.end))..from_utf16(text, range.start.max(range.end))
}

impl DocEditor {
    fn utf16(&self, range: Range<usize>, cx: &Context<Self>) -> Range<usize> {
        let text = self.text(cx);
        to_utf16(text, range.start)..to_utf16(text, range.end)
    }

    /// Where byte `offset` is drawn: its line's layout as last painted.
    pub(super) fn caret_bounds(&self, offset: usize) -> Option<Bounds<Pixels>> {
        let line = markdown::line_at(&self.prepared.lines, offset);
        let start = self.prepared.lines.get(line)?.range.start;
        let layouts = self.layouts.borrow();
        let layout = layouts.get(&line)?;
        let height = layout.line_height();
        let origin = layout
            .position_for_index(offset - start)
            .unwrap_or_else(|| layout.bounds().origin);
        Some(Bounds::new(origin, size(px(1.), height)))
    }

    /// The byte offset drawn at `position`, among the painted lines.
    pub(super) fn offset_at(&self, position: Point<Pixels>) -> Option<usize> {
        let layouts = self.layouts.borrow();
        let (line, layout) = layouts.iter().min_by_key(|(_, layout)| {
            let bounds = layout.bounds();
            if position.y < bounds.top() {
                f32::from(bounds.top() - position.y) as i64
            } else if position.y >= bounds.bottom() {
                f32::from(position.y - bounds.bottom()) as i64 + 1
            } else {
                0
            }
        })?;
        let bounds = layout.bounds();
        let y = position.y.clamp(bounds.top(), bounds.bottom() - px(1.));
        let index = layout
            .index_for_position(point(position.x, y))
            .unwrap_or_else(|index| index);
        let line = self.prepared.lines.get(*line)?;
        Some((line.range.start + index).min(line.range.end))
    }
}

impl EntityInputHandler for DocEditor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.text(cx);
        let range = bytes(text, range);
        *adjusted = Some(to_utf16(text, range.start)..to_utf16(text, range.end));
        Some(text[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.utf16(self.selection.range(), cx),
            reversed: self.selection.head < self.selection.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, cx: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.clone().map(|range| self.utf16(range, cx))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(range) = range {
            let range = bytes(self.text(cx), range);
            self.marked = None;
            self.selection = Selection {
                anchor: range.start,
                head: range.end,
            };
        }
        let text = text.replace("\r\n", "\n");
        self.insert(&text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = match range {
            Some(range) => bytes(self.text(cx), range),
            None => self
                .marked
                .clone()
                .unwrap_or_else(|| self.selection.range()),
        };
        self.marked = None;
        self.selection = Selection {
            anchor: range.start,
            head: range.end,
        };
        // Composition steps are not undo steps of their own.
        self.insert(text, cx);
        let start = range.start;
        if !text.is_empty() {
            self.marked = Some(start..start + text.len());
        }
        if let Some(selected) = selected {
            // Relative to the composed text.
            let caret = |offset: usize| start + from_utf16(text, offset);
            self.selection = Selection {
                anchor: caret(selected.start),
                head: caret(selected.end),
            };
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = bytes(self.text(cx), range);
        let start = self.caret_bounds(range.start)?;
        let end = self.caret_bounds(range.end).unwrap_or(start);
        if end.origin.y != start.origin.y {
            return Some(start);
        }
        Some(Bounds::from_corners(
            start.origin,
            point(end.origin.x.max(start.origin.x + px(1.)), start.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.offset_at(position)?;
        Some(to_utf16(self.text(cx), offset))
    }
}

#[cfg(test)]
pub(super) fn utf16_round_trip(text: &str, offset: usize) -> usize {
    from_utf16(text, to_utf16(text, offset))
}
