use super::*;

#[gpui::test]
fn rounded_frame_uses_cell_paths_instead_of_font_glyphs(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| Empty);
    let painter = std::rc::Rc::new(std::cell::RefCell::new(TerminalPainter::default()));
    let frame = FrameData {
        width: 4,
        height: 3,
        cells: ["╭", "─", "─", "╮", "│", " ", " ", "│", "╰", "─", "─", "╯"]
            .into_iter()
            .map(cell)
            .collect(),
        cursor: None,
        hyperlinks: vec![],
        graphics: vec![],
    };
    let painted = painter.clone();
    cx.draw(Point::default(), size(px(200.), px(100.)), |_, _| {
        canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                let font = font("Menlo");
                let mut painter = painted.borrow_mut();
                let width = painter.cell_width(&font, window, cx);
                painter.paint_frame(
                    &frame,
                    bounds.origin,
                    None,
                    Ground::Opaque,
                    width,
                    &font,
                    &[],
                    &[],
                    None,
                    None,
                    window,
                    cx,
                );
            },
        )
        .size_full()
    });
    let painter = painter.borrow();
    assert_eq!(
        painter.glyphs.len(),
        0,
        "box corners must not depend on the font baseline"
    );
    assert_eq!(
        painter.painted_separators.len(),
        4,
        "all four corner paths reach the painter"
    );
}
