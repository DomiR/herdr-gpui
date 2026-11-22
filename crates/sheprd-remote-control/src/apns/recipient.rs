//! Where one of a device's pushes goes: a push relay handle
//! (`docs/apns-relay-plan.md`), which the phone sealed to the relay so this
//! daemon never sees the token inside, or a plain APNs token.
//!
//! Pushes go only through the push relay, which reaches handles alone. An
//! app from before the relay still registers plain tokens: `POST /api/apns`
//! takes them, and the device store keeps and loads them, so that app keeps
//! registering without errors, but nothing is ever sent to a token, and the
//! daemon never posts to Apple. That app gets no APNs push.
//!
//! The device store and `POST /api/apns` write a token as hex, as before,
//! and a handle as an object naming the relay environment its credential is
//! for, since the daemon cannot open the handle to read it:
//!
//! ```json
//! "9f02…"
//! { "handle": "1a2b3c4d.BHk3…", "environment": "production" }
//! ```
//!
//! The environment only picks one of the two relays compiled into
//! `sheprd_apns::RelayHost`, so a device still cannot send the daemon
//! anywhere else.

use super::{DeviceToken, Error, Result};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sheprd_apns::RelayHost;

/// The longest handle taken, as the relay bounds it: a 256-byte token as
/// hex and a credential, sealed and in base64url, fit well within it.
const MAX_HANDLE: usize = 2048;
/// A relay key id: four bytes as lowercase hex.
const KEY_ID_LEN: usize = 8;
/// The shortest sealed text: a point, a nonce, and a tag, in base64url.
const MIN_SEALED: usize = 124;

/// One push recipient.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum Recipient {
    /// A raw APNs token from an app older than the push relay. Kept, so its
    /// registration still loads, but never sent to.
    Token(DeviceToken),
    /// A handle the push relay opens.
    Handle(Handle),
}

/// A push relay handle and the relay it goes to. It reaches this user's
/// phone, so it is a secret: `Debug` hides it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "Raw", into = "Raw")]
pub(crate) struct Handle {
    environment: RelayHost,
    text: SecretString,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    environment: RelayHost,
    handle: String,
}

impl Handle {
    pub(crate) fn parse(text: &str, environment: RelayHost) -> Result<Self> {
        let text = text.trim();
        let (key_id, sealed) = text.split_once('.').ok_or(Error::Handle)?;
        let valid = text.len() <= MAX_HANDLE
            && key_id.len() == KEY_ID_LEN
            && key_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            && sealed.len() >= MIN_SEALED
            && sealed
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        if !valid {
            return Err(Error::Handle);
        }
        Ok(Self {
            environment,
            text: text.to_owned().into(),
        })
    }

    pub(crate) fn environment(&self) -> RelayHost {
        self.environment
    }

    pub(crate) fn secret(&self) -> &SecretString {
        &self.text
    }
}

impl PartialEq for Handle {
    fn eq(&self, other: &Self) -> bool {
        self.environment == other.environment
            && self.text.expose_secret() == other.text.expose_secret()
    }
}

impl Eq for Handle {}

impl TryFrom<Raw> for Handle {
    type Error = Error;

    fn try_from(raw: Raw) -> Result<Self> {
        Self::parse(&raw.handle, raw.environment)
    }
}

impl From<Handle> for Raw {
    fn from(handle: Handle) -> Self {
        Self {
            environment: handle.environment,
            handle: handle.text.expose_secret().to_owned(),
        }
    }
}

/// The text the push service is handed: a token's hex, or a handle.
impl ExposeSecret<str> for Recipient {
    fn expose_secret(&self) -> &str {
        match self {
            Self::Token(token) => token.expose_secret(),
            Self::Handle(handle) => handle.text.expose_secret(),
        }
    }
}

impl From<DeviceToken> for Recipient {
    fn from(token: DeviceToken) -> Self {
        Self::Token(token)
    }
}

impl From<Handle> for Recipient {
    fn from(handle: Handle) -> Self {
        Self::Handle(handle)
    }
}
