//! Hide statuses: a menu of the Linear section's workflow states whose
//! checks hide the issues in them, rows from the list and lanes from the
//! board, with how many are hidden on its button.
use super::*;
use crate::orchestrator::{
    Error, IssueSource, Status, StatusKind, Tracker,
    view::{OrchestratorView, board::Layout, list::Empty},
};
use gpui::{Entity, VisualTestContext};
use std::sync::Arc;

fn state(name: &str, kind: StatusKind, rank: i64) -> Status {
    Status {
        name: name.into(),
        kind,
        rank,
        color: Some(0x4ea7fc),
        id: Some(format!("s-{name}")),
    }
}

/// Two issues in Todo, one in In Progress; their team also has In Review,
/// which none is in, and Canceled, which the board leaves off.
fn listing() -> Vec<Item> {
    let team = Arc::new(vec![
        state("Todo", StatusKind::Unstarted, 0),
        state("In Progress", StatusKind::Started, 1000),
        state("In Review", StatusKind::Started, 2000),
        state("Canceled", StatusKind::Canceled, 0),
    ]);
    [("ENG-1", 0), ("ENG-2", 0), ("ENG-3", 1)]
        .into_iter()
        .map(|(id, status)| {
            let mut issue = item(Provider::Linear, id, "Task", "2026-10-06T00:00:00Z");
            issue.tracker = Some(Tracker {
                status: Some(team[status].clone()),
                states: team.clone(),
                ..Tracker::default()
            });
            issue
        })
        .collect()
}

fn shown(view: &Entity<OrchestratorView>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| view.read(cx).keys())
}

fn shows(cx: &mut VisualTestContext, selector: &str) -> bool {
    cx.debug_bounds(selector.to_owned().leak()).is_some()
}

#[gpui::test]
fn the_menu_lists_every_state_of_the_team_but_canceled(cx: &mut gpui::TestAppContext) {
    let (_host, _view, cx) = open_linear(cx, listing(), 1400.);
    assert!(!shows(cx, "orchestrator-hide-menu"));
    click_on(cx, "orchestrator-hide-statuses");
    for name in ["Todo", "In Progress", "In Review"] {
        assert!(
            shows(cx, &format!("orchestrator-hide-status-{name}")),
            "{name}"
        );
    }
    assert!(!shows(cx, "orchestrator-hide-status-Canceled"));
    // Escape folds it.
    cx.simulate_keystrokes("escape");
    assert!(!shows(cx, "orchestrator-hide-menu"));
}

#[gpui::test]
fn a_hidden_status_leaves_the_list_and_the_board(cx: &mut gpui::TestAppContext) {
    let (_host, view, cx) = open_linear(cx, listing(), 1400.);
    assert!(!shows(cx, "orchestrator-hidden-count"));
    click_on(cx, "orchestrator-hide-statuses");
    click_on(cx, "orchestrator-hide-status-Todo");
    let eng3 = listing()[2].key.canonical();
    assert_eq!(shown(&view, cx), [eng3]);
    // The menu stays open for more, and the button counts what is hidden.
    assert!(shows(cx, "orchestrator-hide-menu"));
    assert!(shows(cx, "orchestrator-hidden-count"));

    // The board drops Todo's lane, and In Review's once that is hidden
    // too, though it was there only for a card to be dragged to.
    click_on(cx, "orchestrator-hide-statuses");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.layout = Layout::Board;
            cx.notify();
        })
    });
    cx.run_until_parked();
    let lanes = |cx: &mut VisualTestContext| -> Vec<Option<String>> {
        cx.update(|_, cx| {
            view.read(cx)
                .board_lanes()
                .into_iter()
                .map(|(status, _)| status.map(|status| status.name))
                .collect()
        })
    };
    assert_eq!(
        lanes(cx),
        [Some("In Progress".to_owned()), Some("In Review".to_owned())]
    );
    assert!(shows(cx, "orchestrator-lane-1"));
    assert!(!shows(cx, "orchestrator-lane-2"));
    click_on(cx, "orchestrator-hide-statuses");
    click_on(cx, "orchestrator-hide-status-In Review");
    assert_eq!(lanes(cx), [Some("In Progress".to_owned())]);

    // Shown again, Todo's issues are back.
    click_on(cx, "orchestrator-hide-status-Todo");
    click_on(cx, "orchestrator-hide-status-In Review");
    assert_eq!(shown(&view, cx).len(), 3);
    assert!(!shows(cx, "orchestrator-hidden-count"));
}

#[gpui::test]
fn hiding_every_issue_says_the_filters_hide_them(cx: &mut gpui::TestAppContext) {
    let (_host, view, cx) = open_linear(cx, listing(), 1400.);
    let empty = cx.update(|_, cx| {
        view.update(cx, |view, _| {
            // Settled, so the list is not still loading.
            view.repo_mut().snapshot.error = Some(Arc::new(Error::NoDataDirectory));
            view.hidden_statuses = ["Todo", "In Progress"].map(String::from).into();
            view.refresh_rows();
            let hidden = (view.keys().len(), view.empty());
            // Only Linear's issues have statuses: elsewhere nothing hides.
            view.issue_source = IssueSource::Repositories;
            view.refresh_rows();
            (hidden, view.empty())
        })
    });
    assert_eq!(empty, ((0, Empty::Filtered), Empty::Nothing));
}
