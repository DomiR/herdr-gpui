//! What Sheprd's two hosted relays share for their operators: the relay
//! (`sheprd-relay`) and the push relay (`sheprd-push-relay`).
//!
//! - [`board`]: the read-only dashboard's routes, for any relay that is a
//!   [`Site`], and the bearer check for a `/metrics` scrape.
//! - [`auth`]: the access token, the session cookies it signs in, and the
//!   login form.
//! - [`lockout`]: blocking addresses that guess the token.
//! - [`live`]: snapshots streamed as Server-Sent Events, bounded.
//! - [`limit`]: rate limits in memory, and the keys addresses get.
//! - [`metrics`]: the label table counters are declared with, and the
//!   Prometheus text exposition.
//! - [`secret`]: secrets compared in constant time.
//!
//! Nothing here keeps state on disk, and nothing logs an address or a token.

pub mod auth;
pub mod board;
mod error;
pub mod limit;
pub mod live;
pub mod lockout;
pub mod metrics;
pub mod secret;

pub use board::{Page, Site, bearer, router, same_origin};
pub use error::{Error, Result};
pub use secret::Secret;

#[cfg(test)]
mod tests;
