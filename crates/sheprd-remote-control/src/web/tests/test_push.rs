//! `POST /api/apns/test`, up to the point it would reach the push relay.

use super::*;
use axum::http::{Request as HttpRequest, StatusCode, header};

fn test_post(token: Option<&str>, body: serde_json::Value) -> HttpRequest<Body> {
    let mut request =
        HttpRequest::post("/api/apns/test").header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    request.body(Body::from(body.to_string())).unwrap()
}

/// A state whose daemon sends through the push relay.
fn with_relay(dir: &std::path::Path) -> Shared {
    let mut state = state(dir);
    Arc::get_mut(&mut state).unwrap().apns =
        crate::apns::Delivery::Relay(Box::new(crate::apns::Relays::new().unwrap()));
    state
}

#[tokio::test]
async fn a_test_push_needs_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_relay(dir.path());
    let (status, _, _) = send(&state, test_post(None, json!({"id": "a"}))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn without_the_push_relay_the_phone_hears_push_is_not_set_up() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (_, token) = signed_in(&state, 1);
    let (status, _, _) = send(&state, test_post(Some(&token), json!({"id": "a"}))).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn a_bad_id_or_an_unregistered_phone_is_refused_before_apns() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_relay(dir.path());
    let (_, token) = signed_in(&state, 1);
    for body in [
        json!({"id": "not valid"}),
        json!({"id": "a", "extra": 1}),
        json!({}),
        json!({"id": "a"}),
    ] {
        let (status, _, text) = send(&state, test_post(Some(&token), body.clone())).await;
        assert!(status.is_client_error(), "{body}: {status} {text}");
    }
    let (_, _, text) = send(&state, test_post(Some(&token), json!({"id": "a"}))).await;
    assert!(text.contains("not registered for alerts"), "{text}");
}

#[tokio::test]
async fn a_phone_with_only_a_plain_token_is_not_registered() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_relay(dir.path());
    let (device, token) = signed_in(&state, 1);
    let key = p256::SecretKey::from_slice(&[5; 32])
        .unwrap()
        .public_key()
        .to_sec1_bytes();
    state
        .auth()
        .devices_mut()
        .set_apns(
            &device,
            Some(json!({
                "alert": "9f".repeat(32),
                "notify_key": base64::Engine::encode(
                    &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                    key,
                ),
            })),
        )
        .unwrap();
    let (status, _, text) = send(&state, test_post(Some(&token), json!({"id": "a"}))).await;
    assert!(status.is_client_error(), "{status} {text}");
    assert!(text.contains("not registered for alerts"), "{text}");
}
