//! The worker reads Linear's workflow states once, and again only for a new
//! key or Refresh, a move Linear refused, or an issue of a team they lack.

use super::linear::{listing, worker};
use super::*;
use crate::linear::{MOVE, Move, STATES};
use serde_json::json;

static READS: AtomicUsize = AtomicUsize::new(0);

/// A listing of ENG-1 in team `team`.
fn of_team(team: &str) -> Value {
    let mut listing = listing("acme", &["ENG-1"]);
    listing["data"]["issues"]["nodes"][0]["team"] = json!({ "id": team });
    listing
}

fn answer(query: &str, team: &str) -> Result<Value> {
    match query {
        STATES => {
            READS.fetch_add(1, Ordering::SeqCst);
            Ok(json!({ "data": { "teams": { "nodes": [{
                "id": "team-eng",
                "states": { "nodes": [
                    { "id": "s-todo", "name": "Todo", "type": "unstarted", "position": 1.0 },
                ] },
            }] } } }))
        }
        MOVE => Err(Error::LinearStatus {
            status: 400,
            message: Some("Entity not found".into()),
        }),
        _ => Ok(of_team(team)),
    }
}

fn eng(_: &SecretString, query: &str, _: Value) -> Result<Value> {
    answer(query, "team-eng")
}

fn ops(_: &SecretString, query: &str, _: Value) -> Result<Value> {
    answer(query, "team-ops")
}

fn change(state: Option<&str>) -> Move {
    Move {
        issue: "uuid-1".into(),
        identifier: "ENG-1".into(),
        state: state.map(str::to_owned),
        sort_order: None,
        sequence: 1,
    }
}

fn movable(worker: &Worker) -> usize {
    worker.snapshot.items[0]
        .tracker
        .as_ref()
        .map_or(0, |tracker| tracker.states.len())
}

#[test]
fn workflow_states_are_read_once_until_something_changes_them() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = worker(dir.path());
    let reads = || READS.load(Ordering::SeqCst);
    worker.linear_fetch = eng;

    worker.sync(None, Linear::Now);
    worker.sync(None, Linear::Now);
    assert_eq!(reads(), 1);
    assert_eq!(movable(&worker), 1, "the kept states still apply");

    worker.set_linear_key(LinearKey::Found("lin_api_other".into()));
    worker.sync(None, Linear::Now);
    assert_eq!(reads(), 2, "a new key, or Refresh, reads them again");

    assert!(!worker.move_linear_issue(&change(None)));
    worker.sync(None, Linear::Now);
    assert_eq!(reads(), 2, "a refused reorder says nothing of the states");

    assert!(!worker.move_linear_issue(&change(Some("s-gone"))));
    worker.sync(None, Linear::Now);
    assert_eq!(reads(), 3, "a refused state may be gone");

    worker.linear_fetch = ops;
    worker.sync(None, Linear::Now);
    assert_eq!(movable(&worker), 0);
    worker.sync(None, Linear::Now);
    assert_eq!(reads(), 4, "an issue of a team they lack");
}
