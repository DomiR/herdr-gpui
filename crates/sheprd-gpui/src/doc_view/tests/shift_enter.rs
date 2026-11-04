use super::*;

#[gpui::test]
fn shift_enter_keeps_notes_under_tasks(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] task", cx);
    doc.caret_at("task", 4);
    doc.keys("shift-enter");
    doc.type_text("note");
    assert_eq!(doc.text(), "- [ ] task\n      note");
    doc.keys("shift-enter");
    doc.type_text("more");
    assert_eq!(doc.text(), "- [ ] task\n      note\n      more");
}

#[gpui::test]
fn shift_enter_handles_nested_lines(cx: &mut TestAppContext) {
    let mut nested = open("- a\n  1. b", cx);
    nested.caret_at("b", 1);
    nested.keys("shift-enter");
    assert_eq!(nested.text(), "- a\n  1. b\n     ");
}

#[gpui::test]
fn shift_enter_preserves_plain_line_indentation(cx: &mut TestAppContext) {
    let mut plain = open("  note", cx);
    plain.caret_at("note", 4);
    plain.keys("shift-enter");
    assert_eq!(plain.text(), "  note\n  ");
}

#[gpui::test]
fn shift_enter_replaces_a_selection_with_a_plain_break(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] task", cx);
    doc.caret_at("task", 4);
    doc.keys("shift-left shift-left");
    doc.keys("shift-enter");
    assert_eq!(doc.text(), "- [ ] ta\n");
    assert_eq!(doc.selection(), Selection::caret(9));
}
