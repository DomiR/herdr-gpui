use super::*;

fn toggle_fold(doc: &mut Fixture<'_>, line: usize) {
    doc.cx.update(|_, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.toggle_fold(line, cx));
    });
}

#[gpui::test]
fn alt_up_moves_an_item_to_the_previous_section(cx: &mut TestAppContext) {
    let mut doc = open("# Intro\nintro\n\n## Features\n- a\n  - child\n", cx);
    doc.caret_at("- a", 3);
    doc.keys("alt-up");
    let expected = "# Intro\nintro\n- a\n  - child\n\n## Features\n";
    assert_eq!(doc.text(), expected);
    assert_eq!(
        doc.selection(),
        Selection::caret(expected.find("- a").unwrap() + 3)
    );
}

#[gpui::test]
fn alt_down_moves_an_item_under_the_next_heading(cx: &mut TestAppContext) {
    let mut doc = open("# Intro\n- a\n  - child\n\n## Features\nnext\n", cx);
    doc.caret_at("- a", 3);
    doc.keys("alt-down");
    let expected = "# Intro\n\n## Features\n- a\n  - child\nnext\n";
    assert_eq!(doc.text(), expected);
    assert_eq!(
        doc.selection(),
        Selection::caret(expected.find("- a").unwrap() + 3)
    );
}

#[gpui::test]
fn a_folded_item_keeps_its_children_folded_when_moved(cx: &mut TestAppContext) {
    let mut doc = open("## A\n- a\n  - child\n\n## B\ntext\n", cx);
    toggle_fold(&mut doc, 1);
    assert_eq!(doc.rows(), vec![0, 1, 3, 4, 5, 6]);
    doc.caret_at("- a", 3);
    doc.keys("alt-down");
    assert_eq!(doc.text(), "## A\n\n## B\n- a\n  - child\ntext\n");
    assert_eq!(doc.rows(), vec![0, 1, 2, 3, 5, 6]);
}

#[gpui::test]
fn a_folded_item_moves_up_without_folding_its_next_sibling(cx: &mut TestAppContext) {
    let mut doc = open("## A\nx\n## B\n- a\n  - a1\n- b\n  - b1\n", cx);
    toggle_fold(&mut doc, 3);
    doc.caret_at("- a", 3);
    doc.keys("alt-up");
    assert_eq!(doc.text(), "## A\nx\n- a\n  - a1\n## B\n- b\n  - b1\n");
    assert_eq!(doc.rows(), vec![0, 1, 2, 4, 5, 6, 7]);
}

#[gpui::test]
fn moving_into_a_folded_section_unfolds_the_destination(cx: &mut TestAppContext) {
    let mut doc = open("## A\n- a\n\n## B\n- b\n  - child\n", cx);
    toggle_fold(&mut doc, 3);
    assert_eq!(doc.rows(), vec![0, 1, 2, 3, 6]);
    doc.caret_at("- a", 3);
    doc.keys("alt-down");
    assert_eq!(doc.text(), "## A\n\n## B\n- a\n- b\n  - child\n");
    assert_eq!(doc.rows(), vec![0, 1, 2, 3, 4, 5, 6]);
}
