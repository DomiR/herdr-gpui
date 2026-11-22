//! `POST /api/apns` tells the phone how this computer delivers its pushes,
//! so its Settings can show it.

use super::*;
use axum::http::{Request as HttpRequest, StatusCode, header};

fn register(token: &str) -> HttpRequest<Body> {
    HttpRequest::post("/api/apns")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(json!({ "activities": [] }).to_string()))
        .unwrap()
}

async fn delivery(apns: crate::apns::Delivery) -> serde_json::Value {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    Arc::get_mut(&mut state).unwrap().apns = apns;
    let (_, token) = signed_in(&state, 1);
    let (status, _, text) = send(&state, register(&token)).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    serde_json::from_str::<serde_json::Value>(&text).unwrap()["delivery"].clone()
}

#[tokio::test]
async fn the_answer_names_the_push_relay() {
    assert_eq!(
        delivery(crate::apns::Delivery::Relay(Box::new(
            crate::apns::Relays::new().unwrap()
        )))
        .await,
        "push_relay"
    );
}
