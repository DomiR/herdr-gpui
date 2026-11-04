//! A count bubble is only a number, so `[sidebar] numbers = false` draws
//! none, and nothing in its place; the section still says it to a screen
//! reader.
use super::bubbles::{alerts, draw, fixture};
use chrono::Utc;

#[gpui::test]
fn count_bubbles_follow_the_numbers_setting(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    view.update(cx, |view, cx| {
        view.security.read_for_test(vec![alerts(3, Utc::now())]);
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("activity-security-bubble").is_some());
    let glyph = cx.debug_bounds("activity-security-glyph").unwrap();

    view.update(cx, |view, cx| {
        view.config.sidebar_style.numbers = false;
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("activity-security-bubble").is_none());
    // The icon itself stays where it was.
    assert_eq!(cx.debug_bounds("activity-security-glyph"), Some(glyph));

    view.update(cx, |view, cx| {
        view.config.sidebar_style.numbers = true;
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("activity-security-bubble").is_some());
}
