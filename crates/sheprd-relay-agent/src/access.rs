//! The fleet access file (`docs/relay-plan.md`, Fleet, Spreading a host):
//! what a plain host's agent is told about the fleet. Sheprd computes it
//! from the signed fleet log (`sheprd_remote_control::control::fleet::
//! access`) and hands it to `sheprd-relay-agent fleet apply` over SSH; the
//! agent keeps the merge of every file it was given in `fleet`, in its
//! state directory.
//!
//! ```json
//! {
//!   "fleet": "FLEET_ID",
//!   "members": {
//!     "MEMBER_ID": {"allow": true, "dial": "…", "name": "studio", "rev": 2,
//!                   "ssh": "ssh-ed25519 AAAA…", "tunnel": "…"}
//!   },
//!   "removed": ["MEMBER_ID", …],
//!   "settled": ["REMOVAL_ID", …]
//! }
//! ```
//!
//! Ids and keys are base64url without padding: a member's id, its relay
//! dial key (P-256, SEC1 compressed), and its tunnel key (X25519).
//! `settled` names, of the removals the agent reported suspending for,
//! those the log that computed the file holds; it is not kept.
//!
//! [`Access::merge`] is what stops a member that has not heard of a
//! removal from undoing it over SSH: removed members are a union, kept
//! forever and never admitted again, and per member the higher `rev` wins,
//! a deny winning a tie.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use sha2::{Digest, Sha256};
use sheprd_relay::wire::{KEY_LEN, Key};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

/// The file the merged access lives in, in the state directory.
pub const FILE: &str = "fleet";
/// The largest access file Sheprd hands an agent.
pub const MAX_BYTES: usize = 72 * 1024;
/// What one remembered key pair takes in the stored file, at most.
const MAX_PAIR_BYTES: usize = 128;
/// Members one file names, allowed or not: a host names at most 32 fleet
/// dial keys, keeping room in a relay's 64 for desktops enrolled one by one.
pub const MAX_MEMBERS: usize = 32;
/// Removed members remembered, forever; the fleet log's own bound.
pub const MAX_REMOVED: usize = 1024;
/// Removals one file settles.
pub const MAX_SETTLED: usize = 64;
/// The longest member name.
pub const MAX_NAME: usize = 64;

/// Why an access file is not one the agent takes.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the fleet access file is over its bounds")]
    Bounds,
    #[error("fleet member {} has keys or a name that are not valid", member.short())]
    Member { member: Id },
    #[error(
        "this account belongs to fleet {}; a host account joins one fleet, and fleet {} is another",
        held.short(),
        offered.short()
    )]
    OtherFleet { held: Id, offered: Id },
    // keep-sorted end
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A fleet, member, or removal id: 32 bytes, base64url in the file.
#[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Id(pub [u8; 32]);

impl Id {
    /// The first eight bytes in hex, as `authorized_keys` tags and logs
    /// show it, and as the fleet log's own `short` does.
    #[must_use]
    pub fn short(&self) -> String {
        self.0[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

impl std::fmt::Debug for Id {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Id({})", self.short())
    }
}

impl From<[u8; 32]> for Id {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl From<Id> for [u8; 32] {
    fn from(id: Id) -> Self {
        id.0
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&URL_SAFE_NO_PAD.encode(self.0))
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        bytes(deserializer).map(Self)
    }
}

/// Fixed-size bytes as base64url.
mod b64 {
    use super::*;

    pub(super) fn serialize<S: Serializer, const N: usize>(
        bytes: &[u8; N],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&URL_SAFE_NO_PAD.encode(bytes))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>, const N: usize>(
        deserializer: D,
    ) -> Result<[u8; N], D::Error> {
        bytes(deserializer)
    }
}

fn bytes<'de, D: Deserializer<'de>, const N: usize>(deserializer: D) -> Result<[u8; N], D::Error> {
    let text = String::deserialize(deserializer)?;
    URL_SAFE_NO_PAD
        .decode(text.as_bytes())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| de::Error::custom("not base64url of the right length"))
}

/// A computer member, as the log has it for this host.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Member {
    /// Whether it may reach SSH here. A member that may not still passes
    /// removals.
    pub allow: bool,
    /// What it proves to the relay.
    #[serde(with = "b64")]
    pub dial: Key,
    /// Its fleet key (P-256, SEC1 compressed): what signs the proof that
    /// opens each of its sessions (`removal::prove`).
    #[serde(with = "b64")]
    pub fleet_key: Key,
    /// The person's name for it; never used to decide anything.
    pub name: String,
    /// The `Access` register's `rev`: zero until someone set it.
    pub rev: u64,
    /// Its `ssh-ed25519 BASE64` key; none when its bundle carries no
    /// Ed25519 key. Such a member is still listed, so its deny, its
    /// removals, and the retiring of its older keys still land, but it
    /// never reaches SSH.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<String>,
    /// What its end-to-end handshake must show.
    #[serde(with = "b64")]
    pub tunnel: [u8; 32],
    /// How new its keys are: its own highest sequence number in the log
    /// that computed the file, which only climbs, and which a log pruned to
    /// a checkpoint keeps. A rotation raises it, so keys from a file
    /// computed before a rotation never replace newer ones.
    #[serde(default)]
    pub keys: u64,
    /// The SHA-256 of the member's bundle as the log keeps it: what breaks
    /// a tie on `keys` between two views of a member whose key signed in
    /// two places, the same way in every log, pruned or not.
    #[serde(default)]
    pub tiebreak: Id,
    /// The keys it rotated away from, as the agent saw them replaced, so a
    /// desktop enrolled one by one with an old pair goes with the member.
    /// Kept by the agent alone; never in a file Sheprd computes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub former: Vec<Pair>,
}

/// A device's dial and tunnel keys together: what makes a desktop enrolled
/// one by one the same device as a member.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Pair {
    #[serde(with = "b64")]
    pub dial: Key,
    #[serde(with = "b64")]
    pub tunnel: [u8; 32],
}

/// Former key pairs kept per member.
pub const MAX_FORMER: usize = 8;
/// The largest file the agent keeps: an access file and, beside it, every
/// member's key history, so remembering keys never crowds out the fleet.
pub const MAX_STORED_BYTES: usize = MAX_BYTES + MAX_MEMBERS * MAX_FORMER * MAX_PAIR_BYTES;

impl Member {
    fn valid(&self) -> bool {
        p256::PublicKey::from_sec1_bytes(&self.dial).is_ok()
            && p256::PublicKey::from_sec1_bytes(&self.fleet_key).is_ok()
            && self.dial.len() == KEY_LEN
            && self
                .ssh
                .as_deref()
                .is_none_or(|ssh| crate::authorized_keys::ed25519(ssh).as_deref() == Some(ssh))
            && self.name.len() <= MAX_NAME
            && !self.name.chars().any(char::is_control)
    }

    /// Two views of one member merged: whether it may reach SSH by the
    /// higher `rev`, a deny winning a tie; its keys and name by the higher
    /// `keys`, the held ones kept on a tie, so a file computed before a
    /// rotation cannot bring back the keys it replaced.
    fn merged(self, held: &Self) -> Self {
        let allow = match self.rev.cmp(&held.rev) {
            Ordering::Greater => self.allow,
            Ordering::Less => held.allow,
            Ordering::Equal => self.allow && held.allow,
        };
        let rev = self.rev.max(held.rev);
        let mut former = held.former.clone();
        let mut merged = if (self.keys, self.tiebreak) > (held.keys, held.tiebreak) {
            let replaced = held.pair();
            if replaced != self.pair() && !former.contains(&replaced) {
                former.push(replaced);
            }
            self
        } else {
            held.clone()
        };
        // The oldest go first; a member rotating more often than this
        // between files leaves its oldest pairs behind.
        let excess = former.len().saturating_sub(MAX_FORMER);
        former.drain(..excess);
        merged.former = former;
        merged.allow = allow;
        merged.rev = rev;
        merged
    }

    /// Its keys now.
    #[must_use]
    pub fn pair(&self) -> Pair {
        Pair {
            dial: self.dial,
            tunnel: self.tunnel,
        }
    }

    /// Every pair it has had here: its keys now, and those it rotated
    /// away from.
    pub fn pairs(&self) -> impl Iterator<Item = Pair> + '_ {
        std::iter::once(self.pair()).chain(self.former.iter().copied())
    }
}

/// The fleet `bytes`, a stored or handed access file, names, read on its
/// own: what a refusal needs to find the fleet's lines when the rest of
/// the file does not read.
#[must_use]
pub fn fleet_of(bytes: &[u8]) -> Option<Id> {
    #[derive(Deserialize)]
    struct Named {
        fleet: Id,
    }
    serde_json::from_slice::<Named>(bytes)
        .ok()
        .map(|named| named.fleet)
}

/// What a plain host is told about its fleet.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Access {
    pub fleet: Id,
    /// Live computer members, by id.
    #[serde(default)]
    pub members: BTreeMap<Id, Member>,
    /// No member reaches SSH here while this is set: the fleet holds the
    /// host, as two members added it with keys that disagree, until a
    /// person picks one. Members are still admitted, to pass removals, and
    /// removals and denials still land. Each file says it anew: the latest
    /// one handed over decides.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub paused: bool,
    /// Every member ever removed.
    #[serde(default)]
    pub removed: BTreeSet<Id>,
    /// Removals the computing log holds, of those asked about: a
    /// suspension for one whose target is still a member is lifted.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub settled: BTreeSet<Id>,
}

impl Access {
    /// An access file for `fleet` naming no one.
    #[must_use]
    pub fn new(fleet: Id) -> Self {
        Self {
            fleet,
            members: BTreeMap::new(),
            paused: false,
            removed: BTreeSet::new(),
            settled: BTreeSet::new(),
        }
    }

    /// Reads a file Sheprd handed over, checking its bounds and every
    /// member's keys. It carries no key history: that is the agent's own.
    pub fn parse(bytes: &[u8]) -> crate::Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Bounds.into());
        }
        let access: Self = serde_json::from_slice(bytes).map_err(crate::Error::AccessFormat)?;
        access.check()?;
        if access
            .members
            .values()
            .any(|member| !member.former.is_empty())
        {
            return Err(Error::Bounds.into());
        }
        Ok(access)
    }

    /// Reads the file the agent keeps, whose key history has its own
    /// budget beside [`MAX_BYTES`] ([`MAX_STORED_BYTES`]).
    pub fn parse_stored(bytes: &[u8]) -> crate::Result<Self> {
        if bytes.len() > MAX_STORED_BYTES {
            return Err(Error::Bounds.into());
        }
        let access: Self = serde_json::from_slice(bytes).map_err(crate::Error::AccessFormat)?;
        access.check()?;
        Ok(access)
    }

    /// Checks bounds and every member's keys and name.
    pub fn check(&self) -> Result<()> {
        if self.members.len() > MAX_MEMBERS
            || self.removed.len() > MAX_REMOVED
            || self.settled.len() > MAX_SETTLED
            || self
                .members
                .values()
                .any(|member| member.former.len() > MAX_FORMER)
        {
            return Err(Error::Bounds);
        }
        if let Some((id, _)) = self.members.iter().find(|(_, member)| !member.valid()) {
            return Err(Error::Member { member: *id });
        }
        Ok(())
    }

    /// The file's bytes as Sheprd hands it over: compact JSON, maps in id
    /// order, so one access has one encoding; no key history.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if self
            .members
            .values()
            .any(|member| !member.former.is_empty())
        {
            return Err(Error::Bounds);
        }
        self.encode(MAX_BYTES)
    }

    /// The bytes of the file the agent keeps, key history included.
    pub fn to_stored_bytes(&self) -> Result<Vec<u8>> {
        self.encode(MAX_STORED_BYTES)
    }

    fn encode(&self, limit: usize) -> Result<Vec<u8>> {
        self.check()?;
        let bytes = serde_json::to_vec(self).map_err(|_| Error::Bounds)?;
        if bytes.len() > limit {
            return Err(Error::Bounds);
        }
        Ok(bytes)
    }

    /// The SHA-256 of what is kept, `settled` left out, in hex: equal on
    /// the agent and on a member that computed the same file, so Sheprd
    /// sees when a host is behind.
    pub fn digest(&self) -> Result<String> {
        let mut kept = Self {
            settled: BTreeSet::new(),
            ..self.clone()
        };
        // Former keys are the agent's own memory; Sheprd computes none.
        for member in kept.members.values_mut() {
            member.former.clear();
        }
        let digest = Sha256::digest(kept.to_bytes()?);
        Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    /// The members that may reach SSH here: [`Access::granted`], and none
    /// while [`Access::paused`].
    pub fn allowed(&self) -> impl Iterator<Item = (&Id, &Member)> {
        self.granted().filter(|_| !self.paused)
    }

    /// The members the fleet grants SSH here, a pause aside: allowed, with
    /// an SSH key, not removed, and sharing no key with another
    /// ([`Access::shared`]). What decides who may suspend whom, which a
    /// pause does not change.
    pub fn granted(&self) -> impl Iterator<Item = (&Id, &Member)> {
        let shared = self.shared();
        self.members.iter().filter(move |(id, member)| {
            member.allow
                && member.ssh.is_some()
                && !self.removed.contains(id)
                && !shared.contains(id)
        })
    }

    /// `incoming` merged into what was `held`: the same fleet only,
    /// removed members a union that no member outlives, and per member
    /// [`Member::merged`]. `settled` is not kept.
    pub fn merge(held: Option<&Self>, incoming: Self) -> Result<Self> {
        incoming.check()?;
        let mut merged = match held {
            Some(held) if held.fleet != incoming.fleet => {
                return Err(Error::OtherFleet {
                    held: held.fleet,
                    offered: incoming.fleet,
                });
            }
            Some(held) => {
                let mut merged = held.clone();
                merged.paused = incoming.paused;
                merged.removed.extend(incoming.removed);
                for (id, member) in incoming.members {
                    let member = match merged.members.get(&id) {
                        Some(kept) => member.merged(kept),
                        None => member,
                    };
                    merged.members.insert(id, member);
                }
                merged
            }
            None => incoming,
        };
        merged.settled.clear();
        let removed = &merged.removed;
        merged.members.retain(|id, _| !removed.contains(id));
        merged.check()?;
        Ok(merged)
    }

    /// The members sharing a dial or tunnel key with another, suspended
    /// ones included. Keys are public: two members naming one would make
    /// one's sessions look like the other's, so none of them is let in
    /// while they share it. Worked out every time rather than kept, so a
    /// later file that ends the clash lets them in again; a file with a
    /// clash is still taken, so its removals and denies land.
    #[must_use]
    pub fn shared(&self) -> BTreeSet<Id> {
        let mut dials: BTreeMap<Key, usize> = BTreeMap::new();
        let mut tunnels: BTreeMap<[u8; 32], usize> = BTreeMap::new();
        for member in self.members.values() {
            *dials.entry(member.dial).or_default() += 1;
            *tunnels.entry(member.tunnel).or_default() += 1;
        }
        self.members
            .iter()
            .filter(|(_, member)| dials[&member.dial] > 1 || tunnels[&member.tunnel] > 1)
            .map(|(id, _)| *id)
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests;
