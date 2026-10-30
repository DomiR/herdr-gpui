//! Mine and Current cycle on the Issues tab, and their one-time default.
use super::*;
use crate::orchestrator::IssueSource;
use crate::orchestrator::{
    Cycle, Request, Timing, Tracker,
    view::{Look, OrchestratorView},
};
use gpui::TestAppContext;
use std::{collections::HashSet, sync::Arc};

fn linear(id: &str, updated: &str, mine: bool, cycle: Option<bool>) -> Item {
    let mut issue = item(Provider::Linear, id, "Task", updated);
    issue.tracker = Some(Tracker {
        mine,
        cycle: cycle.map(|active| Cycle {
            number: 3,
            name: None,
            active,
        }),
        ..Tracker::default()
    });
    issue
}

fn listing() -> Vec<Item> {
    vec![
        linear("ENG-1", "2026-10-06T00:00:00Z", true, Some(true)),
        linear("ENG-2", "2026-10-05T00:00:00Z", true, None),
        linear("ENG-3", "2026-10-04T00:00:00Z", false, Some(true)),
        linear("ENG-4", "2026-10-03T00:00:00Z", true, Some(false)),
        item(Provider::Github, "377", "Icons", "2026-10-07T00:00:00Z"),
        item(Provider::Beads, "hg-1", "Bead", "2026-10-08T00:00:00Z"),
    ]
}

fn shown(items: &[Item], filters: Filters) -> Vec<String> {
    let runs = Runs::new(&[], &[], &[]);
    issue_rows(items, &runs, "", filters, Sort::Updated, &HashSet::new())
        .iter()
        .map(|row| items[row.item].identifier.clone())
        .collect()
}

#[test]
fn mine_and_current_cycle_narrow_to_tracked_issues() {
    let items = listing();
    let mine = Filters {
        mine: true,
        ..Filters::default()
    };
    let cycle = Filters {
        cycle: true,
        ..Filters::default()
    };
    let both = Filters {
        mine: true,
        cycle: true,
        ..Filters::default()
    };
    assert_eq!(shown(&items, mine), ["ENG-1", "ENG-2", "ENG-4"]);
    assert_eq!(shown(&items, cycle), ["ENG-1", "ENG-3"]);
    assert_eq!(shown(&items, both), ["ENG-1"]);
    assert_eq!(
        shown(&items, Filters::default()),
        ["hg-1", "377", "ENG-1", "ENG-2", "ENG-3", "ENG-4"]
    );
}

#[test]
fn search_matches_the_cycle_and_the_assignee() {
    let mut issue = linear("ENG-1", "2026-10-06T00:00:00Z", true, Some(true));
    if let Some(tracker) = issue.tracker.as_mut() {
        tracker.assignee = Some("Dominique Rau".into());
    }
    assert_eq!(
        [
            matches(&issue, "cycle 3"),
            matches(&issue, "dominique"),
            matches(&issue, "cycle 4"),
        ],
        [true, true, false]
    );
}

#[test]
fn linear_items_turn_mine_and_cycle_on() {
    let chosen = Filters {
        has_run: true,
        ..Filters::default()
    };
    let untracked = vec![item(
        Provider::Github,
        "377",
        "Icons",
        "2026-10-07T00:00:00Z",
    )];
    assert_eq!(linear_defaults(chosen, &untracked), chosen);
    assert_eq!(
        linear_defaults(chosen, &listing()),
        Filters {
            has_run: true,
            mine: true,
            cycle: true,
            ..Filters::default()
        }
    );
}

fn view(cx: &mut TestAppContext) -> gpui::Entity<OrchestratorView> {
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.path().to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: None,
        data_root: Some(dir.path().join("data")),
        gitlab_store: Default::default(),
        timing: Timing::default(),
        linear: None,
    };
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let view = cx
        .add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx))
        .0;
    // As the Linear section shows it.
    cx.update(|cx| view.update(cx, |view, _| view.issue_source = IssueSource::Linear));
    view
}

#[gpui::test]
fn the_default_applies_once_and_never_over_a_choice(cx: &mut TestAppContext) {
    let fresh = view(cx);
    let filters = cx.update(|cx| {
        fresh.update(cx, |view, _| {
            view.default_filters();
            let before = view.filters;
            view.repo_mut().snapshot.items = Arc::new(listing());
            view.default_filters();
            let defaulted = view.filters;
            view.filters.mine = false;
            view.filters.cycle = false;
            view.default_filters();
            (before, defaulted, view.filters)
        })
    });
    let on = Filters {
        mine: true,
        cycle: true,
        ..Filters::default()
    };
    assert_eq!(filters, (Filters::default(), on, Filters::default()));

    let chosen = view(cx);
    let filters = cx.update(|cx| {
        chosen.update(cx, |view, _| {
            view.filters_set = true;
            view.repo_mut().snapshot.items = Arc::new(listing());
            view.default_filters();
            view.filters
        })
    });
    assert_eq!(filters, Filters::default());
}

#[gpui::test]
fn linear_issues_list_only_in_their_own_section(cx: &mut TestAppContext) {
    let view = view(cx);
    let shown = cx.update(|cx| {
        view.update(cx, |view, cx| {
            let mut items = listing();
            items.push(item(Provider::Github, "7", "Bug", "2026-10-01T00:00:00Z"));
            view.repo_mut().snapshot.items = Arc::new(items);
            view.tab = Tab::Issues;
            view.filters = Filters::default();
            view.refresh_rows();
            let linear = view.keys().len();
            // Issues: the repositories' own, with filters of their own.
            view.set_issue_source(IssueSource::Repositories, cx);
            let repositories = (view.keys().len(), view.filters);
            view.set_issue_source(IssueSource::Linear, cx);
            (linear, repositories, view.keys().len())
        })
    });
    // The fixture's GitHub issue and bead, and the GitHub issue added here.
    assert_eq!(shown, (4, (3, Filters::default()), 4));
}

#[gpui::test]
fn linear_defaults_never_narrow_the_repositories_issues(cx: &mut TestAppContext) {
    let view = view(cx);
    let filters = cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.set_issue_source(IssueSource::Repositories, cx);
            view.repo_mut().snapshot.items = Arc::new(listing());
            view.default_filters();
            let issues = view.filters;
            view.set_issue_source(IssueSource::Linear, cx);
            (issues, view.filters.mine, view.filters.cycle)
        })
    });
    assert_eq!(filters, (Filters::default(), true, true));
}
