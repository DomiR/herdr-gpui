//! A linked phone's relay keys: the dial and tunnel keys of its own
//! bundle, so the relays let it through and the tunnel admits it to the
//! API; moved by a `Rotate`, gone with the member, and never keys another
//! device or member holds.

use super::phones::{admit, apply, device, link, nowhere, post};
use super::*;
use crate::auth::{NewDevice, devices::RelayKeys};
use crate::control::fleet::{Body, Reason};
use crate::web::tests::fleet::{Phone, device_key};

fn bundle_keys(phone: &Phone) -> RelayKeys {
    RelayKeys {
        dial: crate::auth::encode(&phone.bundle.dial),
        tunnel: crate::auth::encode(&phone.bundle.tunnel),
    }
}

async fn founded() -> Node {
    let a = Node::new(nowhere(), 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    a
}

#[tokio::test]
async fn a_link_sets_the_bundles_relay_keys() {
    let a = founded().await;
    let phone = Phone::new(7);
    admit(&a, &phone);
    link(&a, &phone, 20);
    apply(&a);
    let record = device(&a, 20).unwrap();
    assert_eq!(record.relay, Some(bundle_keys(&phone)));
    assert!(record.relay_ms.is_some());
    assert!(record.ssh.is_none() && !record.share_github);
    // Nothing more to do while the bundle stands.
    assert!(!apply(&a));
}

#[tokio::test]
async fn the_relay_keys_go_with_the_member() {
    let a = founded().await;
    let phone = Phone::new(7);
    admit(&a, &phone);
    link(&a, &phone, 20);
    apply(&a);
    a.ask(Ask::Remove {
        member: phone.bundle.id().to_string(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Lost,
    });
    apply(&a);
    assert!(device(&a, 20).is_none());
    let tunnel = phone.bundle.tunnel;
    assert!(
        !a.state
            .auth()
            .devices()
            .records()
            .iter()
            .any(|record| record.relay.as_ref().and_then(RelayKeys::tunnel_key) == Some(tunnel))
    );
}

#[tokio::test]
async fn a_rotated_bundle_moves_the_relay_keys() {
    let a = founded().await;
    let phone = Phone::new(7);
    admit(&a, &phone);
    link(&a, &phone, 20);
    apply(&a);
    let mut rotated = Phone::new(7);
    rotated.bundle.tunnel = [70; 32];
    rotated.bundle.dial = Phone::new(71).bundle.key;
    assert_eq!(post(&a, &phone, Body::Rotate(rotated.bundle.clone())), 0);
    assert!(apply(&a));
    assert_eq!(device(&a, 20).unwrap().relay, Some(bundle_keys(&rotated)));
}

#[tokio::test]
async fn keys_the_phone_sent_itself_stay() {
    let a = founded().await;
    let phone = Phone::new(7);
    admit(&a, &phone);
    link(&a, &phone, 20);
    apply(&a);
    let id = device(&a, 20).unwrap().id;
    let own = bundle_keys(&Phone::new(40));
    a.state
        .auth()
        .devices_mut()
        .set_relay(&id, own.clone(), now_ms())
        .unwrap();
    a.fleet.bump();
    assert!(!apply(&a));
    assert_eq!(device(&a, 20).unwrap().relay, Some(own));
}

#[tokio::test]
async fn a_device_paired_later_cannot_take_a_linked_phones_keys() {
    let a = founded().await;
    let phone = Phone::new(7);
    admit(&a, &phone);
    link(&a, &phone, 20);
    apply(&a);
    // Another device posts the bundle's public keys: that proves nothing
    // about holding them, so the linked phone keeps them.
    let (_, public_key) = links::device_id(&device_key(30)).unwrap();
    let direct = crate::control::DeviceId("direct".into());
    let mut auth = a.state.auth();
    auth.devices_mut()
        .add(
            NewDevice {
                app: None,
                fleet: None,
                id: direct.clone(),
                name: "iPhone".into(),
                public_key,
                relay: None,
                ssh: None,
            },
            now_ms(),
        )
        .unwrap();
    assert!(matches!(
        auth.devices_mut()
            .set_relay(&direct, bundle_keys(&phone), now_ms()),
        Err(crate::auth::Error::RelayKeysTaken)
    ));
    drop(auth);
    assert_eq!(device(&a, 20).unwrap().relay, Some(bundle_keys(&phone)));
}

#[tokio::test]
async fn another_devices_keys_are_never_set() {
    let a = founded().await;
    let phone = Phone::new(7);
    // A phone paired here directly already holds these keys: a bundle
    // naming them must not take its sessions.
    let (_, public_key) = links::device_id(&device_key(30)).unwrap();
    a.state
        .auth()
        .devices_mut()
        .add(
            NewDevice {
                app: None,
                fleet: None,
                id: crate::control::DeviceId("direct".into()),
                name: "iPhone".into(),
                public_key,
                relay: Some(bundle_keys(&phone)),
                ssh: None,
            },
            now_ms(),
        )
        .unwrap();
    admit(&a, &phone);
    link(&a, &phone, 20);
    assert!(apply(&a));
    // Admitted to the API, with no relay keys of its own.
    assert_eq!(device(&a, 20).unwrap().relay, None);
    let direct = crate::control::DeviceId("direct".into());
    assert_eq!(
        a.state.auth().devices().get(&direct).unwrap().relay,
        Some(bundle_keys(&phone))
    );
}

#[tokio::test]
async fn a_tunnel_key_another_member_proved_is_never_set() {
    let a = founded().await;
    let phone = Phone::new(7);
    admit(&a, &phone);
    // A computer member proved this tunnel key here, then rotated away.
    let other = MemberId([9; 32]);
    a.fleet.proved(other, phone.bundle.tunnel).unwrap();
    link(&a, &phone, 20);
    apply(&a);
    assert_eq!(device(&a, 20).unwrap().relay, None);
}
