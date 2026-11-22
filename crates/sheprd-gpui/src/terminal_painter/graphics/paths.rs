//! Curved cell graphics cannot use font baselines: rounded box corners must
//! meet the same device-pixel stroke bands as the adjacent straight lines.
use super::*;
use gpui::Window;

pub(in super::super) enum CellPath {
    Separator(CellSeparator),
    Corner { right: bool, down: bool },
}

impl CellPath {
    pub(in super::super) fn from_symbol(symbol: &str) -> Option<Self> {
        match symbol {
            "╭" => Some(Self::Corner {
                right: true,
                down: true,
            }),
            "╮" => Some(Self::Corner {
                right: false,
                down: true,
            }),
            "╯" => Some(Self::Corner {
                right: false,
                down: false,
            }),
            "╰" => Some(Self::Corner {
                right: true,
                down: false,
            }),
            _ => CellSeparator::from_symbol(symbol).map(Self::Separator),
        }
    }

    pub(in super::super) fn path(
        self,
        near: Point<Pixels>,
        far: Point<Pixels>,
        window: &Window,
    ) -> gpui::Result<Path<Pixels>> {
        match self {
            Self::Separator(separator) => {
                separator.path(window.pixel_snap_point(near), window.pixel_snap_point(far))
            }
            Self::Corner { right, down } => rounded_path(
                Bounds::from_corners(near, far),
                window.scale_factor(),
                right,
                down,
            ),
        }
    }
}

fn rounded_path(
    cell: Bounds<Pixels>,
    scale: f32,
    right: bool,
    down: bool,
) -> gpui::Result<Path<Pixels>> {
    let x = f32::from(cell.left()) * scale;
    let y = f32::from(cell.top()) * scale;
    let w = f32::from(cell.size.width) * scale;
    let h = f32::from(cell.size.height) * scale;
    // Match Graphic::Lines, including the half-pixel center of odd strokes.
    let half = (w / 8.).round().max(1.) / 2.;
    let cx = (x + w / 2. - half).floor() + half;
    let cy = (y + h / 2. - half).floor() + half;
    let sx = if right { 1. } else { -1. };
    let sy = if down { 1. } else { -1. };
    let horizontal = if right {
        (x + w).round() - cx
    } else {
        cx - x.round()
    };
    let vertical = if down {
        (y + h).round() - cy
    } else {
        cy - y.round()
    };
    let radius = (horizontal.min(vertical) - half).max(0.);
    let outer = radius + half;
    let inner = (radius - half).max(0.);
    let p = |dx, dy| point(px((cx + sx * dx) / scale), px((cy + sy * dy) / scale));
    let k = 0.552_284_8;
    let mut path = PathBuilder::fill();
    path.move_to(p(horizontal, -half));
    path.line_to(p(radius, -half));
    path.cubic_bezier_to(
        p(-half, radius),
        p(radius - outer * k, -half),
        p(-half, radius - outer * k),
    );
    path.line_to(p(-half, vertical));
    path.line_to(p(half, vertical));
    path.line_to(p(half, radius));
    path.cubic_bezier_to(
        p(radius, half),
        p(half, radius - inner * k),
        p(radius - inner * k, half),
    );
    path.line_to(p(horizontal, half));
    path.close();
    path.build()
}

#[cfg(test)]
mod tests;
