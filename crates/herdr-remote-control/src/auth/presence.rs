//! Presence: a paired phone near the computer over Bluetooth proves it is
//! itself by signing a fresh nonce with its device key, as a session
//! challenge does, but under its own prefix so neither signature serves the
//! other (`docs/ios-app-plan.md`, "Presence, exactly"). Herdr UI relays the
//! steps from the `herdr-nearby` helper; a proof only tells the daemon the
//! phone is the one Herdr UI saw, and whether it is near is Herdr UI's call.

use super::{Devices, Error, Result, SECRET_LEN, decode_fixed, encode, random_bytes};
use crate::control::DeviceId;
use p256::ecdsa::{Signature, signature::Verifier};
use std::collections::HashMap;

/// How long a presence nonce can be answered.
pub(crate) const NONCE_TTL_MS: u64 = 30_000;
/// What a phone signs before the nonce.
pub(crate) const PREFIX: &[u8] = b"herdr-presence-v1";
/// Length of an IEEE P1363 P-256 signature: r || s.
const SIGNATURE_LEN: usize = 64;

#[derive(Debug)]
struct Pending {
    nonce: [u8; SECRET_LEN],
    expires_ms: u64,
}

/// One outstanding nonce per device; a new `hello` replaces it.
#[derive(Debug, Default)]
pub(crate) struct Presences {
    pending: HashMap<DeviceId, Pending>,
}

impl Presences {
    /// A fresh nonce for `device` to sign, base64url.
    pub(crate) fn challenge(
        &mut self,
        device: &DeviceId,
        now_ms: u64,
        devices: &Devices,
    ) -> Result<String> {
        if devices.get(device).is_none() {
            return Err(Error::UnknownDevice);
        }
        self.pending
            .retain(|_, pending| pending.expires_ms > now_ms);
        let nonce = random_bytes::<SECRET_LEN>()?;
        self.pending.insert(
            device.clone(),
            Pending {
                nonce,
                expires_ms: now_ms.saturating_add(NONCE_TTL_MS),
            },
        );
        Ok(encode(&nonce))
    }

    /// Checks `device`'s signature over the nonce it was given; the nonce
    /// is spent either way.
    pub(crate) fn prove(
        &mut self,
        device: &DeviceId,
        nonce: &str,
        signature: &str,
        now_ms: u64,
        devices: &Devices,
    ) -> Result<()> {
        let pending = self
            .pending
            .remove(device)
            .filter(|pending| pending.expires_ms > now_ms)
            .ok_or(Error::UnknownPresence)?;
        let given: [u8; SECRET_LEN] = decode_fixed(nonce).map_err(|_| Error::UnknownPresence)?;
        if !super::constant_time_eq(&given, &pending.nonce) {
            return Err(Error::UnknownPresence);
        }
        let signature: [u8; SIGNATURE_LEN] =
            decode_fixed(signature).map_err(|_| Error::InvalidSignature)?;
        let signature = Signature::from_slice(&signature).map_err(|_| Error::InvalidSignature)?;
        let key = devices
            .get(device)
            .ok_or(Error::UnknownDevice)?
            .public_key
            .verifying_key()?;
        key.verify(&message(&pending.nonce), &signature)
            .map_err(|_| Error::InvalidSignature)
    }
}

/// What a phone signs: [`PREFIX`] then the nonce's 32 bytes.
pub(crate) fn message(nonce: &[u8]) -> Vec<u8> {
    [PREFIX, nonce].concat()
}
