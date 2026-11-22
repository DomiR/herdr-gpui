//! The push relay: forwards pushes to APNs for the iOS app, so a computer
//! needs no APNs key. `docs/apns-relay-plan.md` has the design.
//!
//! The relay should know as little as it can, so that taking it over gives
//! an attacker little:
//!
//! - Daemons hold [`Handle`]s, which the phone seals to the relay's key. The
//!   relay opens one in memory for each push and stores no push token.
//! - Payloads pass through, sealed by the daemon, and are neither stored nor
//!   logged. Nothing here may log a token, a handle, or a payload.
//! - The APNs topic and host come from the relay's own config, never from a
//!   caller.
//!
//! - [`attest`]: App Attest, which a phone passes once per key to get a
//!   credential.
//! - [`credential`]: what proves a device may push, valid for thirty days,
//!   and the challenges phones attest over.
//! - [`handle`]: a token and credential sealed to the relay.
//! - [`limit`]: rate limits, in memory.
//! - [`store`]: revocations, the only state kept, in memory unless the
//!   operator chooses a database.
//! - [`relay`]: the checks, in order, and the reply each one gives.
//! - [`metrics`] and [`devices`]: counters with no per-user labels, and how
//!   many devices the relay serves, counted in memory without naming any.
//! - [`dashboard`]: the read-only dashboard and `/metrics`, behind the
//!   operator's token, served only when there is one.
//! - [`web`]: the HTTP API.
//!
//! A phone asks `POST /v1/challenge`, attests a new key over it, and sends
//! the attestation to `POST /v1/attest` for a credential and the relay's
//! key. Renewing a credential with an assertion, and the relay key signed by
//! an offline root, are still to come (`docs/apns-relay-plan.md`, phase 2).

pub mod attest;
pub mod credential;
pub mod dashboard;
pub mod devices;
mod error;
pub mod handle;
pub mod limit;
pub mod metrics;
mod options;
pub mod relay;
mod serve;
pub mod store;
pub mod web;

pub use error::{Error, Result};
pub use handle::{Handle, Kind};
pub use options::{Backend, Options, USAGE};
pub use serve::serve;

#[cfg(test)]
mod tests;
