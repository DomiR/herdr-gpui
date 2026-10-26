//! The worker moving a Linear issue: the listing shows Linear's answer at
//! once, a refusal becomes a notice naming the issue, and either way the
//! snapshot echoes the move's sequence.

use super::linear::{listing, worker};
use super::*;
use crate::orchestrator::{
    SortOrder,
    linear::{MOVE, Move},
};
use serde_json::json;

fn change() -> Move {
    Move {
        issue: "uuid-1".into(),
        identifier: "ENG-1".into(),
        state: Some("s-review".into()),
        sort_order: Some(SortOrder(2.5)),
        sequence: 7,
    }
}

fn moved(_: &SecretString, query: &str, variables: Value) -> Result<Value> {
    if query != MOVE {
        return Ok(listing("acme", &["ENG-1", "ENG-2"]));
    }
    assert_eq!(
        variables,
        json!({ "id": "uuid-1", "input": { "stateId": "s-review", "sortOrder": 2.5 } })
    );
    Ok(json!({ "data": { "issueUpdate": {
        "success": true,
        "issue": { "sortOrder": 2.5, "state": {
            "id": "s-review", "name": "In Review", "type": "started", "position": 3.0,
        } },
    } } }))
}

fn refused(_: &SecretString, query: &str, _: Value) -> Result<Value> {
    if query != MOVE {
        return Ok(listing("acme", &["ENG-1", "ENG-2"]));
    }
    Err(Error::LinearStatus {
        status: 400,
        message: Some("Entity not found".into()),
    })
}

/// Each listed issue's identifier and status.
fn statuses(worker: &Worker) -> Vec<(String, String)> {
    let mut statuses: Vec<(String, String)> = worker
        .snapshot
        .items
        .iter()
        .map(|item| {
            let status = item
                .tracker
                .as_ref()
                .and_then(|tracker| tracker.status.as_ref());
            (
                item.identifier.clone(),
                status.map(|status| status.name.clone()).unwrap_or_default(),
            )
        })
        .collect();
    statuses.sort();
    statuses
}

fn notices(worker: &Worker) -> Vec<Notice> {
    worker.notices.lock().unwrap().drain(..).collect()
}

#[test]
fn a_move_shows_linears_answer_before_the_next_sync() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = worker(dir.path());
    worker.linear_fetch = moved;
    worker.sync(None);
    assert!(worker.move_linear_issue(&change()));
    assert_eq!(worker.snapshot.linear_moved, 7);
    assert_eq!(
        statuses(&worker),
        [
            ("ENG-1".to_owned(), "In Review".to_owned()),
            ("ENG-2".to_owned(), "Todo".to_owned())
        ]
    );
    let item = worker
        .snapshot
        .items
        .iter()
        .find(|item| item.identifier == "ENG-1")
        .unwrap();
    assert_eq!(item.state, "in review");
    assert_eq!(
        item.tracker.as_ref().and_then(|tracker| tracker.sort_order),
        Some(SortOrder(2.5))
    );
    assert!(notices(&worker).is_empty());
    // What the worker publishes says so too.
    let published = worker.mailbox.lock().unwrap().take().unwrap();
    assert_eq!(published.linear_moved, 7);
}

#[test]
fn a_refused_move_names_the_issue_and_leaves_the_listing() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = worker(dir.path());
    worker.linear_fetch = refused;
    worker.sync(None);
    assert!(!worker.move_linear_issue(&change()));
    assert_eq!(worker.snapshot.linear_moved, 7);
    assert_eq!(
        statuses(&worker),
        [
            ("ENG-1".to_owned(), "Todo".to_owned()),
            ("ENG-2".to_owned(), "Todo".to_owned())
        ]
    );
    let notices = notices(&worker);
    let [
        Notice {
            outcome: Err(error),
        },
    ] = notices.as_slice()
    else {
        panic!("one failure");
    };
    let Error::LinearMove { issue, source } = &**error else {
        panic!("{error:?}");
    };
    assert_eq!(issue, "ENG-1");
    assert!(matches!(**source, Error::LinearStatus { status: 400, .. }));
    assert_eq!(
        error.to_string(),
        "Could not move ENG-1: Linear request failed with HTTP 400: Entity not found"
    );
}

#[test]
fn a_move_without_a_key_asks_to_connect_linear() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = worker(dir.path());
    worker.set_linear_key(LinearKey::Missing);
    assert!(!worker.move_linear_issue(&change()));
    let notices = notices(&worker);
    let [
        Notice {
            outcome: Err(error),
        },
    ] = notices.as_slice()
    else {
        panic!("one failure");
    };
    assert!(matches!(
        &**error,
        Error::LinearMove { source, .. } if matches!(**source, Error::LinearKeyMissing)
    ));
    assert_eq!(worker.snapshot.linear_moved, 7);
}
