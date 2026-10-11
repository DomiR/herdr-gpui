use super::*;
use crate::orchestrator::linear::{self, LIMIT, Listing, interpret, resolve, valid};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use std::{cell::Cell, ffi::OsString};

fn acme() -> SourceKey {
    SourceKey {
        provider: Provider::Linear,
        host: "linear.app".into(),
        repository: "acme".into(),
    }
}

/// A response page holding `nodes`, with a next page after `next`.
fn page(nodes: Vec<Value>, next: Option<&str>) -> Value {
    json!({ "data": {
        "organization": { "urlKey": "acme" },
        "viewer": { "assignedIssues": {
            "pageInfo": { "hasNextPage": next.is_some(), "endCursor": next },
            "nodes": nodes,
        } },
    } })
}

fn node(identifier: &str) -> Value {
    json!({
        "identifier": identifier, "title": "Task", "description": null,
        "url": format!("https://linear.app/acme/issue/{identifier}"),
        "priority": 0, "branchName": "", "createdAt": "2026-10-01T00:00:00.000Z",
        "updatedAt": "2026-10-02T00:00:00.000Z", "state": { "name": "Todo" },
        "creator": null, "parent": null,
        "labels": { "nodes": [] }, "inverseRelations": { "nodes": [] },
    })
}

fn listing(pages: Vec<Value>) -> (Listing, Vec<Value>) {
    let mut pages = pages.into_iter();
    let mut seen = Vec::new();
    let listing = linear::sync(
        |variables| {
            seen.push(variables["after"].clone());
            Ok(pages.next().unwrap())
        },
        || false,
    )
    .unwrap();
    (listing, seen)
}

#[test]
fn an_issue_reads_as_an_item() {
    let issue = json!({
        "identifier": "ENG-12", "title": "Fix login", "description": "Steps\n1. Open",
        "url": "https://linear.app/acme/issue/ENG-12/fix-login", "priority": 2.0,
        "branchName": "dominique/eng-12-fix-login",
        "createdAt": "2026-10-01T09:00:00.000Z", "updatedAt": "2026-10-03T10:30:00.000Z",
        "state": { "name": "In Progress" },
        "creator": { "displayName": "dominique" },
        "parent": { "identifier": "ENG-1" },
        "labels": { "nodes": [{ "name": "Bug" }, { "name": "Auth" }] },
        "inverseRelations": { "nodes": [
            { "type": "blocks", "issue": { "identifier": "ENG-7" } },
            { "type": "related", "issue": { "identifier": "ENG-9" } },
            { "type": "blocks", "issue": { "identifier": "ENG-3" } },
            { "type": "duplicate", "issue": { "identifier": "ENG-4" } },
            { "type": "blocks", "issue": { "identifier": "ENG-7" } },
        ] },
    });
    let (listing, _) = listing(vec![page(vec![issue, node("ENG-13")], None)]);
    assert_eq!(listing.workspace, "acme");
    assert!(listing.complete);
    assert_eq!(listing.items.len(), 2);
    assert_eq!(
        listing.items[0],
        Item {
            key: ItemKey {
                source: acme(),
                native_id: "ENG-12".into(),
            },
            identifier: "ENG-12".into(),
            title: "Fix login".into(),
            description: Some("Steps\n1. Open".into()),
            state: "in progress".into(),
            url: Some("https://linear.app/acme/issue/ENG-12/fix-login".into()),
            author: Some("dominique".into()),
            labels: vec!["Bug".into(), "Auth".into()],
            parent_id: Some("ENG-1".into()),
            blocked_by: vec!["ENG-3".into(), "ENG-7".into()],
            priority: Some(2),
            created_at: Some(at("2026-10-01T09:00:00Z")),
            updated_at: Some(at("2026-10-03T10:30:00Z")),
            pull_request: None,
            activity: None,
            branch: Some("dominique/eng-12-fix-login".into()),
        }
    );
    assert_eq!(
        listing.items[0].key.canonical(),
        "linear:linear.app:acme:ENG-12"
    );
    let bare = &listing.items[1];
    assert_eq!(
        (
            bare.priority,
            bare.description.clone(),
            bare.branch.clone(),
            bare.state.as_str()
        ),
        (None, None, None, "todo")
    );
}

#[test]
fn listing_pages_through_the_cursor() {
    let (listing, seen) = listing(vec![
        page(vec![node("ENG-1"), node("ENG-2")], Some("c1")),
        page(vec![node("ENG-3")], None),
    ]);
    assert_eq!(seen, [Value::Null, json!("c1")]);
    let ids: Vec<&str> = listing
        .items
        .iter()
        .map(|item| item.identifier.as_str())
        .collect();
    assert_eq!(ids, ["ENG-1", "ENG-2", "ENG-3"]);
    assert!(listing.complete);
}

#[test]
fn listing_stops_at_its_cap() {
    let full = |start: usize| {
        page(
            (start..start + 50)
                .map(|n| node(&format!("ENG-{n}")))
                .collect(),
            Some("more"),
        )
    };
    let fetched = Cell::new(0);
    let listing = linear::sync(
        |_| {
            fetched.set(fetched.get() + 1);
            Ok(full(fetched.get() * 50))
        },
        || false,
    )
    .unwrap();
    assert_eq!(fetched.get(), 5);
    assert_eq!(listing.items.len(), linear::MAX);
    assert!(!listing.complete);
}

#[test]
fn listing_stops_when_cancelled_between_pages() {
    let fetched = Cell::new(0);
    let result = linear::sync(
        |_| {
            fetched.set(fetched.get() + 1);
            Ok(page(vec![node("ENG-1")], Some("c1")))
        },
        || fetched.get() == 1,
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(fetched.get(), 1);
}

#[test]
fn responses_say_why_linear_refused() {
    let refusal = |code: &str| {
        json!({ "errors": [{ "message": "No", "extensions": { "code": code } }] })
            .to_string()
            .into_bytes()
    };
    assert!(matches!(
        interpret(400, &refusal("AUTHENTICATION_ERROR")),
        Err(Error::LinearKeyRejected)
    ));
    assert!(matches!(
        interpret(200, &refusal("AUTHENTICATION_ERROR")),
        Err(Error::LinearKeyRejected)
    ));
    assert!(matches!(interpret(401, b""), Err(Error::LinearKeyRejected)));
    assert!(matches!(
        interpret(400, &refusal("RATELIMITED")),
        Err(Error::LinearRateLimited)
    ));
    assert!(matches!(
        interpret(429, b"{}"),
        Err(Error::LinearRateLimited)
    ));

    let broken = json!({ "errors": [{ "message": "  Server\u{202e} broke\n" }] }).to_string();
    let error = interpret(500, broken.as_bytes()).unwrap_err();
    assert!(
        matches!(
            &error,
            Error::LinearStatus { status: 500, message: Some(message) } if message == "Server  broke"
        ),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "Linear request failed with HTTP 500: Server  broke"
    );
    assert!(matches!(
        interpret(502, b"<html>"),
        Err(Error::LinearStatus {
            status: 502,
            message: None
        })
    ));
    assert!(matches!(
        interpret(200, &vec![b' '; LIMIT + 1]),
        Err(Error::LinearSize)
    ));
    assert!(matches!(
        interpret(200, b"not json"),
        Err(Error::OutputJson {
            operation: "Reading Linear",
            ..
        })
    ));
    assert_eq!(
        interpret(200, br#"{"data":{"viewer":null}}"#).unwrap(),
        json!({ "data": { "viewer": null } })
    );
}

fn environment(value: Option<&'static str>) -> impl Fn(&str) -> Option<OsString> {
    move |name| {
        assert_eq!(name, "LINEAR_API_KEY");
        value.map(OsString::from)
    }
}

fn saved() -> Result<Option<SecretString>> {
    Ok(Some("lin_api_saved".into()))
}

fn exposed(key: Result<Option<SecretString>>) -> Option<String> {
    key.unwrap().map(|key| key.expose_secret().to_owned())
}

#[test]
fn the_environment_key_wins_over_the_saved_one() {
    assert_eq!(
        exposed(resolve(environment(Some(" lin_api_env\n")), saved)),
        Some("lin_api_env".into())
    );
    assert_eq!(
        exposed(resolve(environment(None), saved)),
        Some("lin_api_saved".into())
    );
    assert_eq!(
        exposed(resolve(environment(Some("  ")), saved)),
        Some("lin_api_saved".into())
    );
    assert_eq!(exposed(resolve(environment(None), || Ok(None))), None);
    assert!(matches!(
        resolve(environment(Some("lin api env")), saved),
        Err(Error::LinearVariable)
    ));
}

#[test]
fn only_header_safe_keys_are_valid() {
    assert!(valid("lin_api_0123456789abcdef"));
    assert!(!valid(""));
    assert!(!valid("lin_api one"));
    assert!(!valid("lin_api_\u{e9}"));
    assert!(valid(&"k".repeat(512)));
    assert!(!valid(&"k".repeat(513)));
}

#[test]
fn linear_keys_round_trip() {
    assert_eq!("linear".parse::<Provider>().unwrap(), Provider::Linear);
    assert_eq!(Provider::Linear.as_str(), "linear");
    assert_eq!(Provider::Linear.name(), "Linear");
    let key: ItemKey = "linear:linear.app:acme:ENG-12".parse().unwrap();
    assert_eq!(
        key,
        ItemKey {
            source: acme(),
            native_id: "ENG-12".into(),
        }
    );
    assert_eq!(key.canonical(), "linear:linear.app:acme:ENG-12");
    assert_eq!(linear::source("acme"), acme());
    assert_eq!(Item::stand_in(key).identifier, "ENG-12");
}

#[test]
fn linear_priorities_read_as_linear_words() {
    let mut issue = item(&acme(), "ENG-12", "Fix login");
    let labels: Vec<Option<String>> = [None, Some(1), Some(2), Some(3), Some(4), Some(7)]
        .into_iter()
        .map(|priority| {
            issue.priority = priority;
            issue.priority_label()
        })
        .collect();
    assert_eq!(
        labels,
        [
            None,
            Some("Urgent".into()),
            Some("High".into()),
            Some("Medium".into()),
            Some("Low".into()),
            Some("P7".into()),
        ]
    );
    let mut github_issue = item(&github(), "377", "Icons");
    github_issue.priority = Some(2);
    assert_eq!(github_issue.priority_label(), Some("P2".into()));
}

#[test]
fn the_built_in_prompt_names_a_linear_workspace() {
    let issue = item(&acme(), "ENG-12", "Fix login");
    let prompt = crate::orchestrator::prompt::built_in(&issue);
    assert!(
        prompt.contains("\nProvider: linear\nWorkspace: acme\n"),
        "{prompt}"
    );
    assert!(!prompt.contains("Repository:"), "{prompt}");
}
