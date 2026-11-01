//! Dragging a Linear card to another lane moves the issue there at once,
//! until its worker says the move is done; and Go to worktree on a card or
//! row opens the issue's newest run, remembering the issue to come back to.
use super::*;
use crate::orchestrator::{
    Event, IssueSource, Request, Service, Status, StatusKind, Timing, Tracker,
    view::{Look, OrchestratorView, board, rows::Tab},
};
use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn state(name: &str, kind: StatusKind, rank: i64) -> Status {
    Status {
        name: name.into(),
        kind,
        rank,
        color: Some(0x4ea7fc),
        id: Some(format!("s-{name}")),
    }
}

/// One team's states: Todo, In Progress, In Review, and Canceled, which
/// the board leaves off.
fn team() -> Arc<Vec<Status>> {
    Arc::new(vec![
        state("Todo", StatusKind::Unstarted, 0),
        state("In Progress", StatusKind::Started, 1000),
        state("In Review", StatusKind::Started, 2000),
        state("Canceled", StatusKind::Canceled, 0),
    ])
}

fn issue(id: &str, status: &str, states: &Arc<Vec<Status>>) -> Item {
    let mut issue = item(Provider::Linear, id, "Task", "2026-10-06T00:00:00Z");
    issue.tracker = Some(Tracker {
        status: states.iter().find(|state| state.name == status).cloned(),
        id: Some(format!("uuid-{id}")),
        states: states.clone(),
        ..Tracker::default()
    });
    issue
}

fn listing() -> Vec<Item> {
    let team = team();
    vec![
        issue("ENG-1", "Todo", &team),
        issue("ENG-2", "Todo", &team),
        issue("ENG-3", "In Progress", &team),
    ]
}

fn key(id: &str) -> String {
    listing()
        .into_iter()
        .find(|item| item.identifier == id)
        .unwrap()
        .key
        .canonical()
}

fn open_view(cx: &mut TestAppContext) -> (Entity<OrchestratorView>, &mut VisualTestContext) {
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
            // A worker that takes every move and answers none, so the test
            // says when each is done.
            view.repo_mut().service = Some(Service::unserved());
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

fn bounds(cx: &mut VisualTestContext, selector: &str) -> gpui::Bounds<gpui::Pixels> {
    // Test selectors are few; leaking them gives the `'static` lookups need.
    let selector: &'static str = selector.to_owned().leak();
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"))
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    let at = bounds(cx, selector).center();
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
}

/// Drags issue `id`'s card onto lane `lane` and lets go.
fn drag(cx: &mut VisualTestContext, id: &str, lane: usize) {
    let from = bounds(cx, &format!("orchestrator-card-{}", key(id))).center();
    let to = bounds(cx, &format!("orchestrator-lane-{lane}")).center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        gpui::point(from.x + gpui::px(20.), from.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

fn status_of(view: &Entity<OrchestratorView>, cx: &mut VisualTestContext, id: &str) -> String {
    cx.update(|_, cx| {
        let view = view.read(cx);
        let item = view.item(&key(id)).unwrap();
        board::status_of(item).unwrap().name.clone()
    })
}

#[test]
fn the_board_offers_every_state_of_the_team_but_canceled() {
    let items = listing();
    let mut lanes = board::lanes(&items);
    board::add_empty_lanes(&mut lanes, &items);
    let names: Vec<(&str, usize)> = lanes
        .iter()
        .map(|(status, items)| (status.unwrap().name.as_str(), items.len()))
        .collect();
    assert_eq!(names, [("Todo", 2), ("In Progress", 1), ("In Review", 0)]);
}

#[gpui::test]
fn a_dropped_card_shows_in_its_new_lane_until_its_worker_answers(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    click(cx, "orchestrator-layout-board");
    // Lanes: Todo, In Progress, and In Review, which no issue is in yet.
    drag(cx, "ENG-2", 2);
    assert!(cx.update(|_, cx| view.read(cx).moving(&key("ENG-2"))));
    assert_eq!(status_of(&view, cx, "ENG-2"), "In Review");
    assert_eq!(status_of(&view, cx, "ENG-1"), "Todo");

    // Linear refused: the worker's listing still has the issue in Todo, and
    // once it says the move is done the card is back there.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let repo = view.repo_mut();
            repo.snapshot.items = Arc::new(listing());
            repo.snapshot.linear_moved = 1;
            view.settle_moves();
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| view.read(cx).moving(&key("ENG-2"))));
    assert_eq!(status_of(&view, cx, "ENG-2"), "Todo");
}

#[gpui::test]
fn a_listing_read_before_the_move_finished_does_not_undo_it(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    click(cx, "orchestrator-layout-board");
    drag(cx, "ENG-3", 0);
    assert_eq!(status_of(&view, cx, "ENG-3"), "Todo");
    // A sync the worker published before it moved the issue.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().snapshot.items = Arc::new(listing());
            view.settle_moves();
            view.refresh_rows();
            cx.notify();
        })
    });
    assert!(cx.update(|_, cx| view.read(cx).moving(&key("ENG-3"))));
    assert_eq!(status_of(&view, cx, "ENG-3"), "Todo");
}

#[gpui::test]
fn a_card_dropped_in_its_own_lane_stays(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    click(cx, "orchestrator-layout-board");
    drag(cx, "ENG-1", 0);
    assert!(!cx.update(|_, cx| view.read(cx).moving(&key("ENG-1"))));
    assert_eq!(status_of(&view, cx, "ENG-1"), "Todo");
}

#[gpui::test]
fn a_move_no_worker_takes_says_so_and_stays_put(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    cx.update(|_, cx| view.update(cx, |view, _| view.repo_mut().service = None));
    click(cx, "orchestrator-layout-board");
    drag(cx, "ENG-1", 1);
    assert!(!cx.update(|_, cx| view.read(cx).moving(&key("ENG-1"))));
    assert_eq!(status_of(&view, cx, "ENG-1"), "Todo");
    let notice = cx.update(|_, cx| view.read(cx).notice_text()).unwrap();
    assert!(notice.starts_with("Could not move ENG-1: "), "{notice}");
}

/// ENG-3 with a run in its own Herdr workspace.
fn with_run(view: &Entity<OrchestratorView>, cx: &mut VisualTestContext) {
    let issue = listing().remove(2);
    let run = run("r1", &issue, RunState::Running, "2026-10-06T01:00:00Z");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().recorded_runs = Arc::new(vec![run]);
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
}

fn opened(view: &Entity<OrchestratorView>, cx: &mut VisualTestContext) -> Rc<RefCell<Vec<String>>> {
    let opened = Rc::new(RefCell::new(Vec::new()));
    let seen = opened.clone();
    cx.update(|_, cx| {
        cx.subscribe(view, move |_, event: &Event, _| {
            if let Event::OpenWorkspace { workspace_id, .. } = event {
                seen.borrow_mut().push(workspace_id.clone());
            }
        })
        .detach()
    });
    opened
}

fn go_to_worktree(cx: &mut VisualTestContext, hover: &str) {
    let over = bounds(cx, hover).center();
    cx.simulate_mouse_move(over, None, Modifiers::none());
    cx.run_until_parked();
    click(cx, &format!("orchestrator-worktree-{}", key("ENG-3")));
}

#[gpui::test]
fn go_to_worktree_on_a_row_opens_the_issues_run(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    with_run(&view, cx);
    let opened = opened(&view, cx);
    // Only an issue with a run has the button.
    let none: &'static str = format!("orchestrator-worktree-{}", key("ENG-1")).leak();
    assert!(cx.debug_bounds(none).is_none());
    go_to_worktree(cx, &format!("orchestrator-row-{}", key("ENG-3")));
    assert_eq!(*opened.borrow(), ["w-r1"]);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.worktree_issue(), Some(key("ENG-3").as_str()));
        // Its page is open, for the way back to find.
        assert_eq!(
            view.detail.as_ref().map(|detail| detail.key.clone()),
            Some(key("ENG-3"))
        );
    });
}

#[gpui::test]
fn go_to_worktree_on_a_card_opens_the_issues_run(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    with_run(&view, cx);
    click(cx, "orchestrator-layout-board");
    let opened = opened(&view, cx);
    go_to_worktree(cx, &format!("orchestrator-card-{}", key("ENG-3")));
    assert_eq!(*opened.borrow(), ["w-r1"]);
    // Coming back reopens the page and forgets the issue.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.detail = None;
            view.return_to_worktree_issue(cx);
        })
    });
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.detail.as_ref().map(|detail| detail.key.clone()),
            Some(key("ENG-3"))
        );
        assert_eq!(view.worktree_issue(), None);
    });
}
