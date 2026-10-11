//! The Connect Linear dialog in a real window: the chip opens it, a key that
//! is not one keeps it open with the reason, and Escape closes it.
use super::*;
use crate::orchestrator::{
    Request, SourceStatus, SyncState, Timing,
    view::{Look, OrchestratorView},
};
use gpui::{Modifiers, TestAppContext};

#[gpui::test]
fn the_linear_chip_opens_a_dialog_that_refuses_a_non_key(cx: &mut TestAppContext) {
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
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::new(request, look, cx));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.snapshot.sources = vec![SourceStatus {
                key: crate::orchestrator::linear::source(""),
                state: SyncState::SignIn,
            }];
            cx.notify();
        })
    });
    cx.run_until_parked();
    let chip = cx
        .debug_bounds("orchestrator-source-linear")
        .unwrap_or_else(|| panic!("the Linear chip"));
    cx.simulate_click(chip.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-overlay-card").is_some());
    assert!(cx.update(|_, cx| view.read(cx).linear.is_some()));

    cx.simulate_input("lin api");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let (typed, problem) = cx.update(|_, cx| {
        let dialog = view.read(cx).linear.as_ref().unwrap();
        (dialog.key.read(cx).text().to_owned(), dialog.problem)
    });
    assert_eq!(typed, "lin api");
    assert_eq!(problem, Some("Paste a Linear personal API key."));

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    let (open, focused) = cx.update(|window, cx| {
        let view = view.read(cx);
        (view.linear.is_some(), view.focus.is_focused(window))
    });
    assert_eq!((open, focused), (false, true));
    assert!(cx.debug_bounds("orchestrator-overlay-card").is_none());
}
