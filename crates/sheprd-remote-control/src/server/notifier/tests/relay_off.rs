//! A phone that turns its push relay off clears the handles it gave this
//! computer, so nothing goes through the push relay to it again, even when
//! the phone could not revoke them at the push relay (it was unreachable,
//! or refused with 429). Its registration says so with an explicit `null`.

use super::*;
use axum::{
    body::Body,
    http::{Request as HttpRequest, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::SecretKey;
use serde_json::json;

fn with_relay(dir: &std::path::Path) -> Arc<State> {
    let mut state = crate::web::tests::state(dir);
    Arc::get_mut(&mut state).unwrap().apns =
        apns::Delivery::Relay(Box::new(apns::Relays::new().unwrap()));
    state
}

fn handle(byte: char) -> serde_json::Value {
    json!({"handle": format!("1a2b3c4d.{}", byte.to_string().repeat(160)), "environment": "production"})
}

async fn register(state: &Arc<State>, token: &str, body: serde_json::Value) {
    let request = HttpRequest::post("/api/apns")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap();
    let (status, _, text) = crate::web::tests::send(state, request).await;
    assert_eq!(status, StatusCode::OK, "{text}");
}

fn registration(state: &State, device: &DeviceId) -> Registration {
    Registration::from_stored(
        state
            .auth()
            .devices()
            .get(device)
            .and_then(|record| record.apns.as_ref()),
    )
}

#[tokio::test]
async fn after_the_phone_turns_the_push_relay_off_nothing_goes_through_it() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_relay(dir.path());
    let (device, token) = crate::web::tests::signed_in(&state, 3);
    let notify_key = URL_SAFE_NO_PAD.encode(
        SecretKey::from_slice(&[5; 32])
            .unwrap()
            .public_key()
            .to_sec1_bytes(),
    );
    register(
        &state,
        &token,
        json!({
            "push_to_start": handle('A'),
            "alert": handle('B'),
            "activities": [{"pane": "w1-p3", "token": handle('C')}],
            "notify_key": notify_key,
        }),
    )
    .await;
    assert_eq!(alert_devices(&state), vec![device.clone()]);

    // The phone turned its push relay off. Its revocation at the push
    // relay failed, so the handles still work there; this computer is all
    // that stands between them and a push.
    register(
        &state,
        &token,
        json!({"push_to_start": null, "alert": null, "activities": []}),
    )
    .await;
    let left = registration(&state, &device);
    assert!(left.handles().is_empty(), "{left:?}");
    assert!(
        [left.push_to_start.as_ref(), left.alert.as_ref()]
            .into_iter()
            .flatten()
            .all(|recipient| !state.apns.reaches(recipient))
    );
    assert!(alert_devices(&state).is_empty());
    // Sending would take a DNS lookup and a TLS handshake at least; an
    // alert with nowhere to go takes neither, and falls to Web Push.
    let carried = tokio::time::timeout(
        Duration::from_millis(200),
        send_alert(&state, &device, &Notification::badge()),
    )
    .await
    .unwrap();
    assert!(!carried);
}
