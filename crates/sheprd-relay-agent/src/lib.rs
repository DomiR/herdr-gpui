//! `sheprd-relay-agent`: lets Sheprd reach this machine's SSH server
//! through relays when it has no direct path (`docs/relay-plan.md`,
//! Clients, Sheprd and SSH machines).
//!
//! The agent listens on each relay it is given with its own route key,
//! names the dial keys of the desktops in its `authorized` file and of the
//! fleet members in its access file, and admits only their tunnel keys.
//! Each admitted session is spliced, inside the end-to-end tunnel, to the
//! local SSH server; SSH's own encryption and host keys run inside it
//! unchanged. It runs no command and opens no other port, and its logs
//! name no address, route, or key.
//!
//! Without the `runtime` feature only what Sheprd's daemon and app share
//! with the agent is built, with no Tokio:
//!
//! - [`access`]: the fleet access file Sheprd computes from the fleet log,
//!   its format, bounds, digest, and merge.
//! - [`removal`]: the tunnel's purpose bytes and the removal frames.
//! - [`authorized_keys`]: the fleet's `authorized_keys` lines.
//! - [`fleet`]: the access file and suspensions this agent holds, and who
//!   they admit.
//! - [`authorized`]: the desktops allowed to dial, one by one.
//! - [`files`]: the state directory and its private files.
//!
//! With it, the agent itself:
//!
//! - [`cli`]: the command line, parsed from OS strings.
//! - [`keys`]: the route key and tunnel keys, made once.
//! - [`status`]: what `status --json` reports; Sheprd parses it without
//!   the runtime.
//! - [`agent`]: listening on every relay and forwarding sessions.

pub mod access;
#[cfg(feature = "runtime")]
pub mod agent;
pub mod authorized;
pub mod authorized_keys;
#[cfg(feature = "runtime")]
pub mod cli;
pub mod error;
pub mod files;
pub mod fleet;
#[cfg(feature = "runtime")]
pub mod keys;
#[cfg(feature = "runtime")]
mod listen;
pub mod removal;
#[cfg(feature = "runtime")]
mod serve;
pub mod status;

pub use error::{Error, Result};

#[cfg(all(test, feature = "runtime"))]
mod tests;
