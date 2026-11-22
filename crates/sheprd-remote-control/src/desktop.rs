//! Adding a nearby Sheprd from another network (`docs/relay-plan.md`,
//! Adding a nearby Sheprd from another network): this computer's side,
//! run as `sheprd-remote-control desktop-pair` and `relay-connect`.
//!
//! The other computer opens Remote control > Pair device, which advertises
//! it over Bluetooth. This computer then runs a phone's side of nearby
//! pairing over Bluetooth, which Sheprd carries ([`exchange`]); claims
//! the sealed token through the other computer's relays with its relay and
//! SSH keys ([`claim`]); and, once the person there allows it, keeps the
//! machine's route and host keys ([`record`]). From then on `ssh` reaches
//! it through `relay-connect` ([`connect`]), which the other computer's
//! daemon splices to its SSH server (`crate::ssh_access`).
//!
//! - [`keys`]: this computer's dial, tunnel, and SSH keys, made once.
//! - [`files`]: the private files they and the records live in.
//! - [`joining`]: what an approved claim says about the fleet, checked.

mod claim;
pub(crate) mod connect;
mod error;
mod exchange;
pub(crate) mod files;
mod joining;
pub(crate) mod keys;
mod pair;
pub(crate) mod record;

pub use error::{Error, Result};

/// The steps `desktop-pair` takes, for other modules' tests to run them
/// against a daemon of their own: Unix only, as those tests are.
#[cfg(all(test, unix))]
pub(crate) mod for_tests {
    pub(crate) use super::{
        claim::{Allowed, Claimant, claim},
        exchange::{Exchange, Opened},
        joining::checked,
    };
}

use exchange::Opened;
use keys::Keys;
use std::{path::Path, process::ExitCode};

/// Runs `desktop-pair` to the end, reporting each step on stdout; `link`
/// reads a pairing link from stdin in place of the Bluetooth exchange.
pub fn run_pair(state_dir: Option<&Path>, name: String, link: bool) -> ExitCode {
    let Some(state_dir) = pair::state_dir(state_dir) else {
        pair::emit(&crate::control::desktop::PairOutput::Failed {
            message: "this computer has no state directory".to_owned(), // i18n: data
        });
        return ExitCode::from(2);
    };
    let source = if link {
        pair::Source::Link
    } else {
        pair::Source::Nearby
    };
    let result =
        runtime().and_then(|runtime| runtime.block_on(pair::pair(state_dir, name, source)));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            pair::emit(&crate::control::desktop::PairOutput::Failed {
                message: chain(&error),
            });
            ExitCode::FAILURE
        }
    }
}

/// Runs `relay-connect` until either side closes; SSH shows what goes to
/// standard error.
pub fn run_connect(state_dir: Option<&Path>, host: &str) -> ExitCode {
    let Some(state_dir) = pair::state_dir(state_dir) else {
        eprintln!("relay-connect: this computer has no state directory");
        return ExitCode::from(2);
    };
    let result = runtime().and_then(|runtime| runtime.block_on(connect::connect(&state_dir, host)));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("relay-connect: {}", chain(&error));
            ExitCode::FAILURE
        }
    }
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(Error::Runtime)
}

/// `error` and its causes, one sentence.
fn chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
