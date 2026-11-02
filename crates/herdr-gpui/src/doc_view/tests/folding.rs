use super::*;

const DOC: &str = "# One\n- [ ] a\n  - [x] a1\n- [x] b\n# Two\ntext\n";

#[gpui::test]
fn folding_a_heading_hides_its_section(cx: &mut TestAppContext) {
    let mut doc = open(DOC, cx);
    assert_eq!(doc.rows(), vec![0, 1, 2, 3, 4, 5, 6]);
    doc.caret_at("a1", 0);
    doc.keys(&primary("alt-["));
    // The innermost fold at the caret: item "a" hides "a1".
    assert_eq!(doc.rows(), vec![0, 1, 3, 4, 5, 6]);
    // The caret left the hidden line for the line folding it.
    assert_eq!(doc.selection(), Selection::caret("# One\n- [ ] a".len()));
    doc.keys(&primary("alt-shift-["));
    assert_eq!(doc.rows(), vec![0, 4, 6]);
    doc.keys(&primary("alt-shift-]"));
    assert_eq!(doc.rows(), vec![0, 1, 2, 3, 4, 5, 6]);
}

#[gpui::test]
fn a_fold_stays_with_its_heading_when_text_lands_above(cx: &mut TestAppContext) {
    let mut doc = open(DOC, cx);
    doc.cx.update(|_, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.toggle_fold(4, cx));
    });
    assert_eq!(doc.rows(), vec![0, 1, 2, 3, 4, 6]);
    std::fs::write(&doc.path, format!("intro\n{DOC}")).unwrap();
    doc.wait(600);
    // "# Two" is now line 5, still folded.
    assert_eq!(doc.rows(), vec![0, 1, 2, 3, 4, 5, 7]);
}

#[gpui::test]
fn hide_done_hides_done_tasks_with_their_children(cx: &mut TestAppContext) {
    let mut doc = open(DOC, cx);
    doc.keys(&primary("shift-h"));
    assert_eq!(doc.rows(), vec![0, 1, 4, 5, 6]);
    doc.keys(&primary("shift-h"));
    assert_eq!(doc.rows().len(), 7);
}

#[gpui::test]
fn a_diagram_shows_its_source_only_with_the_caret_in_it(cx: &mut TestAppContext) {
    let mut doc = open("top\n```mermaid\ngraph TD; A-->B\n```\nend\n", cx);
    // The caret starts on line 0: the diagram is a picture on its fence.
    assert_eq!(doc.rows(), vec![0, 1, 4, 5]);
    doc.caret_at("graph", 0);
    assert_eq!(doc.rows(), vec![0, 1, 2, 3, 4, 5]);
    doc.caret_at("end", 0);
    assert_eq!(doc.rows(), vec![0, 1, 4, 5]);
}
