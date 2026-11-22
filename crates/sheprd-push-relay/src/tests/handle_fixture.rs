//! `Fixtures/push_relay/handle.json` in the Swift package: a handle the iOS app
//! makes, byte for byte, with fixed one-time values, so the phone's sealing
//! and the relay's opening are held to one format. `HERDR_UPDATE_FIXTURES=1`
//! rewrites it.

use super::*;
use crate::Kind;
use secrecy::ExposeSecret as _;
use serde_json::Value;
use std::path::PathBuf;

const ID: &str = "0123456789abcdef0123456789abcdef";

fn ephemeral() -> SecretKey {
    SecretKey::from_slice(&[9; 32]).unwrap()
}

fn fixture() -> Value {
    let credential = issuer().issue(PHONE, Host::Sandbox, NOW_S);
    // Longer than an alert token, as an activity's is.
    let token = "80a1".repeat(20);
    // Keys in order, as the phone's encoder sorts them.
    let plaintext = serde_json::to_string(&json!({
        "credential": credential,
        "id": ID,
        "kind": "activity",
        "token": token,
    }))
    .unwrap();
    let key = relay_key();
    let sealed = seal::seal_with(
        &key.public_key(),
        plaintext.as_bytes(),
        seal::HANDLE_INFO,
        &ephemeral(),
        &[1; 12],
    )
    .unwrap();
    let (relay_key_id, relay_public) = Keyring::new(key.clone(), None).current().unwrap();
    json!({
        "credential": credential,
        "ephemeral_private_key": URL_SAFE_NO_PAD.encode(ephemeral().to_bytes()),
        "handle": format!("{relay_key_id}.{}", URL_SAFE_NO_PAD.encode(sealed)),
        "id": ID,
        "kind": "activity",
        "nonce": URL_SAFE_NO_PAD.encode([1; 12]),
        "plaintext": plaintext,
        "relay_key": relay_public,
        "relay_key_id": relay_key_id,
        "relay_private_key": URL_SAFE_NO_PAD.encode(key.to_bytes()),
        "token": token,
    })
}

#[test]
fn the_handle_fixture_is_what_the_relay_opens() {
    let fixture = fixture();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../ios/SheprdRemoteCore/Tests/SheprdRemoteCoreTests/Fixtures/push_relay/handle.json",
    );
    if std::env::var_os("HERDR_UPDATE_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
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

    let handle = Keyring::new(relay_key(), None)
        .open(expected["handle"].as_str().unwrap())
        .unwrap();
    assert_eq!(handle.id, ID);
    assert_eq!(handle.kind, Kind::Activity);
    assert_eq!(
        handle.token,
        DeviceToken::parse(expected["token"].as_str().unwrap()).unwrap()
    );
    let credential = issuer().check(handle.credential.expose_secret()).unwrap();
    assert_eq!(credential.environment, Host::Sandbox);
    assert_eq!(credential.device, PHONE);
}
