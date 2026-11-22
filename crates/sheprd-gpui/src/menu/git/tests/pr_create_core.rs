use super::*;
use crate::{
    browser::{Location, Store},
    core_terminals::panes::{Axis, Panes},
    git::Git,
};
use sheprd_backend::{RunState, TerminalId, TerminalInfo};
use sheprd_client::protocol::AgentStatus;
use sheprd_vcs::git::Identity;

#[gpui::test]
fn pr_create_cancels_when_the_focused_core_pane_changes_before_the_next_tick(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            snapshot.focused_workspace_id = Some("w0".into());
            view.live.snapshot = Some(Arc::new(snapshot));
            let first = TerminalId::new("t1").unwrap();
            let second = TerminalId::new("t2").unwrap();
            let mut panes = Panes::single(first.clone());
            panes.split(&first, Axis::Row, second.clone()).unwrap();
            let scope = crate::browser::scope(&view.endpoints[0]);
            let location = Location::core_terminal(panes.clone(), false);
            let tab = Store::update(cx, |store| {
                store.open(scope.clone(), "w0", Some(location), None)
            })
            .unwrap();
            let folder = std::env::temp_dir().join("sheprd-pr-confirm");
            let infos = [first.clone(), second.clone()]
                .into_iter()
                .map(|id| TerminalInfo {
                    agent: AgentStatus::Unknown,
                    cwd: Some(folder.clone()),
                    foreground: None,
                    id,
                    labels: Default::default(),
                    name: None,
                    reporter: None,
                    state: RunState::Running,
                    title: None,
                })
                .collect();
            view.core_terminals.show_for_test(tab, panes, false, infos);
            view.core_terminals.place_for_test(tab, scope, "w0");
            view.core_terminals.seed_checkout_for_test(
                folder.to_str().unwrap(),
                Some(Identity {
                    branch: Some("feature".into()),
                    key: folder.join(".git").to_str().unwrap().into(),
                    label: "app".into(),
                    linked: false,
                }),
            );
            view.menu.github = crate::github::Auth::connected_fixture();
            view.show_browser_tab(tab, window, cx);
            view.core_terminals.set_shown_for_test(tab, first);
            let input = view.git_input().unwrap();
            view.git = Git::fixture(input.clone(), Status::default());
            assert!(view.ask_pr_create(window, cx));
            view.core_terminals.set_active(tab, second);
            assert_eq!(view.git_input(), Some(input));
            view.activate_git_row(Row::PullRequest, window, cx);
            assert!(view.menu.page.is_none());
            assert!(view.git.running().is_none());
        })
    });
}
