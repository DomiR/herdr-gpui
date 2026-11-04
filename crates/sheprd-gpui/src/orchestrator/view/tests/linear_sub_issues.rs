//! A Linear issue's page lists its sub-issues: a listed one opens its own
//! page, another opens in Linear, and Add sub-issue creates one under it in
//! the parent's team.
use super::*;
use crate::orchestrator::{
    Event, Service, Status, StatusKind, SubItem, Tracker,
    linear::{NewIssue, Options, TeamOptions},
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn linear(id: &str, title: &str) -> Item {
    let mut item = item(Provider::Linear, id, title, "2026-10-30T10:00:00Z");
    item.tracker = Some(Tracker::default());
    item
}

fn sub(identifier: &str, title: &str) -> SubItem {
    SubItem {
        identifier: identifier.into(),
        title: title.into(),
        url: Some(format!("https://linear.app/acme/issue/{identifier}")),
        status: Some(Status {
            name: "Todo".into(),
            kind: StatusKind::Unstarted,
            rank: 1000,
            color: Some(0xE2E2E2),
            id: Some("s-todo".into()),
        }),
        assignee: Some("Dominique".into()),
    }
}

fn items() -> Vec<Item> {
    let mut parent = linear("ENG-12", "Auth overhaul");
    if let Some(tracker) = &mut parent.tracker {
        tracker.id = Some("uuid-12".into());
        tracker.children = vec![
            sub("ENG-13", "Write tests"),
            sub("ENG-99", "Someone else's"),
        ];
    }
    let mut child = linear("ENG-13", "Write tests");
    child.parent_id = Some("ENG-12".into());
    // A team the parent is not in comes first, so the dialog must pick the
    // parent's.
    vec![parent, child, linear("OPS-1", "Unrelated")]
}

#[gpui::test]
fn sub_issues_open_here_or_in_linear(cx: &mut gpui::TestAppContext) {
    let (_host, view, cx) = open_linear(cx, items(), 1400.);
    let opened = Rc::new(RefCell::new(Vec::new()));
    let _subscription = cx.update(|_, cx| {
        let opened = opened.clone();
        cx.subscribe(&view, move |_, event: &Event, _| {
            if let Event::OpenUrl(url) = event {
                opened.borrow_mut().push(url.clone());
            }
        })
    });
    let key = |id: &str| {
        ItemKey {
            source: source(Provider::Linear),
            native_id: id.into(),
        }
        .canonical()
    };
    let parent = key("ENG-12");
    cx.update(|_, cx| view.update(cx, |view, cx| view.open_detail(parent.clone(), cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-sub-issues").is_some());
    // One the listing does not hold opens in Linear.
    click_on(cx, "orchestrator-sub-issue-ENG-99");
    assert_eq!(*opened.borrow(), ["https://linear.app/acme/issue/ENG-99"]);
    // A listed one opens its page, whose parent tag leads back.
    click_on(cx, "orchestrator-sub-issue-ENG-13");
    let shown = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| {
            view.read(cx)
                .detail
                .as_ref()
                .map(|detail| detail.key.clone())
        })
    };
    assert_eq!(shown(cx), Some(key("ENG-13")));
    click_on(cx, "orchestrator-parent");
    assert_eq!(shown(cx), Some(parent));
}

#[gpui::test]
fn add_sub_issue_creates_under_the_parent_in_its_team(cx: &mut gpui::TestAppContext) {
    let (_host, view, cx) = open_linear(cx, items(), 1400.);
    let (service, recorder) = Service::recording();
    let options = Options {
        viewer: "u-me".into(),
        teams: vec![
            TeamOptions {
                id: "t-ops".into(),
                key: "OPS".into(),
                name: "Operations".into(),
                ..TeamOptions::default()
            },
            TeamOptions {
                id: "t-eng".into(),
                key: "ENG".into(),
                name: "Engineering".into(),
                ..TeamOptions::default()
            },
        ],
    };
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().service = Some(service);
            view.repo_mut().request.linear = Some(crate::github::Store::Environment);
            view.repo_mut().snapshot.linear_create.options = Some(Ok(Arc::new(options)));
            let parent = ItemKey {
                source: source(Provider::Linear),
                native_id: "ENG-12".into(),
            }
            .canonical();
            view.open_detail(parent, cx);
        })
    });
    cx.run_until_parked();
    click_on(cx, "orchestrator-add-sub-issue");
    assert!(cx.debug_bounds("linear-create-parent").is_some());
    cx.simulate_input("Write docs");
    cx.simulate_keystrokes("cmd-enter");
    cx.run_until_parked();
    let (_, issues) = recorder.linear_asks();
    assert_eq!(
        issues,
        [NewIssue {
            team: "t-eng".into(),
            title: "Write docs".into(),
            parent: Some("uuid-12".into()),
            sequence: 1,
            ..NewIssue::default()
        }]
    );
}
