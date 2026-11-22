//! `sheprd-remote-control revoke` and Sheprd's Revoke reach the running
//! daemon's control socket, which queues the device's push relay handles.

use super::*;
use crate::control::{Request, Response};
use crate::web::tests::signed_in;

#[test]
fn a_control_revoke_queues_the_push_relay_handles() {
    let dir = tempfile::tempdir().unwrap();
    let state = web::tests::state(dir.path());
    let (revocations, mut receiver) = crate::apns::push_relay_revoke::channel();
    state.auth().set_push_relay_revocations(revocations);
    let (device, _) = signed_in(&state, 1);
    let alert = serde_json::json!({
        "alert": { "handle": format!("1a2b3c4d.{}", "A".repeat(160)), "environment": "sandbox" },
    });
    state
        .auth()
        .devices_mut()
        .set_apns(&device, Some(alert))
        .unwrap();
    let (stop, _) = watch::channel(false);
    assert_eq!(
        control::answer(&state, &stop, Request::Revoke { device }),
        Response::Done
    );
    let handles = receiver.try_recv().unwrap();
    assert_eq!(handles.len(), 1);
    assert!(receiver.try_recv().is_err());
}
