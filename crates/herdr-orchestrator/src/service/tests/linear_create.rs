//! The worker answering the new issue dialog: its choices read once and
//! kept, a creation's outcome echoed with its sequence, and a created issue
//! syncing the listing.

use super::linear::{listing, worker};
use super::*;
use crate::linear::{CREATE, NewIssue, OPTIONS};
use crate::service::linear_create::Ask;
use serde_json::json;
use std::sync::atomic::AtomicUsize;

fn options() -> Value {
    json!({ "data": {
        "viewer": { "id": "u-me", "teams": { "nodes": [{
            "id": "t-eng", "key": "ENG", "name": "Engineering",
            "issueEstimationType": "fibonacci",
            "issueEstimationAllowZero": false, "issueEstimationExtended": false,
            "defaultIssueState": { "id": "s-todo" }, "activeCycle": null,
            "states": { "nodes": [
                { "id": "s-todo", "name": "Todo", "type": "unstarted", "position": 0.0 },
            ] },
            "members": { "nodes": [{ "id": "u-me", "displayName": "me", "active": true }] },
            "projects": { "nodes": [] }, "labels": { "nodes": [] }, "cycles": { "nodes": [] },
        }] } },
        "issueLabels": { "nodes": [] },
    } })
}

static OPTIONS_READ: AtomicUsize = AtomicUsize::new(0);

fn counted(_: &SecretString, query: &str, _: Value) -> Result<Value> {
    assert_eq!(query, OPTIONS);
    OPTIONS_READ.fetch_add(1, Ordering::SeqCst);
    Ok(options())
}

fn created(_: &SecretString, query: &str, variables: Value) -> Result<Value> {
    if query != CREATE {
        return Ok(listing("acme", &["ENG-1", "ENG-42"]));
    }
    assert_eq!(
        variables,
        json!({ "input": { "teamId": "t-eng", "title": "Fix login", "stateId": "s-todo" } })
    );
    Ok(json!({ "data": { "issueCreate": {
        "success": true,
        "issue": { "identifier": "ENG-42", "url": "https://linear.app/acme/issue/ENG-42" },
    } } }))
}

fn refused(_: &SecretString, query: &str, _: Value) -> Result<Value> {
    assert_eq!(query, CREATE);
    Err(Error::LinearStatus {
        status: 400,
        message: Some("Team not found".into()),
    })
}

fn issue() -> NewIssue {
    NewIssue {
        team: "t-eng".into(),
        title: "Fix login".into(),
        state: Some("s-todo".into()),
        sequence: 4,
        ..NewIssue::default()
    }
}

#[test]
fn the_choices_are_read_once_and_published() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = worker(dir.path());
    worker.linear_fetch = counted;
    assert!(!worker.linear_create(Ask::Options));
    assert!(!worker.linear_create(Ask::Options));
    assert_eq!(OPTIONS_READ.load(Ordering::SeqCst), 1);
    let published = worker.mailbox.lock().unwrap().take().unwrap();
    let Some(Ok(options)) = &published.linear_create.options else {
        panic!("{:?}", published.linear_create.options);
    };
    assert_eq!(options.viewer, "u-me");
    assert_eq!(options.teams[0].key, "ENG");
}

#[test]
fn without_a_key_the_dialog_is_told_to_connect_linear_and_asks_again() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = worker(dir.path());
    worker.set_linear_key(LinearKey::Missing);
    worker.linear_create(Ask::Options);
    let Some(Err(error)) = &worker.snapshot.linear_create.options else {
        panic!("{:?}", worker.snapshot.linear_create.options);
    };
    assert!(
        matches!(&**error, Error::LinearOptions(source) if matches!(**source, Error::LinearKeyMissing))
    );
    assert_eq!(
        error.to_string(),
        "Could not read your Linear teams: Connect Linear first"
    );
    // A failure is not kept: the next ask reads again.
    worker.set_linear_key(LinearKey::Found("lin_api_test".into()));
    worker.linear_fetch = options_only;
    worker.linear_create(Ask::Options);
    assert!(matches!(worker.snapshot.linear_create.options, Some(Ok(_))));
}

fn options_only(_: &SecretString, query: &str, _: Value) -> Result<Value> {
    assert_eq!(query, OPTIONS);
    Ok(options())
}

#[test]
fn a_created_issue_echoes_its_sequence_and_asks_for_a_sync() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = worker(dir.path());
    worker.linear_fetch = created;
    assert!(worker.linear_create(Ask::Create(Box::new(issue()))));
    let published = worker.mailbox.lock().unwrap().take().unwrap();
    let outcome = published.linear_create.created.unwrap();
    assert_eq!(outcome.sequence, 4);
    assert_eq!(outcome.result.unwrap().identifier, "ENG-42");
    // The sync that follows lists it.
    worker.sync(None);
    assert!(
        worker
            .snapshot
            .items
            .iter()
            .any(|item| item.identifier == "ENG-42")
    );
}

#[test]
fn a_refused_creation_keeps_why_and_does_not_sync() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = worker(dir.path());
    worker.linear_fetch = refused;
    assert!(!worker.linear_create(Ask::Create(Box::new(issue()))));
    let outcome = worker.snapshot.linear_create.created.clone().unwrap();
    assert_eq!(outcome.sequence, 4);
    let error = outcome.result.unwrap_err();
    assert_eq!(
        error.to_string(),
        "Could not create the issue: Linear request failed with HTTP 400: Team not found"
    );
    // Not a notice: the dialog shows it, beside what was written.
    assert!(worker.notices.lock().unwrap().is_empty());
}

#[test]
fn the_service_queues_the_dialogs_asks() {
    let (service, recorder) = Service::recording();
    assert!(service.read_linear_options());
    assert!(service.create_linear_issue(issue()));
    assert_eq!(recorder.linear_asks(), (1, vec![issue()]));
}
