//! Naming a tab from its rename page with an agent (`crate::naming`).

use super::*;
use crate::naming::{Error, Way};

fn renaming(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(
            serde_json::from_str(include_str!(
                "../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
            ))
            .unwrap(),
        ));
        view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Local;
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_tab_menu("w1:t1", None, Point::default(), window, cx);
            view.activate_tab_menu(Action::Rename, window, cx);
        });
    });
    (view, cx)
}

fn field(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, cx| view.tab_rename_text(cx).unwrap())
}

#[gpui::test]
fn the_rename_page_offers_claude_and_the_tabs_agent(cx: &mut TestAppContext) {
    let (view, cx) = renaming(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::RenameTab));
        let naming = view.menu.naming.as_ref().unwrap();
        // The fixture's pane runs Claude in a terminal, which only a client
        // that runs host scripts can ask.
        assert_eq!(
            naming.agent().is_some(),
            cfg!(any(target_os = "linux", target_os = "macos"))
        );
        assert_eq!(naming.running(), None);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("naming-claude").is_some());
    assert_eq!(
        cx.debug_bounds("naming-agent").is_some(),
        cfg!(any(target_os = "linux", target_os = "macos"))
    );
}

#[gpui::test]
fn only_the_latest_request_fills_the_field(cx: &mut TestAppContext) {
    let (view, cx) = renaming(cx);
    assert_eq!(field(&view, cx), "main");
    let (old, current) = view.update(cx, |view, _| {
        let old = view.begin_naming_for_test(Way::Agent);
        (old, view.begin_naming_for_test(Way::Claude))
    });
    view.update(cx, |view, cx| {
        view.finish_naming_for_test(old, Ok("Old answer".into()), cx);
        assert_eq!(
            view.menu.naming.as_ref().unwrap().running(),
            Some(Way::Claude)
        );
    });
    assert_eq!(
        field(&view, cx),
        "main",
        "a replaced request's answer is dropped"
    );
    view.update(cx, |view, cx| {
        view.finish_naming_for_test(current, Ok("Push relay".into()), cx);
        assert_eq!(view.menu.naming.as_ref().unwrap().running(), None);
    });
    assert_eq!(field(&view, cx), "Push relay");
    // The page still renames only once the person submits.
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::RenameTab));
        assert!(view.menu.tab.as_ref().unwrap().pending.is_none());
    });
}

#[gpui::test]
fn a_failure_is_told_and_leaves_the_field(cx: &mut TestAppContext) {
    let (view, cx) = renaming(cx);
    view.update(cx, |view, cx| {
        let request = view.begin_naming_for_test(Way::Agent);
        view.finish_naming_for_test(
            request,
            Err(Error::TimedOut {
                agent: "Claude".into(),
            }),
            cx,
        );
        let error = view.menu.naming.as_ref().unwrap().error().unwrap();
        assert!(error.contains("did not answer in time"), "{error}");
    });
    assert_eq!(field(&view, cx), "main");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("naming-error").is_some());
}

#[gpui::test]
fn closing_the_page_drops_the_request(cx: &mut TestAppContext) {
    let (view, cx) = renaming(cx);
    let request = view.update(cx, |view, _| view.begin_naming_for_test(Way::Claude));
    cx.update(|window, cx| view.update(cx, |view, cx| view.dismiss_menu(window, cx)));
    view.update(cx, |view, cx| {
        assert!(view.menu.naming.is_none());
        // Its answer, arriving late, changes nothing.
        view.finish_naming_for_test(request, Ok("Late".into()), cx);
        assert!(view.menu.naming.is_none());
    });
}
