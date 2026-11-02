use super::*;

#[gpui::test]
fn folded_task_gets_a_sibling_after_its_children(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n  - [x] a1\n- [ ] b\n", cx);
    doc.cx.update(|_, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.toggle_fold(0, cx));
    });
    doc.caret_at("a", 0);
    doc.keys(&primary("enter"));
    assert_eq!(
        doc.selection(),
        Selection::caret("- [ ] a\n  - [x] a1\n- [ ] ".len())
    );
    doc.type_text("new");
    assert_eq!(doc.text(), "- [ ] a\n  - [x] a1\n- [ ] new\n- [ ] b\n");
    assert!(!doc.rows().contains(&1));
}

#[gpui::test]
fn folded_task_stays_before_a_trailing_blank(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n  - [ ] a1\n\n# Next\n", cx);
    doc.cx.update(|_, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.toggle_fold(0, cx));
    });
    doc.caret_at("a", 0);
    doc.keys(&primary("enter"));
    assert_eq!(doc.text(), "- [ ] a\n  - [ ] a1\n- [ ] \n\n# Next\n");
}

#[gpui::test]
fn folded_task_includes_a_fenced_child(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n  ```\n  x\n  ```\n- [ ] b\n", cx);
    doc.cx.update(|_, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.toggle_fold(0, cx));
    });
    let rows = doc.rows();
    let last_hidden = (1..4)
        .filter(|line| !rows.contains(line))
        .max()
        .expect("folded child");
    doc.caret_at("a", 0);
    doc.keys(&primary("enter"));
    let text = doc.text();
    let new_line = text
        .lines()
        .position(|line| line == "- [ ] ")
        .expect("new task");
    assert_eq!(new_line, last_hidden + 1);
}

#[gpui::test]
fn unfolded_task_still_toggles(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n  - [ ] a1\n", cx);
    doc.caret_at("a", 0);
    doc.keys(&primary("enter"));
    assert_eq!(doc.text(), "- [x] a\n  - [ ] a1\n");
}

#[gpui::test]
fn a_selection_on_a_folded_task_still_toggles(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n  - [ ] a1\n- [ ] b\n", cx);
    doc.cx.update(|_, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.toggle_fold(0, cx));
    });
    doc.caret_at("a", 0);
    doc.keys("home shift-end");
    assert!(!doc.selection().is_empty());
    doc.keys(&primary("enter"));
    assert_eq!(doc.text(), "- [x] a\n  - [ ] a1\n- [ ] b\n");
}
