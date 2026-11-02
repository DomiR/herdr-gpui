//! Leaving the Linear section for a worktree and coming back keeps where
//! the list and the board were scrolled, though the section is not drawn
//! in between and the issue's page opens again on the way back.
use super::*;
use crate::orchestrator::{
    Status, StatusKind, Tracker,
    view::{OrchestratorView, board::Layout},
};
use gpui::{
    Bounds, Entity, Modifiers, Pixels, Point, ScrollDelta, ScrollWheelEvent, VisualTestContext,
    point, px,
};
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

/// Sixty issues, fifteen in each of four states: more lanes than a narrow
/// window fits, each longer than it is tall.
fn listing() -> Vec<Item> {
    let team = Arc::new(vec![
        state("Backlog", StatusKind::Backlog, 0),
        state("Todo", StatusKind::Unstarted, 0),
        state("In Progress", StatusKind::Started, 1000),
        state("In Review", StatusKind::Started, 2000),
    ]);
    (0..60)
        .map(|number| {
            let id = format!("ENG-{number}");
            let mut issue = item(Provider::Linear, &id, "Task", "2026-10-06T00:00:00Z");
            issue.tracker = Some(Tracker {
                status: Some(team[number % 4].clone()),
                states: team.clone(),
                ..Tracker::default()
            });
            issue
        })
        .collect()
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> Bounds<Pixels> {
    let selector: &'static str = selector.to_owned().leak();
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"))
}

fn wheel(cx: &mut VisualTestContext, at: Point<Pixels>, delta: Point<Pixels>) {
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Pixels(delta),
        ..Default::default()
    });
    cx.run_until_parked();
}

/// Opens the first issue's page, as Go to worktree does before it leaves.
fn open_page(view: &Entity<OrchestratorView>, cx: &mut VisualTestContext) -> String {
    let key = listing()[0].key.canonical();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.selected = Some(key.clone());
            view.open_detail(key.clone(), cx);
        })
    });
    cx.run_until_parked();
    key
}

/// Stops drawing the section for a frame, then draws it again with the
/// issue `key`'s page reopened, as going to its worktree and coming back
/// does.
fn leave_and_return(
    host: &Entity<Host>,
    view: &Entity<OrchestratorView>,
    key: String,
    cx: &mut VisualTestContext,
) {
    cx.update(|_, cx| {
        host.update(cx, |host, cx| {
            host.shown = false;
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-list").is_none());
    assert!(cx.debug_bounds("orchestrator-board").is_none());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_worktree_issue(Some(key));
            view.detail = None;
            view.return_to_worktree_issue(cx);
        });
        host.update(cx, |host, cx| {
            host.shown = true;
            cx.notify();
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn the_list_keeps_its_scroll_across_a_trip_to_a_worktree(cx: &mut gpui::TestAppContext) {
    let (host, view, cx) = open_linear(cx, listing(), 1200.);
    let key = open_page(&view, cx);
    let list = bounds(cx, "orchestrator-list");
    wheel(cx, list.center(), point(px(0.), px(-400.)));
    // Where each row is drawn, `None` while scrolled out of sight.
    let rows = |cx: &mut VisualTestContext| -> Vec<Option<Point<Pixels>>> {
        listing()
            .iter()
            .map(|issue| {
                let row = format!("orchestrator-row-{}", issue.key.canonical());
                cx.debug_bounds(row.leak()).map(|row| row.origin)
            })
            .collect()
    };
    let offset = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| view.read(cx).scroll.0.borrow().base_handle.offset())
    };
    let scrolled = (offset(cx), rows(cx));
    assert!(scrolled.0.y < px(0.), "{:?}", scrolled.0);
    assert!(scrolled.1.iter().any(Option::is_none));
    leave_and_return(&host, &view, key, cx);
    assert_eq!((offset(cx), rows(cx)), scrolled);
}

#[gpui::test]
fn the_board_keeps_its_scrolls_across_a_trip_to_a_worktree(cx: &mut gpui::TestAppContext) {
    let (host, view, cx) = open_linear(cx, listing(), 1100.);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.layout = Layout::Board;
            cx.notify();
        })
    });
    cx.run_until_parked();
    let key = open_page(&view, cx);
    // Down the first lane, then the row of lanes sideways, from its
    // padding, where no lane takes the wheel.
    let lane = bounds(cx, "orchestrator-lane-0").center();
    wheel(cx, lane, point(px(0.), px(-300.)));
    let board = bounds(cx, "orchestrator-board");
    wheel(
        cx,
        board.origin + point(px(4.), px(4.)),
        point(px(-200.), px(0.)),
    );
    let card = format!("orchestrator-card-{}", listing()[0].key.canonical());
    let at = |cx: &mut VisualTestContext| {
        (
            bounds(cx, "orchestrator-lane-0").origin,
            cx.debug_bounds(card.clone().leak()).map(|card| card.origin),
        )
    };
    let scrolled = at(cx);
    let offsets = cx.update(|_, cx| {
        let scrolls = &view.read(cx).board_scrolls;
        (scrolls.row(), scrolls.lane(Some("Backlog")))
    });
    assert!(offsets.0.x < px(0.), "{offsets:?}");
    assert!(offsets.1.is_some_and(|lane| lane.y < px(0.)), "{offsets:?}");
    leave_and_return(&host, &view, key, cx);
    assert_eq!(at(cx), scrolled);
}
