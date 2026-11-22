use super::*;

#[gpui::test]
fn selecting_a_worktree_gives_the_terminal_the_keyboard(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    // Focus events reach listeners only in the active window.
    cx.update(|window, _| window.activate_window());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    view.update_in(cx, |view, window, cx| {
        // As a click in the layout leaves it: on something other than the
        // terminal.
        let elsewhere = cx.focus_handle();
        window.focus(&elsewhere, cx);
        view.live.snapshot = Some(Arc::new(crate::sidebar::layout_tests::snapshot(3)));
        let state = view.hosted_state(cx);
        let workspace = state.main["workspaces"][1]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        view.hosted_action(
            Action::FocusWorkspace {
                endpoint: None,
                workspace,
            },
            window,
            cx,
        );
        assert!(view.focus.is_focused(window));
    });
}
