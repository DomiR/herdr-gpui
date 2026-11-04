//! `confirm_close_tab = "always"` asks before closing even an idle tab.
use super::*;

#[gpui::test]
fn always_asks_before_closing_an_idle_tab(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        snapshot.tabs[0].agent_status = AgentStatus::Idle;
        snapshot.agents[0].agent_status = AgentStatus::Done;
        view.live.snapshot = Some(Arc::new(snapshot));
        view.config.confirm_close_tab = TabCloseConfirm::Always;
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // Nothing mid-task, yet each way of closing the tab waits on the
            // dialog with Cancel selected instead of attempting the close.
            view.open_tab_close("w1:t1", window, cx);
            let close = view.menu.close.as_ref().unwrap();
            assert!(close.error.is_none());
            assert!(!close.confirm_selected);
            assert!(view.menu.page == Some(Page::ConfirmClose));
            view.dismiss_menu(window, cx);
            view.open_close_confirmation(Command::CloseTab, window, cx);
            assert!(view.menu.close.as_ref().unwrap().error.is_none());
            assert!(view.menu.page == Some(Page::ConfirmClose));
        })
    });
}

#[gpui::test]
fn never_closes_a_busy_tab_without_asking(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        // The fixture's agent is blocked on a prompt.
        view.live.snapshot = Some(Arc::new(snapshot));
        view.config.confirm_close_tab = TabCloseConfirm::Never;
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // The disconnected fixture attempts the close at once and refuses it.
            view.open_tab_close("w1:t1", window, cx);
            assert!(view.menu.close.as_ref().unwrap().error.is_some());
        })
    });
}
