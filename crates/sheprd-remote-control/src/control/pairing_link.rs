//! A pairing link (`docs/relay-plan.md`, Adding a Sheprd from a link): what
//! a computer's pairing dialog copies so another Sheprd, on any network,
//! can claim it through its relays without Bluetooth.
//!
//! It carries what a nearby offer seals: the claim token, which is also the
//! relays' pairing secret, the certificate authority to pin, the relays,
//! route, and tunnel identity, and the fleet, if any. It is a secret for
//! its two minutes: anyone holding it may claim, and the person on the
//! computer still compares the code and answers.
//!
//! The text is `sheprd-pair:` and base64url of a JSON object, so it
//! survives any chat or mail and is never taken for a web address.

use super::{fleet::bluetooth::Hint, host_relay::Info};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// What every link starts with.
pub const PREFIX: &str = "sheprd-pair:";
/// The longest link read: a tunnel identity (1,216 bytes), a few relays,
/// and a name, base64url of JSON.
pub const MAX_LEN: usize = 4 * 1024;
/// The relays a link names at most.
pub const MAX_RELAYS: usize = 3;
/// The only version written and read.
const VERSION: u8 = 1;
/// The longest computer name read: the daemon's device name limit.
const MAX_COMPUTER: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(
        "This is not a pairing link. On the other computer, open Remote control > Pair device and click Copy link."
    )]
    Format,
    #[error(
        "That computer listens on no relay, so it cannot be reached from another network. Turn on relays in its Settings > Remote control."
    )]
    NoRelays,
    #[error("This is longer than any pairing link. Copy it again.")]
    TooLong,
    #[error("The link comes from a newer Sheprd. Update this one.")]
    Version,
    // keep-sorted end
}

/// The 32 bytes a link's holder claims with, kept out of `Debug`.
#[derive(Clone, Eq, PartialEq)]
pub struct Token(pub [u8; 32]);

impl fmt::Debug for Token {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Token(..)")
    }
}

/// A pairing link, read or about to be written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingLink {
    /// What the computer calls itself, as its dialog names it.
    pub computer: String,
    /// Its certificate authority's SHA-256, which its TLS must present.
    pub fingerprint: [u8; 32],
    /// Its fleet, when it is a computer member of one.
    pub fleet: Option<Hint>,
    /// How to reach it through its relays, at most [`MAX_RELAYS`].
    pub relay: Info,
    pub token: Token,
}

#[derive(Deserialize, Serialize)]
struct Wire {
    v: u8,
    computer: String,
    fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fleet: Option<Hint>,
    relay: Info,
    token: String,
}

impl fmt::Display for PairingLink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let wire = Wire {
            v: VERSION,
            computer: self.computer.clone(),
            fingerprint: URL_SAFE_NO_PAD.encode(self.fingerprint),
            fleet: self.fleet.clone(),
            relay: self.relay.clone(),
            token: URL_SAFE_NO_PAD.encode(self.token.0),
        };
        let json = serde_json::to_vec(&wire).map_err(|_| fmt::Error)?;
        write!(formatter, "{PREFIX}{}", URL_SAFE_NO_PAD.encode(json))
    }
}

impl FromStr for PairingLink {
    type Err = Error;

    /// Reads a link as pasted: surrounding white space and line breaks a
    /// chat inserted are dropped.
    fn from_str(text: &str) -> Result<Self, Error> {
        if text.len() > MAX_LEN * 2 {
            return Err(Error::TooLong);
        }
        let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        if text.len() > MAX_LEN {
            return Err(Error::TooLong);
        }
        let body = text.strip_prefix(PREFIX).ok_or(Error::Format)?;
        let json = URL_SAFE_NO_PAD.decode(body).map_err(|_| Error::Format)?;
        let wire: Wire = serde_json::from_slice(&json).map_err(|_| Error::Format)?;
        if wire.v != VERSION {
            return Err(Error::Version);
        }
        if wire.relay.relays.is_empty() {
            return Err(Error::NoRelays);
        }
        let relay = Some(wire.relay)
            .filter(|relay| relay.relays.len() <= MAX_RELAYS && relay.valid())
            .ok_or(Error::Format)?;
        let computer = wire.computer.trim();
        if computer.is_empty()
            || computer.chars().count() > MAX_COMPUTER
            || computer.chars().any(char::is_control)
        {
            return Err(Error::Format);
        }
        Ok(Self {
            computer: computer.to_owned(),
            fingerprint: bytes(&wire.fingerprint)?,
            fleet: wire.fleet.filter(Hint::valid),
            relay,
            token: Token(bytes(&wire.token)?),
        })
    }
}

fn bytes(text: &str) -> Result<[u8; 32], Error> {
    URL_SAFE_NO_PAD
        .decode(text)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(Error::Format)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
