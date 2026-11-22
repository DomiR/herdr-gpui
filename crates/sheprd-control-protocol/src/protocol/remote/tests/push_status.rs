//! `remote push-status`: its request, its reply, and how long it waits.

use super::*;
use sheprd_remote_control::control::push::{Last, Mode, Outcome, PushStatus};

#[test]
fn push_status_has_a_stable_wire_form_and_round_trips() {
    let request: Request =
        serde_json::from_str(r#"{"method":"remote","caller":{},"op":{"op":"push_status"}}"#)
            .unwrap();
    assert_eq!(
        request,
        Request::Remote(Call {
            caller: Caller::default(),
            op: RemoteOp::PushStatus,
        })
    );
    assert!(
        RemoteOp::PushStatus.wait_seconds()
            > sheprd_remote_control::control::RESPONSE_TIMEOUT.as_secs()
    );
    for reply in [
        Reply::PushStatus {
            enabled: false,
            push_relay: false,
            status: None,
            error: None,
            hosts: Vec::new(),
        },
        Reply::PushStatus {
            enabled: true,
            push_relay: true,
            status: None,
            error: Some("could not reach the remote control daemon".into()),
            hosts: Vec::new(),
        },
        Reply::PushStatus {
            enabled: true,
            push_relay: true,
            status: Some(Box::new(PushStatus {
                mode: Mode::PushRelay,
                alert: Some(Last {
                    at_ms: 9,
                    outcome: Outcome::Delivered,
                }),
                live_activity: None,
            })),
            error: None,
            hosts: Vec::new(),
        },
    ] {
        let response = Response::Remote { reply };
        let text = serde_json::to_string(&response).unwrap();
        assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
    }
}
