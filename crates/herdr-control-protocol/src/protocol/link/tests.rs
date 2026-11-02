#![allow(clippy::unwrap_used)]
use super::*;
use crate::protocol::{Call, Caller, Request, Response};

#[test]
fn link_make_round_trips() {
    let request = Request::LinkMake(Call {
        caller: Caller::default(),
        op: LinkMakeOp {
            target: LinkTarget::Tab,
            pane_id: Some("w1:p2".into()),
        },
    });
    let text = serde_json::to_string(&request).unwrap();
    assert_eq!(
        text,
        r#"{"method":"link.make","caller":{"pane_id":null,"workspace_id":null,"daemon_socket":null},"op":{"target":"tab","pane_id":"w1:p2"}}"#
    );
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);

    let response = Response::Link {
        reply: Reply {
            url: "herdr-gpui://focus?tab=build".into(),
            label: "build".into(),
            markdown: "[build](herdr-gpui://focus?tab=build)".into(),
        },
    };
    let text = serde_json::to_string(&response).unwrap();
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
}

#[test]
fn link_make_rejects_unknown_fields() {
    for invalid in [
        r#"{"method":"link.make","caller":{},"op":{"target":"tab","pane_id":"p","extra":1}}"#,
        r#"{"method":"link.make","caller":{},"op":{"target":"tab","pane":"p"}}"#,
        r#"{"method":"link.make","caller":{},"op":{"target":"build"}}"#,
    ] {
        assert!(
            serde_json::from_str::<Request>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn pane_ids_are_bounded() {
    assert!(
        LinkMakeOp {
            target: LinkTarget::Agent,
            pane_id: Some("w:p".into()),
        }
        .bounded()
    );
    assert!(
        !LinkMakeOp {
            target: LinkTarget::Agent,
            pane_id: Some("x".repeat(MAX_PANE_ID + 1)),
        }
        .bounded()
    );
}
