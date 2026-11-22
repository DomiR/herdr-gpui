use super::super::tests::rectangles;
use super::*;
use gpui::size;

#[test]
fn rounded_corners_meet_the_straight_stroke_bands() -> anyhow::Result<()> {
    for scale in [1., 1.25, 1.5, 2., 3.] {
        for (width, height) in [(8., 20.), (8.53, 20.7), (12.81, 30.5)] {
            for (x, y) in [(0., 0.), (3.27, 7.13), (-0.5, 1.5)] {
                let cell = Bounds::new(point(px(x), px(y)), size(px(width), px(height)));
                let horizontal = rectangles("─", cell, scale);
                let vertical = rectangles("│", cell, scale);
                for (symbol, right, down) in [
                    ("╭", true, true),
                    ("╮", false, true),
                    ("╯", false, false),
                    ("╰", true, false),
                ] {
                    let Some(CellPath::Corner {
                        right: actual_right,
                        down: actual_down,
                    }) = CellPath::from_symbol(symbol)
                    else {
                        panic!("missing rounded corner {symbol}");
                    };
                    assert_eq!((actual_right, actual_down), (right, down));
                    let path = rounded_path(cell, scale, actual_right, actual_down)?;
                    let device = |value: Pixels| f32::from(value) * scale;
                    let edge_x = (if right {
                        device(cell.right())
                    } else {
                        device(cell.left())
                    })
                    .round();
                    let edge_y = (if down {
                        device(cell.bottom())
                    } else {
                        device(cell.top())
                    })
                    .round();
                    let mut end_y = Vec::new();
                    let mut end_x = Vec::new();
                    for vertex in &path.vertices {
                        let p = vertex.xy_position;
                        if (device(p.x) - edge_x).abs() < 0.001 {
                            end_y.push(device(p.y));
                        }
                        if (device(p.y) - edge_y).abs() < 0.001 {
                            end_x.push(device(p.x));
                        }
                    }
                    for (ends, low, high) in [
                        (
                            end_y,
                            device(horizontal[0].top()),
                            device(horizontal[0].bottom()),
                        ),
                        (
                            end_x,
                            device(vertical[0].left()),
                            device(vertical[0].right()),
                        ),
                    ] {
                        assert!(
                            !ends.is_empty(),
                            "{symbol} misses cell edge at scale {scale}"
                        );
                        let min = ends.iter().copied().fold(f32::INFINITY, f32::min);
                        let max = ends.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                        assert!((min - low).abs() < 0.001, "{symbol}: {min} != {low}");
                        assert!((max - high).abs() < 0.001, "{symbol}: {max} != {high}");
                    }
                    assert!(!path.vertices.is_empty());
                }
            }
        }
    }
    for symbol in ["", "a", "╭\u{301}", "╮\u{fe0f}", "╭╮", "╱", "═", "┆"] {
        assert!(CellPath::from_symbol(symbol).is_none());
    }
    Ok(())
}
