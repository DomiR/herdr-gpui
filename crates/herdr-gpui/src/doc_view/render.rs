//! Drawing a document: a header with the file's state and the view's
//! toggles, then only the rows in view, each its styled text with the
//! selection, caret, checkbox, and highlights painted around it.

use super::{
    cursors::{self, RowParts},
    editor::DocEditor,
    fold_keys::FoldToLevel,
    layout::{self, Row},
    markers::{self, MarkerColors, MarkerStyle},
};
use crate::{actions, i18n::fl, input::ViewInputHandler, motion, words};
use gpui::{prelude::*, *};
use herdr_doc::markdown::{Kind, TaskState};
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
    extras: Vec<RowParts>,
    find_matches: Vec<(Range<usize>, bool)>,
    highlights: Vec<(Range<usize>, f32)>,
    checkbox: Option<(Range<usize>, TaskState)>,
    marked: Option<Range<usize>>,
    colors: Colors,
}

#[derive(Clone, Copy)]
struct Colors {
    selection: Hsla,
    caret: Hsla,
    find: Hsla,
    find_current: Hsla,
    highlight: Hsla,
    caret_row: Option<Hsla>,
    marker_style: MarkerStyle,
    markers: MarkerColors,
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

fn paint_overlay(overlay: &Overlay, bounds: Bounds<Pixels>, window: &mut Window) {
    let layout = &overlay.layout;
    let colors = overlay.colors;
    if let Some(color) = colors.caret_row
        && let Some(caret) = overlay.caret
        && let Some(origin) = layout.position_for_index(caret)
    {
        window.paint_quad(fill(
            Bounds::from_corners(
                point(bounds.left(), origin.y),
                point(bounds.right(), origin.y + layout.line_height()),
            ),
            color,
        ));
    }
    for (range, current) in &overlay.find_matches {
        let color = if *current {
            colors.find_current
        } else {
            colors.find
        };
        for span in spans(layout, range.clone(), false) {
            window.paint_quad(fill(span, color));
        }
    }
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
    for extra in &overlay.extras {
        if let Some(range) = &extra.selection {
            for span in spans(layout, range.clone(), extra.selects_break) {
                window.paint_quad(fill(span, colors.selection));
            }
        }
    }
    if let Some((range, state)) = &overlay.checkbox {
        markers::paint(
            layout,
            range,
            *state,
            colors.marker_style,
            colors.markers,
            window,
        );
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
    for extra in &overlay.extras {
        let Some(caret) = extra.caret else {
            continue;
        };
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
        if let Some(inks) = self.look.inks {
            return Colors {
                selection: inks.selection,
                caret: inks.caret,
                highlight: rgb(theme.primary_wash()).into(),
                caret_row: Some(inks.caret_row),
                marker_style: MarkerStyle::PlainTasks,
                markers: MarkerColors {
                    border: inks.open_marker,
                    check: inks.done_marker,
                    cancel: inks.cancelled_marker,
                    fill: transparent_black(),
                },
                underline: inks.caret,
            };
        }
        Colors {
            selection: rgb(theme.active).into(),
            caret: rgb(theme.cursor).into(),
            find: rgba((theme.palette[3] << 8) | 0x38).into(),
            find_current: rgba((theme.palette[3] << 8) | 0x98).into(),
            highlight: rgb(theme.primary_wash()).into(),
            caret_row: None,
            marker_style: MarkerStyle::Checkbox,
            markers: MarkerColors {
                border: rgb(theme.muted).into(),
                check: rgb(theme.text_on(theme.primary())).into(),
                cancel: rgb(theme.muted).into(),
                fill: rgb(theme.primary()).into(),
            },
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
        let files = |span: &str| self.links.get(span).cloned();
        let row: Row = layout::row(
            text,
            &line,
            &self.look,
            !active,
            self.code_spans(line_ix),
            &files,
        );
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
        let main_parts = cursors::row_parts(self.selection, line.range.clone(), &map, focused);
        let extras: Vec<RowParts> = self
            .cursors
            .extra
            .iter()
            .copied()
            .map(|selection| cursors::row_parts(selection, line.range.clone(), &map, focused))
            .collect();
        let find_matches = if self.find.shown.is_open() {
            let start = self
                .find
                .matches
                .partition_point(|range| range.end <= line.range.start);
            self.find.matches[start..]
                .iter()
                .enumerate()
                .take_while(|(_, range)| range.start < line.range.end)
                .filter_map(|(offset, range)| {
                    local(range)
                        .map(|range| (shown(range), self.find.current == Some(start + offset)))
                })
                .collect()
        } else {
            Vec::new()
        };
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
            selection: main_parts.selection,
            selects_break: main_parts.selects_break,
            caret: main_parts.caret,
            extras,
            find_matches,
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
        let cards = self.comment_cards(line_ix, cx);
        let mouse_layout = text_layout.clone();
        let mouse_map = map.clone();
        let drawn = div()
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
                        let color = self
                            .look
                            .inks
                            .map_or_else(|| rgb(theme.muted).into(), |inks| inks.separator);
                        body.border_l_2().border_color(color).pl(px(8.))
                    })
                    .text_size(px(row.size))
                    .line_height(px(row.line_height))
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| paint_overlay(&overlay, bounds, window),
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
            .into_any_element();
        if cards.is_empty() {
            return drawn;
        }
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(drawn)
            .children(cards)
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
        checkbox: Option<&(Range<usize>, TaskState)>,
        links: &[(Range<usize>, layout::Target)],
        cx: &mut Context<Self>,
    ) {
        if let Some((range, _)) = checkbox
            && index >= range.start
            && index <= range.end
            && !event.modifiers.shift
        {
            if event.modifiers.alt {
                self.toggle_cancel_task(line, cx);
            } else {
                self.toggle_task(line, cx);
            }
            return;
        }
        let offset = self.caret_snap(offset);
        let primary = if crate::controls::CTRL_SHORTCUTS {
            event.modifiers.control
        } else {
            event.modifiers.platform
        };
        if primary
            && let Some((_, target)) = links.iter().find(|(range, _)| range.contains(&source))
        {
            match target {
                layout::Target::Url(url) if crate::link::is_link(std::ffi::OsStr::new(url)) => {
                    self.request(super::Request::Link(url.clone()), cx);
                    return;
                }
                layout::Target::Url(url)
                    if ["http://", "https://", "mailto:"]
                        .iter()
                        .any(|scheme| url.starts_with(scheme)) =>
                {
                    cx.open_url(url);
                    return;
                }
                layout::Target::File(file) => {
                    self.request(
                        super::Request::Open(crate::editor::EditorTarget {
                            path: file.path.clone(),
                            line: file.line,
                        }),
                        cx,
                    );
                    return;
                }
                layout::Target::Url(_) => {}
            }
        }
        if primary && event.click_count == 1 && !event.modifiers.shift {
            self.add_cursor(super::editor::Selection::caret(offset), cx);
            self.selecting = false;
            cx.notify();
            return;
        }
        self.clear_cursors();
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
        let find_bar = self.render_find_bar(cx);
        if self.find.shown.moving(motion::now()) {
            window.request_animation_frame();
        }
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
            .on_action(cx.listener(|this, action: &FoldToLevel, _, cx| {
                this.fold_to_level(action.level, cx);
            }))
            .on_action(cx.listener(|this, _: &actions::SelectAll, window, cx| {
                this.key_down(&actions::edit_key("a"), window, cx);
            }))
            .on_action(cx.listener(|this, _: &cursors::AddNextOccurrence, _, cx| {
                this.add_next_occurrence(cx);
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
                    )
                    .when_some(find_bar, |body, find_bar| body.child(find_bar)),
            )
    }
}
