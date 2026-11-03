use super::*;

use gpui::Modifiers;

#[gpui::test]
fn primary_alt_enter_toggles_cancellation(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] open\n- [x] done\n- [-] cancelled\n", cx);
    let key = primary("alt-enter");
    doc.caret_at("open", 0);
    doc.keys(&key);
    doc.caret_at("done", 0);
    doc.keys(&key);
    doc.caret_at("cancelled", 0);
    doc.keys(&key);
    assert_eq!(doc.text(), "- [-] open\n- [-] done\n- [ ] cancelled\n");

    doc.caret_at("cancelled", 0);
    doc.keys(&key);
    doc.keys(&primary("enter"));
    assert_eq!(doc.text(), "- [-] open\n- [-] done\n- [ ] cancelled\n");
}

#[gpui::test]
fn primary_alt_enter_cancels_each_task_in_a_selection(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] one\n- [x] two\n- [ ] three\n", cx);
    let end = doc.text().find("two").unwrap() + "two".len();
    doc.cx.update(|_, cx| {
        doc.editor.update(cx, |editor, cx| {
            editor.select(Selection { anchor: 0, head: end }, cx);
        });
    });
    doc.keys(&primary("alt-enter"));
    assert_eq!(doc.text(), "- [-] one\n- [-] two\n- [ ] three\n");
}

#[gpui::test]
fn alt_click_cancels_and_plain_click_reopens(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] task\n", cx);
    let position = doc.marker_position(0);
    doc.cx.simulate_click(
        position,
        Modifiers {
            alt: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(doc.text(), "- [-] task\n");

    let position = doc.marker_position(0);
    doc.cx.simulate_click(position, Modifiers::default());
    assert_eq!(doc.text(), "- [ ] task\n");
}
