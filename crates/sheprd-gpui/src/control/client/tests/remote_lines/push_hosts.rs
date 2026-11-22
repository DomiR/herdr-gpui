use super::*;
use sheprd_control_protocol::protocol::remote::HostPush;
use sheprd_remote_control::control::push::{Last, Mode, Outcome, PushStatus};

#[test]
fn push_status_lists_each_linked_host_after_this_computer() {
    let now = crate::remote::now_ms();
    let output = remote::output(&Reply::PushStatus {
        enabled: false,
        push_relay: true,
        status: None,
        error: None,
        hosts: vec![
            HostPush {
                name: "box".into(),
                status: Some(PushStatus {
                    mode: Mode::PushRelay,
                    alert: Some(Last {
                        at_ms: now,
                        outcome: Outcome::Delivered,
                    }),
                    live_activity: None,
                }),
                error: None,
            },
            HostPush {
                name: "old".into(),
                status: None,
                error: None,
            },
            HostPush {
                name: "far".into(),
                status: None,
                error: Some("Could not run sheprd-remote-control on the host".into()),
            },
        ],
    })
    .unwrap();
    assert_eq!(
        output.text,
        "Remote control is off; the push relay is on in Settings\n\
         \n\
         Linked hosts:\n\
         \x20 box: through the push relay; last alert: delivered, just now\n\
         \x20 old: unknown (no remote control daemon runs there, or it is too old to say)\n\
         \x20 far: unavailable: Could not run sheprd-remote-control on the host"
    );
    assert_eq!(output.json["hosts"][0]["name"], "box");
    assert_eq!(output.json["hosts"][0]["status"]["mode"], "push_relay");
    assert_eq!(output.json["hosts"][1]["status"], serde_json::Value::Null);
}
