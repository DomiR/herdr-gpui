use super::*;

#[gpui::test]
fn arrows_treat_a_task_marker_as_one_character(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n", cx);
    doc.caret_at("a", 0);
    doc.keys("left");
    assert_eq!(doc.selection(), Selection::caret(0));
    doc.keys("right");
    assert_eq!(doc.selection(), Selection::caret("- [ ] ".len()));
    doc.keys("shift-left");
    assert_eq!(doc.selection().range(), 0.."- [ ] ".len());
}

#[gpui::test]
fn delete_before_a_task_or_list_marker_removes_it(cx: &mut TestAppContext) {
    let mut task = open("- [ ] a\n", cx);
    task.caret_at("- [ ] a", 0);
    task.keys("delete");
    assert_eq!(task.text(), "a\n");

    let mut item = open("- a\n", cx);
    item.caret_at("- a", 0);
    item.keys("delete");
    assert_eq!(item.text(), "a\n");
}

#[gpui::test]
fn deletion_inside_a_task_marker_removes_the_whole_marker(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n", cx);
    doc.cx.update(|_, cx| {
        doc.editor.update(cx, |editor, cx| {
            editor.select(Selection { anchor: 2, head: 2 }, cx);
        });
    });
    doc.keys("backspace");
    assert_eq!(doc.text(), "a\n");
}

#[gpui::test]
fn plain_list_arrows_still_move_by_character(cx: &mut TestAppContext) {
    let mut doc = open("- a\n", cx);
    doc.caret_at("a", 0);
    doc.keys("left");
    assert_eq!(doc.selection(), Selection::caret(1));
}

#[gpui::test]
fn vertical_movement_and_typing_snap_past_a_task_marker(cx: &mut TestAppContext) {
    let mut doc = open("abc\n- [ ] a\n", cx);
    doc.caret_at("abc", 3);
    doc.keys("down");
    let marker_end = "abc\n- [ ] ".len();
    assert_eq!(doc.selection(), Selection::caret(marker_end));
    doc.type_text("z");
    assert_eq!(doc.text(), "abc\n- [ ] za\n");
}

#[gpui::test]
fn typing_inside_a_task_marker_starts_after_it(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n", cx);
    doc.cx.update(|_, cx| {
        doc.editor.update(cx, |editor, cx| {
            editor.select(Selection { anchor: 2, head: 2 }, cx);
        });
    });
    doc.type_text("z");
    assert_eq!(doc.text(), "- [ ] za\n");
}

#[gpui::test]
fn word_movement_stays_outside_a_task_marker(cx: &mut TestAppContext) {
    let mut doc = open("- [x] first word\n", cx);
    doc.caret_at("first", 2);
    doc.keys("alt-left");
    assert_eq!(doc.selection(), Selection::caret("- [x] ".len()));
    doc.keys("alt-right");
    assert_eq!(doc.selection(), Selection::caret("- [x] first".len()));

    let mut from_start = open("- [x] first\n", cx);
    from_start.caret_at("- [x] first", 0);
    from_start.keys("alt-right");
    assert_eq!(from_start.selection(), Selection::caret("- [x] ".len()));

    let mut deleted = open("- [x] first\n", cx);
    deleted.caret_at("first", 0);
    deleted.keys("alt-backspace");
    assert_eq!(deleted.text(), "first\n");
}
