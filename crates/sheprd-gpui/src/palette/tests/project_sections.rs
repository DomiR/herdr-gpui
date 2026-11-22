use super::*;

fn go(label: &str, target: OwnedNavigationTarget) -> Entry {
    Entry::new(
        label.into(),
        String::new(),
        "Workspace",
        Action::Go {
            endpoint: crate::endpoint::LOCAL.into(),
            boot: "boot".into(),
            target,
        },
        None,
    )
}

fn project(label: &str, boost: u32) -> Entry {
    let mut entry = Entry::new(
        label.into(),
        String::new(),
        "Project",
        Action::Project(projects::Project {
            path: format!("/code/{label}").into(),
            label: label.into(),
        }),
        None,
    );
    entry.boost = boost;
    entry
}

fn entries() -> Vec<Entry> {
    vec![
        go("herdr-gpui", NavigationTarget::Workspace("w1".into())),
        go("claude", NavigationTarget::Pane("w1:p1".into())),
        Section::Open.entry(),
        Section::NotOpen.entry(),
        project("herdr", 0),
        project("garden", 9),
    ]
}

/// The rows `filter` lists for `query`, from the rows it accepts as the
/// palette passes them.
fn ranked(filter: Filter, entries: &[Entry], query: &str) -> Vec<search::Hit> {
    let candidates = (0..entries.len())
        .filter(|index| filter.accepts(&entries[*index].action))
        .collect();
    rank_rows(
        filter,
        AgentFilter::All,
        entries,
        candidates,
        &search::Query::parse(query),
        &mut search::matcher(),
    )
}

fn rows(filter: Filter, entries: &[Entry], query: &str) -> Vec<String> {
    ranked(filter, entries, query)
        .iter()
        .map(|hit| match entries[hit.index].action {
            Action::Section(Section::Open) => "open:".into(),
            Action::Section(Section::NotOpen) => "not open:".into(),
            _ => entries[hit.index].label.to_string(),
        })
        .collect()
}

#[test]
fn projects_list_open_workspaces_then_projects_under_their_headings() {
    let entries = entries();
    // With no query the most frecent project leads its section.
    assert_eq!(
        rows(Filter::Projects, &entries, ""),
        ["open:", "herdr-gpui", "not open:", "garden", "herdr"]
    );
    assert_eq!(
        rows(Filter::Projects, &entries, "herdr"),
        ["open:", "herdr-gpui", "not open:", "herdr"]
    );
    // A heading shows only above rows of its own.
    assert_eq!(
        rows(Filter::Projects, &entries, "garden"),
        ["not open:", "garden"]
    );
}

#[test]
fn only_the_projects_filter_searches_projects() {
    let entries = entries();
    for filter in [
        Filter::All,
        Filter::Navigation,
        Filter::Commands,
        Filter::Notes,
    ] {
        let rows = rows(filter, &entries, "");
        assert!(
            rows.iter()
                .all(|row| !["herdr", "garden"].contains(&row.as_str()) && !row.ends_with(':')),
            "{filter:?}: {rows:?}"
        );
    }
    assert_eq!(rows(Filter::All, &entries, "herdr"), ["herdr-gpui"]);
}

#[test]
fn moving_passes_over_headings_and_wraps() {
    let entries = entries();
    // open:, herdr-gpui, not open:, garden, herdr
    let rows = ranked(Filter::Projects, &entries, "");
    assert_eq!(sections::first_row(&entries, &rows), 1);
    assert_eq!(sections::step(&entries, &rows, 1, true), 3);
    assert_eq!(sections::step(&entries, &rows, 3, false), 1);
    assert_eq!(sections::step(&entries, &rows, 4, true), 1);
    assert_eq!(sections::step(&entries, &rows, 1, false), 4);
}

#[gpui::test]
fn projects_are_looked_for_only_once_their_filter_is_shown(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("garden")).unwrap();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.palette.project_roots = vec![root.path().to_string_lossy().into_owned()];
            view.open_palette(Filter::All, window, cx);
            view.filter_palette("garden", cx);
        })
    });
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        let palette = view.menu.palette.as_ref().unwrap();
        assert_eq!(palette.scan, Scan::Waiting);
        assert!(palette.projects.projects.is_empty());
        assert!(palette.searches_elsewhere());
        view.set_palette_filter(Filter::Projects, cx);
        let palette = view.menu.palette.as_ref().unwrap();
        assert_eq!(palette.scan, Scan::Loading);
        assert!(!palette.searches_elsewhere());
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let palette = view.menu.palette.as_ref().unwrap();
        assert_eq!(palette.scan, Scan::Done);
        assert!(matches!(
            palette.entries[palette.filtered[0].index].action,
            Action::Section(Section::NotOpen)
        ));
        assert!(palette.filtered.iter().any(|hit| matches!(
            &palette.entries[hit.index].action,
            Action::Project(project) if project.label == "garden"
        )));
        assert!(!matches!(
            palette.selected_entry().unwrap().action,
            Action::Section(_)
        ));
        // One heading, the rest rows.
        assert_eq!(palette.shown_rows(), palette.filtered.len() - 1);
    });
}

#[gpui::test]
fn a_project_open_in_a_workspace_is_listed_as_that_workspace(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let snapshot = view.live.snapshot.clone().unwrap();
            let root = launch_root(&snapshot, &snapshot.workspaces[0].workspace_id)
                .unwrap()
                .to_owned();
            view.open_palette(Filter::Projects, window, cx);
            let mut palette = view.menu.palette.take().unwrap();
            palette.projects.projects = vec![
                projects::Project {
                    path: root.into(),
                    label: "open".into(),
                },
                projects::Project {
                    path: "/projects/closed".into(),
                    label: "closed".into(),
                },
            ];
            view.prepare_palette_entries(&mut palette, None, cx);
            let listed: Vec<_> = palette
                .base
                .iter()
                .filter_map(|entry| match &entry.action {
                    Action::Project(project) => Some(project.label.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(listed, ["closed"]);
        })
    });
}
