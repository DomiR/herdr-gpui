//! Moving a Linear issue: the mutation asked for, and what its answer says.
use super::*;
use crate::linear::{self, MOVE, Move, Placed, QUERY, STATES};
use serde_json::{Value, json};
use std::cell::RefCell;

fn change() -> Move {
    Move {
        issue: "uuid-12".into(),
        identifier: "ENG-12".into(),
        state: Some("s-review".into()),
        sort_order: None,
        sequence: 1,
    }
}

fn answer(success: bool) -> Value {
    json!({ "data": { "issueUpdate": {
        "success": success,
        "issue": { "sortOrder": 4.5, "state": {
            "id": "s-review", "name": "In Review", "type": "started",
            "position": 3.0, "color": "#0F783C",
        } },
    } } })
}

#[test]
fn a_move_sets_the_issues_state_and_reads_where_it_is() {
    let asked = RefCell::new(Vec::new());
    let placed = linear::move_issue(
        |query, variables| {
            asked.borrow_mut().push((query, variables));
            Ok(answer(true))
        },
        &change(),
    )
    .unwrap();
    // Only the state changes: Linear keeps the issue's place.
    assert_eq!(
        *asked.borrow(),
        [(
            MOVE,
            json!({ "id": "uuid-12", "input": { "stateId": "s-review" } })
        )]
    );
    assert_eq!(
        placed,
        Placed {
            status: Status {
                name: "In Review".into(),
                kind: StatusKind::Started,
                rank: 3000,
                color: Some(0x0F783C),
                id: Some("s-review".into()),
            },
            sort_order: Some(SortOrder(4.5)),
        }
    );
}

#[test]
fn a_move_within_a_lane_sends_only_its_place() {
    let asked = RefCell::new(Vec::new());
    let change = Move {
        state: None,
        sort_order: Some(SortOrder(4.5)),
        ..change()
    };
    linear::move_issue(
        |query, variables| {
            asked.borrow_mut().push((query, variables));
            Ok(answer(true))
        },
        &change,
    )
    .unwrap();
    assert_eq!(
        *asked.borrow(),
        [(
            MOVE,
            json!({ "id": "uuid-12", "input": { "sortOrder": 4.5 } })
        )]
    );
}

#[test]
fn a_move_to_a_place_in_another_lane_sends_both() {
    let change = Move {
        sort_order: Some(SortOrder(-1.0)),
        ..change()
    };
    assert_eq!(
        change.input(),
        json!({ "stateId": "s-review", "sortOrder": -1.0 })
    );
}

#[test]
fn a_move_linear_did_not_make_fails() {
    let refused = linear::move_issue(|_, _| Ok(answer(false)), &change());
    assert!(matches!(refused, Err(Error::Output { .. })), "{refused:?}");
    let rejected = linear::move_issue(|_, _| Err(Error::LinearKeyRejected), &change());
    assert!(matches!(rejected, Err(Error::LinearKeyRejected)));
}

#[test]
fn a_failed_move_names_the_issue_and_keeps_why() {
    let error = Error::LinearMove {
        issue: "ENG-12".into(),
        source: std::sync::Arc::new(Error::LinearRateLimited),
    };
    assert_eq!(
        error.to_string(),
        "Could not move ENG-12: Linear is limiting requests; the list syncs again later"
    );
    let source = std::error::Error::source(&error).unwrap();
    assert_eq!(source.to_string(), Error::LinearRateLimited.to_string());
}

/// The listing's issues, each a node of team `team-eng` in Todo.
fn page() -> Value {
    json!({ "data": {
        "organization": { "urlKey": "acme" },
        "issues": {
            "pageInfo": { "hasNextPage": false, "endCursor": null },
            "nodes": [{
                "id": "uuid-1", "identifier": "ENG-1", "title": "Task", "description": null,
                "url": "https://linear.app/acme/issue/ENG-1", "priority": 0, "branchName": "",
                "createdAt": "2026-10-01T00:00:00.000Z", "updatedAt": "2026-10-02T00:00:00.000Z",
                "state": { "id": "s-todo", "name": "Todo", "type": "unstarted", "position": 0.0 },
                "team": { "id": "team-eng" },
                "creator": null, "assignee": null, "cycle": null, "parent": null,
                "labels": { "nodes": [] }, "inverseRelations": { "nodes": [] },
            }],
        },
    } })
}

#[test]
fn the_listing_still_shows_when_the_states_cannot_be_read() {
    let listing = linear::sync(
        |query, _| match query {
            STATES => Err(Error::LinearStatus {
                status: 400,
                message: Some("Query too complex".into()),
            }),
            QUERY => Ok(page()),
            other => panic!("unexpected query {other}"),
        },
        || false,
    )
    .unwrap();
    // Listed twice, once per scope, and kept once.
    assert_eq!(listing.items.len(), 1);
    let tracker = listing.items[0].tracker.as_ref().unwrap();
    assert_eq!(tracker.id.as_deref(), Some("uuid-1"));
    assert!(tracker.states.is_empty());
    assert_eq!(
        tracker
            .status
            .as_ref()
            .and_then(|status| status.id.as_deref()),
        Some("s-todo")
    );
}

#[test]
fn a_cancelled_states_read_stops_the_listing() {
    let result = linear::sync(|_, _| Err(Error::Cancelled), || false);
    assert!(matches!(result, Err(Error::Cancelled)));
}
