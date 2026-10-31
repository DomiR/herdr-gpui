use super::*;

/// Annotate always folds the notes panel away, notes and a waiting batch
/// included, and brings it all back when pressed again.
#[gpui::test]
fn annotate_folds_the_panel_whatever_it_holds(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    with_agent(&view, cx, "working");
    let tab = noted_tab(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // A batch waits for the busy agent, which would keep it open.
            view.send_notes(&tab, cx);
            assert!(view.browser.annotations.open(tab.id));
            view.toggle_annotating(tab.id, window, cx);
            assert!(!view.browser.annotations.open(tab.id));
            // Nothing was dropped: the note and the batch are still there.
            assert_eq!(
                view.browser.annotations.listed(tab.id),
                [("Make it blue".to_owned(), true)]
            );
            assert!(view.queued_notes(tab.id, cx).is_some());
            view.toggle_annotating(tab.id, window, cx);
            assert!(view.browser.annotations.open(tab.id));
        });
    });
}
