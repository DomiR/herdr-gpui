use super::*;

#[gpui::test]
fn typing_is_written_after_a_pause(cx: &mut TestAppContext) {
    let mut doc = open("a\n", cx);
    doc.caret_at("a", 1);
    doc.type_text("b");
    assert_eq!(doc.disk(), "a\n");
    doc.wait(500);
    assert_eq!(doc.disk(), "ab\n");
}

#[gpui::test]
fn a_file_rewritten_while_typing_merges_and_is_written_back(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n- [ ] b\n", cx);
    doc.caret_at("a", 1);
    doc.type_text("!");
    // An agent checks "b" on disk before the typing is written.
    std::fs::write(&doc.path, "- [ ] a\n- [x] b\n").unwrap();
    doc.wait(1500);
    assert_eq!(doc.text(), "- [ ] a!\n- [x] b\n");
    assert_eq!(doc.disk(), "- [ ] a!\n- [x] b\n");
}

#[gpui::test]
fn a_deleted_file_is_not_recreated_until_saved(cx: &mut TestAppContext) {
    let mut doc = open("a\n", cx);
    std::fs::remove_file(&doc.path).unwrap();
    doc.wait(600);
    doc.caret_at("a", 1);
    doc.type_text("b");
    doc.wait(1500);
    assert!(!doc.path.exists());
    let missing = doc
        .cx
        .update(|_, cx| doc.editor.read(cx).buffer.read(cx).missing());
    assert!(missing);
    doc.keys(&primary("s"));
    doc.wait(100);
    assert_eq!(doc.disk(), "ab\n");
}

#[gpui::test]
fn a_file_that_comes_back_is_merged_not_overwritten(cx: &mut TestAppContext) {
    let mut doc = open("a\n", cx);
    // An editor saves by deleting and creating the file; a poll lands in
    // between.
    std::fs::remove_file(&doc.path).unwrap();
    doc.wait(600);
    std::fs::write(&doc.path, "a\nfrom tool\n").unwrap();
    doc.wait(600);
    assert_eq!(doc.text(), "a\nfrom tool\n");
    doc.caret_at("a", 1);
    doc.type_text("b");
    doc.keys(&primary("s"));
    doc.wait(600);
    assert_eq!(doc.disk(), "ab\nfrom tool\n");
}

#[gpui::test]
fn closing_a_document_writes_its_last_typing_merged_with_the_disk(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("todo.md");
    std::fs::write(&path, "one\n").unwrap();
    let buffer = cx.update(|cx| Docs::open(&path, cx));
    cx.run_until_parked();
    cx.update(|cx| {
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit(&[herdr_doc::Edit::insert(3, " typed")], cx)
                .unwrap();
        });
    });
    // Another program changes the file before the typing is written, and
    // the last view closes.
    std::fs::write(&path, "one\ntwo\n").unwrap();
    drop(buffer);
    // The app lets go of the entity when it next flushes its effects.
    cx.update(|_| {});
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one typed\ntwo\n");
}

#[gpui::test]
fn another_writer_ends_a_composition(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let mut doc = open("x\n", cx);
    doc.caret_at("x", 1);
    let editor = doc.editor.clone();
    doc.cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.replace_and_mark_text_in_range(None, "か", None, window, cx);
        });
    });
    std::fs::write(&doc.path, "new line\nx\n").unwrap();
    doc.wait(600);
    let marked = doc
        .cx
        .update(|window, cx| editor.update(cx, |editor, cx| editor.marked_text_range(window, cx)));
    assert_eq!(marked, None);
    assert_eq!(doc.text(), "new line\nxか\n");
}
