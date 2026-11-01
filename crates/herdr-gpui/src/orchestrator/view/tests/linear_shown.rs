//! The Linear section shown again after longer than Linear's sync interval
//! syncs Linear at once; shown all along, it waits for the interval.
use crate::orchestrator::{
    Request, Service, Timing,
    view::{Look, OrchestratorView},
};
use gpui::TestAppContext;
use std::time::{Duration, Instant};

#[gpui::test]
fn showing_linear_again_after_its_interval_syncs_it(cx: &mut TestAppContext) {
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
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    let (service, recorder) = Service::recording();
    let start = Instant::now();
    let interval = Timing::default().linear;
    let shown = |cx: &mut gpui::VisualTestContext, at: Instant| {
        cx.update(|_, cx| view.update(cx, |view, _| view.linear_shown(at)));
    };
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.repo_mut().service = Some(service);
            view.repo_mut().request.linear = Some(crate::github::Store::Environment);
        })
    });

    // First shown: the worker syncs as it starts.
    shown(cx, start);
    // Shown on each tick: no sync of its own.
    shown(cx, start + Duration::from_secs(1));
    shown(cx, start + interval);
    assert_eq!(recorder.linear_syncs(), 0);
    // Away for longer than the interval.
    shown(cx, start + interval * 2 + Duration::from_secs(1));
    assert_eq!(recorder.linear_syncs(), 1);
}
