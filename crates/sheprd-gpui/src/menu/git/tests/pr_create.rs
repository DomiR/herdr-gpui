use super::*;
use crate::{
    git::{Action, Git},
    pull_request::Input,
    state::ConnectionStatus,
};
use gpui::{KeyDownEvent, Keystroke};

fn prepare(view: &mut crate::SheprdWindow) -> Input {
    view.live.status = ConnectionStatus::Connected;
    view.live.local_daemon_peer = true;
    view.menu.github = crate::github::Auth::connected_fixture();
    let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
    snapshot.focused_workspace_id = Some("w3".into());
    view.live.snapshot = Some(Arc::new(snapshot));
    let input = view.git_input().unwrap();
    view.git = Git::fixture(input.clone(), Status::default());
    input
}

#[gpui::test]
fn command_opened_creation_ignores_enter_and_only_a_click_starts_work(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare(view);
            assert!(view.ask_pr_create(window, cx));
            assert_eq!(view.menu.git_selected, Some(Row::PullRequest));
            for is_held in [false, true] {
                view.git_key(
                    &KeyDownEvent {
                        keystroke: Keystroke::parse("enter").unwrap(),
                        is_held,
                        prefer_character_input: false,
                    },
                    window,
                    cx,
                );
                assert!(view.git.running().is_none());
            }
            view.activate_git_row(Row::PullRequest, window, cx);
            assert_eq!(view.git.running(), Some(&Action::CreatePullRequest));
        });
    });
}

#[gpui::test]
fn changing_the_checkout_cancels_a_command_opened_creation(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let mut input = prepare(view);
            assert!(view.ask_pr_create(window, cx));
            input.branch = "another-branch".into();
            view.git = Git::fixture(input, Status::default());
            view.activate_git_row(Row::PullRequest, window, cx);
            assert!(view.git.running().is_none());
            assert!(view.menu.page.is_none());
        });
    });
}

#[gpui::test]
fn another_menu_or_running_git_operation_is_not_replaced(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare(view);
            view.menu.page = Some(Page::GitHub);
            assert!(!view.ask_pr_create(window, cx));
            assert_eq!(view.menu.page, Some(Page::GitHub));
            view.menu.reset();
            view.start_git(Action::Push);
            assert!(!view.ask_pr_create(window, cx));
            assert!(view.menu.page.is_none());
            assert_eq!(view.git.running(), Some(&Action::Push));
        });
    });
}
