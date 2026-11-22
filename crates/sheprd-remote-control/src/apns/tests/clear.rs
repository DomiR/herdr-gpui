//! An explicit `null` clears a recipient, which the app sends when its push
//! relay goes off; a missing one keeps it, as older apps rely on.

use super::*;
use crate::apns::tokens::Update;
use serde_json::json;

fn handle(byte: char) -> serde_json::Value {
    json!({"handle": format!("1a2b3c4d.{}", byte.to_string().repeat(160)), "environment": "production"})
}

fn stored() -> Registration {
    Registration::parse(
        json!({
            "push_to_start": handle('A'),
            "alert": handle('B'),
            "activities": [{"pane": "w1-p3", "token": handle('C')}],
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap()
}

fn update(body: serde_json::Value) -> Update {
    Update::parse(body.to_string().as_bytes()).unwrap()
}

#[test]
fn an_explicit_null_clears_the_handles_a_phone_turned_off() {
    let mut registration = stored();
    assert_eq!(registration.handles().len(), 3);
    registration.apply(update(
        json!({"push_to_start": null, "alert": null, "activities": []}),
    ));
    assert!(registration.handles().is_empty());
    assert!(registration.push_to_start.is_none());
    assert!(registration.alert.is_none());
}

#[test]
fn a_missing_recipient_keeps_what_was_registered() {
    let mut registration = stored();
    registration.apply(update(json!({"activities": []})));
    assert!(registration.push_to_start.is_some());
    assert!(registration.alert.is_some());
    // Only the activity list is replaced whole.
    assert!(registration.activities.is_empty());
}

#[test]
fn one_cleared_recipient_leaves_the_other() {
    let mut registration = stored();
    registration.apply(update(json!({"alert": null, "activities": []})));
    assert!(registration.alert.is_none());
    assert!(registration.push_to_start.is_some());
}

#[test]
fn a_null_body_field_still_parses_as_missing_for_the_stored_shape() {
    // What the device store keeps never says null, and reading a body
    // without `Update` (as older daemons do) sees null as absent.
    let registration = Registration::parse(
        json!({"alert": null, "push_to_start": null})
            .to_string()
            .as_bytes(),
    )
    .unwrap();
    assert!(registration.is_empty());
    assert!(Update::parse(br#"{"alert": null, "extra": 1}"#).is_err());
}
