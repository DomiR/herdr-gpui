//! One worker lists Linear however many projects a view lists, as Linear is
//! the user's rather than a project's.
use crate::{
    github::Store,
    orchestrator::{
        Request, Timing,
        view::{Look, OrchestratorView, RepoSpec},
    },
};
use gpui::{TestAppContext, VisualTestContext};
use std::path::Path;

/// A project whose checkout is missing, so its worker stops before a sync.
fn spec(dir: &Path, name: &str) -> RepoSpec {
    RepoSpec {
        key: name.into(),
        host: "local".into(),
        request: Request {
            target: herdr_client::ConnectTarget::Local,
            checkout: dir.join(name).to_string_lossy().into_owned(),
            workspace_id: name.into(),
            token: None,
            data_root: Some(dir.join("data")),
            timing: Timing::default(),
            linear: Some(Store::Environment),
        },
    }
}

#[gpui::test]
fn one_project_lists_linear_and_the_first_takes_over_when_it_closes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::new(look, cx));
    let set = |cx: &mut VisualTestContext, names: &[&str]| {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                let specs = names.iter().map(|name| spec(dir.path(), name)).collect();
                view.set_repos(specs, cx);
            })
        });
    };
    let owners = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            view.read(cx)
                .repos
                .iter()
                .filter(|repo| repo.request.linear.is_some())
                .map(|repo| repo.key.clone())
                .collect::<Vec<_>>()
        })
    };
    set(cx, &["app", "site"]);
    assert_eq!(owners(cx), ["app"]);
    // A project listed ahead of the one listing Linear starts without it.
    set(cx, &["new", "app", "site"]);
    assert_eq!(owners(cx), ["app"]);
    // The project listing Linear closed: the first takes it over.
    set(cx, &["new", "site"]);
    assert_eq!(owners(cx), ["new"]);
}
