//! The Linear API key: `LINEAR_API_KEY` when set, else the key saved like the
//! GitHub sign-in (the macOS Keychain, the Linux Secret Service, Windows
//! Credential Manager, or with an explicit opt-in a private file). Reading and saving block on that store,
//! so only the orchestrator's worker calls them.

use crate::{Error, Result};
use herdr_credentials::Store;
use secrecy::{ExposeSecret, SecretString};
use std::{ffi::OsString, path::PathBuf};
use zeroize::Zeroizing;

pub const VARIABLE: &str = "LINEAR_API_KEY";
const KEY_LIMIT: usize = 512;

/// Whether `key` could be an API key, and so is safe in a header.
pub fn valid(key: &str) -> bool {
    !key.is_empty() && key.len() <= KEY_LIMIT && key.bytes().all(|b| b.is_ascii_graphic())
}

const ENTRY: herdr_credentials::Entry<Error> = herdr_credentials::Entry {
    service: "dev.herdr.gpui.linear",
    account: "api-key",
    label: "Herdr GPUI Linear API key",
    file: c"linear-api-key",
    validate: |value| {
        if valid(value.expose_secret()) {
            Ok(())
        } else {
            Err(Error::LinearKeyRecord)
        }
    },
};

fn storage(error: Error) -> Error {
    Error::LinearKey(Box::new(error))
}

/// The directory the GUI config, and so the private credential file, lives
/// in, as for the GitHub sign-in.
fn directory() -> Result<PathBuf> {
    Ok(herdr_client::local_path::config_root()?.join("herdr"))
}

/// The key to use, `None` when there is none.
pub fn read(store: Store) -> Result<Option<SecretString>> {
    resolve(
        |name| std::env::var_os(name),
        || {
            let directory = directory().map_err(storage)?;
            herdr_credentials::read_entry(store, &ENTRY, &directory).map_err(storage)
        },
    )
}

/// The environment's key, read through `var`, else `saved`'s. A set but
/// blank variable counts as unset.
pub fn resolve(
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
pub fn save(store: Store, key: Option<&SecretString>) -> Result<()> {
    if key.is_some_and(|key| !valid(key.expose_secret())) {
        return Err(Error::LinearKeyFormat);
    }
    let directory = directory().map_err(storage)?;
    herdr_credentials::save_entry(store, &ENTRY, &directory, key).map_err(storage)
}
