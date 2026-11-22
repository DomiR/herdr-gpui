use super::super::pr::create;
use super::*;
use crate::{
    git::{Git, Status},
    menu::Page,
};
use std::sync::Arc;

fn prepare(view: &mut SheprdWindow) {
    view.live.status = crate::state::ConnectionStatus::Connected;
    view.live.local_daemon_peer = true;
    let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
    snapshot.focused_workspace_id = Some("w3".into());
    view.live.snapshot = Some(Arc::new(snapshot));
    view.menu.github = crate::github::Auth::connected_fixture();
    view.git = Git::fixture(view.git_input().unwrap(), Status::default());
}

#[gpui::test]
fn creating_only_opens_the_native_action_for_the_focused_checkout(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare(view);
            let response = create(view, &caller(Some("w3")), window, cx);
            let Response::Done { message } = response else {
                panic!("{response:?}")
            };
            assert!(message.contains("Nothing has been pushed or created"));
            assert_eq!(view.menu.page, Some(Page::Git));
            assert!(view.git.running().is_none());
            let busy = create(view, &caller(Some("w3")), window, cx);
            assert_eq!(error_code(&busy), Some(ErrorCode::WindowBusy));
            assert_eq!(view.menu.page, Some(Page::Git));
        });
    });
}

#[gpui::test]
fn creation_refuses_other_workspaces_sessions_and_existing_pull_requests(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare(view);
            assert_eq!(
                error_code(&create(view, &caller(Some("w4")), window, cx)),
                Some(ErrorCode::InvalidRequest)
            );
            let elsewhere = Caller {
                daemon_socket: Some("/tmp/another-session.sock".into()),
                ..caller(Some("w3"))
            };
            assert_eq!(
                error_code(&create(view, &elsewhere, window, cx)),
                Some(ErrorCode::WorkspaceNotFound)
            );
            let input = view.git_input().unwrap();
            view.seed_pr(
                input,
                crate::pull_request::fixture().unwrap(),
                std::time::Instant::now(),
            );
            assert_eq!(
                error_code(&create(view, &caller(Some("w3")), window, cx)),
                Some(ErrorCode::InvalidRequest)
            );
            assert!(view.menu.page.is_none());
            assert!(view.git.running().is_none());
        });
    });
}

#[gpui::test]
fn creation_requires_sign_in_and_the_git_worker_to_follow_this_checkout(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare(view);
            view.menu.github = crate::github::Auth::default();
            assert_eq!(
                error_code(&create(view, &caller(Some("w3")), window, cx)),
                Some(ErrorCode::InvalidRequest)
            );
            view.menu.github = crate::github::Auth::connected_fixture();
            view.git = Git::default();
            assert_eq!(
                error_code(&create(view, &caller(Some("w3")), window, cx)),
                Some(ErrorCode::InvalidRequest)
            );
            view.live.local_daemon_peer = false;
            assert_eq!(
                error_code(&create(view, &caller(Some("w3")), window, cx)),
                Some(ErrorCode::Unsupported)
            );
            assert!(view.menu.page.is_none());
        });
    });
}
