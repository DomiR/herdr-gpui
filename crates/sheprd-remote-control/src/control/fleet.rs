//! The fleet's signed log (`docs/relay-plan.md`, Fleet): a person's
//! computers and phones, the hosts they share, and who may reach which.
//!
//! Every member signs operations with its own P-256 key, and every member
//! keeps all of them. Each operation names the ones its signer had applied
//! (`parents`), so they form a graph like Git's commits, and two
//! operations are concurrent when neither descends from the other. The
//! fleet's [`State`] is computed from that graph alone, the same on every
//! member that holds the same operations, whatever order they arrived in.
//!
//! The rules that matter most:
//!
//! - An operation counts only when its signer was a member in its own
//!   history, so a removed member signs nothing that counts, however it
//!   dates it.
//! - A removal voids every operation by the removed member that the
//!   remover had not seen. The one exception is a counter-removal of
//!   whoever removed it concurrently, so a removed attacker can take with
//!   it only the members that removed it. It also reaches the members
//!   the removed member admitted that the remover had not seen, so no one
//!   keeps trust that came only from a voided admission.
//! - Every conflict resolves to the safer side: a removal over an
//!   addition, a deny over an allow.
//! - A checkpoint that every other operation has seen or was seen by
//!   settles what came before it: the state is folded from the state it
//!   names, in a log pruned to it and in the whole log alike, so nothing
//!   signed after it changes what was decided before it.
//!
//! Sans-I/O and without Tokio: the daemon, Sheprd, and the tests drive it,
//! and the iOS app's Swift copy is checked against the vectors
//! `tests/vectors.rs` writes.

pub mod access;
pub mod bluetooth;
mod body;
mod error;
mod fold;
mod graph;
mod hosts;
mod log;
mod membership;
mod op;
pub mod phone;
pub mod removal;
mod routes;
pub mod saved;
mod state;
pub mod sync;
mod view;
mod wire;

pub use body::{
    Access, Admit, Body, Bundle, Ca, HostIntro, HostKind, HostSeen, Kind, Link, Method, Reason,
    Remove, SelfRecord, Verified,
};
pub use error::{Error, Result};
pub use log::{Inserted, Log};
pub use op::{Op, Unsigned};
pub use state::{AccessRegister, Host, Member, State};
pub use view::{HostView, MemberView, View};

use sha2::{Digest, Sha256};

/// What every signed message starts with, before its own suffix.
pub const DOMAIN: &[u8] = b"sheprd-fleet-v1";
/// Members one removal names beside its target.
pub const MAX_ALSO_MEMBERS: usize = 16;
/// Live computer members at once.
pub const MAX_COMPUTERS: usize = 16;
/// Live phone members at once.
pub const MAX_PHONES: usize = 16;
/// Live hosts, members' own not counted.
pub const MAX_HOSTS: usize = 256;
/// Removed members remembered, forever.
pub const MAX_TOMBSTONES: usize = 1024;
/// Operations kept between checkpoints.
pub const MAX_OPS: usize = 2048;
/// Membership operations (`Found`, `Admit`, `Remove`, `Rotate`) kept
/// between checkpoints; they are the costly ones to fold.
pub const MAX_MEMBERSHIP_OPS: usize = 256;
/// Removals kept between checkpoints that members no longer live signed.
/// Removals count against no other bound, and each live member has its
/// own share ([`MAX_REMOVALS_PER_SIGNER`]), so filling the log never stops
/// a live member's removal.
pub const MAX_REMOVALS: usize = 1024;
/// Removals one member may have signed between checkpoints.
pub const MAX_REMOVALS_PER_SIGNER: usize = 32;
/// Checkpoints one member may have signed between prunes. They count
/// against no other bound, so a full log can always checkpoint and prune.
pub const MAX_CHECKPOINTS_PER_SIGNER: usize = 8;
/// The largest operation, encoded.
pub const MAX_OP: usize = 16 * 1024;
/// The largest checkpoint state, encoded.
pub const MAX_STATE: usize = 8 * 1024 * 1024;
/// Parents one operation may name.
pub const MAX_PARENTS: usize = 16;
/// Operations waiting for their parents, across every peer.
pub const MAX_PENDING: usize = 256;
/// How long an operation waits for its parents.
pub const PENDING_TTL_MS: u64 = 60 * 60 * 1000;
/// How far ahead of the receiver's clock an operation may be signed.
pub const MAX_SKEW_MS: u64 = 10 * 60 * 1000;

macro_rules! ids {
    ($($(#[$meta:meta])* $name:ident;)*) => {$(
        $(#[$meta])*
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(pub [u8; 32]);

        impl $name {
            /// The first eight bytes in hex, as tags and logs show it.
            #[must_use]
            pub fn short(&self) -> String {
                hex(&self.0[..8])
            }

            /// Reads 64 hex digits, in either case.
            #[must_use]
            pub fn from_hex(text: &str) -> Option<Self> {
                from_hex(text).map(Self)
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "{}({})", stringify!($name), self.short())
            }
        }

        impl From<[u8; 32]> for $name {
            fn from(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }
        }

        impl From<$name> for [u8; 32] {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&hex(&self.0))
            }
        }
    )*};
}

ids! {
    /// A fleet: the id of the operation that founded it.
    FleetId;
    /// A host: its certificate authority's fingerprint, or for a plain
    /// host the hash of its first SSH host key.
    HostId;
    /// A member: the SHA-256 of its fleet key, SEC1 compressed.
    MemberId;
    /// An operation: the SHA-256 of its signed bytes, without the
    /// signature, so a re-encoded signature names the same operation.
    OpId;
}

fn from_hex(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 || !text.is_ascii() {
        return None;
    }
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
