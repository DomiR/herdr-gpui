use super::*;

/// A remote reply is deliberately withheld. Both the worktree highlight and
/// its last screen must change anyway, while terminal input remains fenced.
#[gpui::test]
fn cached_navigation_is_visible_before_the_server_answers(cx: &mut gpui::TestAppContext) {
    let (endpoint, mut server) = connected_endpoint_reporting("cached-navigation", Some("0.9.5"));
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.read_with(cx, |fixture, _| fixture.0.clone());
    view.update(cx, |view, cx| {
        view.endpoints = vec![endpoint];
        view.options = ConnectOptions::default();
        view.reset_selected(cx);
        let mut destination = view.live.clone();
        let snapshot = Arc::make_mut(destination.snapshot.as_mut().unwrap());
        // This transport fixture has no painted panes. The cache validates
        // pane membership, so keep its metadata consistent with that picture.
        snapshot.panes.clear();
        let mut other = snapshot.workspaces[0].clone();
        other.workspace_id = "other".into();
        other.active_tab_id = "other-tab".into();
        other.focused = false;
        snapshot.workspaces.push(other);
        let mut tab = snapshot.tabs[0].clone();
        tab.tab_id = "other-tab".into();
        tab.workspace_id = "other".into();
        tab.focused = false;
        snapshot.tabs.push(tab);
        // We previously visited w1, then moved to the other workspace.
        view.presentation.remember(&destination);
        let remembered = destination.surface.clone().unwrap();
        view.live = destination.clone();
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.focused_workspace_id = Some("other".into());
        snapshot.focused_tab_id = Some("other-tab".into());
        snapshot.workspaces[0].focused = false;
        snapshot.workspaces[1].focused = true;
        Arc::make_mut(view.live.surface.as_mut().unwrap()).surface_revision += 1;
        *view.endpoints[0].connection.inbox.lock().unwrap() = view.live.clone();
        assert!(view.navigate_endpoint("cached-navigation", NavigationTarget::Workspace("w1"), cx));
        assert!(!view.input_ready());
        let current = view.live.snapshot.as_ref().unwrap();
        assert_eq!(current.focused_workspace_id.as_deref(), Some("other"));
        assert!(view.workspace_selected(0, &current.workspaces[0]));
        assert!(!view.workspace_selected(0, &current.workspaces[1]));
        let target = view.navigation_preview().cloned().unwrap();
        let picture = view
            .presentation
            .navigation_picture(&view.live, Some(target.as_deref()), (80, 24))
            .unwrap();
        assert!(Arc::ptr_eq(&picture.frame, &remembered));
        view.send(
            ClientPaneInputEvent::TextCommit("must not reach old pane".into()),
            cx,
        );
        view.endpoints[0]
            .connection
            .handle
            .as_ref()
            .unwrap()
            .set_focus(&view.live.snapshot.as_ref().unwrap().boot_id, false)
            .unwrap();
    });
    for expected in ["workspace.focus", "client_shell.surface.set"] {
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("expected navigation request");
        };
        let request: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["method"], expected);
    }
    // Neither response was sent; the FIFO marker proves typing was not sent.
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );
    view.update(cx, |view, cx| {
        assert!(view.navigate_endpoint(
            "cached-navigation",
            NavigationTarget::Workspace("other"),
            cx
        ));
        assert_eq!(
            view.pending_navigation,
            Some(NavigationTarget::Workspace("other".into()))
        );
        let snapshot = view.live.snapshot.as_ref().unwrap();
        assert!(view.workspace_selected(0, &snapshot.workspaces[1]));
        assert!(!view.workspace_selected(0, &snapshot.workspaces[0]));
        let target = view.navigation_preview().cloned().unwrap();
        let picture = view
            .presentation
            .navigation_picture(&view.live, Some(target.as_deref()), (80, 24))
            .unwrap();
        assert_eq!(picture.frame.surface_revision, 2);
    });
}
