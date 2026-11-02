//! The worker's Linear source against a real database, with Linear's answers
//! injected: no network, no keychain, no `LINEAR_API_KEY`.

use super::*;
use crate::{Repository, store::Store};
use serde_json::json;

fn issue(identifier: &str) -> Value {
    json!({
        "identifier": identifier, "title": "Task", "description": null,
        "url": format!("https://linear.app/team/issue/{identifier}"),
        "priority": 3, "branchName": "", "createdAt": "2026-10-01T00:00:00.000Z",
        "updatedAt": "2026-10-02T00:00:00.000Z", "state": { "name": "Todo" },
        "creator": null, "assignee": null, "cycle": null, "parent": null,
        "labels": { "nodes": [] }, "inverseRelations": { "nodes": [] },
    })
}

pub(super) fn listing(workspace: &str, issues: &[&str]) -> Value {
    json!({ "data": {
        "organization": { "urlKey": workspace },
        "issues": {
            "pageInfo": { "hasNextPage": false, "endCursor": null },
            "nodes": issues.iter().map(|id| issue(id)).collect::<Vec<_>>(),
        },
    } })
}

fn acme(_: &SecretString, _: &str, _: Value) -> Result<Value> {
    Ok(listing("acme", &["ENG-1", "ENG-2"]))
}

fn beta(_: &SecretString, _: &str, _: Value) -> Result<Value> {
    Ok(listing("beta", &["OPS-3"]))
}

fn rejected(_: &SecretString, _: &str, _: Value) -> Result<Value> {
    Err(Error::LinearKeyRejected)
}

fn unreachable(_: &SecretString, _: &str, _: Value) -> Result<Value> {
    panic!("Linear was asked while it waits out a failure")
}

/// A worker on a repository with no remote and no Beads, so Linear is its
/// only source.
pub(super) fn worker(dir: &std::path::Path) -> Worker {
    let request = Request {
        target: ConnectTarget::Local,
        checkout: dir.to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: None,
        data_root: None,
        gitlab_store: Default::default(),
        timing: Timing::default(),
        linear: Some(herdr_credentials::Store::Environment),
    };
    let (commands, _) = mpsc::sync_channel(COMMANDS);
    let mut worker = Worker::new(request, commands);
    let info = RepoInfo {
        repository: Repository::Local(dir.to_owned()),
        main_root: dir.to_string_lossy().into_owned(),
        beads: false,
        remote: None,
    };
    worker.snapshot.sources = sources(&info, worker.linear_source())
        .into_iter()
        .map(|(key, _)| SourceStatus {
            key,
            state: SyncState::Syncing { count: None },
        })
        .collect();
    worker.snapshot.repo = Some(info);
    worker.store = Some(Store::open(&dir.join("data").join("state.sqlite3")).unwrap());
    worker.host = Some(Host::new(&ConnectTarget::Local).unwrap());
    worker.set_linear_key(LinearKey::Found("lin_api_test".into()));
    worker
}

fn status(worker: &Worker) -> (String, &SyncState) {
    let [status] = worker.snapshot.sources.as_slice() else {
        panic!("one source: {:?}", worker.snapshot.sources);
    };
    (status.key.canonical(), &status.state)
}

fn listed(worker: &Worker) -> Vec<String> {
    let mut keys: Vec<String> = worker
        .snapshot
        .items
        .iter()
        .map(|item| item.key.canonical())
        .collect();
    keys.sort();
    keys
}

#[test]
fn linear_syncs_in_memory_and_follows_its_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = worker(dir.path());

    worker.linear_fetch = acme;
    worker.sync(None);
    let (key, state) = status(&worker);
    assert_eq!(key, "linear:linear.app:acme");
    assert!(
        matches!(state, SyncState::Synced { count: 2, .. }),
        "{state:?}"
    );
    assert_eq!(
        listed(&worker),
        [
            "linear:linear.app:acme:ENG-1",
            "linear:linear.app:acme:ENG-2"
        ]
    );
    let stored = worker.store.as_ref().unwrap().items().unwrap();
    assert_eq!(stored, []);

    worker.linear_fetch = beta;
    worker.sync(None);
    let (key, state) = status(&worker);
    assert_eq!(key, "linear:linear.app:beta");
    assert!(
        matches!(state, SyncState::Synced { count: 1, .. }),
        "{state:?}"
    );
    assert_eq!(listed(&worker), ["linear:linear.app:beta:OPS-3"]);

    worker.linear_fetch = rejected;
    worker.sync(None);
    let (key, state) = status(&worker);
    assert_eq!(key, "linear:linear.app:beta");
    assert!(
        matches!(state, SyncState::Failed(error) if matches!(**error, Error::LinearKeyRejected)),
        "{state:?}"
    );
    let waiting: Vec<String> = worker
        .retry
        .iter()
        .map(|(source, _)| source.canonical())
        .collect();
    assert_eq!(waiting, ["linear:linear.app:beta"]);
    assert_eq!(listed(&worker), ["linear:linear.app:beta:OPS-3"]);

    worker.linear_fetch = unreachable;
    worker.sync(None);
    assert!(matches!(status(&worker).1, SyncState::Failed(_)));

    worker.set_linear_key(LinearKey::Missing);
    worker.sync(None);
    let (key, state) = status(&worker);
    assert_eq!(key, "linear:linear.app:beta");
    assert!(matches!(state, SyncState::SignIn), "{state:?}");
    assert_eq!(listed(&worker), Vec::<String>::new());
}
