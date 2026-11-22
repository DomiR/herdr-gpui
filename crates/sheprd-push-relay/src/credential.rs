//! A device credential: the relay's word, for thirty days, that a device
//! passed App Attest. It travels inside handles, so the relay stores none.
//!
//! `credential = base64url(device (16) || environment (1) || expires_s (8, big
//! endian) || HMAC-SHA256(secret, the rest))`
//!
//! The same secret keys the challenges a phone attests over and the device
//! ids attested keys get, each through its own derived key, so the relay
//! keeps no state for either:
//!
//! - `challenge = base64url(random (16) || expires_s (8) || HMAC-SHA256(
//!   challenge key, the rest))`, good for [`CHALLENGE_LIFETIME_S`].
//! - `device = HMAC-SHA256(device key, key id)`, first 16 bytes: the same
//!   key always gets the same device, which tells nothing of the phone.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit as _, Mac as _};
use sha2::Sha256;
use sheprd_apns::Host;
use std::collections::HashMap;

/// How long a credential lasts, matching the daemon's `device_idle_days`.
pub const LIFETIME_S: u64 = 30 * 24 * 60 * 60;
/// How long a phone has to attest over a challenge: App Attest took under
/// two seconds on an iPhone 15 Pro.
pub const CHALLENGE_LIFETIME_S: u64 = 5 * 60;
const CHALLENGE_RANDOM: usize = 16;
const CHALLENGE_FACTS: usize = CHALLENGE_RANDOM + 8;
/// Labels the derived keys. Each differs in length from a credential's
/// facts, so no derivation can be passed off as a credential.
const CHALLENGE_LABEL: &[u8] = b"sheprd-push-relay challenge";
const DEVICE_LABEL: &[u8] = b"sheprd-push-relay device";
const DEVICE_LEN: usize = 16;
const FACTS_LEN: usize = DEVICE_LEN + 1 + 8;
const MAC_LEN: usize = 32;

/// A device, by a random id that says nothing of the phone.
pub type DeviceId = [u8; DEVICE_LEN];

/// What a credential vouches for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Credential {
    pub device: DeviceId,
    /// Which APNs environment the app's tokens belong to.
    pub environment: Host,
    /// When it stops working, in seconds since the Unix epoch.
    pub expires_s: u64,
}

/// The most challenges [`Spent`] remembers, which bounds its memory: more
/// attestations than that within one challenge's lifetime are refused.
const MAX_SPENT: usize = 65_536;

/// Challenges already attested over, each until it would have expired, so
/// a captured attestation cannot be sent again for another credential. In
/// memory: after a restart, a challenge spent in the last five minutes could
/// be spent once more, for the same key and so the same device.
#[derive(Debug, Default)]
pub struct Spent {
    until_s: HashMap<String, u64>,
}

impl Spent {
    /// Marks `challenge`, which [`Issuer::check_challenge`] took at `now_s`,
    /// as used: `false` if it was already, or no more can be remembered.
    pub fn spend(&mut self, challenge: &str, now_s: u64) -> bool {
        if self.until_s.len() >= MAX_SPENT {
            self.until_s.retain(|_, until_s| *until_s > now_s);
        }
        if self.until_s.len() >= MAX_SPENT || self.until_s.contains_key(challenge) {
            return false;
        }
        self.until_s
            .insert(challenge.to_owned(), now_s + CHALLENGE_LIFETIME_S);
        true
    }
}

/// Issues and checks credentials and challenges with the relay's secret.
pub struct Issuer {
    challenges: Hmac<Sha256>,
    devices: Hmac<Sha256>,
    keyed: Hmac<Sha256>,
}

impl Issuer {
    /// The issuer for `secret`, or `None` if it cannot key an HMAC.
    #[must_use]
    pub fn new(secret: &[u8]) -> Option<Self> {
        let keyed = <Hmac<Sha256>>::new_from_slice(secret).ok()?;
        let derive = |label: &[u8]| {
            let mut mac = keyed.clone();
            mac.update(label);
            <Hmac<Sha256>>::new_from_slice(&mac.finalize().into_bytes()).ok()
        };
        Some(Self {
            challenges: derive(CHALLENGE_LABEL)?,
            devices: derive(DEVICE_LABEL)?,
            keyed,
        })
    }

    /// A fresh challenge for a phone to attest over, from `now_s`.
    pub fn challenge(&self, now_s: u64) -> Result<String, getrandom::Error> {
        let mut bytes = vec![0; CHALLENGE_RANDOM];
        getrandom::fill(&mut bytes)?;
        bytes.extend_from_slice(&(now_s + CHALLENGE_LIFETIME_S).to_be_bytes());
        let mut mac = self.challenges.clone();
        mac.update(&bytes);
        bytes.extend_from_slice(&mac.finalize().into_bytes());
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }

    /// Whether this relay issued `text` and it is still good at `now_s`.
    #[must_use]
    pub fn check_challenge(&self, text: &str, now_s: u64) -> bool {
        let Ok(bytes) = URL_SAFE_NO_PAD.decode(text) else {
            return false;
        };
        if bytes.len() != CHALLENGE_FACTS + MAC_LEN {
            return false;
        }
        let (facts, tag) = bytes.split_at(CHALLENGE_FACTS);
        let mut mac = self.challenges.clone();
        mac.update(facts);
        if mac.verify_slice(tag).is_err() {
            return false;
        }
        facts
            .get(CHALLENGE_RANDOM..)
            .and_then(|expires| expires.try_into().ok())
            .is_some_and(|expires| u64::from_be_bytes(expires) > now_s)
    }

    /// The device an attested key belongs to.
    #[must_use]
    pub fn device(&self, key_id: &[u8]) -> DeviceId {
        let mut mac = self.devices.clone();
        mac.update(key_id);
        let mut device = [0; DEVICE_LEN];
        device.copy_from_slice(&mac.finalize().into_bytes()[..DEVICE_LEN]);
        device
    }

    /// The credential's text for `device` from `now_s`.
    #[must_use]
    pub fn issue(&self, device: DeviceId, environment: Host, now_s: u64) -> String {
        let mut bytes = Vec::with_capacity(FACTS_LEN + MAC_LEN);
        bytes.extend_from_slice(&device);
        bytes.push(match environment {
            Host::Production => 0,
            Host::Sandbox => 1,
        });
        bytes.extend_from_slice(&(now_s + LIFETIME_S).to_be_bytes());
        let mac = self.mac(&bytes).finalize().into_bytes();
        bytes.extend_from_slice(&mac);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// What `text` vouches for, when this relay issued it. Expiry is the
    /// caller's check, since a revocation still wants the date.
    #[must_use]
    pub fn check(&self, text: &str) -> Option<Credential> {
        let bytes = URL_SAFE_NO_PAD.decode(text).ok()?;
        if bytes.len() != FACTS_LEN + MAC_LEN {
            return None;
        }
        let (facts, mac) = bytes.split_at(FACTS_LEN);
        self.mac(facts).verify_slice(mac).ok()?;
        let (device, rest) = facts.split_at(DEVICE_LEN);
        let (environment, expires) = rest.split_at(1);
        Some(Credential {
            device: device.try_into().ok()?,
            environment: match environment {
                [0] => Host::Production,
                [1] => Host::Sandbox,
                _ => return None,
            },
            expires_s: u64::from_be_bytes(expires.try_into().ok()?),
        })
    }

    fn mac(&self, facts: &[u8]) -> Hmac<Sha256> {
        let mut mac = self.keyed.clone();
        mac.update(facts);
        mac
    }
}
