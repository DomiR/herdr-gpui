use super::*;

fn stamps_on(doc: &mut Fixture<'_>) {
    doc.cx.update(|_, cx| {
        let mut appearance = crate::app::InitialAppearance::default();
        appearance.config.documents.stamps = true;
        cx.set_global(appearance);
    });
}

fn has_date_stamp(text: &str, prefix: &str) -> bool {
    let Some(start) = text.find(prefix).map(|start| start + prefix.len()) else {
        return false;
    };
    let Some(date) = text.as_bytes().get(start..start + 14) else {
        return false;
    };
    date.iter().enumerate().all(|(index, byte)| match index {
        2 | 5 => *byte == b'-',
        8 => *byte == b' ',
        11 => *byte == b':',
        _ => byte.is_ascii_digit(),
    }) && text.get(start + 14..start + 15) == Some(")")
}

#[gpui::test]
fn clicking_a_box_writes_a_done_stamp(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n", cx);
    stamps_on(&mut doc);
    doc.cx.update(|_, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.toggle_task(0, cx));
    });
    let text = doc.text();
    assert!(text.starts_with("- [x] a"));
    assert!(has_date_stamp(&text, " @done ("));
}

#[gpui::test]
fn primary_enter_stamps_and_reopens_a_task(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n", cx);
    stamps_on(&mut doc);
    doc.caret_at("a", 1);
    doc.keys(&primary("enter"));
    let checked = doc.text();
    assert!(checked.starts_with("- [x] a"));
    assert!(has_date_stamp(&checked, " @done ("));
    assert_eq!(
        doc.selection(),
        Selection::caret(checked.find('\n').unwrap())
    );

    doc.keys(&primary("enter"));
    assert_eq!(doc.text(), "- [ ] a\n");
}

#[gpui::test]
fn primary_alt_enter_writes_a_cancelled_stamp(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n", cx);
    stamps_on(&mut doc);
    doc.caret_at("a", 1);
    doc.keys(&primary("alt-enter"));
    let text = doc.text();
    assert!(text.starts_with("- [-] a"));
    assert!(has_date_stamp(&text, " @cancelled ("));
}

#[gpui::test]
fn stamped_checkbox_edits_keep_the_caret_and_undo_as_one_step(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] a\n", cx);
    stamps_on(&mut doc);
    doc.caret_at("a", 1);
    doc.keys(&primary("enter"));
    let checked = doc.text();
    assert_eq!(
        doc.selection(),
        Selection::caret(checked.find('\n').unwrap())
    );

    doc.keys(&primary("z"));
    assert_eq!(doc.text(), "- [ ] a\n");
}
