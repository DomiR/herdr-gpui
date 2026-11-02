//! The new issue dialog in a real window: New issue opens it and asks for
//! its choices, which start it in the team's default state, assigned to the
//! key's owner, in the active cycle; a picked status and a typed title go
//! to the worker on Cmd-Enter; and the worker's answer closes the dialog.
use super::*;
use crate::orchestrator::{
    Cycle, Service, Status, StatusKind,
    linear::{Choice, Created, CycleChoice, NewIssue, Options, TeamOptions},
    service::linear_create::Outcome,
    view::OrchestratorView,
};
use gpui::{Entity, VisualTestContext};
use std::sync::Arc;

fn state(id: &str, name: &str, kind: StatusKind, rank: i64) -> Status {
    Status {
        name: name.into(),
        kind,
        rank,
        color: Some(0x4ea7fc),
        id: Some(id.into()),
    }
}

fn options() -> Options {
    Options {
        viewer: "u-me".into(),
        teams: vec![TeamOptions {
            id: "t-eng".into(),
            key: "ENG".into(),
            name: "Engineering".into(),
            states: vec![
                state("s-backlog", "Backlog", StatusKind::Backlog, 0),
                state("s-todo", "Todo", StatusKind::Unstarted, 0),
                state("s-progress", "In Progress", StatusKind::Started, 0),
            ],
            default_state: Some("s-todo".into()),
            members: vec![Choice {
                id: "u-me".into(),
                name: "Dominique".into(),
            }],
            cycles: vec![CycleChoice {
                id: "c-56".into(),
                cycle: Cycle {
                    number: 56,
                    name: None,
                    active: true,
                },
            }],
            active_cycle: Some("c-56".into()),
            ..TeamOptions::default()
        }],
    }
}

/// The Linear section with its worker recording what it is asked.
fn open(
    cx: &mut gpui::TestAppContext,
) -> (
    Entity<OrchestratorView>,
    crate::orchestrator::service::linear_create::Recorder,
    &mut VisualTestContext,
) {
    let (_host, view, cx) = open_linear(cx, Vec::new(), 1200.);
    let (service, recorder) = Service::recording();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().service = Some(service);
            view.repo_mut().request.linear = Some(crate::github::Store::Environment);
            cx.notify();
        })
    });
    cx.run_until_parked();
    (view, recorder, cx)
}

#[gpui::test]
fn new_issue_creates_with_the_picked_status_on_cmd_enter(cx: &mut gpui::TestAppContext) {
    let (view, recorder, cx) = open(cx);
    click_on(cx, "linear-create-open");
    assert!(cx.debug_bounds("linear-create").is_some());
    // The choices are asked for; until they come no chip shows.
    assert_eq!(recorder.linear_asks(), (1, Vec::new()));
    assert!(cx.debug_bounds("linear-create-chip-status").is_none());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().snapshot.linear_create.options = Some(Ok(Arc::new(options())));
            cx.notify();
        })
    });
    cx.run_until_parked();

    cx.simulate_input("Fix login");
    click_on(cx, "linear-create-chip-status");
    assert!(cx.debug_bounds("linear-create-menu-status").is_some());
    click_on(cx, "linear-create-pick-status-In Progress");
    assert!(cx.debug_bounds("linear-create-menu-status").is_none());
    cx.simulate_keystrokes("cmd-enter");
    cx.run_until_parked();

    let (_, issues) = recorder.linear_asks();
    assert_eq!(
        issues,
        [NewIssue {
            team: "t-eng".into(),
            title: "Fix login".into(),
            state: Some("s-progress".into()),
            assignee: Some("u-me".into()),
            cycle: Some("c-56".into()),
            sequence: 1,
            ..NewIssue::default()
        }]
    );

    // Linear's answer closes the dialog and gives the keyboard back.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().snapshot.linear_create.created = Some(Outcome {
                sequence: 1,
                result: Ok(Created {
                    identifier: "ENG-42".into(),
                    url: "https://linear.app/acme/issue/ENG-42".into(),
                }),
            });
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("linear-create").is_none());
    let (notice, focused) = cx.update(|window, cx| {
        let view = view.read(cx);
        (view.notice_text(), view.focus.is_focused(window))
    });
    assert_eq!(notice.as_deref(), Some("Linear issue created"));
    assert!(focused);
}

#[gpui::test]
fn c_opens_the_dialog_and_a_refusal_keeps_it_open_with_why(cx: &mut gpui::TestAppContext) {
    let (view, _recorder, cx) = open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().snapshot.linear_create.options = Some(Ok(Arc::new(options())));
            window.focus(&view.focus, cx);
        })
    });
    cx.simulate_keystrokes("c");
    cx.run_until_parked();
    assert!(cx.debug_bounds("linear-create").is_some());
    // The team's default state, as Linear starts a new issue.
    let state = cx.update(|_, cx| {
        let view = view.read(cx);
        view.create.dialog.as_ref().unwrap().state.clone()
    });
    assert_eq!(state.as_deref(), Some("s-todo"));

    // Without a title nothing is sent.
    cx.simulate_keystrokes("cmd-enter");
    cx.run_until_parked();
    assert!(cx.debug_bounds("linear-create-problem").is_some());

    cx.simulate_input("Typed c stays in the title");
    cx.simulate_keystrokes("cmd-enter");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().snapshot.linear_create.created = Some(Outcome {
                sequence: 1,
                result: Err(Arc::new(crate::orchestrator::Error::LinearCreate(
                    Arc::new(crate::orchestrator::Error::LinearRateLimited),
                ))),
            });
            cx.notify();
        })
    });
    cx.run_until_parked();
    let (problem, title) = cx.update(|_, cx| {
        let dialog = view.read(cx).create.dialog.as_ref().unwrap();
        (
            dialog.problem.clone().map(|problem| problem.to_string()),
            dialog.title.read(cx).text().to_owned(),
        )
    });
    assert_eq!(
        problem.as_deref(),
        Some("Could not create the issue: Linear is limiting requests; the list syncs again later")
    );
    assert_eq!(title, "Typed c stays in the title");

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.debug_bounds("linear-create").is_none());
}

#[gpui::test]
fn create_more_starts_the_next_issue_in_the_same_dialog(cx: &mut gpui::TestAppContext) {
    let (view, recorder, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.repo_mut().snapshot.linear_create.options = Some(Ok(Arc::new(options())));
        })
    });
    click_on(cx, "linear-create-open");
    click_on(cx, "linear-create-more");
    cx.simulate_input("First");
    click_on(cx, "linear-create-submit");
    assert_eq!(recorder.linear_asks().1.len(), 1);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().snapshot.linear_create.created = Some(Outcome {
                sequence: 1,
                result: Ok(Created {
                    identifier: "ENG-42".into(),
                    url: "https://linear.app/acme/issue/ENG-42".into(),
                }),
            });
            cx.notify();
        })
    });
    cx.run_until_parked();
    let (title, created, more, focused) = cx.update(|window, cx| {
        let dialog = view.read(cx).create.dialog.as_ref().unwrap();
        (
            dialog.title.read(cx).text().to_owned(),
            dialog.created.clone(),
            dialog.more,
            dialog.title.read(cx).focus.is_focused(window),
        )
    });
    assert_eq!(title, "");
    assert_eq!(created.as_deref(), Some("ENG-42"));
    assert!(more && focused);
    cx.simulate_input("Second");
    cx.simulate_keystrokes("cmd-enter");
    let (_, issues) = recorder.linear_asks();
    assert_eq!(
        issues
            .iter()
            .map(|issue| (issue.title.as_str(), issue.sequence))
            .collect::<Vec<_>>(),
        [("Second", 2)]
    );
}

/// A lane's "+" and a status group's "+" start the new issue in their state.
#[gpui::test]
fn a_lanes_plus_starts_a_new_issue_in_its_state(cx: &mut gpui::TestAppContext) {
    let mut issue = item(Provider::Linear, "ENG-1", "Task", "2026-10-06T00:00:00Z");
    issue.tracker = Some(crate::orchestrator::Tracker {
        status: Some(state("s-progress", "In Progress", StatusKind::Started, 0)),
        ..Default::default()
    });
    let (_host, view, cx) = open_linear(cx, vec![issue], 1200.);
    let (service, recorder) = Service::recording();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().service = Some(service);
            view.repo_mut().request.linear = Some(crate::github::Store::Environment);
            view.repo_mut().snapshot.linear_create.options = Some(Ok(Arc::new(options())));
            cx.notify();
        })
    });
    cx.run_until_parked();
    // The list's status group offers it too.
    assert!(cx.debug_bounds("orchestrator-state-new-issue-1").is_some());
    click_on(cx, "orchestrator-layout-board");
    click_on(cx, "orchestrator-lane-new-issue-0");
    assert!(cx.debug_bounds("linear-create").is_some());
    cx.simulate_input("Started already");
    cx.simulate_keystrokes("cmd-enter");
    cx.run_until_parked();
    let (_, issues) = recorder.linear_asks();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].state.as_deref(), Some("s-progress"));
}
