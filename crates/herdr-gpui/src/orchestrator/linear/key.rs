//! The Linear API key: `LINEAR_API_KEY` when set, else the key saved like the
//! GitHub sign-in (the macOS Keychain, the Linux Secret Service, or with an
//! explicit opt-in a private file). Reading and saving block on that store,
//! so only the orchestrator's worker calls them.

use crate::{
    github::{Entry, Store, read_entry, save_entry},
    orchestrator::{Error, Result},
};
use secrecy::{ExposeSecret, SecretString};
use std::ffi::OsString;
use zeroize::Zeroizing;

pub(crate) const VARIABLE: &str = "LINEAR_API_KEY";
const KEY_LIMIT: usize = 512;

/// Whether `key` could be an API key, and so is safe in a header.
pub(crate) fn valid(key: &str) -> bool {
    !key.is_empty() && key.len() <= KEY_LIMIT && key.bytes().all(|b| b.is_ascii_graphic())
}

const ENTRY: Entry = Entry {
    service: "dev.herdr.gpui.linear",
    account: "api-key",
    label: "Herdr GPUI Linear API key",
    file: c"linear-api-key",
    validate: |value| {
        if valid(value.expose_secret()) {
            Ok(())
        } else {
            Err(crate::Error::LinearKeyRecord)
        }
    },
};

fn storage(error: crate::Error) -> Error {
    Error::LinearKey(Box::new(error))
}

/// The key to use, `None` when there is none.
pub(crate) fn read(store: Store) -> Result<Option<SecretString>> {
    resolve(
        |name| std::env::var_os(name),
        || read_entry(store, &ENTRY).map_err(storage),
    )
}

/// The environment's key, read through `var`, else `saved`'s. A set but
/// blank variable counts as unset.
pub(crate) fn resolve(
    var: impl Fn(&str) -> Option<OsString>,
    saved: impl FnOnce() -> Result<Option<SecretString>>,
) -> Result<Option<SecretString>> {
    let Some(value) = var(VARIABLE) else {
        return saved();
    };
    let bytes = Zeroizing::new(value.into_encoded_bytes());
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error::LinearVariable)?
        .trim();
    if text.is_empty() {
        return saved();
    }
    if !valid(text) {
        return Err(Error::LinearVariable);
    }
    Ok(Some(text.into()))
}

/// Saves `key`, or with `None` removes the saved one.
pub(crate) fn save(store: Store, key: Option<&SecretString>) -> Result<()> {
    if key.is_some_and(|key| !valid(key.expose_secret())) {
        return Err(Error::LinearKeyFormat);
    }
    save_entry(store, &ENTRY, key).map_err(storage)
}
