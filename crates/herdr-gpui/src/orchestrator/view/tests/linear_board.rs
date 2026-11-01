//! The Linear section's statuses: a pill on each row, a Status filter, and
//! the board, a lane per workflow state in the team's order.
use super::*;
use crate::orchestrator::{
    IssueSource, Request, Status, StatusKind, Timing, Tracker,
    view::{Look, OrchestratorView, board, rows::Tab},
};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use std::sync::Arc;

fn status(name: &str, kind: StatusKind, rank: i64) -> Status {
    Status {
        name: name.into(),
        kind,
        rank,
        color: Some(0x4ea7fc),
        id: None,
    }
}

fn issue(id: &str, status: Option<Status>) -> Item {
    let mut issue = item(Provider::Linear, id, "Task", "2026-10-06T00:00:00Z");
    issue.tracker = Some(Tracker {
        status,
        ..Tracker::default()
    });
    issue
}

fn listing() -> Vec<Item> {
    vec![
        issue(
            "ENG-1",
            Some(status("In Review", StatusKind::Started, 2000)),
        ),
        issue("ENG-2", Some(status("Todo", StatusKind::Unstarted, 0))),
        issue(
            "ENG-3",
            Some(status("In Progress", StatusKind::Started, 1000)),
        ),
        issue("ENG-4", None),
        issue("ENG-5", Some(status("Todo", StatusKind::Unstarted, 0))),
        issue("ENG-6", Some(status("Backlog", StatusKind::Backlog, 0))),
    ]
}

#[test]
fn lanes_follow_the_kind_then_the_teams_order() {
    let items = listing();
    let lanes = board::lanes(&items);
    let names: Vec<Option<&str>> = lanes
        .iter()
        .map(|(status, _)| status.map(|status| status.name.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            Some("Backlog"),
            Some("Todo"),
            Some("In Progress"),
            Some("In Review"),
            None
        ]
    );
    let todo: Vec<&str> = lanes[1]
        .1
        .iter()
        .map(|item| item.identifier.as_str())
        .collect();
    assert_eq!(todo, ["ENG-2", "ENG-5"]);
}

pub(super) fn open_view(
    cx: &mut TestAppContext,
) -> (Entity<OrchestratorView>, &mut VisualTestContext) {
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.path().join("missing").to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: None,
        data_root: Some(dir.path().join("data")),
        gitlab_store: Default::default(),
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
    cx.simulate_resize(gpui::size(gpui::px(1400.), gpui::px(700.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().snapshot.items = Arc::new(listing());
            view.tab = Tab::Issues;
            view.set_issue_source(IssueSource::Linear, cx);
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    (view, cx)
}

pub(super) fn click(cx: &mut VisualTestContext, selector: &str) {
    // Test selectors are few; leaking them gives the `'static` lookups need.
    let selector: &'static str = selector.to_owned().leak();
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"));
    cx.simulate_click(at.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn the_status_filter_keeps_one_workflow_state(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    click(cx, "orchestrator-filters");
    click(cx, "orchestrator-facet-status-Todo");
    let keys = cx.update(|_, cx| view.read(cx).keys());
    assert_eq!(keys.len(), 2);
    // Folded, the filter shows as a chip that clears it.
    click(cx, "orchestrator-filters");
    click(cx, "orchestrator-chip-status");
    let keys = cx.update(|_, cx| view.read(cx).keys());
    assert_eq!(keys.len(), listing().len());
}

#[gpui::test]
fn the_board_shows_a_lane_per_status_and_a_card_opens_its_page(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    assert!(cx.debug_bounds("orchestrator-board").is_none());
    click(cx, "orchestrator-layout-board");
    assert!(cx.debug_bounds("orchestrator-board").is_some());
    assert!(cx.debug_bounds("orchestrator-lane-4").is_some());
    assert!(cx.debug_bounds("orchestrator-lane-5").is_none());
    let key = listing()[2].key.canonical();
    click(cx, &format!("orchestrator-card-{key}"));
    let open = cx.update(|_, cx| {
        view.read(cx)
            .detail
            .as_ref()
            .map(|detail| detail.key.clone())
    });
    assert_eq!(open, Some(key));
    // Issues has no statuses to lay out, so it stays a list.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_issue_source(IssueSource::Repositories, cx)
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-board").is_none());
    assert!(cx.debug_bounds("orchestrator-layout-board").is_none());
}

#[gpui::test]
fn the_linear_section_names_no_projects(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    // A project picked in Issues stays picked, but never hides Linear's issues.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.projects.insert("elsewhere".into());
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| view.read(cx).keys().len()),
        listing().len()
    );
    assert!(cx.debug_bounds("orchestrator-pills").is_none());
}
