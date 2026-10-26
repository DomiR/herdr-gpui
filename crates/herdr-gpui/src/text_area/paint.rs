//! Drawing the text area's text: wrapped to the field's width, scrolled to
//! keep the caret in view, with the selection behind it and the caret over
//! it. The field's height follows the rows the text took when last drawn,
//! so a line typed past the edge grows the field a frame later.

use super::{Layout, TextArea};
use crate::input::ViewInputHandler;
use gpui::{prelude::*, *};

impl TextArea {
    pub(super) fn canvas(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let area = cx.entity();
        let painter = area.clone();
        canvas(
            move |bounds, window, cx| {
                let area = area.read(cx);
                let line_height = area.line_height();
                let empty = area.edit.text.is_empty();
                let shown: SharedString = if empty {
                    area.placeholder.clone()
                } else {
                    area.edit.text.clone().into()
                };
                let base = TextRun {
                    len: shown.len(),
                    font: window.text_style().font(),
                    color: rgb(if empty {
                        area.theme.muted
                    } else {
                        area.theme.foreground
                    })
                    .into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                // Text an input method composes is underlined.
                let runs = match area.edit.marked.clone().filter(|_| !empty) {
                    Some(marked) => vec![
                        TextRun {
                            len: marked.start,
                            ..base.clone()
                        },
                        TextRun {
                            len: marked.len(),
                            underline: Some(UnderlineStyle {
                                color: Some(base.color),
                                thickness: px(1.),
                                wavy: false,
                            }),
                            ..base.clone()
                        },
                        TextRun {
                            len: shown.len() - marked.end,
                            ..base
                        },
                    ],
                    None => vec![base],
                };
                let mut starts = vec![0];
                starts.extend(shown.match_indices('\n').map(|(at, _)| at + 1));
                let lines = window
                    .text_system()
                    .shape_text(
                        shown,
                        px(area.font.size),
                        &runs,
                        Some(bounds.size.width.max(px(1.))),
                        None,
                    )
                    .map(|lines| lines.into_vec())
                    .unwrap_or_default();
                Layout {
                    lines,
                    starts,
                    line_height,
                }
            },
            move |bounds, layout, window, cx| {
                painter.update(cx, |area, cx| area.paint(bounds, layout, window, cx));
            },
        )
        .size_full()
    }

    fn paint(
        &mut self,
        bounds: Bounds<Pixels>,
        layout: Layout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let line_height = layout.line_height;
        let rows = layout.rows();
        if rows != self.rows {
            self.rows = rows;
            cx.notify();
        }
        let focused = self.focus.is_focused(window);
        let empty = self.edit.text.is_empty();
        let caret = if empty {
            Some(Point::default())
        } else {
            layout.position(self.edit.cursor)
        };
        // Keep the caret in view as it moves.
        if let Some(caret) = caret.filter(|_| focused) {
            if caret.y < self.scroll {
                self.scroll = caret.y;
            } else if caret.y + line_height > self.scroll + bounds.size.height {
                self.scroll = caret.y + line_height - bounds.size.height;
            }
        }
        let content = line_height * rows as f32;
        self.scroll = self
            .scroll
            .clamp(px(0.), (content - bounds.size.height).max(px(0.)));
        window.handle_input(&self.focus, ViewInputHandler::new(bounds, cx.entity()), cx);
        let origin = point(bounds.left(), bounds.top() - self.scroll);
        let width = bounds.size.width;
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            let selection = self.edit.selection();
            if focused && !empty && !selection.is_empty() {
                let color = rgb(self.theme.active);
                if let (Some(start), Some(end)) = (
                    layout.position(selection.start),
                    layout.position(selection.end),
                ) {
                    let row = |y: Pixels, from: Pixels, to: Pixels, window: &mut Window| {
                        window.paint_quad(fill(
                            Bounds::from_corners(
                                point(origin.x + from, origin.y + y),
                                point(origin.x + to, origin.y + y + line_height),
                            ),
                            color,
                        ));
                    };
                    if start.y == end.y {
                        row(start.y, start.x, end.x, window);
                    } else {
                        row(start.y, start.x, width, window);
                        let mut y = start.y + line_height;
                        while y < end.y {
                            row(y, px(0.), width, window);
                            y += line_height;
                        }
                        row(end.y, px(0.), end.x, window);
                    }
                }
            }
            let mut top = origin.y;
            for line in &layout.lines {
                let _ = line.paint(
                    point(origin.x, top),
                    line_height,
                    TextAlign::Left,
                    Some(bounds),
                    window,
                    cx,
                );
                top += line.size(line_height).height;
            }
            if let Some(caret) = caret.filter(|_| focused) {
                window.paint_quad(fill(
                    Bounds::new(
                        point(origin.x + caret.x, origin.y + caret.y),
                        size(px(1.), line_height),
                    ),
                    rgb(self.theme.cursor),
                ));
            }
        });
        self.bounds = Some(bounds);
        self.layout = (!empty).then_some(layout);
    }
}
