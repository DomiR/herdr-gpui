//! The Tickets section, herdr-dog's web UI, takes the window's body with
//! the board's page, or says why it cannot, and hands the keyboard back as
//! another section shows. Tickets (native) has its own tests in
//! `crate::tickets`.
use super::*;
use sections::{local, show};

#[gpui::test]
fn the_tickets_section_takes_the_body_and_leaves_it_again(cx: &mut gpui::TestAppContext) {
    let (view, cx) = local(cx);
    show(&view, cx, ActivityPanel::Tickets);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).activity_section()),
        Some(ActivityPanel::Tickets)
    );
    // A headless window has no web view to give the page, so the section
    // says so in its place, with the board's address to open instead.
    assert!(cx.debug_bounds("tickets-section").is_some());
    assert!(cx.debug_bounds("tickets-notice").is_some());
    assert!(cx.debug_bounds("tickets-open-browser").is_some());
    show(&view, cx, ActivityPanel::Workspaces);
    assert_eq!(cx.update(|_, cx| view.read(cx).activity_section()), None);
    assert!(cx.debug_bounds("tickets-section").is_none());
}
