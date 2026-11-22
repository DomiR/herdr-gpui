//! Which devices get sealed APNs alerts rather than Web Push.

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

fn register(state: &State, device: &DeviceId, registration: serde_json::Value) {
    state
        .auth()
        .devices_mut()
        .set_apns(device, Some(registration))
        .unwrap();
}

fn alerts(alert: serde_json::Value) -> serde_json::Value {
    let key = SecretKey::from_slice(&[5; 32])
        .unwrap()
        .public_key()
        .to_sec1_bytes();
    json!({"alert": alert, "notify_key": URL_SAFE_NO_PAD.encode(key)})
}

fn handle(byte: char) -> serde_json::Value {
    json!({"handle": format!("1a2b3c4d.{}", byte.to_string().repeat(160)), "environment": "production"})
}

#[test]
fn a_device_registered_for_alerts_gets_them_through_the_push_relay() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_relay(dir.path());
    let (alerted, _) = crate::web::tests::signed_in(&state, 3);
    let (activities_only, _) = crate::web::tests::signed_in(&state, 4);
    register(&state, &alerted, alerts(handle('A')));
    register(
        &state,
        &activities_only,
        json!({"push_to_start": handle('B')}),
    );
    // Neither has a Web Push subscription; only the first is offered
    // notifications, and through the push relay.
    assert_eq!(alert_devices(&state), vec![alerted]);
}

#[test]
fn without_the_push_relay_nobody_gets_alerts_through_apns() {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::web::tests::state(dir.path());
    let (device, _) = crate::web::tests::signed_in(&state, 3);
    register(&state, &device, alerts(handle('A')));
    assert!(alert_devices(&state).is_empty());
}

#[tokio::test]
async fn a_plain_token_from_an_older_app_is_left_to_web_push() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_relay(dir.path());
    let (device, _) = crate::web::tests::signed_in(&state, 3);
    register(&state, &device, alerts(json!("9f".repeat(32))));
    // Kept with the device, but never sent to: the daemon has no key
    // that reaches a token, and posts nothing to Apple.
    assert!(state.auth().devices().get(&device).unwrap().apns.is_some());
    assert!(alert_devices(&state).is_empty());
    let carried = tokio::time::timeout(
        Duration::from_millis(500),
        send_alert(&state, &device, &Notification::badge()),
    )
    .await
    .unwrap();
    assert!(!carried, "Web Push should carry it");
}
