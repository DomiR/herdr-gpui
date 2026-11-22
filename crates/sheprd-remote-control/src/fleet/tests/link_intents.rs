//! What `fleet::links` is about to change is written to `fleet/links.json`
//! before `devices.json`, so a stop between the two leaves this module's
//! own work readable as its own: a revocation it started is not taken for
//! a hand refusal, and relay keys it was moving to do not freeze.

use super::phones::{admit, apply, device, link, nowhere, post};
use super::*;
use crate::auth::devices::RelayKeys;
use crate::control::fleet::Body;
use crate::web::tests::fleet::Phone;

fn keys(phone: &Phone) -> RelayKeys {
    RelayKeys {
        dial: crate::auth::encode(&phone.bundle.dial),
        tunnel: crate::auth::encode(&phone.bundle.tunnel),
    }
}

fn write_links(node: &Node, value: &serde_json::Value) {
    std::fs::write(
        node.dir.path().join("fleet/links.json"),
        serde_json::to_vec(value).unwrap(),
    )
    .unwrap();
}

/// A computer that founded a fleet and admitted a phone, whose link made
/// its device here.
async fn linked() -> (Phone, Node) {
    let phone = Phone::new(7);
    let a = Node::new(nowhere(), 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    admit(&a, &phone);
    link(&a, &phone, 20);
    apply(&a);
    (phone, a)
}

#[tokio::test]
async fn a_revocation_cut_short_is_not_a_refusal() {
    let (phone, a) = linked().await;
    let id = device(&a, 20).unwrap().id;
    // A suspension's revocation: the intent written, the device removed,
    // and the daemon stopped before the file was written again.
    write_links(
        &a,
        &serde_json::json!({
            "admitted": { id.0.clone(): keys(&phone) },
            "revoking": [id.0.clone()],
        }),
    );
    assert!(a.state.auth().revoke(&id).unwrap());
    // The suspension is lifted: the device comes back.
    assert!(apply(&a));
    assert!(device(&a, 20).is_some());
}

#[tokio::test]
async fn relay_keys_a_move_left_behind_still_follow_the_bundle() {
    let (phone, a) = linked().await;
    let id = device(&a, 20).unwrap().id;
    // A rotation's move: the intent written, the device moved to the new
    // keys, and the daemon stopped before the file was written again.
    let mut moved = Phone::new(7);
    moved.bundle.tunnel = [70; 32];
    moved.bundle.dial = Phone::new(71).bundle.key;
    write_links(
        &a,
        &serde_json::json!({
            "admitted": { id.0.clone(): keys(&phone) },
            "setting": { id.0.clone(): keys(&moved) },
        }),
    );
    a.state
        .auth()
        .devices_mut()
        .replace_relay(&id, Some(keys(&moved)), now_ms())
        .unwrap();
    // The bundle rotates again: the device follows, not frozen.
    let mut again = Phone::new(7);
    again.bundle.tunnel = [80; 32];
    again.bundle.dial = Phone::new(81).bundle.key;
    assert_eq!(post(&a, &phone, Body::Rotate(again.bundle.clone())), 0);
    assert!(apply(&a));
    assert_eq!(device(&a, 20).unwrap().relay, Some(keys(&again)));
}

#[tokio::test]
async fn an_add_cut_short_keeps_the_keys_it_meant_to_set() {
    let (phone, a) = linked().await;
    let id = device(&a, 20).unwrap().id;
    // The intent written, the device added, and the daemon stopped before
    // the file was written again.
    write_links(
        &a,
        &serde_json::json!({ "admitted": {}, "adding": { id.0.clone(): keys(&phone) } }),
    );
    // The bundle rotates: the device follows, its keys being this module's.
    let mut again = Phone::new(7);
    again.bundle.tunnel = [80; 32];
    again.bundle.dial = Phone::new(81).bundle.key;
    assert_eq!(post(&a, &phone, Body::Rotate(again.bundle.clone())), 0);
    assert!(apply(&a));
    assert_eq!(device(&a, 20).unwrap().relay, Some(keys(&again)));
}

#[tokio::test]
async fn a_recovered_device_keeps_keys_the_phone_sent_itself() {
    let (_, a) = linked().await;
    let id = device(&a, 20).unwrap().id;
    // The phone sent keys of its own, and the file never named the device.
    let own = keys(&Phone::new(40));
    a.state
        .auth()
        .devices_mut()
        .set_relay(&id, own.clone(), now_ms())
        .unwrap();
    std::fs::remove_file(a.dir.path().join("fleet/links.json")).unwrap();
    apply(&a);
    assert_eq!(device(&a, 20).unwrap().relay, Some(own));
}

#[tokio::test]
async fn a_held_host_lets_no_phone_in() {
    let (_, a) = linked().await;
    let (state, proven, me) = {
        let inner = a.fleet.inner();
        let state = inner.log.as_ref().unwrap().state().clone();
        (state, inner.proven.clone(), a.fleet.me())
    };
    let (wanted, linked) = links::wanted_in(&state, &proven, &me, None, &[]);
    assert_eq!(wanted.len(), 1);
    // Two members claim this computer's host: until a person settles it,
    // no phone keeps access through it, while its link still counts.
    let mut held = state.clone();
    let host = grants::own_host(&held, &me, None).unwrap();
    held.hosts.get_mut(&host).unwrap().held = true;
    let (none, still) = links::wanted_in(&held, &proven, &me, None, &[]);
    assert!(none.is_empty());
    assert_eq!(still, linked);
}
