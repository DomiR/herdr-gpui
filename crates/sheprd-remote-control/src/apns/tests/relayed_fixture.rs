//! `Fixtures/push_relay/activity.json` in the Swift package: a Live Activity
//! start and update as the daemon seals them for the push relay, and the
//! phone's key, so the widget extension's opening is held to what the
//! daemon writes. Sealing is random, so the check is that the fixture's
//! sealed fields open to what the daemon serializes today.
//! `HERDR_UPDATE_FIXTURES=1` rewrites it.

use super::{
    relayed::{open, payload, phone, start},
    *,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use sheprd_chat_protocol::{PermissionKind, PermissionOption};
use std::path::PathBuf;

fn live() -> Live {
    let mut live = start("relay");
    live.state.options = vec![
        PermissionOption {
            id: "allow".to_owned(),
            kind: PermissionKind::AllowOnce,
            name: "Allow once".to_owned(),
        },
        PermissionOption {
            id: "reject".to_owned(),
            kind: PermissionKind::RejectOnce,
            name: "Reject".to_owned(),
        },
    ];
    live
}

#[test]
fn the_activity_fixture_opens_to_what_the_daemon_seals() {
    let live = live();
    let Event::Start { attributes, .. } = &live.event else {
        unreachable!()
    };
    let attributes = serde_json::to_value(attributes).unwrap();
    let state = serde_json::to_value(&live.state).unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../ios/SheprdRemoteCore/Tests/SheprdRemoteCoreTests/Fixtures/push_relay/activity.json",
    );
    if std::env::var_os("HERDR_UPDATE_FIXTURES").is_some() {
        let mut update = live.clone();
        update.event = Event::Update { alert: None };
        let fixture = json!({
            "attributes": attributes,
            "content_state": state,
            "device_private_key": URL_SAFE_NO_PAD.encode(phone().to_bytes()),
            "start": payload(&live),
            "update": payload(&update),
        });
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text = format!("{}\n", serde_json::to_string_pretty(&fixture).unwrap());
        std::fs::write(&path, text).unwrap();
    }
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let stale = format!(
        "{} is out of date; run with HERDR_UPDATE_FIXTURES=1",
        path.display()
    );
    assert_eq!(fixture["attributes"], attributes, "{stale}");
    assert_eq!(fixture["content_state"], state, "{stale}");
    assert_eq!(
        fixture["device_private_key"],
        URL_SAFE_NO_PAD.encode(phone().to_bytes()),
        "{stale}"
    );
    let start = &fixture["start"]["aps"];
    assert_eq!(open(&start["attributes"]), attributes, "{stale}");
    assert_eq!(open(&start["content-state"]), state, "{stale}");
    let update = &fixture["update"]["aps"];
    assert!(update.get("attributes").is_none());
    assert_eq!(open(&update["content-state"]), state, "{stale}");
}
