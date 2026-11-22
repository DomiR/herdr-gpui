use super::*;
use crate::{endpoint::Endpoint, state::ConnectionStatus};

#[gpui::test]
fn native_setup_keeps_the_selected_host_and_needs_no_local_daemon(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(Endpoint::new(
                "remote".into(),
                "Remote".into(),
                sheprd_client::ConnectTarget::Socket("/unused.sock".into()),
                false,
            ));
            view.selected_endpoint = 1;
            view.endpoints[0].live.status = ConnectionStatus::Disconnected;
            let request = setup::Request::new("dev@box", "Box", "").unwrap();
            open_form(view, Step::Verifying(request, Offer::Install), cx);
            view.device_setup_started(Ok(crate::setup_relay::gui::Session::fixture()), cx);
            assert!(matches!(step(view), Step::Running(..)));
            assert_eq!(view.selected_endpoint, 1);
            assert_eq!(view.menu.page, Some(Page::AddDevice));
            assert!(view.pending_navigation.is_none());
            view.cancel_device_step(cx);
            assert!(matches!(step(view), Step::Form));
            assert!(view.menu.device_setup.as_ref().unwrap().claim.is_none());
        });
    });
}

#[gpui::test]
fn native_setup_failure_preserves_the_key_offer(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("dev@box", "Box", "").unwrap();
            open_form(view, Step::Verifying(request, Offer::InstallKey), cx);
            view.device_setup_started(
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into()),
                cx,
            );
            assert!(matches!(step(view), Step::Confirm(_, Offer::InstallKey)));
            assert!(view.menu.device_setup.as_ref().unwrap().error.is_some());
        });
    });
}

#[gpui::test]
fn native_confirmation_stays_inside_narrow_and_wide_dialogs(cx: &mut gpui::TestAppContext) {
    use gpui::{px, size};
    let (view, cx) = cx.add_window_view(fixture_window);
    for width in [360., 960.] {
        cx.simulate_resize(size(px(width), px(640.)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let request = setup::Request::new("dev@box", "Box", "").unwrap();
                open_form(view, Step::Verifying(request, Offer::Restart), cx);
                view.device_setup_started(Ok(crate::setup_relay::gui::Session::fixture_state(
                    "Restarting the remote server stops its running panes. Review this before continuing.\nRestart the server? [y/N]", None,
                )), cx);
                view.poll_device_setup(window, cx);
                assert!(view.menu.focus.is_focused(window));
            });
            window.draw(cx).clear(cx);
        });
        let output = cx.debug_bounds("device-progress-output").unwrap();
        let yes = cx.debug_bounds("device-progress-yes").unwrap();
        let no = cx.debug_bounds("device-progress-no").unwrap();
        assert!(yes.right() <= px(width));
        assert!(no.origin.x >= px(0.));
        assert!(yes.origin.y >= output.bottom());
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| {
            assert!(matches!(step(view.read(cx)), Step::Running(..)));
        });
    }
}

#[gpui::test]
fn native_completion_and_failures_stay_with_the_ssh_form(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    for code in [1, 0] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let request = setup::Request::new("dev@box", "Box", "").unwrap();
                open_form(view, Step::Verifying(request, Offer::Install), cx);
                view.device_setup_started(
                    Ok(crate::setup_relay::gui::Session::fixture_state(
                        "Connection refused",
                        Some(Ok(code)),
                    )),
                    cx,
                );
                view.poll_device_setup(window, cx);
                if code == 0 {
                    assert!(matches!(step(view), Step::Saved));
                } else {
                    assert!(matches!(step(view), Step::Form));
                    assert!(
                        view.menu
                            .device_setup
                            .as_ref()
                            .unwrap()
                            .error
                            .as_deref()
                            .unwrap()
                            .contains("Connection refused")
                    );
                }
            });
        });
    }
}
