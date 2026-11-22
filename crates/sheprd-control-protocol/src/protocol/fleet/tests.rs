#![allow(clippy::unwrap_used)]
use super::*;
use crate::protocol::{Call, Caller, Request, Response};

#[test]
fn fleet_requests_have_a_stable_wire_form() {
    let request = Request::Fleet(Call {
        caller: Caller::default(),
        op: FleetOp::Remove {
            member: "studio".into(),
        },
    });
    let text = serde_json::to_string(&request).unwrap();
    assert_eq!(
        text,
        r#"{"method":"fleet","caller":{"pane_id":null,"workspace_id":null,"daemon_socket":null},"op":{"op":"remove","member":"studio"}}"#
    );
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    for (text, op) in [
        (r#"{"op":"status"}"#, FleetOp::Status),
        (r#"{"op":"open"}"#, FleetOp::Open),
        (
            r#"{"op":"scrub","machine":"build-01"}"#,
            FleetOp::Scrub {
                machine: "build-01".into(),
            },
        ),
        (
            r#"{"op":"remove_host","host":"build-01"}"#,
            FleetOp::RemoveHost {
                host: "build-01".into(),
            },
        ),
    ] {
        let request: Request = serde_json::from_str(&format!(
            r#"{{"method":"fleet","caller":{{}},"op":{text}}}"#
        ))
        .unwrap();
        assert_eq!(
            request,
            Request::Fleet(Call {
                caller: Caller::default(),
                op,
            })
        );
    }
    assert!(serde_json::from_str::<FleetOp>(r#"{"op":"found"}"#).is_err());
    assert_eq!(FleetOp::Status.wait_seconds(), 12);
    assert_eq!(FleetOp::Open.wait_seconds(), 0);
}

#[test]
fn removals_wait_for_the_daemon_they_read_first() {
    let remove = FleetOp::Remove {
        member: "studio".into(),
    };
    let remove_host = FleetOp::RemoveHost {
        host: "build-01".into(),
    };
    assert_eq!(remove.wait_seconds(), FleetOp::Status.wait_seconds());
    assert_eq!(remove_host.wait_seconds(), FleetOp::Status.wait_seconds());
}

#[test]
fn fleet_replies_read_back() {
    let response = Response::Fleet {
        reply: Reply::Status {
            view: None,
            error: Some("off".into()),
        },
    };
    let text = serde_json::to_string(&response).unwrap();
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
}
