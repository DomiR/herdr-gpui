//! The Linear section's board draws Linear's cards: the identifier and
//! parent, the state's mark by the title, and chips for the priority,
//! project, estimate, sub-issue progress, and linked pull requests; a lane
//! heads with its state's mark and its estimates summed.
use super::*;
use crate::orchestrator::{
    LinkState, LinkedPullRequest, Progress, Project, Status, StatusKind, Tracker,
    view::{Event, OrchestratorView, board, linear_parts},
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc};

fn state(name: &str, kind: StatusKind, rank: i64) -> Status {
    Status {
        name: name.into(),
        kind,
        rank,
        color: Some(0xf2c94c),
        id: None,
    }
}

fn issue(id: &str, status: Status, estimate: Option<u32>) -> Item {
    let mut issue = item(Provider::Linear, id, "Task", "2026-10-06T00:00:00Z");
    issue.tracker = Some(Tracker {
        status: Some(status),
        estimate,
        ..Tracker::default()
    });
    issue
}

/// ENG-1 with every chip, in progress beside ENG-2; ENG-3 alone in Todo,
/// estimated by nobody.
fn listing() -> Vec<Item> {
    let doing = state("In Progress", StatusKind::Started, 1000);
    let mut items = vec![
        issue("ENG-1", doing.clone(), Some(2)),
        issue("ENG-2", doing, Some(3)),
        issue("ENG-3", state("Todo", StatusKind::Unstarted, 0), None),
    ];
    items[0].priority = Some(1);
    let tracker = items[0].tracker.as_mut().unwrap();
    tracker.parent = Some("Bug bash".into());
    tracker.project = Some(Project {
        name: "Trumpf AP".into(),
        color: Some(0x5e6ad2),
    });
    tracker.progress = Some(Progress {
        done: 1,
        total: 3,
        more: false,
    });
    tracker.pull_requests = vec![LinkedPullRequest {
        url: "https://github.com/acme/app/pull/10807".into(),
        number: 10807,
        state: Some(LinkState::Open),
    }];
    items
}

fn shows(cx: &mut VisualTestContext, selector: &str) -> bool {
    cx.debug_bounds(selector.to_owned().leak()).is_some()
}

fn on_board(view: &Entity<OrchestratorView>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.layout = board::Layout::Board;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
}

#[test]
fn progress_counts_the_done_and_marks_more_unread() {
    let progress = |done, total, more| Progress { done, total, more };
    assert_eq!(linear_parts::progress_label(progress(1, 3, false)), "1/3");
    assert_eq!(linear_parts::progress_label(progress(4, 10, true)), "4/10+");
}

#[gpui::test]
fn a_card_shows_its_chips_and_a_lane_its_estimates(cx: &mut TestAppContext) {
    let items = listing();
    let key = |index: usize| items[index].key.canonical();
    let (_host, view, cx) = open_linear(cx, items.clone(), 1400.);
    on_board(&view, cx);
    for part in ["priority", "project", "estimate", "progress", "pr"] {
        let selector = format!("orchestrator-card-{part}-{}", key(0));
        assert!(shows(cx, &selector), "{selector}");
    }
    // A card shows only what its issue has; the priority always.
    assert!(shows(cx, &format!("orchestrator-card-priority-{}", key(2))));
    for part in ["project", "estimate", "progress", "pr"] {
        let selector = format!("orchestrator-card-{part}-{}", key(2));
        assert!(!shows(cx, &selector), "{selector}");
    }
    // Todo leads, then In Progress, whose estimates sum; Todo has none.
    assert!(shows(cx, "orchestrator-lane-head-0"));
    assert!(!shows(cx, "orchestrator-lane-estimate-0"));
    assert!(shows(cx, "orchestrator-lane-estimate-1"));
    let lanes = cx.update(|_, cx| view.read(cx).board_lanes());
    let names: Vec<Option<String>> = lanes
        .iter()
        .map(|(status, _)| status.as_ref().map(|status| status.name.clone()))
        .collect();
    assert_eq!(names, [Some("Todo".into()), Some("In Progress".into())]);

    // The pull request chip opens the pull request, not the issue's page;
    // the card opens the page.
    let opened = Rc::new(RefCell::new(Vec::new()));
    let seen = opened.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &Event, _| {
            if let Event::OpenUrl(url) = event {
                seen.borrow_mut().push(url.clone());
            }
        })
        .detach()
    });
    click_on(cx, &format!("orchestrator-card-pr-{}", key(0)));
    assert_eq!(*opened.borrow(), ["https://github.com/acme/app/pull/10807"]);
    assert!(cx.update(|_, cx| view.read(cx).detail.is_none()));
    click_on(cx, &format!("orchestrator-card-estimate-{}", key(1)));
    let open = cx.update(|_, cx| {
        view.read(cx)
            .detail
            .as_ref()
            .map(|detail| detail.key.clone())
    });
    assert_eq!(open, Some(key(1)));
}
