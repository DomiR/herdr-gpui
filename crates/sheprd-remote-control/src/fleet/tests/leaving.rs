//! Leaving a fleet: the log stays until the removal was passed on, then
//! this computer may found or join another.

use super::*;
use crate::control::fleet::{Reason, sync::Ask};

#[tokio::test(flavor = "multi_thread")]
async fn a_computer_that_left_may_found_another_fleet_once_its_removal_is_passed_on() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    let old = a.view().fleet;
    assert_eq!(
        b.ask(Ask::Remove {
            member: b.me(),
            also_members: Vec::new(),
            also_hosts: Vec::new(),
            reason: Reason::Left,
        }),
        Answer::Done
    );
    // Until a holds the removal, b keeps the log to pass it on.
    assert!(b.fleet.in_fleet().unwrap());
    assert!(matches!(
        ask::answer(&b.state, &b.fleet, Ask::Found, now_ms()),
        Err(Error::InFleet)
    ));
    let removal: Vec<OpId> = b
        .ops()
        .iter()
        .filter(|op| matches!(op.body(), Body::Remove(_)))
        .map(Op::id)
        .collect();
    b.fleet
        .held(Holder::Member(a.fleet.me()), removal, [])
        .unwrap();
    // Then the fleet is gone here, on disk too, and b founds another.
    assert!(ask::view(&b.fleet).is_none());
    assert!(
        Fleet::load(b.dir.path(), now_ms())
            .unwrap()
            .inner()
            .log
            .is_none()
    );
    assert_eq!(b.ask(Ask::Found), Answer::Done);
    assert_ne!(b.view().fleet, old);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_member_still_in_the_fleet_is_never_released() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    b.fleet
        .held(Holder::Member(a.fleet.me()), a.ops().iter().map(Op::id), [])
        .unwrap();
    assert!(b.fleet.in_fleet().unwrap());
    assert!(ask::view(&b.fleet).is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn leaving_keeps_the_keys_members_proved_until_their_pairings_go() {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    // A desktop paired with b one by one, with a's keys, which a then
    // proved on b.
    let keys = crate::desktop::keys::Keys::load_or_create(a.dir.path()).unwrap();
    let ssh = crate::desktop::keys::ssh_public_key(a.dir.path()).unwrap();
    let tunnel = keys.tunnel.identity().x25519();
    let point = SigningKey::from_slice(&[9; 32])
        .unwrap()
        .verifying_key()
        .to_sec1_point(false);
    let legacy = crate::control::DeviceId("legacy".into());
    b.state
        .auth()
        .devices_mut()
        .add(
            crate::auth::NewDevice {
                app: None,
                fleet: None,
                id: legacy.clone(),
                name: "a over Bluetooth".into(),
                public_key: crate::auth::PublicKey::parse(
                    &URL_SAFE_NO_PAD.encode(point.as_bytes()),
                )
                .unwrap(),
                relay: Some(crate::control::host_relay::Keys {
                    dial: URL_SAFE_NO_PAD
                        .encode(keys.dial.verifying_key().to_sec1_point(true).as_bytes()),
                    tunnel: URL_SAFE_NO_PAD.encode(tunnel),
                }),
                ssh: Some(crate::ssh_access::SshKey::parse(&ssh).unwrap()),
            },
            now_ms(),
        )
        .unwrap();
    assert!(b.fleet.proved(a.fleet.me(), tunnel).unwrap());
    // b leaves, and is released before any grants are applied.
    b.ask(Ask::Remove {
        member: b.me(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Left,
    });
    let removal: Vec<OpId> = b
        .ops()
        .iter()
        .filter(|op| matches!(op.body(), Body::Remove(_)))
        .map(Op::id)
        .collect();
    b.fleet
        .held(Holder::Member(a.fleet.me()), removal, [])
        .unwrap();
    assert!(!b.fleet.in_fleet().unwrap());
    // The keys survive the release, on disk too, and the pairing goes.
    assert!(b.fleet.inner().proven.contains_key(&a.fleet.me()));
    assert!(
        Fleet::load(b.dir.path(), now_ms())
            .unwrap()
            .inner()
            .proven
            .contains_key(&a.fleet.me())
    );
    grants::apply(&b.state, &b.fleet).unwrap();
    assert!(b.state.auth().devices().get(&legacy).is_none());
    assert!(b.fleet.inner().proven.is_empty());
}
