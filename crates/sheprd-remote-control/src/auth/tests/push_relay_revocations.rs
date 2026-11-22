//! Every path that unpairs a device queues its push relay handles to be
//! revoked, once, and still unpairs when the queue cannot take them.

use super::*;
use crate::apns::{Handle, push_relay_revoke};
use sheprd_apns::RelayHost;
use tokio::sync::mpsc::Receiver;

fn handle(byte: char) -> Handle {
    Handle::parse(
        &format!("1a2b3c4d.{}", byte.to_string().repeat(160)),
        RelayHost::Production,
    )
    .unwrap()
}

/// What the phone registers: an alert handle, a push-to-start token, and
/// an activity handle.
fn registration() -> serde_json::Value {
    serde_json::json!({
        "activities": [{
            "pane": "w1-p3",
            "token": { "handle": format!("1a2b3c4d.{}", "B".repeat(160)), "environment": "production" },
        }],
        "alert": { "handle": format!("1a2b3c4d.{}", "A".repeat(160)), "environment": "production" },
        "push_to_start": "ab".repeat(32),
    })
}

/// An auth that revokes push relay handles, and the queue it fills.
fn revoking(dir: &Path) -> (Auth, Receiver<Vec<Handle>>) {
    let mut auth = auth(dir);
    let (revocations, receiver) = push_relay_revoke::channel();
    auth.set_push_relay_revocations(revocations);
    (auth, receiver)
}

/// A device paired with `seed`'s key that registered [`registration`].
fn registered(auth: &mut Auth, seed: u8) -> DeviceId {
    let device = pair(auth, &signing_key(seed), T0);
    auth.devices_mut()
        .set_apns(&device, Some(registration()))
        .unwrap();
    device
}

fn queued(receiver: &mut Receiver<Vec<Handle>>) -> Vec<Vec<Handle>> {
    std::iter::from_fn(|| receiver.try_recv().ok()).collect()
}

#[test]
fn revoking_a_device_queues_its_push_relay_handles_once() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, mut receiver) = revoking(dir.path());
    let device = registered(&mut auth, 1);
    assert!(auth.revoke(&device).unwrap());
    assert_eq!(queued(&mut receiver), vec![vec![handle('A'), handle('B')]]);
    // A second revoke finds no device and queues nothing.
    assert!(!auth.revoke(&device).unwrap());
    assert!(queued(&mut receiver).is_empty());
}

#[test]
fn a_device_with_only_tokens_queues_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, mut receiver) = revoking(dir.path());
    let device = pair(&mut auth, &signing_key(1), T0);
    let tokens = serde_json::json!({ "alert": "cd".repeat(32), "push_to_start": "ab".repeat(32) });
    auth.devices_mut().set_apns(&device, Some(tokens)).unwrap();
    assert!(auth.revoke(&device).unwrap());
    assert!(queued(&mut receiver).is_empty());
}

#[test]
fn an_idle_device_queues_its_push_relay_handles() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, mut receiver) = revoking(dir.path());
    let idle = registered(&mut auth, 1);
    let day = 24 * 60 * 60 * 1000;
    assert_eq!(auth.expire_idle(T0 + 7 * day, 7 * day).unwrap(), vec![idle]);
    assert_eq!(queued(&mut receiver), vec![vec![handle('A'), handle('B')]]);
}

#[test]
fn an_enrolled_device_replaced_queues_its_push_relay_handles() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, mut receiver) = revoking(dir.path());
    let enroll = |replace| Enroll {
        name: "iPhone".into(),
        public_key: public_key(&signing_key(3)),
        via: "studio".into(),
        replace,
    };
    let old = auth.enroll(&enroll(None), T0).unwrap();
    auth.devices_mut()
        .set_apns(&old, Some(registration()))
        .unwrap();
    auth.enroll(&enroll(Some(old.clone())), T0).unwrap();
    assert!(auth.devices().get(&old).is_none());
    assert_eq!(queued(&mut receiver), vec![vec![handle('A'), handle('B')]]);
}

#[test]
fn fleet_removals_queue_their_devices_push_relay_handles() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, mut receiver) = revoking(dir.path());
    let mut device = new_device(4, "iPhone");
    device.id = DeviceId("fleet-abcd".into());
    let id = device.id.clone();
    auth.devices_mut()
        .add_with(device, T0, Some("fleet abcd".into()), Vec::new(), false)
        .unwrap();
    auth.devices_mut()
        .set_apns(&id, Some(registration()))
        .unwrap();
    crate::fleet::links::forget(&mut auth, dir.path()).unwrap();
    assert!(auth.devices().get(&id).is_none());
    assert_eq!(queued(&mut receiver), vec![vec![handle('A'), handle('B')]]);
}

#[test]
fn unpairing_succeeds_when_the_push_relay_queue_is_gone_or_full() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, receiver) = revoking(dir.path());
    drop(receiver);
    let device = registered(&mut auth, 1);
    assert!(auth.revoke(&device).unwrap());

    let full_dir = tempfile::tempdir().unwrap();
    let mut auth = super::auth(full_dir.path());
    let (revocations, mut full) = push_relay_revoke::channel();
    for _ in 0..push_relay_revoke::QUEUE {
        push_relay_revoke::queue(&revocations, vec![handle('Z')]);
    }
    auth.set_push_relay_revocations(revocations);
    let device = registered(&mut auth, 2);
    assert!(auth.revoke(&device).unwrap());
    assert!(auth.devices().get(&device).is_none());
    // Only what filled the queue is there: this device's handles were dropped.
    assert!(
        queued(&mut full)
            .iter()
            .all(|handles| handles == &vec![handle('Z')])
    );
}

#[test]
fn without_the_push_relay_nothing_is_collected() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let device = registered(&mut auth, 1);
    assert!(
        auth.push_relay_handles(auth.devices().get(&device).unwrap())
            .is_empty()
    );
    assert!(auth.revoke(&device).unwrap());
}
