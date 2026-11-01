use super::*;

#[gpui::test]
fn typing_and_deleting(cx: &mut TestAppContext) {
    let mut doc = open("hello\n", cx);
    doc.caret_at("hello", 5);
    doc.type_text(" wörld");
    assert_eq!(doc.text(), "hello wörld\n");
    doc.keys("backspace backspace");
    assert_eq!(doc.text(), "hello wör\n");
    doc.keys("left shift-left");
    // "ö" is two bytes.
    assert_eq!(doc.selection().range().len(), 2);
    doc.type_text("e");
    assert_eq!(doc.text(), "hello wer\n");
}

#[gpui::test]
fn undo_reverts_typing_but_keeps_a_disk_change(cx: &mut TestAppContext) {
    let mut doc = open("one\n", cx);
    doc.caret_at("one", 3);
    doc.type_text(" two");
    // Another program appends a line while the user types.
    doc.wait(450);
    std::fs::write(&doc.path, "one two\nthree\n").unwrap();
    doc.wait(600);
    assert_eq!(doc.text(), "one two\nthree\n");
    doc.keys(&primary("z"));
    assert_eq!(doc.text(), "one\nthree\n");
    doc.keys(&primary("shift-z"));
    assert_eq!(doc.text(), "one two\nthree\n");
}

#[gpui::test]
fn compositions_replace_their_own_text(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let mut doc = open("x\n", cx);
    doc.caret_at("x", 1);
    let editor = doc.editor.clone();
    doc.cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.replace_and_mark_text_in_range(None, "か", None, window, cx);
            editor.replace_and_mark_text_in_range(None, "かん", None, window, cx);
            assert_eq!(editor.marked_text_range(window, cx), Some(1..3));
            editor.replace_text_in_range(None, "漢", window, cx);
            assert_eq!(editor.marked_text_range(window, cx), None);
        });
    });
    assert_eq!(doc.text(), "x漢\n");
    assert_eq!(doc.selection(), Selection::caret("x漢".len()));
    assert_eq!(super::super::input::utf16_round_trip("a漢😀", 4), 4);
}

#[gpui::test]
fn a_long_document_lays_out_only_what_is_in_view(cx: &mut TestAppContext) {
    let text: String = (0..5000)
        .map(|n| format!("- [ ] task number {n} with **bold** and `code`\n"))
        .collect();
    let mut doc = open(&text, cx);
    assert_eq!(doc.rows().len(), 5001);
    let laid_out = doc
        .cx
        .update(|_, cx| doc.editor.read(cx).layouts.borrow().len());
    assert!(laid_out > 0 && laid_out < 200, "{laid_out} rows laid out");
    // Typing at the end of a long document stays a small edit.
    doc.keys(&primary("down"));
    doc.type_text("- [ ] last");
    assert!(doc.text().ends_with("- [ ] last"));
}
