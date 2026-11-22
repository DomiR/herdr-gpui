use super::*;

#[gpui::test]
fn a_kept_workspace_close_checks_without_herdr_1_and_waits_for_consent(cx: &mut TestAppContext) {
    let (view, cx, id) = window(cx, PLAIN, None);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.confirm_close_workspace = true;
            view.open_workspace_dialog_for(
                &id,
                WorkspaceAction::Close,
                gpui::Point::default(),
                window,
                cx,
            );
            assert!(view.live.snapshot.is_none());
            assert!(view.menu.close_check.is_some());
        });
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("close-git-status").is_some());
    let submit = cx.debug_bounds("dialog-submit").unwrap();
    cx.simulate_click(submit.center(), gpui::Modifiers::none());
    view.read_with(cx, |view, _| assert!(view.is_kept_workspace(&id)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.update_workspace_dialog(window, cx);
            assert!(
                view.menu
                    .close_check
                    .as_ref()
                    .unwrap()
                    .report
                    .as_ref()
                    .unwrap()
                    .unknown
            );
        });
        window.draw(cx).clear(cx);
    });
    let submit = cx.debug_bounds("dialog-submit").unwrap();
    cx.simulate_click(submit.center(), gpui::Modifiers::none());
    view.read_with(cx, |view, _| assert!(view.is_kept_workspace(&id)));
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            view.menu.input = Some(DialogInput::new("close".into()));
        });
        window.draw(cx).clear(cx);
    });
    let submit = cx.debug_bounds("dialog-submit").unwrap();
    cx.simulate_click(submit.center(), gpui::Modifiers::none());
    view.read_with(cx, |view, _| {
        assert!(view.menu.page.is_none());
        assert!(!view.is_kept_workspace(&id));
    });
}
