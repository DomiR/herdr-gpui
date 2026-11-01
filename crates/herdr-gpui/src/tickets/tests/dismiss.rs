//! The board's failures close: a read error stays closed until another, or
//! one after a read succeeds; a closed notice is gone.
use super::native::{native, tick};
use crate::sidebar::ActivityPanel;
use gpui::{Entity, Modifiers, VisualTestContext};

fn fail(view: &Entity<crate::HerdrWindow>, cx: &mut VisualTestContext, error: &'static str) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.ticket_board.error = Some(error.into());
            cx.notify();
        })
    });
    tick(view, cx);
}

fn click(view: &Entity<crate::HerdrWindow>, cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(at, Modifiers::default());
    tick(view, cx);
}

fn shown(cx: &mut VisualTestContext) -> bool {
    cx.debug_bounds("tickets-error").is_some()
}

#[gpui::test]
fn a_closed_read_error_stays_closed_until_it_changes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = native(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::TicketsNative, window, cx)
        })
    });
    tick(&view, cx);
    assert!(!shown(cx));

    fail(&view, cx, "Cannot reach herdr-dog");
    assert!(shown(cx));
    click(&view, cx, "tickets-error-close");
    assert!(!shown(cx));
    fail(&view, cx, "Cannot reach herdr-dog");
    assert!(!shown(cx), "the same error after another read stays closed");

    fail(&view, cx, "herdr-dog answered HTTP 500");
    assert!(shown(cx));
    click(&view, cx, "tickets-error-close");
    assert!(!shown(cx));

    // A read that succeeds, then the same error again.
    cx.update(|_, cx| view.update(cx, |view, _| view.ticket_board.read_now()));
    tick(&view, cx);
    assert!(cx.update(|_, cx| view.read(cx).ticket_board.error.is_none()));
    fail(&view, cx, "herdr-dog answered HTTP 500");
    assert!(shown(cx));
}

#[gpui::test]
fn a_closed_notice_is_gone(cx: &mut gpui::TestAppContext) {
    let (view, cx) = native(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::TicketsNative, window, cx)
        })
    });
    tick(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.ticket_board.notice = Some("Cannot move ticket 18".into());
            cx.notify();
        })
    });
    tick(&view, cx);
    assert!(shown(cx));
    click(&view, cx, "tickets-notice-close");
    assert!(!shown(cx));
    assert!(cx.update(|_, cx| view.read(cx).ticket_board.notice.is_none()));
}
