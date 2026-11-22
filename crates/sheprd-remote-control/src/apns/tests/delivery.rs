//! Which mode sends, what each reaches, and that a relay outage drops
//! pushes at once instead of holding them. No mode posts to Apple.

use super::*;
use crate::apns::recipient::Handle;
use p256::pkcs8::{EncodePrivateKey as _, LineEnding};
use secrecy::ExposeSecret as _;
use sheprd_apns::RelayHost;
use std::time::Duration;

fn handle() -> Recipient {
    Handle::parse(&format!("1a2b3c4d.{}", "A".repeat(160)), RelayHost::Sandbox)
        .unwrap()
        .into()
}

fn relay() -> Delivery {
    Delivery::Relay(Box::new(Relays::new().unwrap()))
}

fn write_private(path: &std::path::Path, contents: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[test]
fn only_the_push_relay_sends_and_an_apns_key_on_disk_is_ignored() {
    assert!(Delivery::new(false).unwrap().is_off());
    assert_eq!(Delivery::new(true).unwrap().name(), "push relay");
    // A key an earlier release used, or a host was sent when linked, is
    // left on disk and changes nothing.
    let dir = tempfile::tempdir().unwrap();
    let apns = sheprd_apns::dir(dir.path());
    let pem = p256::SecretKey::from_slice(&[7; 32])
        .unwrap()
        .to_pkcs8_pem(LineEnding::LF)
        .unwrap();
    write_private(&apns.join("AuthKey.p8"), pem.as_bytes());
    write_private(
        &apns.join("config.json"),
        br#"{"team_id":"ABCDE12345","key_id":"XYZ9876543","topic":"so.pen.herdr.remote"}"#,
    );
    assert!(Delivery::new(false).unwrap().is_off());
    assert_eq!(Delivery::new(true).unwrap().name(), "push relay");
}

#[test]
fn the_relay_reaches_handles_and_nothing_reaches_a_token() {
    assert!(relay().reaches(&handle()));
    assert!(!relay().reaches(&token(1)));
    assert!(!Delivery::Off.reaches(&token(1)));
    assert!(!Delivery::Off.reaches(&handle()));
}

#[tokio::test]
async fn a_plain_token_is_never_sent_to() {
    let key = p256::SecretKey::from_slice(&[5; 32]).unwrap().public_key();
    let token = token(1);
    let sealed =
        crate::apns::alert::Sealed::test("t1".to_owned(), "computer".to_owned(), "MacBook", 0);
    // Sending would take a DNS lookup and a TLS handshake at least; a
    // recipient no mode reaches takes neither.
    for delivery in [relay(), Delivery::Off] {
        let sent = tokio::time::timeout(
            Duration::from_millis(200),
            delivery.alert(&token, &key, sealed.clone(), "test-t1", 0),
        )
        .await
        .unwrap();
        assert_eq!(sent, Sent::Unsent, "{}", delivery.name());
    }
    assert_eq!(token.expose_secret(), "01".repeat(32));
}

#[test]
fn each_reply_says_what_came_of_the_push() {
    use sheprd_apns::RelayOutcome;
    let refused = |status| Sent::Refused { status };
    for (outcome, sent) in [
        (RelayOutcome::Delivered, Sent::Delivered),
        (RelayOutcome::Gone, Sent::Gone),
        (RelayOutcome::Limited, Sent::Limited),
        (RelayOutcome::Invalid, refused(400)),
        (RelayOutcome::Stale, refused(403)),
        (RelayOutcome::Unavailable, Sent::Unreachable),
    ] {
        assert_eq!(Sent::from(outcome), sent, "{outcome:?}");
    }
}

#[test]
fn an_outage_lasts_its_window() {
    let relays = Relays::new().unwrap();
    let host = RelayHost::Production;
    assert!(!relays.is_down(host, 1_000));
    relays.mark_down(host, 1_000);
    assert!(relays.is_down(host, 1_000));
    assert!(relays.is_down(host, 1_000 + crate::apns::delivery::OUTAGE_MS - 1));
    assert!(!relays.is_down(host, 1_000 + crate::apns::delivery::OUTAGE_MS));
    // A later failure extends it; an earlier one does not shorten it.
    relays.mark_down(host, 500);
    assert!(relays.is_down(host, 1_000 + crate::apns::delivery::OUTAGE_MS - 1));
}

#[test]
fn one_relay_down_leaves_the_other_up() {
    let relays = Relays::new().unwrap();
    relays.mark_down(RelayHost::Sandbox, 1_000);
    assert!(relays.is_down(RelayHost::Sandbox, 1_000));
    assert!(!relays.is_down(RelayHost::Production, 1_000));
}

#[tokio::test]
async fn during_an_outage_pushes_are_dropped_without_a_call() {
    let relays = Relays::new().unwrap();
    let now = 1_800_000_000_000;
    relays.mark_down(RelayHost::Sandbox, now);
    let delivery = Delivery::Relay(Box::new(relays));
    let key = p256::SecretKey::from_slice(&[5; 32]).unwrap().public_key();
    let live = Live {
        event: Event::Update { alert: None },
        pane: "w1-p3".to_owned(),
        state: ContentState {
            options: Vec::new(),
            permission: None,
            phase: Phase::Working,
            plan_done: None,
            plan_total: None,
            previous: Vec::new(),
            question: None,
            started_at: 0,
            step: "Working".to_owned(),
            summary: None,
            waiting: 0,
            working: 1,
        },
    };
    // Reaching the relay would take a DNS lookup and a TLS handshake at
    // least; a dropped push takes neither.
    let sent = tokio::time::timeout(
        Duration::from_millis(200),
        delivery.live(&handle(), Some(&key), &live, now),
    )
    .await
    .unwrap();
    assert_eq!(sent, Sent::Unreachable);
}

#[tokio::test]
async fn a_relayed_activity_needs_the_phones_key() {
    let live_without_key = relay()
        .live(
            &handle(),
            None,
            &Live {
                event: Event::End { dismissal_s: 0 },
                pane: "w1-p3".to_owned(),
                state: ContentState {
                    options: Vec::new(),
                    permission: None,
                    phase: Phase::Finished,
                    plan_done: None,
                    plan_total: None,
                    previous: Vec::new(),
                    question: None,
                    started_at: 0,
                    step: String::new(),
                    summary: None,
                    waiting: 0,
                    working: 0,
                },
            },
            0,
        )
        .await;
    assert_eq!(live_without_key, Sent::Unsent);
}
