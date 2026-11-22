//! Through the push relay, alerts go to devices that registered handles,
//! and an alert the relay cannot take falls back to Web Push.

use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::SecretKey;
use serde_json::json;

fn with_relay(dir: &std::path::Path) -> Arc<State> {
    let mut state = crate::web::tests::state(dir);
    Arc::get_mut(&mut state).unwrap().apns =
        apns::Delivery::Relay(Box::new(apns::Relays::new().unwrap()));
    state
}

fn notify_key() -> String {
    let key = SecretKey::from_slice(&[5; 32])
        .unwrap()
        .public_key()
        .to_sec1_bytes();
    URL_SAFE_NO_PAD.encode(key)
}

fn register(state: &State, device: &DeviceId, alert: serde_json::Value) {
    state
        .auth()
        .devices_mut()
        .set_apns(
            device,
            Some(json!({"alert": alert, "notify_key": notify_key()})),
        )
        .unwrap();
}

fn handle() -> serde_json::Value {
    json!({"handle": format!("1a2b3c4d.{}", "A".repeat(160)), "environment": "production"})
}

#[test]
fn the_relay_alerts_devices_with_handles_only() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_relay(dir.path());
    let (relayed, _) = crate::web::tests::signed_in(&state, 3);
    let (older_app, _) = crate::web::tests::signed_in(&state, 4);
    register(&state, &relayed, handle());
    // A plain token, from an app older than the push relay.
    register(&state, &older_app, json!("9f".repeat(32)));
    assert_eq!(alert_devices(&state), vec![relayed]);
}

#[test]
fn a_gone_handle_is_dropped_from_the_device() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_relay(dir.path());
    let (device, _) = crate::web::tests::signed_in(&state, 3);
    register(&state, &device, handle());
    let gone: Recipient = serde_json::from_value(handle()).unwrap();
    forget(&state, &device, &gone);
    assert!(alert_devices(&state).is_empty());
}

#[tokio::test]
async fn during_a_relay_outage_an_alert_is_left_to_web_push_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_relay(dir.path());
    let (device, _) = crate::web::tests::signed_in(&state, 3);
    register(&state, &device, handle());
    let apns::Delivery::Relay(relays) = &state.apns else {
        unreachable!()
    };
    relays.mark_down(sheprd_apns::RelayHost::Production, now_ms());
    let notification = Notification::badge();
    let carried = tokio::time::timeout(
        Duration::from_millis(500),
        send_alert(&state, &device, &notification),
    )
    .await
    .unwrap();
    assert!(!carried, "Web Push should carry it");
}
