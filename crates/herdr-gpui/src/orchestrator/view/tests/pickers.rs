//! The Project and Author pickers: a menu per facet, most items first,
//! narrowed by typing, closed by Escape, its button, or a click elsewhere.
use super::*;
use crate::orchestrator::{
    PullRequest, Request, Timing,
    view::{Look, OrchestratorView},
};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use std::{collections::HashSet, sync::Arc};

fn pull(number: u64, author: &str) -> Item {
    let updated = format!("2026-10-{:02}T00:00:00Z", number);
    let mut pull = item(
        Provider::Github,
        &format!("pr/{number}"),
        "Change",
        &updated,
    );
    pull.identifier = format!("#{number}");
    pull.author = Some(author.into());
    pull.pull_request = Some(PullRequest {
        number,
        additions: None,
        deletions: None,
        base_ref: "main".into(),
        head_ref: format!("b-{number}"),
        base_sha: String::new(),
        head_sha: String::new(),
        head_repository: None,
    });
    pull
}

/// The view on the Pull requests list, and the directory its data is in,
/// which must outlive it.
fn open_view(
    cx: &mut TestAppContext,
) -> (
    Entity<OrchestratorView>,
    &mut VisualTestContext,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().unwrap();
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let request = Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.path().join("app").to_string_lossy().into_owned(),
        workspace_id: "app".into(),
        token: None,
        data_root: Some(dir.path().join("data")),
        gitlab_store: Default::default(),
        timing: Timing::default(),
        linear: None,
    };
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(600.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repos[0].snapshot.items = Arc::new(vec![
                pull(1, "amy"),
                pull(2, "kai"),
                pull(3, "leo"),
                pull(4, "kai"),
                pull(5, "bob"),
                pull(6, "leo"),
                pull(7, "kai"),
            ]);
            view.tab = Tab::PullRequests;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    (view, cx, dir)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"))
        .center();
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
}

fn top(cx: &mut VisualTestContext, selector: &'static str) -> gpui::Pixels {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"))
        .top()
}

#[gpui::test]
fn the_author_menu_lists_people_most_pull_requests_first(cx: &mut TestAppContext) {
    let (view, cx, _dir) = open_view(cx);
    assert!(cx.debug_bounds("orchestrator-picker-menu").is_none());
    click(cx, "orchestrator-picker-author");
    assert!(cx.debug_bounds("orchestrator-picker-menu").is_some());
    // Kai opened three, Leo two; Amy and Bob one each, by name.
    let order = [
        top(cx, "orchestrator-pick-author-kai"),
        top(cx, "orchestrator-pick-author-leo"),
        top(cx, "orchestrator-pick-author-amy"),
        top(cx, "orchestrator-pick-author-bob"),
    ];
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
    // Picks keep the menu open, so several can be checked in a row.
    click(cx, "orchestrator-pick-author-amy");
    click(cx, "orchestrator-pick-author-bob");
    assert!(cx.debug_bounds("orchestrator-picker-menu").is_some());
    assert_eq!(
        cx.update(|_, cx| view.read(cx).authors.clone()),
        HashSet::from(["amy".to_owned(), "bob".to_owned()])
    );
    assert_eq!(cx.update(|_, cx| view.read(cx).keys().len()), 2);
    // Each pick shows as a chip beside the button.
    assert!(cx.debug_bounds("orchestrator-picked-author-amy").is_some());
    assert!(cx.debug_bounds("orchestrator-picked-author-bob").is_some());
}

#[gpui::test]
fn typing_narrows_the_menu_and_enter_checks_the_first_left(cx: &mut TestAppContext) {
    let (view, cx, _dir) = open_view(cx);
    click(cx, "orchestrator-picker-author");
    // The field has the keyboard as the menu opens.
    cx.simulate_input("LE");
    assert!(cx.debug_bounds("orchestrator-pick-author-leo").is_some());
    assert!(cx.debug_bounds("orchestrator-pick-author-kai").is_none());
    // The list's own keys stay the field's while it is typed in: "LEj"
    // names no one.
    cx.simulate_keystrokes("j");
    assert!(cx.debug_bounds("orchestrator-pick-author-leo").is_none());
    cx.simulate_keystrokes("backspace enter");
    assert_eq!(
        cx.update(|_, cx| view.read(cx).authors.clone()),
        HashSet::from(["leo".to_owned()])
    );
    // Escape shuts the menu; the next open starts unnarrowed.
    cx.simulate_keystrokes("escape");
    assert!(cx.debug_bounds("orchestrator-picker-menu").is_none());
    click(cx, "orchestrator-picker-author");
    assert!(cx.debug_bounds("orchestrator-pick-author-kai").is_some());
}

#[gpui::test]
fn a_typed_name_no_one_has_says_so(cx: &mut TestAppContext) {
    let (view, cx, _dir) = open_view(cx);
    click(cx, "orchestrator-picker-author");
    cx.simulate_input("zed");
    cx.simulate_keystrokes("enter");
    assert!(cx.update(|_, cx| view.read(cx).authors.is_empty()));
    assert!(cx.debug_bounds("orchestrator-pick-author-kai").is_none());
}

#[gpui::test]
fn the_open_menus_button_or_a_click_elsewhere_shuts_it(cx: &mut TestAppContext) {
    let (view, cx, _dir) = open_view(cx);
    click(cx, "orchestrator-picker-author");
    click(cx, "orchestrator-picker-author");
    assert!(cx.debug_bounds("orchestrator-picker-menu").is_none());
    click(cx, "orchestrator-picker-author");
    assert!(cx.debug_bounds("orchestrator-picker-menu").is_some());
    let below = cx
        .debug_bounds("orchestrator-picker-menu")
        .unwrap()
        .bottom_right()
        + gpui::point(gpui::px(40.), gpui::px(40.));
    cx.simulate_click(below, Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-picker-menu").is_none());
    assert!(cx.update(|_, cx| view.read(cx).authors.is_empty()));
}
