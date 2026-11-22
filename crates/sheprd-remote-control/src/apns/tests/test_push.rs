//! The test push a phone asks for: sealed like any alert, urgent, in its
//! own thread, and carrying the id the phone's extension records.

use super::alert::{COMPUTER, device, fixed_seal, json, open, push};
use crate::apns::{
    Priority, PushType,
    alert::{MAX_TEST_ID, Sealed, TEST_PANE, scope, valid_test_id},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use std::path::PathBuf;

const ID: &str = "6F1D2C3B-0A4E-4F5D-9C8B-7A6E5D4C3B2A";

fn test_push() -> Sealed {
    Sealed {
        // Fixed, for the fixture; a real one is random.
        event: Some(super::alert::EVENT.to_owned()),
        ..Sealed::test(ID.to_owned(), COMPUTER.to_owned(), "Studio", 3)
    }
}

#[test]
fn a_test_push_is_urgent_sealed_and_carries_its_id() {
    let push = push(test_push(), &format!("test-{ID}"), fixed_seal).unwrap();
    assert_eq!(push.priority, Priority::High);
    assert_eq!(push.push_type, PushType::Alert);
    assert_eq!(
        push.collapse_id.as_deref(),
        Some(format!("{}-test-{ID}", scope(&device().public_key())).as_str())
    );
    let payload = json(&push.body);
    assert_eq!(payload["aps"]["alert"]["body"], "New activity");
    assert_eq!(
        payload["aps"]["thread-id"],
        format!("{}/{TEST_PANE}", scope(&device().public_key()))
    );
    assert!(!String::from_utf8(push.body.clone()).unwrap().contains(ID));

    let opened = json(&open(payload["sealed"].as_str().unwrap()));
    assert_eq!(opened["kind"], "test");
    assert_eq!(opened["test"], ID);
    assert_eq!(opened["badge"], 3);
    assert_eq!(opened["computer"], COMPUTER);
    assert!(opened["body"].as_str().unwrap().contains("Studio"));
}

#[test]
fn only_a_test_push_has_a_test_id() {
    let plain = serde_json::to_value(test_push()).unwrap();
    assert_eq!(plain["test"], ID);
    let mut other = test_push();
    other.test = None;
    assert!(serde_json::to_value(&other).unwrap().get("test").is_none());
}

#[test]
fn test_ids_are_short_plain_text() {
    for good in [ID, "a", &"f".repeat(MAX_TEST_ID)] {
        assert!(valid_test_id(good), "{good}");
    }
    for bad in [
        "",
        "has space",
        "dot.ted",
        "é",
        &"f".repeat(MAX_TEST_ID + 1),
    ] {
        assert!(!valid_test_id(bad), "{bad}");
    }
}

/// `Fixtures/notify/test.json`, which the Swift package opens to check the
/// extension finds the id. `HERDR_UPDATE_FIXTURES=1` rewrites it.
#[test]
fn the_test_push_fixture_opens_to_its_plaintext() {
    let sealed = test_push();
    let push = push(sealed.clone(), &format!("test-{ID}"), fixed_seal).unwrap();
    let fixture = json!({
        "device_private_key": URL_SAFE_NO_PAD.encode(device().to_bytes()),
        "plaintext": sealed,
        "apns": json(&push.body),
    });
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ios/SheprdRemoteCore/Tests/SheprdRemoteCoreTests/Fixtures/notify/test.json");
    if std::env::var_os("HERDR_UPDATE_FIXTURES").is_some() {
        let text = format!("{}\n", serde_json::to_string_pretty(&fixture).unwrap());
        std::fs::write(&path, text).unwrap();
    }
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        fixture,
        expected,
        "{} is out of date; run with HERDR_UPDATE_FIXTURES=1",
        path.display()
    );
}
