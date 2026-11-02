use super::*;

#[gpui::test]
fn home_and_typing_snap_to_a_task_content_start(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] buy", cx);
    doc.caret_at("buy", 0);
    doc.keys("home");
    assert_eq!(doc.selection(), Selection::caret(6));
    doc.type_text("x");
    assert_eq!(doc.text(), "- [ ] xbuy");
}

#[gpui::test]
fn horizontal_moves_cross_a_task_prefix_atomically(cx: &mut TestAppContext) {
    let mut doc = open("a\n- [ ] b", cx);
    doc.caret_at("b", 0);
    doc.keys("left");
    assert_eq!(doc.selection(), Selection::caret(1));
    doc.keys("right");
    assert_eq!(doc.selection(), Selection::caret(2 + 6));

    let mut doc = open("a\n- [ ] b", cx);
    doc.caret_at("a", 1);
    doc.keys("right");
    assert_eq!(doc.selection(), Selection::caret(2 + 6));
}

#[gpui::test]
fn backspace_at_task_content_start_removes_its_marker(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] b", cx);
    doc.caret_at("b", 0);
    doc.keys("backspace");
    assert_eq!(doc.text(), "b");
}

#[gpui::test]
fn indentation_before_a_task_stays_reachable(cx: &mut TestAppContext) {
    let mut doc = open("  - [ ] b", cx);
    doc.caret_at("b", 0);
    doc.keys("left");
    assert_eq!(doc.selection(), Selection::caret(1));
    doc.keys("right");
    assert_eq!(doc.selection(), Selection::caret(8));

    let mut doc = open("  - [ ] b", cx);
    doc.caret_at("  - [ ] b", 1);
    doc.keys("delete");
    assert_eq!(doc.text(), " b");
}

#[gpui::test]
fn plain_bullets_keep_raw_home_and_shift_home_keeps_the_anchor(cx: &mut TestAppContext) {
    let mut doc = open("- a", cx);
    doc.caret_at("a", 0);
    doc.keys("home");
    assert_eq!(doc.selection(), Selection::caret(0));

    let mut doc = open("- [ ] buy", cx);
    doc.caret_at("buy", 1);
    doc.keys("shift-home");
    assert_eq!(doc.selection(), Selection { anchor: 7, head: 6 });
}

fn word(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("alt-{key}")
    } else {
        format!("ctrl-{key}")
    }
}

#[gpui::test]
fn word_left_from_task_content_start_crosses_to_the_previous_line(cx: &mut TestAppContext) {
    let mut doc = open("a\n- [x] done", cx);
    doc.caret_at("done", 0);
    doc.keys(&word("left"));
    assert_eq!(doc.selection(), Selection::caret(0));
}

#[gpui::test]
fn word_backspace_at_task_content_start_removes_the_marker(cx: &mut TestAppContext) {
    let mut doc = open("- [x] done", cx);
    doc.caret_at("done", 0);
    doc.keys(&word("backspace"));
    assert_eq!(doc.text(), "done");
}

#[gpui::test]
fn word_right_from_task_indent_crosses_to_content_end(cx: &mut TestAppContext) {
    let mut doc = open("  - [x] b", cx);
    doc.caret_at("  - [x] b", 1);
    doc.keys(&word("right"));
    assert_eq!(doc.selection(), Selection::caret(9));
}

#[gpui::test]
fn word_forward_from_the_previous_line_preserves_a_task(cx: &mut TestAppContext) {
    let mut doc = open("a\n- [x] b", cx);
    doc.caret_at("a", 1);
    doc.keys(&word("right"));
    assert_eq!(doc.selection(), Selection::caret(9));

    let mut doc = open("a\n- [x] b", cx);
    doc.caret_at("a", 1);
    doc.keys(&word("delete"));
    assert_eq!(doc.text(), "a");
}

#[gpui::test]
fn making_a_task_snaps_the_inserted_caret_to_content(cx: &mut TestAppContext) {
    let mut doc = open("buy", cx);
    doc.caret_at("buy", 0);
    doc.keys(&primary("enter"));
    assert_eq!(doc.text(), "- [ ] buy");
    assert_eq!(doc.selection(), Selection::caret(6));
}

#[gpui::test]
fn undo_and_redo_keep_a_task_caret_out_of_its_marker(cx: &mut TestAppContext) {
    let mut doc = open("buy", cx);
    doc.caret_at("buy", 0);
    doc.keys(&primary("enter"));
    doc.keys(&primary("z"));
    doc.keys(&primary("shift-z"));
    assert_eq!(doc.text(), "- [ ] buy");
    assert!(doc.selection().head >= 6);
}
