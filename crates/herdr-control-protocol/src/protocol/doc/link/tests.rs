#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::protocol::doc::{DocOp, Reply};

#[test]
fn link_operations_round_trip_with_optional_off() {
    let on = DocOp::Link {
        path: "/a.md".into(),
        off: false,
    };
    let text = serde_json::to_string(&on).unwrap();
    assert_eq!(text, r#"{"op":"link","path":"/a.md"}"#);
    assert_eq!(serde_json::from_str::<DocOp>(&text).unwrap(), on);

    let off = DocOp::Link {
        path: "/a.md".into(),
        off: true,
    };
    let text = serde_json::to_string(&off).unwrap();
    assert_eq!(text, r#"{"op":"link","path":"/a.md","off":true}"#);
    assert_eq!(serde_json::from_str::<DocOp>(&text).unwrap(), off);
}

#[test]
fn link_replies_round_trip_with_full_and_minimal_status() {
    let full = Reply::Link {
        path: "/a.md".into(),
        status: LinkStatus {
            acked: Some(3),
            agent: Some("agent".into()),
            mode: LinkMode::On,
            pane: Some("pane".into()),
            pending: Some(LinkPending {
                from: 3,
                in_flight: true,
                to: 4,
            }),
        },
    };
    let text = serde_json::to_string(&full).unwrap();
    assert_eq!(serde_json::from_str::<Reply>(&text).unwrap(), full);

    let minimal: Reply =
        serde_json::from_str(r#"{"kind":"link","path":"/a.md","status":{"mode":"off"}}"#).unwrap();
    assert_eq!(
        minimal,
        Reply::Link {
            path: "/a.md".into(),
            status: LinkStatus {
                acked: None,
                agent: None,
                mode: LinkMode::Off,
                pane: None,
                pending: None,
            },
        }
    );
}

#[test]
fn old_document_operations_still_parse() {
    let operation: DocOp = serde_json::from_str(r#"{"op":"sections","path":"/a.md"}"#).unwrap();
    assert_eq!(
        operation,
        DocOp::Sections {
            path: "/a.md".into()
        }
    );
}
