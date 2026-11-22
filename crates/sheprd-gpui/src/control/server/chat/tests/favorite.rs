use super::*;
use crate::{
    agent_favorites::AgentFavorites, control::protocol::app::Switch, sidebar::layout_tests,
};
use std::sync::Arc;

fn favorite(switch: Switch, pane: &str) -> ChatOp {
    ChatOp::Favorite {
        switch,
        pane: Some(pane.into()),
    }
}

#[gpui::test]
fn favorite_needs_a_window(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        assert_eq!(
            code(&serve(call(favorite(Switch::On, "p0")), cx)),
            Some(ErrorCode::NoWindow)
        );
    });
}

#[gpui::test]
fn a_pane_agent_is_starred_and_unstarred_on_its_host(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.cx.update(|cx| {
        AgentFavorites::install(cx);
        view.update(cx, |view, _| {
            view.live.snapshot = Some(Arc::new(layout_tests::snapshot(1)));
        });
        for (switch, expected) in [
            (Switch::On, true),
            (Switch::On, true),
            (Switch::Toggle, false),
            (Switch::Toggle, true),
            (Switch::Off, false),
        ] {
            assert_eq!(
                serve(call(favorite(switch, "p0")), cx),
                Response::Chat {
                    reply: Reply::Favorite {
                        pane_id: "p0".into(),
                        favorite: expected,
                    }
                },
                "{switch:?}"
            );
            // The window hears of it as the panel does.
            let shown = AgentFavorites::current(cx);
            view.update(cx, |view, _| view.agent_favorites = shown.clone());
            assert_eq!(
                shown.contains(crate::endpoint::LOCAL, "p0"),
                expected,
                "{switch:?}"
            );
        }
    });
}

#[gpui::test]
fn a_pane_without_an_agent_cannot_be_starred(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.cx.update(|cx| {
        AgentFavorites::install(cx);
        view.update(cx, |view, _| {
            view.live.snapshot = Some(Arc::new(layout_tests::snapshot(1)));
        });
        for pane in ["no-such-pane", "core:t9"] {
            assert_eq!(
                code(&serve(call(favorite(Switch::On, pane)), cx)),
                Some(ErrorCode::NotFound),
                "{pane}"
            );
        }
        assert!(!AgentFavorites::current(cx).any());
    });
}
