//! Removals passed between daemons, and to agents, inside the relay
//! tunnel (`docs/relay-plan.md`, Removals between daemons), so a removal
//! reaches every member while every Sheprd window is shut.
//!
//! The wire is `sheprd_relay_agent::removal`'s, which plain hosts' agents
//! speak too: a fleet member's tunnel session starts with one purpose
//! byte, SSH or a removal exchange. The dialer then sends
//! [`Message::Removal`]; the listener answers [`Message::Want`] for
//! parents it lacks, at most [`MAX_ROUNDS`] times, then
//! [`Message::Held`] with the removals it now holds. Each message is one
//! frame: its length as a big-endian `u32`, then its bytes.

use super::{MemberId, OpId, State};

pub use sheprd_relay_agent::removal::{
    CONCURRENT, MAX_ALSO, MAX_FRAME, MAX_OPS, MAX_ROUNDS, PROOF_LEN, PURPOSE_REMOVAL, PURPOSE_SSH,
    SESSION, prove,
};

/// One message of a removal exchange.
pub type Message = sheprd_relay_agent::removal::Message<OpId>;

// The agent reads operations with the log's own bound.
const _: () = assert!(sheprd_relay_agent::removal::MAX_OP == super::MAX_OP);

/// Whether `proof` is `member`'s fleet key, live in `state`, signing the
/// session's handshake `hash`.
#[must_use]
pub fn proves(state: &State, member: &MemberId, hash: &[u8; 32], proof: &[u8; PROOF_LEN]) -> bool {
    state
        .members
        .get(member)
        .is_some_and(|member| sheprd_relay_agent::removal::verify(&member.bundle.key, hash, proof))
}
