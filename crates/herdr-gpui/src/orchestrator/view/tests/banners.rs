//! A banner's close button hides it until it says something else, or until
//! its source syncs and fails again.
use super::*;
use crate::orchestrator::{
    Error, Request, SourceStatus, SyncState, Timing,
    view::{Look, OrchestratorView},
};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use std::sync::Arc;

fn set(view: &Entity<OrchestratorView>, cx: &mut VisualTestContext, state: SyncState) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().snapshot.sources = vec![SourceStatus {
                key: source(Provider::Github),
                state,
            }];
            // As a poll that read a snapshot does.
            view.dismissed.settle(&view.repos);
            cx.notify();
        })
    });
    cx.run_until_parked();
}

fn failed(error: Error) -> SyncState {
    SyncState::Failed(Arc::new(error))
}

fn shown(cx: &mut VisualTestContext) -> bool {
    cx.debug_bounds("orchestrator-banner").is_some()
}

fn close(cx: &mut VisualTestContext) {
    let at = cx
        .debug_bounds("orchestrator-banner-close")
        .unwrap()
        .center();
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn a_closed_banner_stays_closed_until_its_trouble_changes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.path().to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: None,
        data_root: Some(dir.path().join("data")),
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

    set(&view, cx, failed(Error::LinearKeyMissing));
    assert!(shown(cx));
    close(cx);
    assert!(!shown(cx));
    // Closing is not the banner's own click, which syncs again.
    assert!(cx.update(|_, cx| view.read(cx).refresh_asked.is_none()));

    // The same failure after another try stays closed.
    set(&view, cx, SyncState::Syncing { count: None });
    set(&view, cx, failed(Error::LinearKeyMissing));
    assert!(!shown(cx));

    // A different failure shows.
    set(&view, cx, failed(Error::LinearKeyRejected));
    assert!(shown(cx));
    close(cx);
    assert!(!shown(cx));

    // The same failure after a sync that succeeded shows again.
    set(&view, cx, SyncState::Capped { count: 3 });
    set(
        &view,
        cx,
        SyncState::Synced {
            at: at("2026-10-27T00:00:00Z"),
            count: 3,
        },
    );
    set(&view, cx, failed(Error::LinearKeyRejected));
    assert!(shown(cx));
}
