//! A Linear lane lists its cards in Linear's own order, `sortOrder`, and a
//! card dropped on another card, or under a lane's last, takes the place
//! midway between its new neighbours, shown at once until its worker says
//! the move is done.
use super::*;
use crate::orchestrator::{
    SortOrder, Status, StatusKind, Tracker,
    view::{OrchestratorView, board, board::Layout, moves::sort_order_at},
};
use gpui::{Entity, Modifiers, MouseButton, Pixels, Point, VisualTestContext, point, px};
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

fn issue(id: &str, status: usize, order: Option<f64>) -> Item {
    let team = Arc::new(vec![
        state("Todo", StatusKind::Unstarted, 0),
        state("In Progress", StatusKind::Started, 1000),
    ]);
    let mut issue = item(Provider::Linear, id, "Task", "2026-10-06T00:00:00Z");
    issue.tracker = Some(Tracker {
        status: Some(team[status].clone()),
        id: Some(format!("uuid-{id}")),
        states: team,
        sort_order: order.map(SortOrder),
        ..Tracker::default()
    });
    issue
}

/// Todo holds ENG-1 to ENG-3 by their places, listed out of that order;
/// In Progress holds ENG-4.
fn listing() -> Vec<Item> {
    vec![
        issue("ENG-3", 0, Some(3.)),
        issue("ENG-1", 0, Some(1.)),
        issue("ENG-4", 1, Some(10.)),
        issue("ENG-2", 0, Some(2.)),
    ]
}

fn key(id: &str) -> String {
    issue(id, 0, None).key.canonical()
}

fn lane(keys: &[(&str, Option<f64>)]) -> Vec<(String, Option<SortOrder>)> {
    keys.iter()
        .map(|(id, order)| (key(id), order.map(SortOrder)))
        .collect()
}

#[test]
fn a_card_lands_midway_between_its_new_neighbours() {
    let todo = lane(&[
        ("ENG-1", Some(1.)),
        ("ENG-2", Some(2.)),
        ("ENG-3", Some(3.)),
    ]);
    // ENG-3 above ENG-2: between ENG-1 and ENG-2.
    assert_eq!(
        sort_order_at(&todo, &key("ENG-3"), Some(&key("ENG-2"))),
        Some(SortOrder(1.5))
    );
    // On top, one before the first; last, one after the last, itself left
    // out.
    assert_eq!(
        sort_order_at(&todo, &key("ENG-3"), Some(&key("ENG-1"))),
        Some(SortOrder(0.))
    );
    assert_eq!(
        sort_order_at(&todo, &key("ENG-1"), None),
        Some(SortOrder(4.))
    );
    // From another lane, between two of these.
    assert_eq!(
        sort_order_at(&todo, &key("ENG-9"), Some(&key("ENG-3"))),
        Some(SortOrder(2.5))
    );
}

#[test]
fn a_card_without_neighbours_with_places_keeps_linears() {
    assert_eq!(sort_order_at(&[], &key("ENG-1"), None), None);
    let alone = lane(&[("ENG-1", Some(1.))]);
    assert_eq!(sort_order_at(&alone, &key("ENG-1"), None), None);
    let unordered = lane(&[("ENG-1", Some(1.)), ("ENG-2", None)]);
    assert_eq!(
        sort_order_at(&unordered, &key("ENG-9"), Some(&key("ENG-2"))),
        None
    );
}

#[test]
fn a_lane_follows_linears_order_those_without_one_last() {
    let mut items = listing();
    items.push(issue("ENG-5", 0, None));
    items.push(issue("ENG-0", 0, Some(-1.)));
    let lanes = board::lanes(&items);
    let todo: Vec<&str> = lanes[0]
        .1
        .iter()
        .map(|item| item.identifier.as_str())
        .collect();
    assert_eq!(todo, ["ENG-0", "ENG-1", "ENG-2", "ENG-3", "ENG-5"]);
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> gpui::Bounds<Pixels> {
    let selector: &'static str = selector.to_owned().leak();
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"))
}

fn card(id: &str) -> String {
    format!("orchestrator-card-{}", key(id))
}

/// Drags issue `id`'s card to `to` and lets go.
fn drag(cx: &mut VisualTestContext, id: &str, to: Point<Pixels>) {
    let from = bounds(cx, &card(id)).center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(from.x + px(20.), from.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

fn open_board(cx: &mut gpui::TestAppContext) -> (Entity<OrchestratorView>, &mut VisualTestContext) {
    let (_host, view, cx) = open_linear(cx, listing(), 1400.);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.layout = Layout::Board;
            cx.notify();
        })
    });
    cx.run_until_parked();
    (view, cx)
}

/// Each lane's issues, top to bottom.
fn lanes(view: &Entity<OrchestratorView>, cx: &mut VisualTestContext) -> Vec<Vec<String>> {
    cx.update(|_, cx| {
        let view = view.read(cx);
        view.board_lanes()
            .into_iter()
            .map(|(_, rows)| {
                rows.iter()
                    .map(|row| view.row_item(row).identifier.clone())
                    .collect()
            })
            .collect()
    })
}

#[gpui::test]
fn a_card_dropped_on_another_takes_its_place(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open_board(cx);
    assert_eq!(
        lanes(&view, cx),
        [vec!["ENG-1", "ENG-2", "ENG-3"], vec!["ENG-4"]]
    );
    let to = bounds(cx, &card("ENG-1")).center();
    drag(cx, "ENG-3", to);
    assert!(cx.update(|_, cx| view.read(cx).moving(&key("ENG-3"))));
    assert_eq!(
        lanes(&view, cx),
        [vec!["ENG-3", "ENG-1", "ENG-2"], vec!["ENG-4"]]
    );

    // Linear refused: once the worker says so, the card is back.
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
    assert_eq!(
        lanes(&view, cx),
        [vec!["ENG-1", "ENG-2", "ENG-3"], vec!["ENG-4"]]
    );
}

#[gpui::test]
fn a_card_dropped_under_a_lanes_last_goes_last(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open_board(cx);
    let lane = bounds(cx, "orchestrator-lane-0");
    drag(cx, "ENG-1", point(lane.center().x, lane.bottom() - px(12.)));
    assert_eq!(
        lanes(&view, cx),
        [vec!["ENG-2", "ENG-3", "ENG-1"], vec!["ENG-4"]]
    );
}

#[gpui::test]
fn a_card_dropped_on_one_in_another_lane_moves_there_above_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open_board(cx);
    let to = bounds(cx, &card("ENG-4")).center();
    drag(cx, "ENG-2", to);
    assert_eq!(
        lanes(&view, cx),
        [vec!["ENG-1", "ENG-3"], vec!["ENG-2", "ENG-4"]]
    );
    let placed = cx.update(|_, cx| {
        let view = view.read(cx);
        let tracker = view.item(&key("ENG-2")).unwrap().tracker.clone().unwrap();
        (tracker.status.unwrap().name, tracker.sort_order)
    });
    assert_eq!(placed, ("In Progress".to_owned(), Some(SortOrder(9.))));
}

#[gpui::test]
fn a_card_dropped_where_it_is_stays(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open_board(cx);
    // On the card under it: the place it already has.
    let to = bounds(cx, &card("ENG-2")).center();
    drag(cx, "ENG-1", to);
    assert!(!cx.update(|_, cx| view.read(cx).moving(&key("ENG-1"))));
    assert_eq!(
        lanes(&view, cx),
        [vec!["ENG-1", "ENG-2", "ENG-3"], vec!["ENG-4"]]
    );
}
