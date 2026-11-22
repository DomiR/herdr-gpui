//! A push whose content is sealed to one phone: the only kind
//! [`crate::RelayClient`] takes, so sending plain text through the push
//! relay is a compile error rather than something a review has to catch
//! (`docs/apns-relay-plan.md`, Live Activity privacy).
//!
//! A [`SealedPush`] is made only by [`SealedPush::build`], which hands the
//! code building the payload a [`Sealer`] for one phone's key, then checks
//! what came back: the body is JSON whose every string is text the sealer
//! made or one the caller named as clear, such as an alert's fixed cover,
//! and so is the collapse id. Plain text has to be named to get through.

use crate::{Error, Push, Result, seal};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::PublicKey;
use serde_json::Value;
use std::cell::RefCell;

/// Seals to one phone while a [`SealedPush`] is built, and keeps what it
/// made.
pub struct Sealer<'a> {
    device: &'a PublicKey,
    info: &'a [u8],
    made: RefCell<Vec<String>>,
}

impl Sealer<'_> {
    /// `plaintext` sealed to the phone, under the push's `info`.
    pub fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let sealed = seal::seal(self.device, plaintext, self.info)?;
        self.made.borrow_mut().push(URL_SAFE_NO_PAD.encode(&sealed));
        Ok(sealed)
    }
}

/// A push built around content sealed to one phone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealedPush(Push);

impl SealedPush {
    /// The push `build` makes with a [`Sealer`] for `device` under `info`.
    /// Its body must be JSON carrying the sealer's base64url output, and no
    /// string, nor a collapse id, that is neither sealed nor in `clear`;
    /// anything else is refused with [`Error::NotSealed`].
    pub fn build<E: From<Error>>(
        device: &PublicKey,
        info: &[u8],
        clear: &[impl AsRef<str>],
        build: impl FnOnce(&Sealer<'_>) -> Result<Push, E>,
    ) -> Result<Self, E> {
        let sealer = Sealer {
            device,
            info,
            made: RefCell::new(Vec::new()),
        };
        let push = build(&sealer)?;
        let made = sealer.made.into_inner();
        let allowed = |text: &str| {
            made.iter().any(|sealed| sealed == text)
                || clear.iter().any(|clear| clear.as_ref() == text)
        };
        let body: Value = serde_json::from_slice(&push.body).map_err(|_| Error::NotSealed)?;
        let mut strings = Vec::new();
        strings_in(&body, &mut strings);
        let carries_sealed = strings
            .iter()
            .any(|text| made.iter().any(|sealed| sealed == text));
        let collapse_id_clear = push.collapse_id.as_deref().is_none_or(allowed);
        if !carries_sealed || !collapse_id_clear || !strings.iter().all(|text| allowed(text)) {
            return Err(Error::NotSealed.into());
        }
        Ok(Self(push))
    }

    pub fn push(&self) -> &Push {
        &self.0
    }
}

/// Every string value in `value`, keys aside: the code that builds a body
/// names its keys.
fn strings_in<'a>(value: &'a Value, strings: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => strings.push(text),
        Value::Array(values) => values.iter().for_each(|value| strings_in(value, strings)),
        Value::Object(map) => map.values().for_each(|value| strings_in(value, strings)),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}
