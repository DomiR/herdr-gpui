//! The Pull requests list grouped by who opened each, and narrowed to the
//! people picked.
use super::*;
use crate::orchestrator::{
    Error, PullRequest, Request, Timing,
    view::{
        Look, OrchestratorView,
        creators::{Grouping, creator_lines},
        list::Empty,
    },
};
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use std::{collections::HashSet, sync::Arc};

fn pull(number: u64, author: Option<&str>, updated: &str) -> Item {
    let mut pull = item(Provider::Github, &format!("pr/{number}"), "Change", updated);
    pull.identifier = format!("#{number}");
    pull.author = author.map(str::to_owned);
    pull.pull_request = Some(PullRequest {
        number,
        additions: None,
        deletions: None,
        base_ref: "main".into(),
        head_ref: format!("b-{number}"),
        base_sha: String::new(),
        head_sha: String::new(),
        head_repository: None,
    });
    pull
}

/// Each line as text: a creator heading as its author (`?` for none) and
/// count, another heading as its debug form, a row as its identifier.
fn describe<'a>(lines: &[Line], item: impl Fn(&ItemRow) -> &'a Item) -> Vec<String> {
    lines
        .iter()
        .map(|line| match line {
            Line::Heading {
                heading: Heading::Creator(row),
                count,
            } => format!("{}:{count}", item(row).author.as_deref().unwrap_or("?")),
            Line::Heading { heading, count } => format!("{heading:?}:{count}"),
            Line::Row(row) => item(row).identifier.clone(),
        })
        .collect()
}

#[test]
fn creators_head_their_pull_requests_by_name_with_no_author_last() {
    let items = [
        pull(1, Some("leo"), "2026-10-05T00:00:00Z"),
        pull(2, None, "2026-10-04T00:00:00Z"),
        pull(3, Some("Zoe"), "2026-10-03T00:00:00Z"),
        pull(4, Some("leo"), "2026-10-02T00:00:00Z"),
        pull(5, Some("amy"), "2026-10-01T00:00:00Z"),
    ];
    let rows: Vec<ItemRow> = (0..items.len())
        .map(|item| ItemRow {
            repo: 0,
            item,
            depth: 0,
            children: None,
            run: None,
        })
        .collect();
    let lines = creator_lines(&rows, |row| items[row.item].author.as_deref());
    // Names compare regardless of case, so Zoe follows leo; each group
    // keeps the rows' own order.
    assert_eq!(
        describe(&lines, |row| &items[row.item]),
        [
            "amy:1", "#5", "leo:2", "#1", "#4", "Zoe:1", "#3", "?:1", "#2",
        ]
    );
}

fn request(dir: &std::path::Path) -> Request {
    Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.join("app").to_string_lossy().into_owned(),
        workspace_id: "app".into(),
        token: None,
        data_root: Some(dir.join("data")),
        timing: Timing::default(),
        linear: None,
    }
}

#[gpui::test]
fn pull_requests_group_by_creator_and_narrow_to_the_people_picked(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let request = request(dir.path());
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(600.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repos[0].snapshot.items = Arc::new(vec![
                pull(1, Some("leo"), "2026-10-04T00:00:00Z"),
                pull(2, Some("amy"), "2026-10-03T00:00:00Z"),
                pull(3, Some("leo"), "2026-10-02T00:00:00Z"),
                pull(4, None, "2026-10-01T00:00:00Z"),
                pull(5, Some("bob"), "2026-10-05T00:00:00Z"),
            ]);
            view.tab = Tab::PullRequests;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    let shown = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let view = view.read(cx);
            describe(&view.lines, |row| view.row_item(row))
        })
    };
    let click = |cx: &mut VisualTestContext, selector: &'static str| {
        let at = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector}"))
            .center();
        cx.simulate_click(at, Modifiers::none());
        cx.run_until_parked();
    };
    // By status, the default: nothing is reviewed or the user's, so all wait.
    assert_eq!(shown(cx), ["Next(Wait):5", "#5", "#1", "#2", "#3", "#4"]);
    // The sort menu offers the grouping on the Pull requests list.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.sort_open = true;
            cx.notify();
        })
    });
    cx.run_until_parked();
    click(cx, "orchestrator-group-Creator");
    assert_eq!(cx.update(|_, cx| view.read(cx).grouping), Grouping::Creator);
    assert!(!cx.update(|_, cx| view.read(cx).sort_open));
    assert_eq!(
        shown(cx),
        [
            "amy:1", "#2", "bob:1", "#5", "leo:2", "#1", "#3", "?:1", "#4",
        ]
    );
    // Several people narrow the list to theirs; the counts stay everyone's.
    assert!(cx.debug_bounds("orchestrator-author-pills").is_some());
    click(cx, "orchestrator-pill-author-leo");
    click(cx, "orchestrator-pill-author-bob");
    assert_eq!(shown(cx), ["bob:1", "#5", "leo:2", "#1", "#3"]);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).authors.clone()),
        HashSet::from(["bob".to_owned(), "leo".to_owned()])
    );
    assert_eq!(
        cx.update(|_, cx| view.read(cx).people()),
        [
            ("amy".to_owned(), 1),
            ("bob".to_owned(), 1),
            ("leo".to_owned(), 2),
        ]
    );
    // Clicking a picked person again drops them.
    click(cx, "orchestrator-pill-author-bob");
    assert_eq!(shown(cx), ["leo:2", "#1", "#3"]);
    // Everyone clears the pick.
    click(cx, "orchestrator-pill-authors-everyone");
    assert!(cx.update(|_, cx| view.read(cx).authors.is_empty()));
    assert_eq!(
        shown(cx),
        [
            "amy:1", "#2", "bob:1", "#5", "leo:2", "#1", "#3", "?:1", "#4",
        ]
    );
}

#[gpui::test]
fn a_person_picked_stays_pickable_once_none_of_theirs_is_listed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let request = request(dir.path());
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(600.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repos[0].snapshot.items = Arc::new(vec![
                pull(1, Some("leo"), "2026-10-04T00:00:00Z"),
                pull(2, Some("amy"), "2026-10-03T00:00:00Z"),
            ]);
            // Amy's pull request merged, so Open hides it.
            Arc::make_mut(&mut view.repos[0].snapshot.items)[1].state = "merged".into();
            // Settled, so the list is not still loading.
            view.repos[0].snapshot.error = Some(Arc::new(Error::NoDataDirectory));
            view.tab = Tab::PullRequests;
            view.authors.insert("amy".into());
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| view.read(cx).lines.is_empty()));
    assert_eq!(cx.update(|_, cx| view.read(cx).empty()), Empty::Filtered);
    // Only Leo's is open, yet Amy's pill stays so the pick can be undone.
    assert_eq!(
        cx.update(|_, cx| view.read(cx).people()),
        [("amy".to_owned(), 0), ("leo".to_owned(), 1)]
    );
    assert!(cx.debug_bounds("orchestrator-pill-author-amy").is_some());
    let at = cx
        .debug_bounds("orchestrator-pill-author-amy")
        .unwrap()
        .center();
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
    assert!(cx.update(|_, cx| view.read(cx).authors.is_empty()));
    assert_eq!(cx.update(|_, cx| view.read(cx).keys().len()), 1);
    // The Issues list has no people pills.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.tab = Tab::Issues;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-author-pills").is_none());
}
