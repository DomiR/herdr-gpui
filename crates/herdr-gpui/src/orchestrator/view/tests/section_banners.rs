//! Each section's banners report only its own sources: the Linear board
//! does not show the projects' GitHub failures, nor they Linear's.
use super::*;
use crate::orchestrator::{
    Error, Request, SourceStatus, SyncState, Timing,
    view::{IssueSource, Look, OrchestratorView},
};
use gpui::{TestAppContext, VisualTestContext};
use std::sync::Arc;

#[gpui::test]
fn a_sources_failure_shows_only_in_its_own_section(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.path().join("app").to_string_lossy().into_owned(),
        workspace_id: "app".into(),
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
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(600.)));
    let show = |cx: &mut VisualTestContext, failed: Provider, source: IssueSource| {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.repo_mut().snapshot.sources = vec![SourceStatus {
                    key: super::source(failed),
                    state: SyncState::Failed(Arc::new(Error::NoDataDirectory)),
                }];
                view.issue_source = source;
                view.refresh_rows();
                cx.notify();
            })
        });
        cx.run_until_parked();
        cx.debug_bounds("orchestrator-banner").is_some()
    };
    assert!(show(cx, Provider::Github, IssueSource::Repositories));
    assert!(!show(cx, Provider::Github, IssueSource::Linear));
    assert!(show(cx, Provider::Linear, IssueSource::Linear));
    assert!(!show(cx, Provider::Linear, IssueSource::Repositories));
}
