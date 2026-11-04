use super::*;

fn open_find(doc: &mut Fixture<'_>) {
    doc.cx.update(|window, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.open_find(window, cx));
    });
}

fn find_again(doc: &mut Fixture<'_>, back: bool) {
    doc.cx.update(|window, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.find_again(back, window, cx));
    });
}

fn status(doc: &mut Fixture<'_>) -> String {
    // Without the isolation marks Fluent puts around each placeable.
    doc.cx
        .update(|_, cx| doc.editor.read(cx).find_status())
        .replace(['\u{2068}', '\u{2069}'], "")
}

#[gpui::test]
fn opening_and_typing_selects_the_first_match_after_the_caret(cx: &mut TestAppContext) {
    let mut doc = open("before\nfoo one\nfoo two\n", cx);
    doc.caret_at("foo two", 0);
    open_find(&mut doc);
    doc.type_text("foo");
    assert_eq!(status(&mut doc), "2 of 2");
    assert_eq!(doc.selection().range(), 15..18);
}

#[gpui::test]
fn an_unmatched_query_reports_no_results(cx: &mut TestAppContext) {
    let mut doc = open("present\n", cx);
    open_find(&mut doc);
    doc.type_text("missing");
    assert_eq!(status(&mut doc), "No results");
}

#[gpui::test]
fn enter_and_shift_enter_step_and_wrap(cx: &mut TestAppContext) {
    let text = "foo one\nfoo two\nfoo three\n";
    let mut doc = open(text, cx);
    open_find(&mut doc);
    doc.type_text("foo");
    assert_eq!(doc.selection().range(), 0..3);
    doc.keys("enter");
    assert_eq!(doc.selection().range(), 8..11);
    doc.keys("shift-enter");
    assert_eq!(doc.selection().range(), 0..3);
    doc.keys("shift-enter");
    assert_eq!(doc.selection().range(), 16..19);
}

#[gpui::test]
fn find_again_opens_then_steps_forward_and_back(cx: &mut TestAppContext) {
    let mut doc = open("foo one\nfoo two\nfoo three\n", cx);
    find_again(&mut doc, false);
    doc.type_text("foo");
    assert_eq!(doc.selection().range(), 0..3);
    find_again(&mut doc, false);
    assert_eq!(doc.selection().range(), 8..11);
    find_again(&mut doc, true);
    assert_eq!(doc.selection().range(), 0..3);
}

#[gpui::test]
fn reopening_find_keeps_query_and_match_count(cx: &mut TestAppContext) {
    let mut doc = open("foo one\nfoo two\n", cx);
    open_find(&mut doc);
    doc.type_text("foo");
    let before = status(&mut doc);
    open_find(&mut doc);
    assert_eq!(status(&mut doc), before);
}

#[gpui::test]
fn escape_closes_and_keeps_the_current_selection(cx: &mut TestAppContext) {
    let mut doc = open("one two one\n", cx);
    open_find(&mut doc);
    doc.type_text("one");
    doc.keys("enter");
    let current = doc.selection();
    doc.keys("escape");
    let focused = doc
        .cx
        .update(|window, cx| doc.editor.read(cx).focus.is_focused(window));
    assert!(focused);
    assert_eq!(doc.selection(), current);
}

#[gpui::test]
fn reopening_after_escape_keeps_the_previous_query(cx: &mut TestAppContext) {
    let mut doc = open("foo one\nfoo two\n", cx);
    open_find(&mut doc);
    doc.type_text("foo");
    assert_eq!(status(&mut doc), "1 of 2");
    doc.keys("escape");
    open_find(&mut doc);
    assert_eq!(status(&mut doc), "1 of 2");
}

#[gpui::test]
fn a_match_in_a_fold_unfolds_its_line(cx: &mut TestAppContext) {
    let mut doc = open("# Heading\nneedle\n", cx);
    doc.cx.update(|_, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.toggle_fold(0, cx));
    });
    assert!(!doc.rows().contains(&1));
    open_find(&mut doc);
    doc.type_text("needle");
    assert!(doc.rows().contains(&1));
}

#[gpui::test]
fn an_external_change_keeps_the_same_match_current(cx: &mut TestAppContext) {
    let original = "head\nfoo\nfoo\n";
    let mut doc = open(original, cx);
    doc.caret_at("foo\nfoo", "foo\n".len());
    open_find(&mut doc);
    doc.type_text("foo");
    assert_eq!(doc.selection().range(), 9..12);
    std::fs::write(&doc.path, format!("intro\n{original}")).unwrap();
    doc.wait(600);
    doc.rows();
    let expected = doc.text().rfind("foo").unwrap();
    assert_eq!(doc.selection().range(), expected..expected + 3);
    assert_eq!(status(&mut doc), "2 of 2");
}

#[gpui::test]
fn hide_done_excludes_done_matches(cx: &mut TestAppContext) {
    let mut doc = open("- [x] task\n- [ ] task\n", cx);
    open_find(&mut doc);
    doc.type_text("task");
    assert_eq!(status(&mut doc), "1 of 2");
    doc.cx.update(|_, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.toggle_hide_done(cx));
    });
    assert_eq!(status(&mut doc), "1 of 1");
}

#[gpui::test]
fn field_keys_do_not_edit_the_document(cx: &mut TestAppContext) {
    let mut doc = open("keep this\n", cx);
    open_find(&mut doc);
    let before = doc.text();
    doc.keys("backspace");
    doc.keys("tab");
    assert_eq!(doc.text(), before);
}

#[gpui::test]
fn match_case_changes_the_count(cx: &mut TestAppContext) {
    let mut doc = open("Foo foo\n", cx);
    open_find(&mut doc);
    doc.type_text("foo");
    assert_eq!(status(&mut doc), "1 of 2");
    doc.cx.update(|window, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.toggle_find_match_case(window, cx));
    });
    assert_eq!(status(&mut doc), "1 of 1");
}
