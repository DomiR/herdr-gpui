use super::*;

const PHONE: &str = "6F9619FF-8B86-D011-B42D-00C04FC964FF";

fn device() -> DeviceId {
    DeviceId("d1".into())
}

fn json(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).unwrap()
}

/// A relay whose phone proved it is `d1`.
fn proven() -> PresenceRelay {
    let mut relay = PresenceRelay::default();
    assert_eq!(
        relay.receive(PHONE, br#"{"type":"hello","device":"d1"}"#),
        Received::Ask(Request::PresenceChallenge { device: device() })
    );
    let reply = relay.answered(PHONE, Ok(Response::PresenceChallenge { nonce: "n".into() }));
    assert_eq!(json(&reply)["nonce"], "n");
    assert_eq!(
        relay.receive(PHONE, br#"{"type":"prove","device":"d1","signature":"s"}"#),
        Received::Ask(Request::PresenceProve {
            device: device(),
            nonce: "n".into(),
            signature: "s".into(),
        })
    );
    let reply = relay.answered(
        PHONE,
        Ok(Response::PresenceProven {
            device: device(),
            addresses: Some("sealed".into()),
        }),
    );
    assert_eq!(
        json(&reply),
        serde_json::json!({"type": "proven", "addresses": "sealed"})
    );
    relay
}

#[test]
fn a_proof_needs_the_nonce_given_to_that_device() {
    let mut relay = PresenceRelay::default();
    let Received::Reply(reply) =
        relay.receive(PHONE, br#"{"type":"prove","device":"d1","signature":"s"}"#)
    else {
        panic!("not refused");
    };
    assert_eq!(json(&reply)["type"], "error");
    relay.receive(PHONE, br#"{"type":"hello","device":"d1"}"#);
    relay.answered(PHONE, Ok(Response::PresenceChallenge { nonce: "n".into() }));
    assert!(matches!(
        relay.receive(PHONE, br#"{"type":"prove","device":"d2","signature":"s"}"#),
        Received::Reply(_)
    ));
}

#[test]
fn an_unknown_device_gets_the_daemons_error() {
    let mut relay = PresenceRelay::default();
    relay.receive(PHONE, br#"{"type":"hello","device":"nobody"}"#);
    let reply = relay.answered(
        PHONE,
        Ok(Response::Error {
            code: control::ErrorCode::UnknownDevice,
            message: "no such device".into(),
        }),
    );
    assert_eq!(
        json(&reply),
        serde_json::json!({"type": "error", "message": "no such device"})
    );
}

#[test]
fn near_needs_a_proven_phone_close_by_and_someone_at_the_desk() {
    let now = Instant::now();
    let mut relay = PresenceRelay::default();
    relay.receive(PHONE, br#"{"type":"near","near":true}"#);
    relay.idle(5);
    assert!(relay.due(now).is_empty(), "not proven");

    let mut relay = proven();
    relay.receive(PHONE, br#"{"type":"near","near":true}"#);
    assert!(relay.due(now).is_empty(), "idle not known yet");
    relay.idle(IDLE_LIMIT_SECONDS);
    assert!(relay.due(now).is_empty(), "nobody at the desk");
    relay.idle(5);
    assert_eq!(
        relay.due(now),
        [Request::PresenceSet {
            device: device(),
            near: true
        }]
    );
    assert!(relay.due(now + Duration::from_secs(1)).is_empty());
    assert_eq!(relay.due(now + RENEW).len(), 1, "renewed");
}

#[test]
fn leaving_or_walking_away_lifts_the_mark_once() {
    let now = Instant::now();
    let mut relay = proven();
    relay.receive(PHONE, br#"{"type":"near","near":true}"#);
    relay.idle(5);
    relay.due(now);
    relay.receive(PHONE, br#"{"type":"near","near":false}"#);
    assert_eq!(
        relay.due(now),
        [Request::PresenceSet {
            device: device(),
            near: false
        }]
    );
    assert!(relay.due(now + RENEW).is_empty());

    relay.receive(PHONE, br#"{"type":"near","near":true}"#);
    relay.due(now);
    relay.disconnected(PHONE);
    assert_eq!(
        relay.due(now),
        [Request::PresenceSet {
            device: device(),
            near: false
        }]
    );
}

#[test]
fn malformed_frames_are_answered_with_an_error() {
    let mut relay = PresenceRelay::default();
    assert!(matches!(relay.receive(PHONE, b"nope"), Received::Reply(_)));
}
