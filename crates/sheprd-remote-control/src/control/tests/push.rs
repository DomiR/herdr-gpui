//! `push_status`'s wire form, which an older Sheprd and a newer daemon
//! must both read.

use super::*;
use crate::control::push::{Last, Mode, Outcome, PushStatus};

#[test]
fn push_status_keeps_its_wire_form() {
    assert_eq!(
        serde_json::to_value(Request::PushStatus).unwrap(),
        json!({"request": "push_status"})
    );
    let status = PushStatus {
        mode: Mode::PushRelay,
        alert: Some(Last {
            at_ms: 5,
            outcome: Outcome::Refused {
                status: 403,
                reason: None,
            },
        }),
        live_activity: None,
    };
    let value = serde_json::to_value(Response::PushStatus(status.clone())).unwrap();
    assert_eq!(
        value,
        json!({
            "response": "push_status",
            "mode": "push_relay",
            "alert": {"at_ms": 5, "outcome": "refused", "status": 403},
        })
    );
    assert_eq!(
        serde_json::from_value::<Response>(value).unwrap(),
        Response::PushStatus(status)
    );
}

#[test]
fn a_newer_daemons_mode_and_outcome_still_read() {
    let status: PushStatus = serde_json::from_value(json!({
        "mode": "carrier_pigeon",
        "live_activity": {"at_ms": 1, "outcome": "queued"},
    }))
    .unwrap();
    assert_eq!(status.mode, Mode::Other);
    assert_eq!(status.alert, None);
    assert_eq!(status.live_activity.unwrap().outcome, Outcome::Other);
    // An older daemon still sending with its own key.
    assert_eq!(
        serde_json::from_value::<PushStatus>(json!({"mode": "own_key"})).unwrap(),
        PushStatus {
            mode: Mode::Other,
            ..PushStatus::default()
        }
    );
}
