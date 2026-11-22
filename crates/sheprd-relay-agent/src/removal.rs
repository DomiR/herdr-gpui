//! What a fleet member's tunnel session carries (`docs/relay-plan.md`,
//! Removals between daemons), shared by Sheprd's daemon
//! (`sheprd_remote_control::control::fleet::removal`) and this agent.
//!
//! A fleet member's session starts with one purpose byte: SSH to splice to
//! the host's SSH server, or a removal exchange. A proof follows it
//! ([`prove`], [`PROOF_LEN`] bytes): the member's fleet key signing the
//! session's handshake hash, so the session is the member's own and not a
//! device whose keys a member's bundle merely names. The dialer then sends
//! [`Message::Removal`]; a daemon answers [`Message::Want`] for parents it
//! lacks, at most [`MAX_ROUNDS`] times, then [`Message::Held`] with the
//! removals it now holds. An agent wants nothing: it answers `Held` at
//! once. Each message is one frame: its length as a big-endian `u32`, then
//! its bytes.
//!
//! The agent checks no signatures, so it reads only what [`peek`] reads
//! from an operation: its id, fleet, signer, and, for a removal, whom it
//! removes. The daemon's tests check that reading against the log's own
//! encoding.

use p256::ecdsa::{
    Signature, SigningKey, VerifyingKey, signature::Signer as _, signature::Verifier as _,
};
use sha2::{Digest, Sha256};
use std::time::Duration;

/// The session carries SSH to the host's SSH server.
pub const PURPOSE_SSH: u8 = 1;
/// The session carries one removal exchange.
pub const PURPOSE_REMOVAL: u8 = 2;
/// A session proof's length: a P-256 signature, `r` then `s`.
pub const PROOF_LEN: usize = 64;
/// What a session proof signs, after [`DOMAIN`] and before the hash.
const SESSION_PROOF: &[u8] = b"session";
/// The largest operation, encoded; the fleet log's own bound.
pub const MAX_OP: usize = 16 * 1024;
/// Operations one message carries.
pub const MAX_OPS: usize = 64;
/// The largest frame: 64 operations and their lengths.
pub const MAX_FRAME: usize = MAX_OPS * (MAX_OP + 2) + 3;
/// `Want` rounds before the listener answers with what it holds.
pub const MAX_ROUNDS: usize = 4;
/// How long one exchange may take, either way.
pub const SESSION: Duration = Duration::from_secs(30);
/// Exchanges at once, each way.
pub const CONCURRENT: usize = 8;

/// Members one `Also` message names beside the removals it goes with.
pub const MAX_ALSO: usize = 256;

const REMOVAL: u8 = 1;
const WANT: u8 = 2;
const HELD: u8 = 3;
const ALSO: u8 = 4;
const ID_LEN: usize = 32;

/// Why a frame or an operation was not read.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("a removal frame is over its bounds")]
    Bounds,
    #[error("a removal frame is not encoded as it must be")]
    Encoding,
    // keep-sorted end
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// One message of a removal exchange, over the operation ids `Id`: the
/// daemon's typed `OpId`, or raw bytes here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Message<Id> {
    /// Removals, and any ancestors asked for, as encoded operations.
    Removal(Vec<Vec<u8>>),
    /// Parents the listener lacks.
    Want(Vec<Id>),
    /// The removals the listener holds now: the dialer owes it no more.
    Held(Vec<Id>),
    /// Sent to a plain host's agent ahead of [`Message::Removal`]: each
    /// pair is a removal and a member the dialer's log removes with it
    /// though the removal does not name it (one a removed member admitted
    /// at the same time). Unsigned: the agent suspends them only in the
    /// dialer's own name, as its own removals' targets, until the next
    /// access file confirms the removal or settles it.
    Also(Vec<(Id, [u8; ID_LEN])>),
}

impl<Id: Copy + Into<[u8; ID_LEN]> + From<[u8; ID_LEN]>> Message<Id> {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let (kind, count, bound) = match self {
            Self::Removal(ops) => (REMOVAL, ops.len(), MAX_OPS),
            Self::Want(ids) => (WANT, ids.len(), MAX_OPS),
            Self::Held(ids) => (HELD, ids.len(), MAX_OPS),
            Self::Also(pairs) => (ALSO, pairs.len(), MAX_ALSO),
        };
        if count > bound {
            return Err(Error::Bounds);
        }
        let mut bytes = vec![kind];
        bytes.extend_from_slice(&length(count)?);
        match self {
            Self::Removal(ops) => {
                for op in ops {
                    if op.len() > MAX_OP {
                        return Err(Error::Bounds);
                    }
                    bytes.extend_from_slice(&length(op.len())?);
                    bytes.extend_from_slice(op);
                }
            }
            Self::Want(ids) | Self::Held(ids) => ids
                .iter()
                .for_each(|id| bytes.extend_from_slice(&(*id).into())),
            Self::Also(pairs) => pairs.iter().for_each(|(id, member)| {
                bytes.extend_from_slice(&(*id).into());
                bytes.extend_from_slice(member);
            }),
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_FRAME {
            return Err(Error::Bounds);
        }
        let (&kind, rest) = bytes.split_first().ok_or(Error::Encoding)?;
        let (count, mut rest) = take_length(rest)?;
        if count > if kind == ALSO { MAX_ALSO } else { MAX_OPS } {
            return Err(Error::Bounds);
        }
        let message = match kind {
            REMOVAL => {
                let mut ops = Vec::with_capacity(count);
                for _ in 0..count {
                    let (length, tail) = take_length(rest)?;
                    if length > MAX_OP {
                        return Err(Error::Bounds);
                    }
                    let (op, tail) = tail.split_at_checked(length).ok_or(Error::Encoding)?;
                    ops.push(op.to_vec());
                    rest = tail;
                }
                Self::Removal(ops)
            }
            ALSO => {
                let mut pairs = Vec::with_capacity(count);
                for _ in 0..count {
                    let (id, tail) = rest.split_at_checked(ID_LEN).ok_or(Error::Encoding)?;
                    let (member, tail) = tail.split_at_checked(ID_LEN).ok_or(Error::Encoding)?;
                    let id: [u8; ID_LEN] = id.try_into().map_err(|_| Error::Encoding)?;
                    let member: [u8; ID_LEN] = member.try_into().map_err(|_| Error::Encoding)?;
                    pairs.push((Id::from(id), member));
                    rest = tail;
                }
                Self::Also(pairs)
            }
            WANT | HELD => {
                let mut ids = Vec::with_capacity(count);
                for _ in 0..count {
                    let (id, tail) = rest.split_at_checked(ID_LEN).ok_or(Error::Encoding)?;
                    let id: [u8; ID_LEN] = id.try_into().map_err(|_| Error::Encoding)?;
                    ids.push(Id::from(id));
                    rest = tail;
                }
                if kind == WANT {
                    Self::Want(ids)
                } else {
                    Self::Held(ids)
                }
            }
            _ => return Err(Error::Encoding),
        };
        if !rest.is_empty() {
            return Err(Error::Encoding);
        }
        Ok(message)
    }

    /// The message as it travels: its length, then its bytes.
    pub fn frame(&self) -> Result<Vec<u8>> {
        let bytes = self.encode()?;
        let length = u32::try_from(bytes.len()).map_err(|_| Error::Bounds)?;
        Ok([&length.to_be_bytes()[..], &bytes].concat())
    }
}

fn proof_message(hash: &[u8; 32]) -> Vec<u8> {
    [DOMAIN, SESSION_PROOF, hash].concat()
}

/// The proof a member sends after the purpose byte: its fleet `key`
/// signing the session's handshake `hash`.
#[must_use]
pub fn prove(key: &SigningKey, hash: &[u8; 32]) -> [u8; PROOF_LEN] {
    let signature: Signature = key.sign(&proof_message(hash));
    signature.to_bytes().into()
}

/// Whether `proof` is the fleet key `key` (SEC1) signing the session's
/// handshake `hash`.
#[must_use]
pub fn verify(key: &[u8], hash: &[u8; 32], proof: &[u8; PROOF_LEN]) -> bool {
    let Ok(key) = VerifyingKey::from_sec1_bytes(key) else {
        return false;
    };
    Signature::from_slice(proof)
        .is_ok_and(|signature| key.verify(&proof_message(hash), &signature).is_ok())
}

/// The length a frame's first four bytes give, refused past [`MAX_FRAME`].
pub fn frame_length(prefix: [u8; 4]) -> Result<usize> {
    let length = usize::try_from(u32::from_be_bytes(prefix)).map_err(|_| Error::Bounds)?;
    if length > MAX_FRAME {
        return Err(Error::Bounds);
    }
    Ok(length)
}

fn length(count: usize) -> Result<[u8; 2]> {
    Ok(u16::try_from(count)
        .map_err(|_| Error::Bounds)?
        .to_be_bytes())
}

fn take_length(bytes: &[u8]) -> Result<(usize, &[u8])> {
    let (length, rest) = bytes.split_at_checked(2).ok_or(Error::Encoding)?;
    Ok((
        usize::from(u16::from_be_bytes([length[0], length[1]])),
        rest,
    ))
}

/// What every fleet operation and session proof starts with.
pub const DOMAIN: &[u8] = b"sheprd-fleet-v1";
const VERSION: u8 = 1;
const SIGNATURE_LEN: usize = 64;
const MAX_PARENTS: usize = 16;
const REMOVE: u8 = 3;
const MAX_ALSO_MEMBERS: usize = 16;
const MAX_ALSO_HOSTS: usize = 256;
const REASONS: std::ops::RangeInclusive<u8> = 1..=4;

/// A removal as an agent reads it, unchecked: nothing here is signed as
/// far as the agent knows, so it only ever suspends what it names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Removed {
    /// The operation's id: the SHA-256 of its bytes before the signature.
    pub id: [u8; ID_LEN],
    pub fleet: [u8; ID_LEN],
    pub signer: [u8; ID_LEN],
    /// The signer's sequence number for it: once a log holds a later one
    /// of the same signer and not this, this is no operation the signer
    /// made.
    pub seq: u64,
    /// The member it removes, then the members removed with it.
    pub targets: Vec<[u8; ID_LEN]>,
}

/// The removal `bytes` encode, `None` for any other operation, or an
/// error for bytes that are no operation.
pub fn peek(bytes: &[u8]) -> Result<Option<Removed>> {
    if bytes.len() > MAX_OP || bytes.len() < SIGNATURE_LEN {
        return Err(Error::Bounds);
    }
    let signed = &bytes[..bytes.len() - SIGNATURE_LEN];
    let mut reader = Reader(signed);
    if reader.take(DOMAIN.len())? != DOMAIN || reader.byte()? != VERSION {
        return Err(Error::Encoding);
    }
    let fleet = reader.id()?;
    let signer = reader.id()?;
    let seq = u64::from_be_bytes(reader.take(8)?.try_into().map_err(|_| Error::Encoding)?);
    reader.take(8)?; // issued_ms
    let parents = usize::from(reader.byte()?);
    if parents > MAX_PARENTS {
        return Err(Error::Bounds);
    }
    reader.take(parents * ID_LEN)?;
    if reader.byte()? != REMOVE {
        return Ok(None);
    }
    let mut targets = vec![reader.id()?];
    let also = reader.count(MAX_ALSO_MEMBERS)?;
    for _ in 0..also {
        targets.push(reader.id()?);
    }
    let hosts = reader.count(MAX_ALSO_HOSTS)?;
    reader.take(hosts * ID_LEN)?;
    if !REASONS.contains(&reader.byte()?) || !reader.0.is_empty() {
        return Err(Error::Encoding);
    }
    Ok(Some(Removed {
        id: Sha256::digest(signed).into(),
        fleet,
        signer,
        seq,
        targets,
    }))
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let (taken, rest) = self.0.split_at_checked(length).ok_or(Error::Encoding)?;
        self.0 = rest;
        Ok(taken)
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn id(&mut self) -> Result<[u8; ID_LEN]> {
        self.take(ID_LEN)?.try_into().map_err(|_| Error::Encoding)
    }

    fn count(&mut self, max: usize) -> Result<usize> {
        let (count, rest) = take_length(self.0)?;
        self.0 = rest;
        if count > max {
            return Err(Error::Bounds);
        }
        Ok(count)
    }
}

#[cfg(test)]
pub(crate) mod tests;
