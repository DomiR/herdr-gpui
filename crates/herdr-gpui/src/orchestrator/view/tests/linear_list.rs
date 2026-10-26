//! The Linear section's list reads as Linear's own: a heading per assignee,
//! the user first and nobody last, a heading per workflow state inside, each
//! with its estimates summed, and rows with chips for linked pull requests,
//! the project, and the estimate.
use super::*;
use crate::orchestrator::{
    LinkState, LinkedPullRequest, Project, Status, StatusKind, Tracker,
    view::{Event, OrchestratorView, linear_groups, linear_parts},
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{cell::RefCell, collections::HashSet, rc::Rc};

fn state(name: &str, kind: StatusKind, rank: i64) -> Status {
    Status {
        name: name.into(),
        kind,
        rank,
        color: Some(0x4ea7fc),
        id: None,
    }
}

fn todo() -> Option<Status> {
    Some(state("Todo", StatusKind::Unstarted, 0))
}

fn in_review() -> Option<Status> {
    Some(state("In Review", StatusKind::Started, 2000))
}

/// Issue `id` assigned to `assignee`, the user when `mine`.
fn issue(
    id: &str,
    assignee: Option<&str>,
    mine: bool,
    status: Option<Status>,
    estimate: Option<u32>,
) -> Item {
    let mut issue = item(Provider::Linear, id, "Task", "2026-10-06T00:00:00Z");
    issue.tracker = Some(Tracker {
        assignee: assignee.map(Into::into),
        mine,
        status,
        estimate,
        ..Tracker::default()
    });
    issue
}

fn listing() -> Vec<Item> {
    let mut items = vec![
        issue("ENG-1", Some("Zoe"), false, in_review(), Some(2)),
        issue("ENG-2", Some("Dominique"), true, todo(), Some(3)),
        issue("ENG-3", None, false, todo(), None),
        issue(
            "ENG-4",
            Some("alice"),
            false,
            Some(state("In Progress", StatusKind::Started, 1000)),
            Some(1),
        ),
        issue("ENG-5", Some("Dominique"), true, in_review(), Some(5)),
        issue("ENG-6", Some("Dominique"), true, todo(), None),
        issue("ENG-7", Some("Zoe"), false, None, None),
    ];
    items[5].parent_id = Some("ENG-2".into());
    items
}

/// The rows as the Issues list hands them over: ENG-6 under its parent.
fn rows(items: &[Item]) -> Vec<ItemRow> {
    (0..items.len())
        .map(|index| ItemRow {
            repo: 0,
            item: index,
            depth: u8::from(index == 5),
            children: (index == 1).then_some(true),
            run: None,
        })
        .collect()
}

fn describe(lines: &[Line], items: &[Item]) -> Vec<String> {
    let name = |row: &ItemRow| {
        let item = &items[row.item];
        let tracker = item.tracker.as_ref().unwrap();
        (
            tracker.assignee.clone().unwrap_or("nobody".into()),
            tracker
                .status
                .as_ref()
                .map_or("none".into(), |status| status.name.clone()),
        )
    };
    lines
        .iter()
        .map(|line| match line {
            Line::Heading {
                heading: Heading::Assignee(group),
                count,
            } => format!("{}:{count}:{:?}", name(&group.row).0, group.estimate),
            Line::Heading {
                heading: Heading::State(group),
                count,
            } => format!("  {}:{count}:{:?}", name(&group.row).1, group.estimate),
            Line::Heading { heading, .. } => format!("{heading:?}"),
            Line::Row(row) => format!("    {}", items[row.item].identifier),
        })
        .collect()
}

#[test]
fn groups_put_the_user_first_and_nobody_last_with_states_in_board_order() {
    let items = listing();
    let lines = linear_groups::lines(
        &rows(&items),
        |row| &items[row.item],
        Sort::Updated,
        &HashSet::new(),
    );
    assert_eq!(
        describe(&lines, &items),
        [
            "Dominique:3:Some(8)",
            "  Todo:2:Some(3)",
            "    ENG-2",
            "    ENG-6",
            "  In Review:1:Some(5)",
            "    ENG-5",
            "alice:1:Some(1)",
            "  In Progress:1:Some(1)",
            "    ENG-4",
            "Zoe:2:Some(2)",
            "  In Review:1:Some(2)",
            "    ENG-1",
            "  none:1:None",
            "    ENG-7",
            "nobody:1:None",
            "  Todo:1:None",
            "    ENG-3",
        ]
    );
    // A sub-issue stands on its own row, which names its parent.
    for line in &lines {
        if let Line::Row(row) = line {
            assert_eq!((row.depth, row.children), (0, None));
        }
    }
}

#[test]
fn a_folded_group_keeps_its_heading_and_hides_what_is_under_it() {
    let items = listing();
    let collapsed = HashSet::from([
        linear_groups::state_key(&items[1]),
        linear_groups::assignee_key(&items[0]),
    ]);
    assert_eq!(
        collapsed,
        HashSet::from(["group:me / Todo".into(), "group:Zoe".into()])
    );
    let lines = linear_groups::lines(
        &rows(&items),
        |row| &items[row.item],
        Sort::Updated,
        &collapsed,
    );
    assert_eq!(
        describe(&lines, &items),
        [
            "Dominique:3:Some(8)",
            "  Todo:2:Some(3)",
            "  In Review:1:Some(5)",
            "    ENG-5",
            "alice:1:Some(1)",
            "  In Progress:1:Some(1)",
            "    ENG-4",
            "Zoe:2:Some(2)",
            "nobody:1:None",
            "  Todo:1:None",
            "    ENG-3",
        ]
    );
    let folded = lines.iter().find_map(|line| match line {
        Line::Heading {
            heading: Heading::Assignee(group),
            ..
        } if !group.open => Some(group.row.item),
        _ => None,
    });
    assert_eq!(folded, Some(0));
}

#[test]
fn estimates_sum_only_what_is_estimated() {
    let items = listing();
    assert_eq!(linear_groups::estimate(&items), Some(11));
    assert_eq!(linear_groups::estimate(&items[5..7]), None);
    assert_eq!(linear_groups::estimate(&items[..0]), None);
    let mut huge = items[0].clone();
    huge.tracker.as_mut().unwrap().estimate = Some(u32::MAX);
    assert_eq!(linear_groups::estimate([&huge, &items[1]]), Some(u32::MAX));
}

fn link(number: u64, state: Option<LinkState>) -> LinkedPullRequest {
    LinkedPullRequest {
        url: format!("https://github.com/acme/app/pull/{number}"),
        number,
        state,
    }
}

#[test]
fn one_pull_request_names_its_number_and_several_count_and_open_the_liveliest() {
    assert_eq!(linear_parts::pull_request_chip(&[]), None);
    let one = [link(9721, Some(LinkState::Closed))];
    let (label, lead) = linear_parts::pull_request_chip(&one).unwrap();
    assert_eq!((label.as_str(), lead.number), ("#9721", 9721));
    let several = [
        link(1, None),
        link(2, Some(LinkState::Merged)),
        link(3, Some(LinkState::Open)),
        link(4, Some(LinkState::Open)),
    ];
    let (label, lead) = linear_parts::pull_request_chip(&several).unwrap();
    assert_eq!((label.as_str(), lead.number), ("4 PRs", 3));
}

fn shows(cx: &mut VisualTestContext, selector: &str) -> bool {
    cx.debug_bounds(selector.to_owned().leak()).is_some()
}

fn drawn() -> Vec<Item> {
    let mut items = listing();
    let tracker = items[4].tracker.as_mut().unwrap();
    tracker.pull_requests = vec![link(10555, Some(LinkState::Open))];
    tracker.project = Some(Project {
        name: "Frontend Standardization".into(),
        color: Some(0x5e6ad2),
    });
    tracker.parent = Some("Auth overhaul".into());
    items[0].tracker.as_mut().unwrap().pull_requests =
        vec![link(1, Some(LinkState::Merged)), link(2, None)];
    items[1].priority = Some(1);
    items[3].priority = Some(3);
    items
}

fn opened_urls(
    view: &Entity<OrchestratorView>,
    cx: &mut VisualTestContext,
) -> Rc<RefCell<Vec<String>>> {
    let opened = Rc::new(RefCell::new(Vec::new()));
    let seen = opened.clone();
    cx.update(|_, cx| {
        cx.subscribe(view, move |_, event: &Event, _| {
            if let Event::OpenUrl(url) = event {
                seen.borrow_mut().push(url.clone());
            }
        })
        .detach()
    });
    opened
}

#[gpui::test]
fn the_list_draws_headings_and_chips_and_a_heading_folds(cx: &mut TestAppContext) {
    let items = drawn();
    let key = |index: usize| items[index].key.canonical();
    let (_host, view, cx) = open_linear(cx, items.clone(), 1400.);
    for selector in [
        "orchestrator-group:me",
        "orchestrator-group:me / Todo",
        "orchestrator-group:me / In Review",
        "orchestrator-group:alice",
        // The last group's heading is below the window's fold.
        "orchestrator-group:Zoe / ",
    ] {
        assert!(shows(cx, selector), "{selector}");
    }
    assert!(shows(cx, &format!("orchestrator-pr-chip-{}", key(4))));
    assert!(shows(cx, &format!("orchestrator-pr-chip-{}", key(0))));
    assert!(!shows(cx, &format!("orchestrator-pr-chip-{}", key(1))));
    // The chip opens its pull request, not the issue's page.
    let opened = opened_urls(&view, cx);
    click_on(cx, &format!("orchestrator-pr-chip-{}", key(4)));
    assert_eq!(*opened.borrow(), ["https://github.com/acme/app/pull/10555"]);
    assert!(cx.update(|_, cx| view.read(cx).detail.is_none()));
    // Folding the user's group hides their rows, from the keys too.
    click_on(cx, "orchestrator-group:me");
    assert!(!shows(cx, &format!("orchestrator-row-{}", key(4))));
    assert!(!shows(cx, "orchestrator-group:me / Todo"));
    let keys = cx.update(|_, cx| view.read(cx).keys());
    assert!(!keys.contains(&key(4)));
    assert!(keys.contains(&key(3)));
    click_on(cx, "orchestrator-group:me");
    assert!(shows(cx, &format!("orchestrator-row-{}", key(4))));
}
