//! Task symbols painted over the existing source-text slot. Clicks continue
//! to use text indices, independently of the symbol's shape.

use gpui::{
    BorderStyle, Bounds, Hsla, PathBuilder, Pixels, Point, TextLayout, Window, point, px, quad,
    size, transparent_black,
};
use sheprd_doc::markdown::TaskState;
use std::ops::Range;

#[derive(Clone, Copy)]
pub(super) enum MarkerStyle {
    // keep-sorted start
    Checkbox,
    PlainTasks,
    // keep-sorted end
}

#[derive(Clone, Copy)]
pub(super) struct MarkerColors {
    pub(super) border: Hsla,
    pub(super) check: Hsla,
    pub(super) cancel: Hsla,
    pub(super) fill: Hsla,
}

/// Preserve the legacy square for an unwrapped marker. A wrapped end is
/// on another row, so reserve a minimum-width slot on the start row instead.
fn slot(start: Point<Pixels>, end: Point<Pixels>, height: Pixels, scale: f32) -> Bounds<Pixels> {
    let end = if start.y == end.y {
        end
    } else {
        point(start.x + px(10.), start.y)
    };
    let side = (height * scale).min((end.x - start.x).max(px(10.)));
    let center = point((start.x + end.x) / 2., start.y + height / 2.);
    Bounds::centered_at(center, size(side, side))
}

pub(super) fn paint(
    layout: &TextLayout,
    range: &Range<usize>,
    state: TaskState,
    style: MarkerStyle,
    colors: MarkerColors,
    window: &mut Window,
) {
    let bounds = layout.bounds();
    let height = layout.line_height();
    let start = layout
        .position_for_index(range.start)
        .unwrap_or(bounds.origin);
    let end = layout.position_for_index(range.end).unwrap_or(start);
    let scale = match style {
        MarkerStyle::Checkbox => 0.62,
        MarkerStyle::PlainTasks => 0.36,
    };
    let square = slot(start, end, height, scale);
    let side = square.size.width;
    match style {
        MarkerStyle::Checkbox => {
            let fill_color = if state.is_done() {
                colors.fill
            } else {
                transparent_black()
            };
            window.paint_quad(quad(
                square,
                px(3.),
                fill_color,
                px(1.5),
                colors.border,
                BorderStyle::Solid,
            ));
            if state.is_done() {
                let mut path = PathBuilder::stroke(px(1.8));
                let s = f32::from(side);
                let o = square.origin;
                path.move_to(point(o.x + px(s * 0.24), o.y + px(s * 0.52)));
                path.line_to(point(o.x + px(s * 0.43), o.y + px(s * 0.72)));
                path.line_to(point(o.x + px(s * 0.78), o.y + px(s * 0.3)));
                if let Ok(path) = path.build() {
                    window.paint_path(path, colors.check);
                }
            } else if state == TaskState::Cancelled {
                let mut path = PathBuilder::stroke(px(1.5));
                let s = f32::from(side);
                let o = square.origin;
                path.move_to(point(o.x + px(s * 0.28), o.y + px(s * 0.28)));
                path.line_to(point(o.x + px(s * 0.72), o.y + px(s * 0.72)));
                path.move_to(point(o.x + px(s * 0.72), o.y + px(s * 0.28)));
                path.line_to(point(o.x + px(s * 0.28), o.y + px(s * 0.72)));
                if let Ok(path) = path.build() {
                    window.paint_path(path, colors.border);
                }
            }
        }
        MarkerStyle::PlainTasks if state == TaskState::Open => {
            window.paint_quad(quad(
                square,
                px(0.),
                transparent_black(),
                px(1.),
                colors.border,
                BorderStyle::Solid,
            ));
        }
        MarkerStyle::PlainTasks if state == TaskState::Cancelled => {
            let mut path = PathBuilder::stroke(px(1.6));
            let s = f32::from(side);
            let o = square.origin;
            path.move_to(point(o.x + px(s * 0.15), o.y + px(s * 0.15)));
            path.line_to(point(o.x + px(s * 0.85), o.y + px(s * 0.85)));
            path.move_to(point(o.x + px(s * 0.85), o.y + px(s * 0.15)));
            path.line_to(point(o.x + px(s * 0.15), o.y + px(s * 0.85)));
            if let Ok(path) = path.build() {
                window.paint_path(path, colors.cancel);
            }
        }
        MarkerStyle::PlainTasks => {
            // Classic standalone heavy check, with no surrounding box.
            let mut path = PathBuilder::stroke(px(2.4));
            let s = f32::from(side);
            let o = square.origin;
            path.move_to(point(o.x + px(s * 0.12), o.y + px(s * 0.48)));
            path.line_to(point(o.x + px(s * 0.36), o.y + px(s * 0.78)));
            path.line_to(point(o.x + px(s * 0.9), o.y + px(s * 0.08)));
            if let Ok(path) = path.build() {
                window.paint_path(path, colors.check);
            }
        }
    }
}

#[cfg(test)]
mod tests;
