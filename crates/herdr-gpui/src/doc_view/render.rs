//! Drawing a document: a header with the file's state and the view's
//! toggles, then only the rows in view, each its styled text with the
//! selection, caret, checkbox, and highlights painted around it.

use super::{
    buffer::State,
    editor::DocEditor,
    layout::{self, Row},
};
use crate::{actions, i18n::fl, input::ViewInputHandler, words};
use gpui::{prelude::*, *};
use herdr_doc::markdown::Kind;
use std::ops::Range;

/// The fold chevrons' column.
pub(super) const GUTTER: f32 = 22.;
/// Space around the text.
pub(super) const PAD: f32 = 16.;

/// What one row paints besides its text, in the line's own offsets.
struct Overlay {
    layout: TextLayout,
    selection: Option<Range<usize>>,
    /// The selection runs on past this line's end.
    selects_break: bool,
    caret: Option<usize>,
    highlights: Vec<(Range<usize>, f32)>,
    checkbox: Option<(Range<usize>, bool)>,
    marked: Option<Range<usize>>,
    colors: Colors,
}

#[derive(Clone, Copy)]
struct Colors {
    selection: Hsla,
    caret: Hsla,
    highlight: Hsla,
    box_border: Hsla,
    box_fill: Hsla,
    check: Hsla,
    underline: Hsla,
}

/// Rectangles covering `range` of a laid-out, possibly wrapped, line.
fn spans(layout: &TextLayout, range: Range<usize>, past_end: bool) -> Vec<Bounds<Pixels>> {
    let bounds = layout.bounds();
    let height = layout.line_height();
    let at = |index: usize| layout.position_for_index(index).unwrap_or(bounds.origin);
    let start = at(range.start);
    let mut end = at(range.end);
    if past_end {
        end.x += px(6.);
    }
    if start.y == end.y {
        return vec![Bounds::from_corners(
            start,
            point(end.x.max(start.x + px(1.)), start.y + height),
        )];
    }
    let mut out = vec![Bounds::from_corners(
        start,
        point(bounds.right(), start.y + height),
    )];
    let mut y = start.y + height;
    while y < end.y {
        out.push(Bounds::from_corners(
            point(bounds.left(), y),
            point(bounds.right(), y + height),
        ));
        y += height;
    }
    out.push(Bounds::from_corners(
        point(bounds.left(), end.y),
        point(end.x, end.y + height),
    ));
    out
}

fn paint_overlay(overlay: &Overlay, window: &mut Window) {
    let layout = &overlay.layout;
    let colors = overlay.colors;
    for (range, opacity) in &overlay.highlights {
        for span in spans(layout, range.clone(), false) {
            window.paint_quad(fill(span, colors.highlight.opacity(*opacity)));
        }
    }
    if let Some(range) = &overlay.selection {
        for span in spans(layout, range.clone(), overlay.selects_break) {
            window.paint_quad(fill(span, colors.selection));
        }
    }
    if let Some((range, done)) = &overlay.checkbox {
        let bounds = layout.bounds();
        let height = layout.line_height();
        let start = layout
            .position_for_index(range.start)
            .unwrap_or(bounds.origin);
        let end = layout.position_for_index(range.end).unwrap_or(start);
        let side = (height * 0.62).min((end.x - start.x).max(px(10.)));
        let center = point((start.x + end.x) / 2., start.y + height / 2.);
        let square = Bounds::centered_at(center, size(side, side));
        let fill_color = if *done {
            colors.box_fill
        } else {
            transparent_black()
        };
        window.paint_quad(quad(
            square,
            px(3.),
            fill_color,
            px(1.5),
            colors.box_border,
            BorderStyle::Solid,
        ));
        if *done {
            let mut path = PathBuilder::stroke(px(1.8));
            let s = f32::from(side);
            let o = square.origin;
            path.move_to(point(o.x + px(s * 0.24), o.y + px(s * 0.52)));
            path.line_to(point(o.x + px(s * 0.43), o.y + px(s * 0.72)));
            path.line_to(point(o.x + px(s * 0.78), o.y + px(s * 0.3)));
            if let Ok(path) = path.build() {
                window.paint_path(path, colors.check);
            }
        }
    }
    if let Some(range) = &overlay.marked {
        let height = layout.line_height();
        for span in spans(layout, range.clone(), false) {
            window.paint_quad(fill(
                Bounds::from_corners(
                    point(span.left(), span.top() + height - px(2.)),
                    point(span.right(), span.top() + height - px(1.)),
                ),
                colors.underline,
            ));
        }
    }
    if let Some(caret) = overlay.caret {
        let height = layout.line_height();
        let origin = layout
            .position_for_index(caret)
            .unwrap_or(layout.bounds().origin);
        window.paint_quad(fill(
            Bounds::new(origin, size(px(1.5), height)),
            colors.caret,
        ));
    }
}

impl DocEditor {
    fn colors(&self) -> Colors {
        let theme = &self.look.theme;
        Colors {
            selection: rgb(theme.active).into(),
            caret: rgb(theme.cursor).into(),
            highlight: rgb(theme.primary_wash()).into(),
            box_border: rgb(theme.muted).into(),
            box_fill: rgb(theme.primary()).into(),
            check: rgb(theme.text_on(theme.primary())).into(),
            underline: rgb(theme.foreground).into(),
        }
    }

    /// Row `ix` of the list: the shown line it stands for.
    fn render_row(
        &mut self,
        ix: usize,
        highlights: &[(Range<usize>, f32, Option<String>)],
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(&line_ix) = self.prepared.rows.get(ix) else {
            return div().into_any_element();
        };
        let Some(line) = self.prepared.lines.get(line_ix).cloned() else {
            return div().into_any_element();
        };
        if let Some(block) = self.prepared.picture_at(line_ix).cloned() {
            return self.render_picture(line_ix, &block, cx);
        }
        // The caret's lines show their Markdown as written; the others are
        // decorated.
        let selection = self.selection.range();
        let active =
            focused && line.range.start <= selection.end && selection.start <= line.range.end;
        let text = self.text(cx);
        let row: Row = layout::row(text, &line, &self.look, !active, self.code_spans(line_ix));
        let map = row.map.clone();
        let shown = |range: Range<usize>| map.to_display(range.start)..map.to_display(range.end);
        let theme = self.look.theme.clone();
        let local = |range: &Range<usize>| -> Option<Range<usize>> {
            let start = range.start.max(line.range.start);
            let end = range.end.min(line.range.end);
            (start < end
                || (start == end && range.start <= line.range.end && range.end > line.range.end))
                .then(|| start - line.range.start..end - line.range.start)
        };
        let selects_break = !self.selection.is_empty()
            && selection.start <= line.range.end
            && selection.end > line.range.end;
        let selected = (!self.selection.is_empty())
            .then(|| local(&selection))
            .flatten()
            .or_else(|| selects_break.then(|| line.range.len()..line.range.len()))
            .map(&shown);
        let caret = (focused && (line.range.start..=line.range.end).contains(&self.selection.head))
            .then(|| map.to_display(self.selection.head - line.range.start));
        let row_highlights: Vec<(Range<usize>, f32)> = highlights
            .iter()
            .filter_map(|(range, opacity, _)| local(range).map(|r| (shown(r), *opacity)))
            .collect();
        let label = highlights
            .iter()
            .find(|(range, _, _)| (line.range.start..=line.range.end).contains(&range.start))
            .map(|(_, opacity, by)| {
                let text = match by {
                    Some(by) => fl!("doc-view-changed-by", who = by.as_str()),
                    None => fl!("doc-view-changed-on-disk"),
                };
                (text, *opacity)
            });
        let styled = StyledText::new(row.text.clone()).with_runs(row.runs.clone());
        let text_layout = styled.layout().clone();
        self.layouts
            .borrow_mut()
            .insert(line_ix, (text_layout.clone(), map.clone()));
        let overlay = Overlay {
            layout: text_layout.clone(),
            selection: selected,
            selects_break,
            caret,
            highlights: row_highlights,
            checkbox: row.checkbox.clone(),
            marked: self.marked.as_ref().and_then(local).map(&shown),
            colors: self.colors(),
        };
        let fold = self.prepared.fold_at(line_ix).is_some();
        let folded = self.prepared.folded.get(&line_ix).copied();
        let count = self.prepared.counts.get(&line_ix).copied();
        let entity = cx.entity();
        let code = matches!(row.kind, Kind::Code | Kind::Fence);
        let checkbox = row.checkbox.clone();
        let links = row.links.clone();
        let start = line.range.start;
        let mouse_layout = text_layout.clone();
        let mouse_map = map.clone();
        div()
            .id(("doc-row", line_ix))
            .w_full()
            .flex()
            .flex_row()
            .items_start()
            .pr(px(PAD))
            .when(matches!(row.kind, Kind::Heading { .. }), |row| {
                row.pt(px(6.))
            })
            .child(
                div()
                    .id(("doc-fold", line_ix))
                    .flex_none()
                    .w(px(GUTTER))
                    .h(px(row.line_height))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(fold, |gutter| {
                        gutter
                            .cursor_pointer()
                            .child(
                                svg()
                                    .path(if folded.is_some() {
                                        "icons/chevron-right.svg"
                                    } else {
                                        "icons/chevron-down.svg"
                                    })
                                    .size(px(10.))
                                    .text_color(rgb(theme.muted)),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle_fold(line_ix, cx);
                                }),
                            )
                    }),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .when(code, |body| body.bg(rgb(theme.surface)).px(px(6.)))
                    .when(row.kind == Kind::Quote, |body| {
                        body.border_l_2().border_color(rgb(theme.muted)).pl(px(8.))
                    })
                    .text_size(px(row.size))
                    .line_height(px(row.line_height))
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |_, _, window, _| paint_overlay(&overlay, window),
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .child(styled)
                    .cursor(CursorStyle::IBeam)
                    .on_mouse_down(
                        MouseButton::Left,
                        move |event: &MouseDownEvent, window, cx| {
                            let index = mouse_layout
                                .index_for_position(event.position)
                                .unwrap_or_else(|index| index);
                            let source = mouse_map.to_source(index);
                            entity.update(cx, |this, cx| {
                                window.focus(&this.focus, cx);
                                cx.stop_propagation();
                                this.click(
                                    line_ix,
                                    start + source,
                                    index,
                                    source,
                                    event,
                                    checkbox.as_ref(),
                                    &links,
                                    cx,
                                );
                            });
                        },
                    ),
            )
            .when_some(folded, |row_el, hidden| {
                row_el.child(
                    div()
                        .id(("doc-folded", line_ix))
                        .flex_none()
                        .ml(px(8.))
                        .mt(px(2.))
                        .px(px(6.))
                        .rounded(px(4.))
                        .bg(rgb(theme.surface))
                        .text_color(rgb(theme.muted))
                        .text_size(px(self.look.ui.size * 0.85))
                        .cursor_pointer()
                        .child(fl!("doc-view-folded-lines", count = hidden))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_fold(line_ix, cx);
                            }),
                        ),
                )
            })
            .when_some(count, |row_el, (done, total)| {
                row_el.child(
                    div()
                        .flex_none()
                        .ml(px(8.))
                        .mt(px(4.))
                        .px(px(6.))
                        .rounded(px(8.))
                        .bg(rgb(if done == total {
                            theme.primary_wash()
                        } else {
                            theme.surface
                        }))
                        .text_color(rgb(theme.muted))
                        .text_size(px(self.look.ui.size * 0.8))
                        .child(fl!("doc-view-count", done = done, total = total)),
                )
            })
            .when_some(label, |row_el, (label, opacity)| {
                row_el.child(
                    div()
                        .flex_none()
                        .ml(px(8.))
                        .mt(px(4.))
                        .px(px(6.))
                        .rounded(px(8.))
                        .bg(rgb(theme.primary_wash()))
                        .text_color(rgb(theme.foreground))
                        .text_size(px(self.look.ui.size * 0.8))
                        .opacity(opacity)
                        .child(label),
                )
            })
            .into_any_element()
    }

    /// A click at byte `offset` of `line` (`index` in the line).
    #[allow(clippy::too_many_arguments)]
    fn click(
        &mut self,
        line: usize,
        offset: usize,
        index: usize,
        source: usize,
        event: &MouseDownEvent,
        checkbox: Option<&(Range<usize>, bool)>,
        links: &[(Range<usize>, String)],
        cx: &mut Context<Self>,
    ) {
        if let Some((range, _)) = checkbox
            && index >= range.start
            && index <= range.end
            && !event.modifiers.shift
        {
            self.toggle_task(line, cx);
            return;
        }
        let primary = if crate::controls::CTRL_SHORTCUTS {
            event.modifiers.control
        } else {
            event.modifiers.platform
        };
        if primary
            && let Some((_, url)) = links.iter().find(|(range, _)| range.contains(&source))
            && ["http://", "https://", "mailto:"]
                .iter()
                .any(|scheme| url.starts_with(scheme))
        {
            cx.open_url(url);
            return;
        }
        self.marked = None;
        let text = self.text(cx);
        let selection = match event.click_count {
            2 => {
                let start = words::previous_start(text, (offset + 1).min(text.len()));
                let end = words::next_end(text, offset);
                super::editor::Selection {
                    anchor: start.min(offset),
                    head: end.max(offset),
                }
            }
            n if n >= 3 => {
                let Some(range) = self.prepared.lines.get(line).map(|l| l.range.clone()) else {
                    return;
                };
                super::editor::Selection {
                    anchor: range.start,
                    head: range.end,
                }
            }
            _ if event.modifiers.shift => super::editor::Selection {
                anchor: self.selection.anchor,
                head: offset,
            },
            _ => super::editor::Selection::caret(offset),
        };
        self.goal_x = None;
        self.selecting = true;
        self.select(selection, cx);
    }

    fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.look.theme;
        let buffer = self.buffer.read(cx);
        let name = buffer
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let version = buffer.document().map(|doc| doc.version());
        let dirty = buffer.document().is_some_and(herdr_doc::Document::dirty);
        let banner = match buffer.state() {
            State::Loading => Some(fl!("doc-view-loading")),
            State::Failed(error) => Some(fl!(
                "doc-view-failed",
                error = crate::i18n::errors::Localize::localize(error)
            )),
            State::Ready if buffer.missing() => Some(fl!("doc-view-missing")),
            State::Ready => buffer.write_error().map(|error| {
                fl!(
                    "doc-view-write-failed",
                    error = crate::i18n::errors::Localize::localize(error)
                )
            }),
        };
        let button = |id: &'static str, label: String| {
            div()
                .id(id)
                .px(px(8.))
                .py(px(2.))
                .rounded(px(4.))
                .cursor_pointer()
                .text_color(rgb(theme.muted))
                .hover(|style| style.bg(rgb(theme.surface)))
                .child(label)
        };
        let hide_label = if self.hide_done {
            fl!("doc-view-show-done")
        } else {
            fl!("doc-view-hide-done")
        };
        div()
            .flex_none()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.))
                    .px(px(PAD))
                    .py(px(6.))
                    .border_b_1()
                    .border_color(rgb(theme.surface))
                    .text_size(px(self.look.ui.size * 0.9))
                    .child(div().text_color(rgb(theme.foreground)).child(name))
                    .when_some(version, |bar, version| {
                        bar.child(
                            div()
                                .text_color(rgb(theme.muted))
                                .child(fl!("doc-view-version", version = version.0)),
                        )
                    })
                    .when(dirty, |bar| {
                        bar.child(div().size(px(6.)).rounded_full().bg(rgb(theme.muted)))
                    })
                    .child(div().flex_1())
                    .child(
                        button("doc-hide-done", hide_label)
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_hide_done(cx))),
                    )
                    .child(
                        button("doc-fold-all", fl!("doc-view-fold-all"))
                            .on_click(cx.listener(|this, _, _, cx| this.fold_all(true, cx))),
                    )
                    .child(
                        button("doc-unfold-all", fl!("doc-view-unfold-all"))
                            .on_click(cx.listener(|this, _, _, cx| this.fold_all(false, cx))),
                    ),
            )
            .when_some(banner, |header, banner| {
                header.child(
                    div()
                        .px(px(PAD))
                        .py(px(6.))
                        .bg(rgb(theme.surface))
                        .text_color(rgb(theme.foreground))
                        .child(banner),
                )
            })
            .into_any_element()
    }
}

impl Focusable for DocEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for DocEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.prepare(cx);
        self.drive_visuals(window, cx);
        let highlights = self.live_highlights(cx);
        if !highlights.is_empty() {
            window.request_animation_frame();
        }
        let focused = self.focus.is_focused(window);
        self.layouts.borrow_mut().clear();
        let entity = cx.entity();
        let rows = entity.clone();
        let input = entity.clone();
        let focus = self.focus.clone();
        let header = self.header(cx);
        div()
            .id("doc-editor")
            .key_context("DocEditor")
            .track_focus(&self.focus)
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(rgb(self.look.theme.background))
            .text_color(rgb(self.look.theme.foreground))
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(|this, _: &actions::Cut, window, cx| {
                this.key_down(&actions::edit_key("x"), window, cx);
            }))
            .on_action(cx.listener(|this, _: &actions::Copy, window, cx| {
                this.key_down(&actions::edit_key("c"), window, cx);
            }))
            .on_action(cx.listener(|this, _: &actions::Paste, window, cx| {
                this.key_down(&actions::edit_key("v"), window, cx);
            }))
            .on_action(cx.listener(|this, _: &actions::SelectAll, window, cx| {
                this.key_down(&actions::edit_key("a"), window, cx);
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if !this.selecting || event.pressed_button != Some(MouseButton::Left) {
                    return;
                }
                if let Some(offset) = this.offset_at(event.position) {
                    let anchor = this.selection.anchor;
                    this.select(
                        super::editor::Selection {
                            anchor,
                            head: offset,
                        },
                        cx,
                    );
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.selecting = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.selecting = false),
            )
            .child(header)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .pt(px(8.))
                    .child(
                        list(self.list.clone(), move |ix, _, cx| {
                            rows.update(cx, |this, cx| {
                                this.render_row(ix, &highlights, focused, cx)
                            })
                        })
                        .size_full(),
                    )
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, cx| {
                                window.handle_input(
                                    &focus,
                                    ViewInputHandler::new(bounds, input.clone()),
                                    cx,
                                );
                            },
                        )
                        .absolute()
                        .inset_0(),
                    ),
            )
    }
}
