//! Every copy of one notification decision carries the same event id, so
//! the phone's extension shows a copy that arrives again only once.
//! `Fixtures/notify/repeat.json` holds three pushes as the phone receives
//! them: a question, the same question sent again later (its badge moved,
//! so its bytes differ), and the next decision for the same chat; the Swift
//! package opens them and checks which show. `HERDR_UPDATE_FIXTURES=1`
//! rewrites it.

use super::alert::{EVENT, device, fixed_seal, json, open, question};
use crate::{
    apns::{
        Push,
        alert::{Sealed, push, scope},
    },
    push::{EventId, Notification},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use std::path::PathBuf;

fn sent(notification: &Notification, badge: u32) -> Push {
    let sealed = Sealed::new(notification, super::alert::COMPUTER.to_owned(), badge);
    push(
        sealed,
        &scope(&device().public_key()),
        &notification.tag,
        fixed_seal,
    )
    .unwrap()
}

fn event(push: &Push) -> Value {
    json(&open(json(&push.body)["sealed"].as_str().unwrap()))["event"].clone()
}

#[test]
fn copies_of_one_decision_share_its_event_and_the_next_does_not() {
    let asked = question();
    let next = Notification {
        event: EventId::fixed("0f1e2d3c4b5a69788796a5b4c3d2e1f0"),
        ..question()
    };
    let pushes = [sent(&asked, 2), sent(&asked, 3), sent(&next, 3)];
    assert_ne!(pushes[0].body, pushes[1].body, "a later copy differs");
    assert_eq!(event(&pushes[0]), EVENT);
    assert_eq!(event(&pushes[1]), EVENT);
    assert_ne!(event(&pushes[2]), EVENT);
    // All three are one chat's, so APNs keeps the newest anyway.
    assert!(
        pushes
            .iter()
            .all(|push| push.collapse_id == pushes[0].collapse_id)
    );

    let fixture = json!({
        "device_private_key": URL_SAFE_NO_PAD.encode(device().to_bytes()),
        "pushes": pushes.iter().map(|push| json(&push.body)["sealed"].clone()).collect::<Vec<_>>(),
        "shows": [true, false, true],
    });
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ios/SheprdRemoteCore/Tests/SheprdRemoteCoreTests/Fixtures/notify/repeat.json");
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

#[test]
fn each_decision_gets_its_own_event() {
    let one = Notification::cleared("w1:p2");
    let two = Notification::cleared("w1:p2");
    assert_ne!(one.event, two.event);
    assert_eq!(one.event.as_str().len(), 32);
    // A test push is a decision of its own too.
    let test = Sealed::test("id".to_owned(), "C".to_owned(), "Studio", 0);
    assert!(test.event.is_some());
}
