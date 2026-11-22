use super::super::pr::create;
use super::*;
use crate::{
    browser::{Location, Store, TabId},
    core_terminals::panes::{Axis, Panes},
    git::{Git, Status},
    menu::Page,
};
use sheprd_backend::{RunState, TerminalId, TerminalInfo};
use sheprd_client::protocol::AgentStatus;
use sheprd_vcs::git::Identity;
use std::path::PathBuf;

fn terminal(name: &str) -> TerminalId {
    TerminalId::new(name).unwrap()
}

fn info(name: &str) -> TerminalInfo {
    TerminalInfo {
        agent: AgentStatus::Unknown,
        cwd: Some(std::env::temp_dir().join("sheprd-pr-create").join(name)),
        foreground: None,
        id: terminal(name),
        labels: Default::default(),
        name: None,
        reporter: None,
        state: RunState::Running,
        title: None,
    }
}

fn tab(view: &mut SheprdWindow, names: &[&str], cx: &mut gpui::Context<SheprdWindow>) -> TabId {
    let scope = crate::browser::scope(&view.endpoints[0]);
    let mut panes = Panes::single(terminal(names[0]));
    for name in &names[1..] {
        panes
            .split(&terminal(names[0]), Axis::Row, terminal(name))
            .unwrap();
    }
    let location = Location::core_terminal(panes.clone(), false);
    let tab = Store::update(cx, |store| {
        store.open(scope.clone(), "w0", Some(location), None)
    })
    .unwrap();
    let infos: Vec<_> = names.iter().map(|name| info(name)).collect();
    for info in &infos {
        let folder = info.cwd.as_ref().unwrap().to_str().unwrap();
        view.core_terminals.seed_checkout_for_test(
            folder,
            Some(Identity {
                branch: Some("feature".into()),
                key: PathBuf::from(folder)
                    .join(".git")
                    .to_string_lossy()
                    .into_owned(),
                label: "app".into(),
                linked: false,
            }),
        );
    }
    view.core_terminals.show_for_test(tab, panes, false, infos);
    view.core_terminals.place_for_test(tab, scope, "w0");
    tab
}

fn show(
    view: &mut SheprdWindow,
    tab: TabId,
    name: &str,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<SheprdWindow>,
) {
    view.core_terminals.set_active(tab, terminal(name));
    view.show_browser_tab(tab, window, cx);
    view.core_terminals.set_shown_for_test(tab, terminal(name));
    view.git = Git::fixture(view.git_input().unwrap(), Status::default());
}

fn from_core(name: &str) -> Caller {
    Caller {
        terminal_id: Some(name.into()),
        ..caller(None)
    }
}

#[gpui::test]
fn pr_create_refuses_a_background_tab_in_the_same_workspace(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.github = crate::github::Auth::connected_fixture();
            let background = tab(view, &["t1"], cx);
            let foreground = tab(view, &["t2"], cx);
            show(view, foreground, "t2", window, cx);
            assert_eq!(Place::of(view, &from_core("t1")).unwrap().workspace, "w0");
            assert_eq!(
                error_code(&create(view, &from_core("t1"), window, cx)),
                Some(ErrorCode::InvalidRequest)
            );
            assert!(view.menu.page.is_none());
            assert!(view.git.running().is_none());
            show(view, background, "t1", window, cx);
            assert!(matches!(
                create(view, &from_core("t1"), window, cx),
                Response::Done { .. }
            ));
            assert_eq!(view.menu.page, Some(Page::Git));
            assert!(view.git.running().is_none());
        })
    });
}

#[gpui::test]
fn pr_create_refuses_a_background_split_and_a_stale_focused_checkout(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.github = crate::github::Auth::connected_fixture();
            let split = tab(view, &["t1", "t2"], cx);
            show(view, split, "t2", window, cx);
            assert_eq!(
                error_code(&create(view, &from_core("t1"), window, cx)),
                Some(ErrorCode::InvalidRequest)
            );
            view.core_terminals.set_active(split, terminal("t1"));
            assert_eq!(
                error_code(&create(view, &from_core("t1"), window, cx)),
                Some(ErrorCode::InvalidRequest)
            );
            assert!(view.menu.page.is_none());
            assert!(view.git.running().is_none());
            show(view, split, "t1", window, cx);
            assert!(matches!(
                create(view, &from_core("t1"), window, cx),
                Response::Done { .. }
            ));
            assert!(view.git.running().is_none());
        })
    });
}
