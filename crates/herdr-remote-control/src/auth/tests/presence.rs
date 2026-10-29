//! Presence proofs: a paired phone signs a fresh nonce under the presence
//! prefix, once, within thirty seconds.

use super::*;
use crate::auth::presence::{NONCE_TTL_MS, message};

fn sign_presence(key: &SigningKey, nonce: &str) -> String {
    let nonce: [u8; 32] = decode_fixed(nonce).unwrap();
    let signature: Signature = key.sign(&message(&nonce));
    encode(&signature.to_bytes())
}

#[test]
fn a_paired_phone_proves_itself_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let key = signing_key(3);
    let device = pair(&mut auth, &key, T0);
    let nonce = auth.presence_challenge(&device, T0).unwrap();
    let signature = sign_presence(&key, &nonce);
    auth.presence_prove(&device, &nonce, &signature, T0 + 10)
        .unwrap();
    let again = auth
        .presence_prove(&device, &nonce, &signature, T0 + 20)
        .unwrap_err();
    assert!(matches!(again, Error::UnknownPresence));
}

#[test]
fn an_unknown_device_gets_no_nonce() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let error = auth
        .presence_challenge(&DeviceId("nobody".into()), T0)
        .unwrap_err();
    assert!(matches!(error, Error::UnknownDevice));
}

#[test]
fn a_late_answer_or_another_nonce_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let key = signing_key(3);
    let device = pair(&mut auth, &key, T0);
    let nonce = auth.presence_challenge(&device, T0).unwrap();
    let late = auth
        .presence_prove(
            &device,
            &nonce,
            &sign_presence(&key, &nonce),
            T0 + NONCE_TTL_MS,
        )
        .unwrap_err();
    assert!(matches!(late, Error::UnknownPresence));

    let nonce = auth.presence_challenge(&device, T0).unwrap();
    let other = encode(&[9; 32]);
    let error = auth
        .presence_prove(&device, &other, &sign_presence(&key, &other), T0)
        .unwrap_err();
    assert!(matches!(error, Error::UnknownPresence));
    // The real nonce was spent by the failed try.
    let error = auth
        .presence_prove(&device, &nonce, &sign_presence(&key, &nonce), T0)
        .unwrap_err();
    assert!(matches!(error, Error::UnknownPresence));
}

#[test]
fn another_key_or_a_session_signature_does_not_prove_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let key = signing_key(3);
    let device = pair(&mut auth, &key, T0);

    let nonce = auth.presence_challenge(&device, T0).unwrap();
    let error = auth
        .presence_prove(&device, &nonce, &sign_presence(&signing_key(4), &nonce), T0)
        .unwrap_err();
    assert!(matches!(error, Error::InvalidSignature));

    // The same nonce signed as a session challenge means nothing here.
    let nonce = auth.presence_challenge(&device, T0).unwrap();
    let error = auth
        .presence_prove(&device, &nonce, &sign(&key, &nonce), T0)
        .unwrap_err();
    assert!(matches!(error, Error::InvalidSignature));
}
