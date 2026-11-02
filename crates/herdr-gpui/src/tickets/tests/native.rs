//! The native board in a headless window, read from a mock herdr-dog.
use super::server::{PROJECTS, TICKETS, serve};
use crate::{
    HerdrWindow,
    sidebar::{
        ActivityPanel,
        layout_tests::{SidebarFixture, fixture_window, full_draw},
    },
};
use gpui::{AppContext as _, Entity, Modifiers, VisualTestContext, px, size};

/// Ticket 18's page, its description long enough to scroll.
fn detail() -> String {
    let description: Vec<String> = std::iter::once("Read **this**".to_owned())
        .chain((1..150).map(|line| format!("Line {line}")))
        .collect();
    let description = serde_json::to_string(&description.join("\n\n")).unwrap();
    format!(
        r#"{{"ok":true,"data":{{"ticket":{{"number":18,"projectId":1,"parent":null,
        "title":"Open picks","description":{description},"status":"todo","priority":"high",
        "kind":"feature","size":"","assignee":"","branch":"","labels":[],"pull":null,
        "sortKey":1,"createdAt":0,"updatedAt":0,"version":1}},
        "subTickets":[],"progress":{{"done":0,"total":0}},"relations":[]}}}}"#
    )
}
const COMMENTS: &str = r#"{"ok":true,"data":[{"id":2,"ticket":18,"replyTo":null,"thread":2,
    "author":{"kind":"agent","name":"claude"},"body":"Plan","createdAt":0,"read":true,
    "question":false}]}"#;

pub(super) fn native(
    cx: &mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx, _) = native_with_requests(cx);
    (view, cx)
}

fn native_with_requests(
    cx: &mut gpui::TestAppContext,
) -> (
    Entity<HerdrWindow>,
    &mut VisualTestContext,
    super::server::Requests,
) {
    let (api, requests) = serve(
        vec![
            ("GET /api/tickets/projects ", PROJECTS.into()),
            ("GET /api/tickets?project=1 ", TICKETS.into()),
            ("GET /api/tickets/18 ", detail()),
            ("GET /api/tickets/18/comments ", COMMENTS.into()),
            (
                "GET /api/tickets/18/start ",
                r#"{"ok":true,"data":null}"#.into(),
            ),
            ("POST /api/tickets/18/comments ", COMMENT.into()),
        ],
        16,
    );
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
            view.config.tickets.api = Some(api);
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(1200.), px(800.)));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    (view, cx, requests)
}

const COMMENT: &str = r#"{"ok":true,"data":{"id":9,"ticket":18,"replyTo":null,"thread":9,
    "author":{"kind":"operator"},"body":"**hello**","createdAt":0,"read":true,"question":false}}"#;

/// Reads what is due, waits for it, and draws.
pub(super) fn tick(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_tickets(cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
}

#[gpui::test]
fn the_native_board_lists_cards_and_opens_a_tickets_page(cx: &mut gpui::TestAppContext) {
    let (view, cx) = native(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::TicketsNative, window, cx)
        })
    });
    tick(&view, cx);
    assert!(cx.debug_bounds("tickets-native").is_some());
    assert!(cx.debug_bounds("tickets-project-1").is_some());
    // The web board's section is the other item.
    assert!(cx.debug_bounds("tickets-notice").is_none());
    let card = cx.debug_bounds("ticket-card-18").unwrap();
    cx.simulate_click(card.center(), Modifiers::default());
    tick(&view, cx);
    assert!(cx.debug_bounds("ticket-properties").is_some());
    assert!(cx.debug_bounds("ticket-description-0").is_some());
    assert!(cx.debug_bounds("ticket-status-Todo").is_some());
    let back = cx.debug_bounds("ticket-back").unwrap();
    cx.simulate_click(back.center(), Modifiers::default());
    tick(&view, cx);
    assert!(cx.debug_bounds("ticket-card-18").is_some());
    assert!(cx.debug_bounds("ticket-properties").is_none());
}

#[gpui::test]
fn toggle_tickets_shows_the_section_then_the_terminals_again(cx: &mut gpui::TestAppContext) {
    let (view, cx) = native(cx);
    let state = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let view = view.read(cx);
            (view.activity_section(), view.sidebar_visible)
        })
    };
    let (_, sidebar) = state(cx);
    let toggle = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| view.update(cx, |view, cx| view.toggle_tickets(window, cx)));
    };
    toggle(cx);
    assert_eq!(state(cx), (Some(ActivityPanel::Tickets), sidebar));
    toggle(cx);
    // Back to the terminals, the sidebar folded or open as it was.
    assert_eq!(state(cx), (None, sidebar));
    let focused = cx.update(|window, cx| view.read(cx).focus.is_focused(window));
    assert!(focused);
}

#[gpui::test]
fn cmd_down_scrolls_the_open_page_to_its_end(cx: &mut gpui::TestAppContext) {
    let (view, cx) = native(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::TicketsNative, window, cx)
        })
    });
    tick(&view, cx);
    let card = cx.debug_bounds("ticket-card-18").unwrap();
    cx.simulate_click(card.center(), Modifiers::default());
    tick(&view, cx);
    // The board holds the keyboard, so the keys reach it.
    cx.update(|window, cx| view.update(cx, |view, cx| view.focus_native_tickets(window, cx)));
    cx.simulate_keystrokes("cmd-down");
    tick(&view, cx);
    // The page's last row is on screen, its first is not.
    let shown = cx.update(|_, cx| {
        let page = &view.read(cx).ticket_board.lists.page;
        let last = page.item_count() - 1;
        (
            page.bounds_for_item(last).is_some(),
            page.logical_scroll_top().item_ix > 0,
        )
    });
    assert_eq!(shown, (true, true));
    cx.simulate_keystrokes("cmd-up");
    tick(&view, cx);
    let top = cx.update(|_, cx| view.read(cx).ticket_board.lists.page.logical_scroll_top());
    assert_eq!(top.item_ix, 0);
    // W switches to the wide page and back.
    cx.simulate_keystrokes("w");
    assert!(cx.update(|_, cx| view.read(cx).ticket_board.wide));
    cx.simulate_keystrokes("w");
    assert!(!cx.update(|_, cx| view.read(cx).ticket_board.wide));
}

#[gpui::test]
fn the_composer_formats_with_herdr_dogs_shortcuts_and_sends_with_cmd_enter(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx, requests) = native_with_requests(cx);
    // The app's own bindings, Cmd-B among them, are what the field must win over.
    cx.update(|_, cx| crate::bind_keys(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::TicketsNative, window, cx)
        })
    });
    tick(&view, cx);
    let card = cx.debug_bounds("ticket-card-18").unwrap();
    cx.simulate_click(card.center(), Modifiers::default());
    tick(&view, cx);
    cx.update(|_, cx| view.read(cx).ticket_board.lists.page.scroll_to_end());
    tick(&view, cx);
    let add = cx.debug_bounds("ticket-comment-new").unwrap();
    cx.simulate_click(add.center(), Modifiers::default());
    tick(&view, cx);
    assert!(cx.debug_bounds("text-area").is_some());
    cx.simulate_input("hello");
    cx.simulate_keystrokes("cmd-a cmd-b");
    let text = cx.update(|_, cx| {
        let view = view.read(cx);
        let composer = view.ticket_board.composer.as_ref().unwrap();
        composer.input.read(cx).text().to_owned()
    });
    assert_eq!(text, "**hello**");
    cx.simulate_keystrokes("cmd-enter");
    tick(&view, cx);
    assert!(cx.update(|_, cx| view.read(cx).ticket_board.composer.is_none()));
    let sent = requests.lock().unwrap().iter().any(|request| {
        request.starts_with("POST /api/tickets/18/comments ")
            && request.ends_with(r#"{"body":"**hello**"}"#)
    });
    assert!(sent);
}
