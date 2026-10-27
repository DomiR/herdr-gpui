use super::*;

#[gpui::test]
fn geometry_reconciliation_preserves_focus_and_defers_background_sizes(
    cx: &mut gpui::TestAppContext,
) {
    let (endpoint, mut server) = connected_endpoint("passive-resize");
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.read_with(cx, |fixture, _| fixture.0.clone());
    view.update(cx, |view, cx| {
        view.endpoints = vec![endpoint];
        view.reset_selected(cx);
        view.active = true;
        view.sent_focus = Some(true);
        view.options.cell_width_px += 1;
        let now = Instant::now();
        view.resize_at(now);
        view.resize_at(now + Duration::from_secs(1));
        assert_eq!(view.sent_focus, Some(true));
        view.active = false;
        view.last_input = Some(now);
        view.options.surface_size.cols += 1;
        let queued = view.last_queued_options;
        for tick in 2..10 {
            view.resize_at(now + Duration::from_secs(tick));
        }
        assert_eq!(view.last_queued_options, queued);
        assert!(view.pending_resize.is_none());
        view.endpoints[0]
            .connection
            .handle
            .as_ref()
            .unwrap()
            .set_focus(&snapshot().boot_id, false)
            .unwrap();
    });
    assert!(matches!(
        server.receive(),
        ClientMessage::ClientShellResize { .. }
    ));
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );

    view.update(cx, |view, _| {
        view.active = true;
        let now = Instant::now();
        view.resize_at(now);
        view.resize_at(now + Duration::from_secs(1));
        assert_eq!(view.last_queued_options, Some(view.options));
        view.endpoints[0]
            .connection
            .handle
            .as_ref()
            .unwrap()
            .set_focus(&snapshot().boot_id, true)
            .unwrap();
    });
    assert!(matches!(
        server.receive(),
        ClientMessage::ClientShellResize { .. }
    ));
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: true }
    );
}

#[gpui::test]
fn explicit_input_claims_geometry_once_and_waits_for_its_projection(cx: &mut gpui::TestAppContext) {
    let (endpoint, mut server) = connected_endpoint("input-resize");
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.read_with(cx, |fixture, _| fixture.0.clone());
    view.update(cx, |view, cx| {
        view.endpoints = vec![endpoint];
        view.reset_selected(cx);
        view.active = true;
        view.activation_deadline = None;
        view.pending_toast = None;
        view.options = ConnectOptions::default();
        assert!(view.flush_pending_input(cx));
        view.options.surface_size.cols += 1;
        assert!(view.surface_activation_ready());
        assert!(!view.surface_matches_options());
        view.send(ClientPaneInputEvent::TextCommit("a".into()), cx);
        assert!(view.activation_deadline.is_some());
        assert!(!view.input_ready());
        view.send(ClientPaneInputEvent::Paste("b".into()), cx);
        assert_eq!(
            view.pending_input.len(),
            2,
            "input survives the geometry-only fence"
        );
        view.endpoints[0]
            .connection
            .handle
            .as_ref()
            .unwrap()
            .set_focus(&snapshot().boot_id, false)
            .unwrap();
    });
    assert!(matches!(
        server.receive(),
        ClientMessage::ClientShellResize { .. }
    ));
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("explicit input must claim the tab before held input can resume");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], "tab.focus");
    assert_eq!(
        request["params"]["tab_id"],
        snapshot().focused_tab_id.unwrap()
    );
    server.respond(&request);
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("the input claim must fence its surface");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], "client_shell.surface.set");
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );
    view.update(cx, |view, cx| {
        let mut next = snapshot();
        next.revision += 1;
        let mut frame = surface(&next);
        Arc::make_mut(&mut frame).frame.width = view.options.surface_size.cols;
        {
            let mut state = view.endpoints[0].connection.inbox.lock().unwrap();
            state.apply(ClientEvent::Response {
                request_id: request["id"].as_str().unwrap().into(),
                response: serde_json::json!({"result": {
                    "type": "client_shell_surface_set", "active": true,
                    "projection_revision": next.revision
                }}),
            });
            state.apply(ClientEvent::Snapshot(Arc::new(next)));
            state.apply(ClientEvent::Surface(frame));
        }
        project_until(view, cx, "claimed geometry", HerdrWindow::input_ready);
        assert_eq!(view.pending_input.len(), 0);
    });
    server.respond(&request);
    for event in [
        ClientPaneInputEvent::TextCommit("a".into()),
        ClientPaneInputEvent::Paste("b".into()),
    ] {
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: snapshot().focused_pane_id.unwrap(),
                events: vec![event],
            }
        );
    }
}

#[gpui::test]
fn pointer_motion_and_background_input_do_not_select_a_tab(cx: &mut gpui::TestAppContext) {
    let (endpoint, mut server) = connected_endpoint("passive-input");
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.read_with(cx, |fixture, _| fixture.0.clone());
    view.update(cx, |view, cx| {
        view.endpoints = vec![endpoint];
        view.reset_selected(cx);
        view.active = true;
        view.activation_deadline = None;
        view.options.surface_size.cols += 1;
        view.note_pointer(point(px(10.), px(10.)));
        view.active = false;
        view.note_input();
        view.endpoints[0]
            .connection
            .handle
            .as_ref()
            .unwrap()
            .set_focus(&snapshot().boot_id, false)
            .unwrap();
    });
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );
}
