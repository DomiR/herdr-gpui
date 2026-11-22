//! `remote push-status` with linked hosts: each host's answer travels by
//! name, and a reply from a Sheprd without hosts still reads.

use super::*;
use sheprd_remote_control::control::push::{Mode, PushStatus};

#[test]
fn linked_hosts_round_trip_and_an_empty_list_is_left_out() {
    let reply = Reply::PushStatus {
        enabled: true,
        push_relay: true,
        status: None,
        error: None,
        hosts: vec![
            HostPush {
                name: "box".into(),
                status: Some(PushStatus {
                    mode: Mode::PushRelay,
                    ..PushStatus::default()
                }),
                error: None,
            },
            HostPush {
                name: "old".into(),
                status: None,
                error: Some("Could not run sheprd-remote-control on the host".into()),
            },
        ],
    };
    let response = Response::Remote { reply };
    let text = serde_json::to_string(&response).unwrap();
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
    assert!(text.contains(r#""name":"box""#));

    let older = r#"{"kind":"push_status","enabled":true,"push_relay":false}"#;
    let Reply::PushStatus { hosts, .. } = serde_json::from_str(older).unwrap() else {
        panic!("not a push status");
    };
    assert!(hosts.is_empty());
    let none = Reply::PushStatus {
        enabled: true,
        push_relay: false,
        status: None,
        error: None,
        hosts: Vec::new(),
    };
    assert!(!serde_json::to_string(&none).unwrap().contains("hosts"));
}

#[test]
fn asking_hosts_waits_longer_than_one_daemon() {
    assert!(RemoteOp::PushStatus.wait_seconds() > RemoteOp::Devices.wait_seconds());
}
