use super::*;

#[test]
fn parked_focus_drift_is_not_a_new_selection() {
    let mut peer = MockPeer::advertising(&["tab.focus"]);
    let snapshot = fixture_snapshot();
    let workspace = snapshot.focused_workspace_id.clone().unwrap();
    let tab = snapshot.focused_tab_id.clone().unwrap();
    let boot = snapshot.boot_id.clone();
    let mut ids = crate::browser::GroupIds::default();
    let mut parked = parked(
        ids.next(),
        (Scope::endpoint("local"), workspace),
        herdr_client::ConnectTarget::Socket("/unused-parked.sock".into()),
        &peer,
        snapshot,
    );
    let now = Instant::now();
    parked.steer(&tab, now, true);
    assert_eq!(
        peer.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );
    Arc::make_mut(parked.live.snapshot.as_mut().unwrap()).focused_tab_id = Some("other".into());
    for tick in 1..10 {
        parked.steer(&tab, now + Duration::from_secs(tick), true);
    }
    peer.client.handle.set_focus(&boot, false).unwrap();
    assert_eq!(
        peer.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );
    // A genuinely different group target still gets its initial selection.
    parked.steer("new-tab", now + Duration::from_secs(11), true);
    let ClientMessage::ClientShellEndpointRequest { request, .. } = peer.receive() else {
        panic!("a new group target must still be selected");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], "tab.focus");
    assert_eq!(request["params"]["tab_id"], "new-tab");
}

#[test]
fn background_groups_defer_selection_and_changed_geometry() {
    let mut peer = MockPeer::advertising(&["tab.focus"]);
    let snapshot = fixture_snapshot();
    let workspace = snapshot.focused_workspace_id.clone().unwrap();
    let boot = snapshot.boot_id.clone();
    let mut ids = crate::browser::GroupIds::default();
    let mut parked = parked(
        ids.next(),
        (Scope::endpoint("local"), workspace),
        herdr_client::ConnectTarget::Socket("/unused-parked.sock".into()),
        &peer,
        snapshot,
    );
    parked.options.surface_size.cols += 1;
    let now = Instant::now();
    for tick in 0..10 {
        parked.steer("other", now + Duration::from_secs(tick), false);
    }
    assert_eq!(
        peer.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );
    peer.client.handle.set_focus(&boot, false).unwrap();
    assert_eq!(
        peer.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );
    assert!(parked.asked.is_none());
    assert!(parked.pending_resize.is_none());
    parked.steer("other", now + Duration::from_secs(10), true);
    assert_eq!(parked.asked.as_deref(), Some("other"));
    assert!(parked.pending_resize.is_some());
    assert!(matches!(
        peer.receive(),
        ClientMessage::ClientShellEndpointRequest { .. }
    ));
}
