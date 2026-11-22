//! The fleet log for the iOS app (`docs/relay-plan.md`, Fleet, phase 5),
//! so the phone keeps its own copy, replays and checks it with the same
//! rules as every computer, and signs only on what that replay supports.
//!
//! The app holds the log's bytes (`saved::encode`, in its container) and
//! passes them with each call; nothing is kept here between calls, so no
//! state or handle crosses the interface. A call is one JSON request
//! ([`Call`]) and one JSON answer, `{"ok": …}` or `{"error": …}`, through
//! the two C functions in [`abi`]. Reading the bytes back checks every
//! operation again, so a file changed in the container computes nothing a
//! signature does not back.

pub mod abi;
mod call;
mod error;

pub use call::{Call, Draft, MAX_BODIES, MAX_SYNC_OPS, respond};
pub use error::{Error, Result};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
