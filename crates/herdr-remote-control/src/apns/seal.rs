//! Notification payloads sealed to one iPhone, so APNs relays bytes it
//! cannot read (`docs/ios-app-plan.md`, Notifications). The app's
//! Notification Service Extension opens them with a Secure Enclave
//! key-agreement key whose public half the device registered.
//!
//! `sealed = ephemeral SEC1 point (65) || nonce (12) || AES-256-GCM(text) || tag`,
//! keyed by `HKDF-SHA256(salt: ephemeral || device, ikm: ECDH, info: INFO)`
//! with `INFO` as the associated data. CryptoKit's
//! `hkdfDerivedSymmetricKey(using:salt:sharedInfo:outputByteCount:)` derives
//! the same key, which the shared fixture checks.
//!
//! Phase 0 spike: tested against the Swift side, wired into pushes in phase 5.

use aes_gcm::{Aes256Gcm, KeyInit as _, aead::Aead as _, aead::Payload};
use hkdf::Hkdf;
use p256::{PublicKey, SecretKey, elliptic_curve::zeroize::Zeroizing};
use sha2::Sha256;

/// Names the format in the key derivation and the associated data.
pub(crate) const INFO: &[u8] = b"herdr-remote-notify-v1";
const POINT_LEN: usize = 65;
const NONCE_LEN: usize = 12;

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum Error {
    #[error("could not seal the notification")]
    Seal,
}

/// Seals `plaintext` to `device` with the given one-time key and nonce;
/// production passes fresh random ones.
pub(crate) fn seal_with(
    device: &PublicKey,
    plaintext: &[u8],
    ephemeral: &SecretKey,
    nonce: &[u8; NONCE_LEN],
) -> Result<Vec<u8>, Error> {
    let ephemeral_point = ephemeral.public_key().to_sec1_bytes();
    let device_point = device.to_sec1_bytes();
    let mut salt = Vec::with_capacity(2 * POINT_LEN);
    salt.extend_from_slice(&ephemeral_point);
    salt.extend_from_slice(&device_point);
    let shared = ephemeral.diffie_hellman(device);
    let mut key = Zeroizing::new([0; 32]);
    Hkdf::<Sha256>::new(Some(&salt), shared.raw_secret_bytes())
        .expand(INFO, key.as_mut_slice())
        .map_err(|_| Error::Seal)?;
    let cipher = Aes256Gcm::new_from_slice(key.as_slice()).map_err(|_| Error::Seal)?;
    let ciphertext = cipher
        .encrypt(
            nonce.into(),
            Payload {
                msg: plaintext,
                aad: INFO,
            },
        )
        .map_err(|_| Error::Seal)?;
    let mut sealed = Vec::with_capacity(POINT_LEN + NONCE_LEN + ciphertext.len());
    sealed.extend_from_slice(&ephemeral_point);
    sealed.extend_from_slice(nonce);
    sealed.extend_from_slice(&ciphertext);
    Ok(sealed)
}
