use super::*;
use crate::{
    config::EnterpriseAccount,
    github::{Account, Auth, Server},
    menu::github::{Adding, AdditionalAccount},
};

const ID: &str = "0123456789abcdef0123456789abcdef";

/// What the Enterprise form's focused field holds.
fn focused_text(view: &crate::SheprdWindow, window: &gpui::Window, cx: &gpui::App) -> String {
    let form = view.menu.github_server.as_ref().unwrap();
    form.focused(window, cx).read(cx).text().to_owned()
}

#[gpui::test]
fn an_enterprise_account_can_be_added_while_a_github_sign_in_waits(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.github_fixture(true, window, cx);
            assert!(view.menu.github.code().is_some());
            assert!(view.github_actions().contains(&Action::AddEnterprise));
            cx.notify();
        });
        crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
    });
    cx.simulate_keystrokes("e");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.menu.github_server.is_some());
        assert_eq!(
            view.github_actions(),
            [Action::SubmitServer, Action::CancelServer, Action::Close]
        );
    });
}

#[gpui::test]
fn the_server_and_token_are_typed_in_one_form(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.github_fixture(false, window, cx);
            view.open_github_server(window, cx);
            cx.notify();
        });
        crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("github-server").is_some());
    // Letters that are page shortcuts elsewhere go into the server field,
    // and Enter there moves on to the empty token field.
    cx.simulate_keystrokes("d e t enter");
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(view.menu.github_adding.is_none());
        assert_eq!(
            focused_text(view, window, cx),
            "",
            "the token field is focused"
        );
    });
    // Tab goes back to the server field, which holds what was typed.
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| {
        assert_eq!(focused_text(view.read(cx), window, cx), "det");
    });
    // A server that is no host name is refused before anything is written.
    cx.simulate_keystrokes("/ tab x enter");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.menu.github_server.is_some());
        assert!(view.menu.github_adding.is_none());
        assert!(view.menu.error.is_some());
    });
}

#[gpui::test]
fn the_typed_token_signs_in_the_account_once_its_entry_loads(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.github_fixture(false, window, cx);
            let server = Server::Enterprise("github.acme.com".into());
            view.menu.github_adding = Some(Adding {
                id: ID.into(),
                token: Some("not a token".into()),
            });
            view.config.github.enterprise = vec![EnterpriseAccount {
                server: server.to_string(),
                id: ID.into(),
                owners: Vec::new(),
            }];
            view.menu.github_accounts = vec![AdditionalAccount {
                auth: Auth::for_account(Account::enterprise(&server, ID).unwrap()),
                id: ID.into(),
                owners: Vec::new(),
                server,
            }];
            view.poll_github(window, cx);
            assert!(view.menu.github_adding.is_none());
            assert_eq!(view.menu.github_account.as_deref(), Some(ID));
            let auth = view.github_auth();
            assert!(auth.busy() || auth.failed, "the token is being checked");
        });
    });
}

#[gpui::test]
fn the_typed_token_waits_while_the_new_sign_in_is_busy(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.github_fixture(false, window, cx);
            let server = Server::Enterprise("github.acme.com".into());
            view.config.github.enterprise = vec![EnterpriseAccount {
                server: server.to_string(),
                id: ID.into(),
                owners: Vec::new(),
            }];
            let mut auth = Auth::for_account(Account::enterprise(&server, ID).unwrap());
            auth.use_token("still not a token".into(), &view.config.github);
            assert!(auth.busy());
            view.menu.github_accounts = vec![AdditionalAccount {
                auth,
                id: ID.into(),
                owners: Vec::new(),
                server,
            }];
            view.menu.github_adding = Some(Adding {
                id: ID.into(),
                token: Some("not a token".into()),
            });
            view.poll_github(window, cx);
            assert_eq!(view.menu.github_account.as_deref(), Some(ID));
            let adding = view.menu.github_adding.as_ref().unwrap();
            assert!(adding.token.is_some(), "the token is kept for later");
        });
    });
}

#[gpui::test]
fn a_finished_github_sign_in_keeps_an_open_enterprise_form(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.github_fixture(true, window, cx);
            view.open_github_server(window, cx);
            view.menu
                .github
                .complete_profile_fixture(Ok(Auth::connected_fixture().profile));
            view.poll_github(window, cx);
            assert!(view.menu.github.connected());
            assert!(view.menu.page == Some(crate::menu::Page::GitHub));
            assert!(view.menu.github_server.is_some(), "the form is kept");
        });
    });
}
