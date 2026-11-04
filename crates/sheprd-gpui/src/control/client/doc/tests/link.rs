use crate::control::protocol::doc::{LinkMode, LinkPending, LinkStatus, Reply};

fn status(mode: LinkMode) -> LinkStatus {
    LinkStatus {
        acked: Some(7),
        agent: Some("claude".into()),
        mode,
        pane: Some("pane-1".into()),
        pending: None,
    }
}

fn text(status: LinkStatus) -> String {
    super::super::message(&Reply::Link {
        path: "/a.md".into(),
        status,
    })
    .0
}

#[test]
fn link_text_covers_each_mode() {
    assert_eq!(text(status(LinkMode::Off)), "off");
    assert_eq!(text(status(LinkMode::Starting)), "starting");
    assert_eq!(text(status(LinkMode::Restoring)), "restoring");
    assert_eq!(
        text(status(LinkMode::On)),
        "on: agent claude in pane pane-1, pushed through version 7"
    );
    assert_eq!(
        text(status(LinkMode::Paused)),
        "paused: agent claude in pane pane-1, pushed through version 7"
    );
}

#[test]
fn link_text_adds_pending_and_in_flight_suffixes() {
    let mut pending = status(LinkMode::On);
    pending.pending = Some(LinkPending {
        from: 7,
        in_flight: false,
        to: 9,
    });
    assert_eq!(
        text(pending.clone()),
        "on: agent claude in pane pane-1, pushed through version 7, versions 7 to 9 pending"
    );
    pending.pending.as_mut().unwrap().in_flight = true;
    assert_eq!(
        text(pending),
        "on: agent claude in pane pane-1, pushed through version 7, versions 7 to 9 being sent"
    );
}

#[test]
fn link_text_omits_missing_parts_and_json_keeps_wire_keys() {
    let reply = Reply::Link {
        path: "/a.md".into(),
        status: LinkStatus {
            acked: None,
            agent: None,
            mode: LinkMode::On,
            pane: None,
            pending: None,
        },
    };
    assert_eq!(text(reply_status(&reply).clone()), "on");
    let json = serde_json::to_value(&reply).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "kind": "link",
            "path": "/a.md",
            "status": {"mode": "on"}
        })
    );
}

fn reply_status(reply: &Reply) -> &LinkStatus {
    let Reply::Link { status, .. } = reply else {
        unreachable!();
    };
    status
}
