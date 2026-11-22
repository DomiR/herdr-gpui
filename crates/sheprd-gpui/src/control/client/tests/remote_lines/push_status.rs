use super::*;
use sheprd_remote_control::control::push::{Last, Mode, Outcome, PushStatus};

#[test]
fn push_status_names_the_mode_and_each_last_push() {
    let now = crate::remote::now_ms();
    let output = remote::output(&Reply::PushStatus {
        enabled: true,
        push_relay: true,
        status: Some(Box::new(PushStatus {
            mode: Mode::PushRelay,
            alert: Some(Last {
                at_ms: now,
                outcome: Outcome::Delivered,
            }),
            live_activity: Some(Last {
                at_ms: now,
                outcome: Outcome::Refused {
                    status: 403,
                    reason: None,
                },
            }),
        })),
        error: None,
        hosts: Vec::new(),
    })
    .unwrap();
    assert_eq!(
        output.text,
        "iPhone pushes: through the push relay (push.sheprd.org)\n\
         Push relay setting: on\n\
         Last alert: delivered, just now\n\
         Last Live Activity push: refused (403), just now"
    );
    assert_eq!(output.json["status"]["mode"], "push_relay");
    assert_eq!(output.json["status"]["alert"]["outcome"], "delivered");
    assert_eq!(output.json["push_relay"], true);
    assert!(output.json.get("running").is_none());
}

#[test]
fn push_status_says_when_an_older_daemon_uses_its_own_key_and_nothing_was_sent() {
    // `own_key`, from a daemon not yet restarted since the update.
    let status: PushStatus = serde_json::from_str(r#"{"mode":"own_key"}"#).unwrap();
    let output = remote::output(&Reply::PushStatus {
        enabled: true,
        push_relay: true,
        status: Some(Box::new(status)),
        error: None,
        hosts: Vec::new(),
    })
    .unwrap();
    assert!(output.text.starts_with(
        "iPhone pushes: a mode this Sheprd does not know; restart remote control, or update Sheprd\n"
    ));
    assert!(output.text.ends_with(
        "Last alert: none since the daemon started\n\
         Last Live Activity push: none since the daemon started"
    ));
}

#[test]
fn push_status_while_remote_control_is_off_or_down() {
    let off = remote::output(&Reply::PushStatus {
        enabled: false,
        push_relay: false,
        status: None,
        error: None,
        hosts: Vec::new(),
    })
    .unwrap();
    assert_eq!(
        off.text,
        "Remote control is off; the push relay is off in Settings"
    );
    let down = remote::output(&Reply::PushStatus {
        enabled: true,
        push_relay: true,
        status: None,
        error: Some("could not reach the remote control daemon".into()),
        hosts: Vec::new(),
    })
    .unwrap();
    assert_eq!(
        down.text,
        "Push status is unavailable: could not reach the remote control daemon"
    );
}

#[test]
fn remote_help_names_push_status_as_read_only() {
    let help = help::usage(Some("remote"));
    assert!(help.contains("sheprd remote push-status"));
    assert!(help.contains("Read-only"));
}
