//! Mine and Current cycle in the Issues filters' column: offered once a
//! tracker's issues are listed, and shown as chips while the column is
//! folded, as the Linear default turns them on.
use super::*;
use crate::orchestrator::{
    Cycle, Request, Timing, Tracker,
    view::{Look, OrchestratorView, rows::Tab},
};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use std::sync::Arc;

fn tracked(id: &str) -> Item {
    let mut issue = item(Provider::Linear, id, "Task", "2026-10-06T00:00:00Z");
    issue.tracker = Some(Tracker {
        mine: true,
        cycle: Some(Cycle {
            number: 3,
            name: None,
            active: true,
        }),
        ..Tracker::default()
    });
    issue
}

fn open_view(
    cx: &mut TestAppContext,
    items: Vec<Item>,
) -> (Entity<OrchestratorView>, &mut VisualTestContext) {
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.path().join("missing").to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: None,
        data_root: Some(dir.path().join("data")),
        timing: Timing::default(),
        linear: None,
    };
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(600.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().snapshot.items = Arc::new(items);
            view.tab = Tab::Issues;
            view.default_filters();
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    (view, cx)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"));
    cx.simulate_click(at.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn linear_issues_offer_mine_and_cycle_and_show_them_as_chips(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx, vec![tracked("ENG-1")]);
    // The default is on, and folded it reads as chips.
    assert!(cx.debug_bounds("orchestrator-chip-mine").is_some());
    assert!(cx.debug_bounds("orchestrator-chip-cycle").is_some());
    click(cx, "orchestrator-filters");
    assert!(cx.debug_bounds("orchestrator-facet-mine").is_some());
    click(cx, "orchestrator-facet-cycle");
    let filters = cx.update(|_, cx| {
        let view = view.read(cx);
        (view.filters.mine, view.filters.cycle, view.filters_set)
    });
    assert_eq!(filters, (true, false, true));
}

#[gpui::test]
fn untracked_issues_offer_neither(cx: &mut TestAppContext) {
    let issue = item(Provider::Github, "377", "Icons", "2026-10-07T00:00:00Z");
    let (_view, cx) = open_view(cx, vec![issue]);
    assert!(cx.debug_bounds("orchestrator-chip-mine").is_none());
    click(cx, "orchestrator-filters");
    assert!(cx.debug_bounds("orchestrator-facet-run").is_some());
    assert!(cx.debug_bounds("orchestrator-facet-mine").is_none());
    assert!(cx.debug_bounds("orchestrator-facet-cycle").is_none());
}
