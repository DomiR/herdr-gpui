#![allow(clippy::expect_used)]

use super::*;

#[test]
fn copy_link_has_a_fluent_label() {
    assert_eq!(Action::CopyLink.label(), "Copy Link");
}

#[gpui::test]
fn copy_link_is_before_close_and_confirms_with_a_flash(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut shown = crate::sidebar::layout_tests::snapshot(1);
        shown.focused_workspace_id = Some("w0".into());
        shown.focused_tab_id = Some("t0".into());
        shown.panes.push(herdr_client::protocol::ClientShellPane {
            pane_id: "p0".into(),
            workspace_id: "w0".into(),
            tab_id: "t0".into(),
            label: Some("shell".into()),
            cwd: Some("/fixture/agent-launcher".into()),
            foreground_cwd: Some("/fixture/agent-launcher".into()),
            focused: false,
            right_click_passthrough: false,
        });
        shown.panes.push(herdr_client::protocol::ClientShellPane {
            pane_id: "p1".into(),
            workspace_id: "w0".into(),
            tab_id: "t1".into(),
            label: Some("shell".into()),
            cwd: Some("/fixture/agent-launcher".into()),
            foreground_cwd: Some("/fixture/agent-launcher".into()),
            focused: false,
            right_click_passthrough: false,
        });
        view.live.snapshot = Some(Arc::new(shown));
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_tab_menu("t1", None, Point::default(), window, cx);
            let actions = view.menu.tab.as_ref().unwrap().actions();
            let copy = actions
                .iter()
                .position(|action| *action == Action::CopyLink)
                .expect("terminal tab menu offers Copy Link");
            let close = actions
                .iter()
                .position(|action| *action == Action::Close)
                .expect("terminal tab menu offers Close");
            assert!(copy < close);
            view.activate_tab_menu(Action::CopyLink, window, cx);
            assert_eq!(
                view.flash.as_ref().map(|(flash, _)| flash.text.as_ref()),
                Some("Link copied")
            );
            let clipboard = cx.read_from_clipboard().and_then(|item| item.text());
            assert!(
                clipboard.as_deref().is_some_and(
                    |text| text.starts_with('[') && text.contains("herdr-gpui://focus?")
                )
            );
        })
    });
}
