use super::*;
use crate::linear::{self, CAP, LIMIT, Listing, QUERY, STATES, Scope, interpret, resolve, valid};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use std::{cell::Cell, ffi::OsString, sync::Arc};

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
        "issues": {
            "pageInfo": { "hasNextPage": next.is_some(), "endCursor": next },
            "nodes": nodes,
        },
    } })
}

fn node(identifier: &str) -> Value {
    json!({
        "identifier": identifier, "title": "Task", "description": null,
        "url": format!("https://linear.app/acme/issue/{identifier}"),
        "priority": 0, "branchName": "", "createdAt": "2026-10-01T00:00:00.000Z",
        "updatedAt": "2026-10-02T00:00:00.000Z", "state": { "name": "Todo" },
        "creator": null, "assignee": null, "cycle": null, "parent": null,
        "labels": { "nodes": [] }, "inverseRelations": { "nodes": [] },
    })
}

/// No issues, the answer for a scope a test leaves empty.
fn empty() -> Value {
    page(Vec::new(), None)
}

/// The workflow states of team `team-eng`: Todo and In Progress.
fn states() -> Value {
    json!({ "data": { "teams": { "nodes": [{
        "id": "team-eng",
        "states": { "nodes": [
            { "id": "s-doing", "name": "In Progress", "type": "started",
              "position": 2.0, "color": "#F2C94C" },
            { "id": "s-todo", "name": "Todo", "type": "unstarted",
              "position": 1.0, "color": "#E2E2E2" },
        ] },
    }] } } })
}

/// Team `team-eng`'s states as an issue of the team holds them, in board
/// order.
fn eng_states() -> Arc<Vec<Status>> {
    Arc::new(vec![
        Status {
            name: "Todo".into(),
            kind: StatusKind::Unstarted,
            rank: 1000,
            color: Some(0xE2E2E2),
            id: Some("s-todo".into()),
        },
        Status {
            name: "In Progress".into(),
            kind: StatusKind::Started,
            rank: 2000,
            color: Some(0xF2C94C),
            id: Some("s-doing".into()),
        },
    ])
}

fn listing(pages: Vec<Value>) -> (Listing, Vec<Value>) {
    let mut pages = pages.into_iter();
    let mut seen = Vec::new();
    let listing = linear::sync(
        |query, variables| {
            if query == STATES {
                return Ok(states());
            }
            assert_eq!(query, QUERY);
            seen.push(variables);
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
        "id": "uuid-12",
        "identifier": "ENG-12", "title": "Fix login", "description": "Steps\n1. Open",
        "url": "https://linear.app/acme/issue/ENG-12/fix-login", "priority": 2.0,
        "branchName": "dominique/eng-12-fix-login",
        "createdAt": "2026-10-01T09:00:00.000Z", "updatedAt": "2026-10-03T10:30:00.000Z",
        "sortOrder": -12.5, "estimate": 3.0,
        "state": { "id": "s-doing", "name": "In Progress", "type": "started",
                   "position": 2.0, "color": "#F2C94C" },
        "team": { "id": "team-eng" },
        "creator": { "displayName": "dominique" },
        "assignee": { "displayName": "Dominique Rau", "isMe": true,
                      "avatarUrl": "https://public.linear.app/a/avatar.png" },
        "cycle": { "number": 42.0, "name": null, "isActive": true },
        "parent": { "identifier": "ENG-1", "title": "Auth overhaul" },
        "project": { "name": "Frontend", "color": "#5E6AD2" },
        "attachments": { "nodes": [
            { "url": "https://github.com/acme/app/pull/9721",
              "metadata": { "status": "merged", "number": 9721 } },
            { "url": "https://www.figma.com/file/abc", "metadata": {} },
            { "url": "https://github.com/acme/app/pull/10555/files",
              "metadata": { "status": "open", "draft": true } },
            { "url": "https://github.com/acme/app/issues/12", "metadata": {} },
            { "url": "https://github.com/acme/app/pull/9721",
              "metadata": { "status": "merged" } },
            { "url": "https://github.com/acme/app/pull/10807" },
        ] },
        "children": { "pageInfo": { "hasNextPage": false }, "nodes": [
            { "state": { "type": "completed" } },
            { "state": { "type": "started" } },
            { "state": { "type": "canceled" } },
            { "state": { "type": "unstarted" } },
        ] },
        "labels": { "nodes": [{ "name": "Bug" }, { "name": "Auth" }] },
        "inverseRelations": { "nodes": [
            { "type": "blocks", "issue": { "identifier": "ENG-7" } },
            { "type": "related", "issue": { "identifier": "ENG-9" } },
            { "type": "blocks", "issue": { "identifier": "ENG-3" } },
            { "type": "duplicate", "issue": { "identifier": "ENG-4" } },
            { "type": "blocks", "issue": { "identifier": "ENG-7" } },
        ] },
    });
    let (listing, _) = listing(vec![page(vec![issue, node("ENG-13")], None), empty()]);
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
            tracker: Some(Tracker {
                branch: Some("dominique/eng-12-fix-login".into()),
                assignee: Some("Dominique Rau".into()),
                mine: true,
                cycle: Some(Cycle {
                    number: 42,
                    name: None,
                    active: true,
                }),
                status: Some(Status {
                    name: "In Progress".into(),
                    kind: StatusKind::Started,
                    rank: 2000,
                    color: Some(0xF2C94C),
                    id: Some("s-doing".into()),
                }),
                id: Some("uuid-12".into()),
                states: eng_states(),
                sort_order: Some(SortOrder(-12.5)),
                avatar: Some("https://public.linear.app/a/avatar.png".into()),
                estimate: Some(3),
                project: Some(Project {
                    name: "Frontend".into(),
                    color: Some(0x5E6AD2),
                }),
                parent: Some("Auth overhaul".into()),
                // GitHub's pull requests only, each once, a draft as one
                // and a link the integration says nothing of without a state.
                pull_requests: vec![
                    LinkedPullRequest {
                        url: "https://github.com/acme/app/pull/9721".into(),
                        number: 9721,
                        state: Some(LinkState::Merged),
                    },
                    LinkedPullRequest {
                        url: "https://github.com/acme/app/pull/10555/files".into(),
                        number: 10555,
                        state: Some(LinkState::Draft),
                    },
                    LinkedPullRequest {
                        url: "https://github.com/acme/app/pull/10807".into(),
                        number: 10807,
                        state: None,
                    },
                ],
                // A canceled sub-issue counts in neither.
                progress: Some(Progress {
                    done: 1,
                    total: 3,
                    more: false,
                }),
            }),
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
            bare.tracker.clone(),
            bare.state.as_str()
        ),
        (
            None,
            None,
            // A state without its type, order, color, or id still names
            // itself; an issue without a team has no states to move to.
            Some(Tracker {
                status: Some(Status {
                    name: "Todo".into(),
                    kind: StatusKind::Other,
                    rank: 0,
                    color: None,
                    id: None,
                }),
                ..Tracker::default()
            }),
            "todo"
        )
    );
}

#[test]
fn listing_reads_each_scope_and_lists_an_issue_once() {
    let (listing, seen) = listing(vec![
        page(vec![node("ENG-1"), node("ENG-2")], Some("c1")),
        page(vec![node("ENG-3")], None),
        page(vec![node("ENG-2"), node("ENG-9")], None),
    ]);
    let open = json!({ "type": { "nin": ["completed", "canceled"] } });
    let mine = json!({ "assignee": { "isMe": { "eq": true } }, "state": open });
    let cycles = json!({
        "cycle": { "isActive": { "eq": true } },
        "team": { "members": { "some": { "isMe": { "eq": true } } } },
        "state": open,
    });
    assert_eq!(Scope::ALL, [Scope::Mine, Scope::Cycles]);
    assert_eq!(
        seen,
        [
            json!({ "filter": mine, "after": null }),
            json!({ "filter": mine, "after": "c1" }),
            json!({ "filter": cycles, "after": null }),
        ]
    );
    let ids: Vec<&str> = listing
        .items
        .iter()
        .map(|item| item.identifier.as_str())
        .collect();
    assert_eq!(ids, ["ENG-1", "ENG-2", "ENG-3", "ENG-9"]);
    assert!(listing.complete);
}

#[test]
fn each_scope_stops_at_its_cap() {
    let fetched = Cell::new(0);
    let listing = linear::sync(
        |query, variables| {
            if query == STATES {
                return Ok(states());
            }
            fetched.set(fetched.get() + 1);
            let start = fetched.get() * 100;
            let nodes = (start..start + 100)
                .map(|n| node(&format!("ENG-{n}")))
                .collect();
            // Mine always has more; Cycles ends on its fifth page.
            let more = variables["filter"].get("assignee").is_some() || fetched.get() < 10;
            Ok(page(nodes, more.then_some("more")))
        },
        || false,
    )
    .unwrap();
    assert_eq!(fetched.get(), 10);
    assert_eq!(listing.items.len(), 2 * CAP);
    assert!(!listing.complete);
}

#[test]
fn a_cycle_reads_as_its_name_or_number() {
    let cycle = |name: Option<&str>| Cycle {
        number: 7,
        name: name.map(str::to_owned),
        active: true,
    };
    assert_eq!(cycle(None).label(), "Cycle 7");
    assert_eq!(cycle(Some("Launch week")).label(), "Launch week");
}

#[test]
fn listing_stops_when_cancelled_between_pages() {
    let fetched = Cell::new(0);
    let result = linear::sync(
        |query, _| {
            if query == STATES {
                return Ok(states());
            }
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
    let prompt = crate::prompt::built_in(&issue);
    assert!(
        prompt.contains("\nProvider: linear\nWorkspace: acme\n"),
        "{prompt}"
    );
    assert!(!prompt.contains("Repository:"), "{prompt}");
}
