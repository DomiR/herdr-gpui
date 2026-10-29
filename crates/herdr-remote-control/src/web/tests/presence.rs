//! Presence through the control socket, as Herdr UI relays it from the
//! `herdr-nearby` helper: a nonce, the phone's proof, the addresses sealed
//! to it, and the near mark that holds its alerts. Also writes the fixture
//! the phone's Swift tests read (`Fixtures/presence/prove.json`).

use super::*;
use crate::{
    apns::seal::{ADDRESSES_INFO, seal_with},
    auth::presence::message,
    control::{ErrorCode, Request, Response},
};
use aes_gcm::{
    Aes256Gcm, KeyInit as _,
    aead::{Aead as _, Payload},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hkdf::Hkdf;
use p256::{
    PublicKey, SecretKey,
    ecdsa::{Signature, SigningKey, signature::Signer},
};
use serde_json::{Value, json};
use sha2::Sha256;

fn control(state: &Shared, request: Request) -> Response {
    let (stop, _) = tokio::sync::watch::channel(false);
    crate::server::answer_for_tests(state, &stop, request)
}

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The key `signed_in` pairs with for `seed`.
fn device_key(seed: u8) -> SigningKey {
    SigningKey::from_slice(&[seed; 32]).unwrap()
}

fn prove(key: &SigningKey, nonce: &str) -> String {
    let nonce = URL_SAFE_NO_PAD.decode(nonce).unwrap();
    let signature: Signature = key.sign(&message(&nonce));
    b64(&signature.to_bytes())
}

/// Opens addresses sealed to `device` as the phone does.
fn open(device: &SecretKey, sealed: &str) -> Vec<u8> {
    let sealed = URL_SAFE_NO_PAD.decode(sealed).unwrap();
    let ephemeral = PublicKey::from_sec1_bytes(&sealed[..65]).unwrap();
    let mut salt = sealed[..65].to_vec();
    salt.extend_from_slice(&device.public_key().to_sec1_bytes());
    let shared = p256::ecdh::diffie_hellman(device.to_nonzero_scalar(), ephemeral.as_affine());
    let mut key = [0; 32];
    Hkdf::<Sha256>::new(Some(&salt), shared.raw_secret_bytes())
        .expand(ADDRESSES_INFO, &mut key)
        .unwrap();
    let nonce: [u8; 12] = sealed[65..77].try_into().unwrap();
    Aes256Gcm::new_from_slice(&key)
        .unwrap()
        .decrypt(
            (&nonce).into(),
            Payload {
                msg: &sealed[77..],
                aad: ADDRESSES_INFO,
            },
        )
        .unwrap()
}

#[test]
fn a_proven_phone_gets_this_computers_addresses_sealed_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (device, _) = signed_in(&state, 4);
    let notify = SecretKey::from_slice(&[6; 32]).unwrap();
    state
        .auth()
        .devices_mut()
        .set_apns(
            &device,
            Some(json!({ "notify_key": b64(&notify.public_key().to_sec1_bytes()) })),
        )
        .unwrap();

    let Response::PresenceChallenge { nonce } = control(
        &state,
        Request::PresenceChallenge {
            device: device.clone(),
        },
    ) else {
        panic!("no challenge");
    };
    let proven = control(
        &state,
        Request::PresenceProve {
            device: device.clone(),
            signature: prove(&device_key(4), &nonce),
            nonce,
        },
    );
    let Response::PresenceProven {
        device: proven_device,
        addresses: Some(addresses),
    } = proven
    else {
        panic!("not proven: {proven:?}");
    };
    assert_eq!(proven_device, device);
    let opened: Value = serde_json::from_slice(&open(&notify, &addresses)).unwrap();
    assert_eq!(opened, json!({ "origins": state.origins() }));
}

#[test]
fn a_phone_without_a_notify_key_is_proven_without_addresses() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (device, _) = signed_in(&state, 4);
    let Response::PresenceChallenge { nonce } = control(
        &state,
        Request::PresenceChallenge {
            device: device.clone(),
        },
    ) else {
        panic!("no challenge");
    };
    let proven = control(
        &state,
        Request::PresenceProve {
            device,
            signature: prove(&device_key(4), &nonce),
            nonce,
        },
    );
    assert!(matches!(
        proven,
        Response::PresenceProven {
            addresses: None,
            ..
        }
    ));
}

#[test]
fn unknown_devices_and_bad_proofs_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let unknown = DeviceId("nobody".into());
    assert!(matches!(
        control(
            &state,
            Request::PresenceChallenge {
                device: unknown.clone()
            }
        ),
        Response::Error {
            code: ErrorCode::UnknownDevice,
            ..
        }
    ));
    assert!(matches!(
        control(
            &state,
            Request::PresenceSet {
                device: unknown,
                near: true
            }
        ),
        Response::Error {
            code: ErrorCode::UnknownDevice,
            ..
        }
    ));
    let (device, _) = signed_in(&state, 4);
    let Response::PresenceChallenge { nonce } = control(
        &state,
        Request::PresenceChallenge {
            device: device.clone(),
        },
    ) else {
        panic!("no challenge");
    };
    assert!(matches!(
        control(
            &state,
            Request::PresenceProve {
                device,
                signature: prove(&device_key(5), &nonce),
                nonce,
            },
        ),
        Response::Error { .. }
    ));
}

#[test]
fn a_near_mark_is_set_and_lifted() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (device, _) = signed_in(&state, 4);
    let set = |near| {
        control(
            &state,
            Request::PresenceSet {
                device: device.clone(),
                near,
            },
        )
    };
    assert_eq!(set(true), Response::Done);
    assert!(state.near().is_near(&device, now_ms()));
    assert_eq!(set(false), Response::Done);
    assert!(!state.near().is_near(&device, now_ms()));
}

/// The phone's side, from fixed values: what it signs, a signature the
/// daemon's verifier accepts, and addresses sealed to its notify key.
#[test]
fn the_presence_fixture_matches() {
    let device = device_key(4);
    let nonce = [3u8; 32];
    let signed = message(&nonce);
    let signature: Signature = device.sign(&signed);
    // As `Presences::prove` checks it.
    let public =
        auth::PublicKey::parse(&b64(device.verifying_key().to_sec1_point(false).as_bytes()))
            .unwrap();
    {
        use p256::ecdsa::signature::Verifier as _;
        public
            .verifying_key()
            .unwrap()
            .verify(&signed, &signature)
            .unwrap();
    }
    let notify = SecretKey::from_slice(&[6; 32]).unwrap();
    let plaintext = json!({ "origins": ["https://192.168.1.20:7443", "https://100.64.0.3:7443"] });
    let sealed = seal_with(
        &notify.public_key(),
        plaintext.to_string().as_bytes(),
        ADDRESSES_INFO,
        &SecretKey::from_slice(&[9; 32]).unwrap(),
        &[1; 12],
    )
    .unwrap();
    let sealed = b64(&sealed);
    assert_eq!(
        serde_json::from_slice::<Value>(&open(&notify, &sealed)).unwrap(),
        plaintext
    );
    let fixture = json!({
        "device_private_key": b64(&device.to_bytes()),
        "device_public_key": b64(device.verifying_key().to_sec1_point(false).as_bytes()),
        "nonce": b64(&nonce),
        "message": b64(&signed),
        "signature": b64(&signature.to_bytes()),
        "addresses": {
            "device_notify_private_key": b64(&notify.to_bytes()),
            "plaintext": plaintext,
            "sealed": sealed,
        },
    });
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ios/HerdrRemoteCore/Tests/HerdrRemoteCoreTests/Fixtures/presence/prove.json");
    if std::env::var_os("HERDR_UPDATE_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&fixture).unwrap()),
        )
        .unwrap();
        return;
    }
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display())),
    )
    .unwrap();
    assert_eq!(
        fixture,
        expected,
        "{} is out of date; run with HERDR_UPDATE_FIXTURES=1",
        path.display()
    );
}
