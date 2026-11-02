use super::*;

#[gpui::test]
fn enter_continues_and_ends_a_task_list(cx: &mut TestAppContext) {
    let mut doc = open("- [x] done", cx);
    doc.caret_at("done", 4);
    doc.keys("enter");
    doc.type_text("next");
    assert_eq!(doc.text(), "- [x] done\n- [ ] next");
    doc.keys("enter enter");
    assert_eq!(doc.text(), "- [x] done\n- [ ] next\n");
    doc.keys("shift-enter");
    assert_eq!(doc.text(), "- [x] done\n- [ ] next\n\n");
}

#[gpui::test]
fn tab_indents_and_alt_arrows_move_items(cx: &mut TestAppContext) {
    let mut doc = open("- a\n- b\n- c\n", cx);
    doc.caret_at("- b", 3);
    doc.keys("tab");
    assert_eq!(doc.text(), "- a\n  - b\n- c\n");
    doc.keys("shift-tab");
    assert_eq!(doc.text(), "- a\n- b\n- c\n");
    doc.keys("alt-up");
    assert_eq!(doc.text(), "- b\n- a\n- c\n");
    // The caret went with the item.
    assert_eq!(doc.selection(), Selection::caret(3));
    doc.keys("alt-down alt-down");
    assert_eq!(doc.text(), "- a\n- c\n- b\n");
}

#[gpui::test]
fn primary_enter_makes_and_checks_tasks(cx: &mut TestAppContext) {
    let mut doc = open("buy milk\n- eggs\n", cx);
    doc.caret_at("milk", 0);
    doc.keys(&primary("enter"));
    assert_eq!(doc.text(), "- [ ] buy milk\n- eggs\n");
    doc.keys(&primary("enter"));
    assert_eq!(doc.text(), "- [x] buy milk\n- eggs\n");
    doc.caret_at("eggs", 0);
    doc.keys(&primary("enter"));
    assert_eq!(doc.text(), "- [x] buy milk\n- [ ] eggs\n");
}

#[gpui::test]
fn clicking_a_box_toggles_its_task(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n", cx);
    doc.cx.update(|_, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.toggle_task(0, cx));
    });
    assert_eq!(doc.text(), "- [x] a\n");
}

#[gpui::test]
fn backspace_after_a_marker_removes_it(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n", cx);
    doc.caret_at("a", 0);
    doc.keys("backspace");
    assert_eq!(doc.text(), "a\n");
}

#[gpui::test]
fn a_comment_goes_to_the_agent_as_a_note(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] ship it\n", cx);
    doc.caret_at("ship", 0);
    doc.keys(&primary("shift-m"));
    doc.type_text("split into two");
    doc.keys("enter");
    let count = doc.cx.update(|_, cx| doc.editor.read(cx).comment_count());
    assert_eq!(count, 1);
    doc.cx.update(|_, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.send_comments(None, cx));
    });
    let requests = doc
        .cx
        .update(|_, cx| doc.editor.update(cx, |editor, _| editor.take_requests()));
    let [super::super::Request::Notes { pane: None, text }] = requests.as_slice() else {
        panic!("expected one note, got {requests:?}");
    };
    assert!(text.contains("On line 1:"), "{text}");
    assert!(text.contains("- [ ] ship it"), "{text}");
    assert!(text.contains("Comment: split into two"), "{text}");
}
