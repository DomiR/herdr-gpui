//! Creating a Linear issue: what the new issue dialog offers, read with one
//! query, and the mutation creating it.
use super::*;
use crate::orchestrator::linear::{
    self, CREATE, CycleChoice, Estimate, Label, NewIssue, OPTIONS, interpret, scale,
};
use serde_json::{Value, json};
use std::cell::RefCell;

/// One team, ENG, as Linear answers the options query, beside a workspace
/// label and a label group.
fn options_answer() -> Value {
    json!({ "data": {
        "viewer": { "id": "u-me", "teams": { "nodes": [
            {
                "id": "t-ops", "key": "OPS", "name": "Operations",
                "issueEstimationType": "notUsed",
                "issueEstimationAllowZero": false, "issueEstimationExtended": false,
                "defaultIssueState": null, "activeCycle": null,
                "states": { "nodes": [
                    { "id": "s-ops-backlog", "name": "Backlog", "type": "backlog", "position": 0.0 },
                ] },
                "members": { "nodes": [] }, "projects": { "nodes": [] },
                "labels": { "nodes": [] }, "cycles": { "nodes": [] },
            },
            {
                "id": "t-eng", "key": "ENG", "name": "Engineering",
                "issueEstimationType": "tShirt",
                "issueEstimationAllowZero": false, "issueEstimationExtended": true,
                "defaultIssueState": { "id": "s-todo" }, "activeCycle": { "id": "c-56" },
                "states": { "nodes": [
                    { "id": "s-done", "name": "Done", "type": "completed", "position": 0.0, "color": "#5E6AD2" },
                    { "id": "s-todo", "name": "Todo", "type": "unstarted", "position": 1.0, "color": "#E2E2E2" },
                    { "id": "s-backlog", "name": "Backlog", "type": "backlog", "position": 0.0 },
                ] },
                "members": { "nodes": [
                    { "id": "u-zoe", "displayName": "zoe", "active": true },
                    { "id": "u-me", "displayName": "Dominique", "active": true },
                    { "id": "u-gone", "displayName": "gone", "active": false },
                ] },
                "projects": { "nodes": [
                    { "id": "p-ap", "name": "Trumpf AP", "completedAt": null, "canceledAt": null },
                    { "id": "p-old", "name": "Shipped", "completedAt": "2026-01-01T00:00:00Z", "canceledAt": null },
                ] },
                "labels": { "nodes": [
                    { "id": "l-bug", "name": "Bug", "color": "#EB5757", "isGroup": false },
                    { "id": "l-area", "name": "Area", "color": null, "isGroup": true },
                ] },
                "cycles": { "nodes": [
                    { "id": "c-57", "number": 57.0, "name": null, "isActive": false },
                    { "id": "c-56", "number": 56.0, "name": "", "isActive": true },
                ] },
            },
        ] } },
        "issueLabels": { "nodes": [
            { "id": "l-chore", "name": "chore", "color": "#BEC2C8", "isGroup": false },
        ] },
    } })
}

#[test]
fn the_options_are_one_query_read_into_each_teams_choices() {
    let asked = RefCell::new(Vec::new());
    let options = linear::options(|query, variables| {
        asked.borrow_mut().push((query, variables));
        Ok(options_answer())
    })
    .unwrap();
    assert_eq!(*asked.borrow(), [(OPTIONS, json!({}))]);
    assert_eq!(options.viewer, "u-me");
    // Teams by name.
    let keys: Vec<&str> = options.teams.iter().map(|team| team.key.as_str()).collect();
    assert_eq!(keys, ["ENG", "OPS"]);
    let eng = &options.teams[0];
    let states: Vec<&str> = eng.states.iter().map(|state| state.name.as_str()).collect();
    assert_eq!(states, ["Backlog", "Todo", "Done"]);
    assert_eq!(
        eng.default_state().and_then(|state| state.id.as_deref()),
        Some("s-todo")
    );
    let members: Vec<&str> = eng
        .members
        .iter()
        .map(|member| member.id.as_str())
        .collect();
    assert_eq!(members, ["u-me", "u-zoe"], "active members, by name");
    let projects: Vec<&str> = eng
        .projects
        .iter()
        .map(|project| project.name.as_str())
        .collect();
    assert_eq!(projects, ["Trumpf AP"], "open projects only");
    assert_eq!(
        eng.labels,
        [
            Label {
                id: "l-bug".into(),
                name: "Bug".into(),
                color: Some(0xEB5757),
            },
            Label {
                id: "l-chore".into(),
                name: "chore".into(),
                color: Some(0xBEC2C8),
            },
        ],
        "the team's and the workspace's labels, no groups"
    );
    assert_eq!(
        eng.cycles,
        [
            CycleChoice {
                id: "c-56".into(),
                cycle: Cycle {
                    number: 56,
                    name: None,
                    active: true,
                },
            },
            CycleChoice {
                id: "c-57".into(),
                cycle: Cycle {
                    number: 57,
                    name: None,
                    active: false,
                },
            },
        ]
    );
    assert_eq!(eng.active_cycle.as_deref(), Some("c-56"));
    let sizes: Vec<&str> = eng
        .estimates
        .iter()
        .flatten()
        .map(|point| point.label.as_str())
        .collect();
    assert_eq!(sizes, ["XS", "S", "M", "L", "XL", "XXL", "XXXL"]);
    // A team that does not estimate offers none; without a default state
    // its first unstarted, else backlog, state is used.
    let ops = &options.teams[1];
    assert_eq!(ops.estimates, None);
    assert_eq!(
        ops.default_state().and_then(|state| state.id.as_deref()),
        Some("s-ops-backlog")
    );
}

#[test]
fn estimate_scales_follow_the_teams_settings() {
    let values = |kind: Option<&str>, zero: bool, extended: bool| -> Option<Vec<i64>> {
        scale(kind, zero, extended).map(|scale| scale.iter().map(|point| point.value).collect())
    };
    assert_eq!(
        values(Some("fibonacci"), false, false),
        Some(vec![1, 2, 3, 5, 8])
    );
    assert_eq!(
        values(Some("exponential"), true, true),
        Some(vec![0, 1, 2, 4, 8, 16, 32, 64])
    );
    assert_eq!(
        values(Some("linear"), false, true),
        Some(vec![1, 2, 3, 4, 5, 6, 7])
    );
    assert_eq!(values(Some("notUsed"), true, true), None);
    // Settings Linear did not send fall back to a plain scale.
    assert_eq!(values(None, false, false), Some(vec![0, 1, 2, 3, 5, 8]));
    assert_eq!(
        scale(Some("tShirt"), true, false).unwrap()[..2],
        [
            Estimate {
                value: 0,
                label: "0".into(),
            },
            Estimate {
                value: 1,
                label: "XS".into(),
            },
        ]
    );
}

fn issue() -> NewIssue {
    NewIssue {
        team: "t-eng".into(),
        title: "Fix login".into(),
        sequence: 3,
        ..NewIssue::default()
    }
}

fn created(success: bool) -> Value {
    json!({ "data": { "issueCreate": {
        "success": success,
        "issue": { "identifier": "ENG-42", "url": "https://linear.app/acme/issue/ENG-42" },
    } } })
}

#[test]
fn a_new_issue_sends_only_what_was_set() {
    let asked = RefCell::new(Vec::new());
    let made = linear::create_issue(
        |query, variables| {
            asked.borrow_mut().push((query, variables));
            Ok(created(true))
        },
        &issue(),
    )
    .unwrap();
    assert_eq!(
        *asked.borrow(),
        [(
            CREATE,
            json!({ "input": { "teamId": "t-eng", "title": "Fix login" } })
        )]
    );
    assert_eq!(made.identifier, "ENG-42");
    assert_eq!(made.url, "https://linear.app/acme/issue/ENG-42");
}

#[test]
fn every_property_set_is_sent() {
    let issue = NewIssue {
        description: "Steps:\n\n1. Open".into(),
        state: Some("s-todo".into()),
        priority: 2,
        assignee: Some("u-me".into()),
        project: Some("p-ap".into()),
        estimate: Some(0),
        labels: vec!["l-bug".into(), "l-chore".into()],
        cycle: Some("c-56".into()),
        ..issue()
    };
    assert_eq!(
        issue.input(),
        json!({
            "teamId": "t-eng", "title": "Fix login", "description": "Steps:\n\n1. Open",
            "stateId": "s-todo", "priority": 2, "assigneeId": "u-me", "projectId": "p-ap",
            "estimate": 0, "labelIds": ["l-bug", "l-chore"], "cycleId": "c-56",
        })
    );
}

#[test]
fn an_issue_linear_did_not_create_fails() {
    let refused = linear::create_issue(|_, _| Ok(created(false)), &issue());
    assert!(matches!(refused, Err(Error::Output { .. })), "{refused:?}");
    // GraphQL refusals arrive as an error envelope, whose message is kept.
    let envelope = br#"{"errors":[{"message":"Argument Validation Error","extensions":{"code":"INVALID_INPUT"}}],"data":null}"#;
    let rejected = linear::create_issue(|_, _| interpret(200, envelope), &issue());
    let Err(Error::LinearStatus { status, message }) = rejected else {
        panic!("{rejected:?}");
    };
    assert_eq!(status, 200);
    assert_eq!(message.as_deref(), Some("Argument Validation Error"));
    let unreadable = linear::options(|_, _| Ok(json!({ "data": { "viewer": null } })));
    assert!(
        matches!(unreadable, Err(Error::OutputJson { .. })),
        "{unreadable:?}"
    );
}

#[test]
fn a_failed_creation_says_why() {
    let error = Error::LinearCreate(std::sync::Arc::new(Error::LinearRateLimited));
    assert_eq!(
        error.to_string(),
        "Could not create the issue: Linear is limiting requests; the list syncs again later"
    );
    let source = std::error::Error::source(&error).unwrap();
    assert_eq!(source.to_string(), Error::LinearRateLimited.to_string());
    let error = Error::LinearOptions(std::sync::Arc::new(Error::LinearKeyMissing));
    assert_eq!(
        error.to_string(),
        "Could not read your Linear teams: Connect Linear first"
    );
}
