//! Who the agent admits: the desktops in `authorized` and the fleet's
//! members, less those suspended, and less every desktop with both keys
//! of a suspended or removed member. A session is matched by its dial and
//! tunnel keys together, the same way everywhere: a suspended member's
//! pair never falls through to a desktop, and a desktop that shares only
//! one key with it is still that desktop.

use super::Held;
use crate::{
    Result,
    access::{Access, Id, Member, Pair},
    authorized::Authorized,
};
use sheprd_relay::wire::{Key, MAX_DIAL_KEYS};
use std::path::Path;

/// Who dialed in, as far as admitting them goes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Peer {
    /// A desktop enrolled one by one in `authorized`: its session is SSH,
    /// with no purpose byte.
    Desktop,
    /// A fleet member: its session starts with a purpose byte. `desktop`
    /// when it is also enrolled one by one, whose sessions send none.
    Member { id: Id, ssh: bool, desktop: bool },
}

/// Everyone the agent admits.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Admitted {
    desktops: Authorized,
    held: Held,
}

impl Admitted {
    #[must_use]
    pub fn new(desktops: Authorized, held: Held) -> Self {
        Self { desktops, held }
    }

    /// Reads `authorized` and the fleet's files as one snapshot, under the
    /// state directory's lock every update takes: a read never mixes the
    /// files from before an update with those from after it, which could
    /// rank members by a wait the access file read does not have yet.
    /// Never call it while holding that lock. Blocking.
    pub fn load(state_dir: &Path) -> Result<Self> {
        let _lock = crate::files::Lock::acquire(state_dir)?;
        Ok(Self::new(
            Authorized::load(state_dir)?,
            Held::load(state_dir)?,
        ))
    }

    /// Shuts the fleet out, keeping what was last read of who is cut off:
    /// what the running agent holds when the fleet's files stop reading.
    pub fn refuse(&mut self) {
        self.held.refused = true;
    }

    #[must_use]
    pub fn desktops(&self) -> &Authorized {
        &self.desktops
    }

    #[must_use]
    pub fn held(&self) -> &Held {
        &self.held
    }

    /// Members admitted, with their ids: not suspended, and sharing no key
    /// with another member, suspended or not ([`Access::shared`]).
    fn members(&self) -> impl Iterator<Item = (&Id, &Member)> {
        let shared = self
            .held
            .access
            .as_ref()
            .map(Access::shared)
            .unwrap_or_default();
        self.held
            .access
            .iter()
            .flat_map(|access| &access.members)
            .filter(move |(id, _)| {
                !self.held.refused && !self.held.suspended.contains_key(id) && !shared.contains(id)
            })
    }

    /// The key pairs, now and before a rotation, of suspended members:
    /// those each suspension keeps, which outlive a refused or unreadable
    /// access file, and those the file held names.
    fn suspended_pairs(&self) -> impl Iterator<Item = Pair> + '_ {
        let access = self.held.access.as_ref();
        self.held
            .suspended
            .iter()
            .flat_map(move |(id, suspension)| {
                suspension.pairs.iter().copied().chain(
                    access
                        .and_then(|access| access.members.get(id))
                        .into_iter()
                        .flat_map(Member::pairs),
                )
            })
    }

    /// Both keys of a suspended member, or of one removed for good (until
    /// a desktop with them is enrolled again): no one's.
    fn cut_off(&self, dial: &Key, tunnel: &[u8; 32]) -> bool {
        let pair = Pair {
            dial: *dial,
            tunnel: *tunnel,
        };
        self.held.revoked.contains(&pair) || self.suspended_pairs().any(|named| named == pair)
    }

    /// The keys the relays let dial, at most a relay's [`MAX_DIAL_KEYS`]:
    /// enrolled desktops first, then members allowed here, then members
    /// that only pass removals. `fleet apply` keeps them within the limit;
    /// past it, a member denied for lack of room is the first left out.
    #[must_use]
    pub fn dial_keys(&self) -> Vec<Key> {
        let desktops = self
            .desktops
            .entries()
            .iter()
            .filter(|entry| !self.cut_off(&entry.dial, &entry.tunnel))
            .map(|entry| entry.dial);
        // Ranked by what the fleet grants, a pause aside, so holding a
        // host never reshuffles who keeps a slot.
        let allowed = self
            .members()
            .filter(|(id, _)| self.held.grants(id))
            .map(|(_, member)| member.dial);
        let denied = self
            .members()
            .filter(|(id, _)| !self.held.grants(id) && !self.held.crowded.contains(id))
            .map(|(_, member)| member.dial);
        // Members whose new keys wait for room come last, so one never
        // takes another member's place.
        let crowded = self
            .members()
            .filter(|(id, _)| self.held.crowded.contains(id))
            .map(|(_, member)| member.dial);
        let mut keys: Vec<Key> = Vec::new();
        for key in desktops.chain(allowed).chain(denied).chain(crowded) {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys.truncate(MAX_DIAL_KEYS);
        keys
    }

    fn desktop(&self, dial: &Key, tunnel: &[u8; 32]) -> bool {
        self.desktops
            .entries()
            .iter()
            .any(|entry| entry.dial == *dial && entry.tunnel == *tunnel)
    }

    /// Who a session dialed with `dial` and showing `tunnel` in its
    /// handshake is, if admitted. A suspended or removed member's pair is
    /// no one's; a pair two members name is still an enrolled desktop's.
    #[must_use]
    pub fn peer(&self, dial: &Key, tunnel: &[u8; 32]) -> Option<Peer> {
        if self.cut_off(dial, tunnel) {
            return None;
        }
        let desktop = self.desktop(dial, tunnel);
        let member = self
            .members()
            .find(|(_, member)| member.dial == *dial && member.tunnel == *tunnel);
        match member {
            Some((id, _)) => Some(Peer::Member {
                id: *id,
                ssh: self.held.reaches_ssh(id),
                desktop,
            }),
            None => desktop.then_some(Peer::Desktop),
        }
    }

    /// Whether `peer`'s SSH session may go on: what a live session checks
    /// after every change.
    #[must_use]
    pub fn keeps_ssh(&self, peer: &Peer, dial: &Key, tunnel: &[u8; 32]) -> bool {
        if self.cut_off(dial, tunnel) {
            return false;
        }
        match peer {
            Peer::Desktop => self.desktop(dial, tunnel),
            Peer::Member { id, .. } => self.members().any(|(member, entry)| {
                member == id
                    && self.held.reaches_ssh(member)
                    && entry.dial == *dial
                    && entry.tunnel == *tunnel
            }),
        }
    }

    /// Whether `proof` is admitted member `member`'s fleet key signing the
    /// session's handshake `hash`.
    #[must_use]
    pub fn proves(
        &self,
        member: &Id,
        hash: &[u8; 32],
        proof: &[u8; crate::removal::PROOF_LEN],
    ) -> bool {
        self.members()
            .find(|(id, _)| *id == member)
            .is_some_and(|(_, entry)| crate::removal::verify(&entry.fleet_key, hash, proof))
    }

    /// Whether `member` may still pass removals.
    #[must_use]
    pub fn admits_member(&self, member: &Id) -> bool {
        self.members().any(|(id, _)| id == member)
    }
}
