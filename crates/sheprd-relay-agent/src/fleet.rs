//! The fleet as this agent holds it (`docs/relay-plan.md`, Fleet): the
//! merge of every access file Sheprd gave it (`access`, kept in `fleet`),
//! and the members a removal passed by another member suspended (kept in
//! `suspended`), both private to this user in the state directory.
//!
//! The agent checks no signatures, so a removal passed inside the tunnel
//! only suspends its targets: their tunnel and dial keys stop being
//! admitted and their `authorized_keys` lines go at once. The next access
//! file, computed over SSH from the signed log, settles each suspension:
//! a removed target becomes a tombstone for good, and a target still a
//! member is readmitted once every removal it was suspended for is one
//! that log holds, which means the log voided it. A suspension the agent
//! cannot settle stays: a member can only ever take access away here this
//! way, never give it. A member suspends only in its own name (the
//! removal's signer must be the member that passed it), and a member
//! denied SSH here suspends only members denied too.
//!
//! A desktop enrolled one by one in `authorized` with both keys of a
//! member (now or before a rotation) fails closed with it: it is cut off
//! while the member is suspended, and after its removal. It is never
//! forgotten: a member may name any desktop's public keys, and nothing a
//! session proves shows the desktop is that member. It stays enrolled,
//! cut off and listed in `status --json` (`revoked`), so a person can
//! enroll it again (`authorize`) when a member named its keys falsely.

mod admitted;

pub use admitted::{Admitted, Peer};

use crate::{
    Error, Result,
    access::{self, Access, Id, Member, Pair},
    authorized::Authorized,
    authorized_keys, files,
    removal::Removed,
};
use serde::{Deserialize, Serialize};
use sheprd_relay::wire::{Key, MAX_DIAL_KEYS};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// The members suspended, in the state directory.
pub const SUSPENDED: &str = "suspended";
/// The key pairs of members removed for good that an enrolled desktop
/// still has, in the state directory.
pub const REVOKED: &str = "revoked";
/// The members whose new keys wait for room on the relays, in the state
/// directory.
pub const CROWDED: &str = "crowded";
/// Present while the last access file handed over could not be read or
/// checked: the fleet reaches nothing here until a good one arrives.
pub const REFUSED: &str = "refused";
/// The fleet whose `authorized_keys` lines the agent may have written,
/// in the state directory: kept before any line is, so a scrub or a
/// refusal finds them even when no access file was saved after.
pub const LINES: &str = "lines";
/// Removals remembered per suspended member; past that, its suspension is
/// one no access file lifts.
pub const MAX_CAUSES: usize = 8;
/// Key pairs kept per suspended member: its keys now and before as one
/// file names them, twice over for rotations between files. Past it the
/// pairs enrolled desktops have go to `revoked` ([`Suspension::keep_pairs`]),
/// so every file stays within what is read back.
pub const MAX_SUSPENDED_PAIRS: usize = 2 * (access::MAX_FORMER + 1);
/// The most read of each of the fleet's small files: suspensions carry
/// each member's key pairs as well as its removals; twice what the largest
/// state the bounds allow takes (`fleet/tests/stored_size.rs`).
const MAX_SUSPENDED_BYTES: u64 = 256 * 1024;

/// Who signed a removal that suspended a member, and with what sequence
/// number: what Sheprd's daemon holds to its log to tell a removal that is
/// on its way from one no member made.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Cause {
    pub signer: Id,
    pub seq: u64,
}

/// Why a member is suspended.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Suspension {
    /// The removals that named it, by id.
    pub removals: BTreeMap<Id, Cause>,
    /// More removals named it than are remembered: only its removal
    /// settles it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub overflow: bool,
    /// The member's key pairs, now and before its rotations, kept with
    /// the suspension: a desktop with one stays cut off whatever becomes
    /// of the access file, refused or unreadable included.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub pairs: BTreeSet<Pair>,
}

impl Suspension {
    /// Adds `pairs`, the member's keys as a file names them, to those
    /// kept: never replaced, so a pair a rotation left behind stays cut
    /// off. Past [`MAX_SUSPENDED_PAIRS`] the suspension overflows, so only
    /// the member's removal ends it, and the pairs that did not fit are
    /// returned: [`cut_off_desktops`] keeps those an enrolled desktop has,
    /// the ones a session could come back with. The member's keys the
    /// access file names stay cut off besides (`Admitted::cut_off`). Only
    /// that member pays for its rotations.
    fn keep_pairs(&mut self, pairs: impl IntoIterator<Item = Pair>) -> Vec<Pair> {
        let mut left = Vec::new();
        for pair in pairs {
            if self.pairs.contains(&pair) {
                continue;
            }
            if self.pairs.len() < MAX_SUSPENDED_PAIRS {
                self.pairs.insert(pair);
            } else {
                self.overflow = true;
                left.push(pair);
            }
        }
        left
    }
}

/// Cuts off, in `revoked`, the desktops among `desktops` that have one of
/// `pairs`, a suspended member's keys past what its suspension keeps. The
/// set holds only enrolled desktops' pairs, at most one each, so it always
/// fits; a person enrolling the desktop again lets it back in, as for a
/// member removed for good.
fn cut_off_desktops(
    revoked: &mut BTreeSet<Pair>,
    desktops: &Authorized,
    pairs: impl IntoIterator<Item = Pair>,
) {
    let enrolled = |pair: &Pair| {
        desktops
            .entries()
            .iter()
            .any(|entry| entry.dial == pair.dial && entry.tunnel == pair.tunnel)
    };
    revoked.retain(|pair| enrolled(pair));
    revoked.extend(pairs.into_iter().filter(|pair| enrolled(pair)));
}

/// The access this agent holds, who is suspended, and the key pairs of
/// members removed for good.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Held {
    pub access: Option<Access>,
    pub suspended: BTreeMap<Id, Suspension>,
    /// Key pairs of members removed for good, and of suspended members
    /// past what their suspension keeps, that an enrolled desktop has: a
    /// desktop with one stays cut off until enrolled again.
    pub revoked: BTreeSet<Pair>,
    /// Members whose rotated keys do not fit in the relay's dial keys yet:
    /// kept apart from the fleet's own allow and deny, which the access
    /// file holds as the log decided, so the block lifts as soon as there
    /// is room, with the same file applied again.
    pub crowded: BTreeSet<Id>,
    /// The last file handed over could not be read or checked: no member
    /// is admitted or reaches SSH, rather than going on with the file held
    /// before, which may grant what the newer one took away.
    pub refused: bool,
}

impl Held {
    /// What `state_dir` holds; nothing for an agent no fleet reached.
    /// Blocking.
    pub fn load(state_dir: &Path) -> Result<Self> {
        let refused = files::read_bounded(&state_dir.join(REFUSED), 64)?.is_some();
        let path = state_dir.join(access::FILE);
        let access = match files::read_bounded(&path, access::MAX_STORED_BYTES as u64)?
            .map(|bytes| Access::parse_stored(&bytes))
            .transpose()
        {
            Ok(access) => access,
            // Refused, the held file no longer counts: one that does not
            // read is set aside, and the next good file starts afresh.
            Err(_) if refused => None,
            Err(error) => return Err(error),
        };
        Ok(Self {
            access,
            suspended: read_json(state_dir, SUSPENDED)?,
            revoked: read_json(state_dir, REVOKED)?,
            crowded: read_json(state_dir, CROWDED)?,
            refused,
        })
    }

    /// `fleet apply` on the bytes it was handed: [`Held::apply`] on the
    /// file they hold. Bytes that are no access file take the fleet's
    /// access away ([`Held::refuse`]) rather than leave the file held
    /// before in force, and the error is returned. Blocking.
    pub fn apply_bytes(state_dir: &Path, keys: Option<&Path>, bytes: &[u8]) -> Result<Self> {
        match Access::parse(bytes) {
            Ok(incoming) => Self::apply(state_dir, keys, incoming),
            Err(error) => {
                Self::refuse(state_dir, keys)?;
                Err(error)
            }
        }
    }

    /// `fleet apply` was handed a file it could not read or check: the
    /// fleet stops reaching this account, its `authorized_keys` lines go,
    /// until a good file arrives. The mark is kept before the lines go, so
    /// the running agent stops admitting members first. Blocking.
    pub fn refuse(state_dir: &Path, keys: Option<&Path>) -> Result<()> {
        let _lock = files::Lock::acquire(state_dir)?;
        Self::refuse_locked(state_dir, keys)
    }

    /// Needs nothing else here to read: the fleet's lines are found from
    /// the held file's fleet id alone, or, when even that does not read,
    /// every fleet line goes. A running agent that cannot read the
    /// fleet's files after this shuts the fleet out (`Agent::reload`).
    fn refuse_locked(state_dir: &Path, keys: Option<&Path>) -> Result<()> {
        files::write_private(&state_dir.join(REFUSED), b"1\n")?;
        remove_lines(state_dir, keys)
    }

    /// Writes each file whole, in the order [`Held::files`] gives, from
    /// what was held with `before` waiting for room.
    fn save(&self, state_dir: &Path, before: &BTreeSet<Id>) -> Result<()> {
        for (name, bytes) in self.files(before)? {
            files::write_private(&state_dir.join(name), &bytes)?;
        }
        Ok(())
    }

    /// The files kept, by name, in the order that fails closed when saving
    /// stops part way, from what was held with `before` waiting for room:
    /// the revoked pairs before the access file that drops the members
    /// they come from (a retry finds them there again); the members
    /// waiting for room, those before and those now, before the file that
    /// gives the new ones their keys, and only those now after the file
    /// that frees the room; and the suspensions after it, so one is lifted
    /// only once the file that lifts it, or removes its member, is kept.
    fn files(&self, before: &BTreeSet<Id>) -> Result<Vec<(&'static str, Vec<u8>)>> {
        let waiting: BTreeSet<Id> = before.union(&self.crowded).copied().collect();
        let mut files = vec![
            (REVOKED, json(REVOKED, &self.revoked)?),
            (CROWDED, json(CROWDED, &waiting)?),
        ];
        if let Some(access) = &self.access {
            files.push((access::FILE, access.to_stored_bytes()?));
        }
        if waiting != self.crowded {
            files.push((CROWDED, json(CROWDED, &self.crowded)?));
        }
        files.push((SUSPENDED, json(SUSPENDED, &self.suspended)?));
        Ok(files)
    }

    /// `fleet apply`: merges `incoming` into what `state_dir` holds,
    /// settles suspensions, cuts off the desktops with the keys of members
    /// removed for good, and writes the fleet's lines in `keys` (when there is
    /// one). Refuses a file for another fleet. Blocking.
    pub fn apply(state_dir: &Path, keys: Option<&Path>, incoming: Access) -> Result<Self> {
        let _lock = files::Lock::acquire(state_dir)?;
        // Files that do not read shut the fleet out rather than leave the
        // running agent on what it read before.
        let mut held = match Self::load(state_dir) {
            Ok(held) => held,
            Err(error) => {
                Self::refuse_locked(state_dir, keys)?;
                return Err(error);
            }
        };
        let before = held.crowded.clone();
        let settled = incoming.settled.clone();
        let mut merged = match Access::merge(held.access.as_ref(), incoming) {
            Ok(merged) => merged,
            // Another fleet's file says nothing of this one's: what is
            // held stands.
            Err(error @ access::Error::OtherFleet { .. }) => return Err(error.into()),
            // A file that does not check out takes the fleet's access
            // away rather than leave the one held before in force.
            Err(error) => {
                Self::refuse_locked(state_dir, keys)?;
                return Err(error.into());
            }
        };
        // A desktop with both keys of a member removed for good stays
        // enrolled, cut off: never forgotten on a member's word.
        let removed = |id: &Id| merged.removed.contains(id);
        // From the file held and from the suspensions, which keep their
        // pairs when that file was set aside.
        let pairs: Vec<Pair> = held
            .access
            .iter()
            .flat_map(|access| &access.members)
            .filter(|(id, _)| removed(id))
            .flat_map(|(_, member)| member.pairs())
            .chain(
                held.suspended
                    .iter()
                    .filter(|(id, _)| removed(id))
                    .flat_map(|(_, suspension)| suspension.pairs.iter().copied()),
            )
            .collect();
        let desktops = Authorized::load(state_dir)?;
        let enrolled = |pair: &Pair| {
            desktops
                .entries()
                .iter()
                .any(|entry| entry.dial == pair.dial && entry.tunnel == pair.tunnel)
        };
        // A pair no desktop has any more blocks nothing: forgotten, so the
        // set holds at most one pair per enrolled desktop and a required
        // block always fits.
        held.revoked.retain(|pair| enrolled(pair));
        for pair in pairs.into_iter().filter(|pair| enrolled(pair)) {
            if !held.revoked.contains(&pair) && held.revoked.len() >= crate::authorized::MAX_ENTRIES
            {
                return Err(Error::Revoked);
            }
            held.revoked.insert(pair);
        }
        // Taking access away always applies; only what adds a dial key
        // waits for room, counted after the removals.
        let refused = hold_back_additions(
            held.access.as_ref(),
            &mut merged,
            &desktops,
            &mut held.crowded,
        );
        // Each settled removal is forgotten as it is settled, so files
        // that each settle some of a suspension's removals, as the bound
        // on `settled` may make them, lift it between them.
        held.suspended.retain(|member, suspension| {
            if merged.removed.contains(member) {
                return false;
            }
            suspension.removals.retain(|id, _| !settled.contains(id));
            suspension.overflow || !suspension.removals.is_empty()
        });
        // Each suspended member's keys as the merged file has them, added
        // to those kept: a rotation while suspended cuts the new pair off
        // and leaves the old one cut off. Past the bound the member stays
        // suspended until removed and the pairs desktops have stay cut
        // off; the rest of the file applies.
        for (member, suspension) in &mut held.suspended {
            if let Some(entry) = merged.members.get(member) {
                let left = suspension.keep_pairs(entry.pairs());
                cut_off_desktops(&mut held.revoked, &desktops, left);
            }
        }
        held.access = Some(merged);
        held.refused = false;
        // The SSH lines first: a write that fails leaves the file it was
        // computed from unsaved, so the digest still shows the host behind
        // and Sheprd applies it again.
        held.write_keys(state_dir, keys)?;
        held.save(state_dir, &before)?;
        // Last: until the good file is kept, members stay out.
        files::remove(&state_dir.join(REFUSED))?;
        match refused {
            Some(error) => Err(error),
            None => Ok(held),
        }
    }

    /// `fleet scrub`: forgets the fleet, its `authorized_keys` lines in
    /// `keys`, its access file, and its suspensions, so no member of it
    /// reaches this account through the agent. Desktops in `authorized`
    /// stay. Blocking.
    pub fn scrub(state_dir: &Path, keys: Option<&Path>) -> Result<()> {
        let _lock = files::Lock::acquire(state_dir)?;
        // The lines first, needing nothing else to read; their record goes
        // last, once nothing is left that it names.
        remove_lines(state_dir, keys)?;
        for name in [access::FILE, SUSPENDED, REVOKED, CROWDED, REFUSED, LINES] {
            let path = state_dir.join(name);
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => return Err(Error::Io { path, source }),
            }
        }
        Ok(())
    }

    /// A removal exchange from member `sender`: suspends every target of
    /// `removals` in this fleet that `sender` signed and that is a member
    /// here, never `sender` itself, and, when `sender` is denied SSH here,
    /// only members denied too. Returns the ids of the removals taken:
    /// every one for this fleet, acted on or not, so the sender owes them
    /// no more. Blocking.
    pub fn suspend(
        state_dir: &Path,
        keys: Option<&Path>,
        sender: Id,
        removals: &[Removed],
    ) -> Result<Vec<[u8; 32]>> {
        let _lock = files::Lock::acquire(state_dir)?;
        let mut held = Self::load(state_dir)?;
        let desktops = Authorized::load(state_dir)?;
        let Some(access) = held.access.as_ref().filter(|_| !held.refused) else {
            return Ok(Vec::new());
        };
        // The sender may have been suspended since it was let in.
        if !access.members.contains_key(&sender) || held.suspended.contains_key(&sender) {
            return Ok(Vec::new());
        }
        // By what the fleet grants, a pause aside: a held host's members
        // reach no SSH, yet a denied one still suspends only the denied.
        let sender_allowed = held.grants(&sender);
        let reaching: BTreeSet<Id> = access
            .members
            .keys()
            .filter(|id| held.grants(id))
            .copied()
            .collect();
        let mut taken = Vec::new();
        let mut changed = false;
        for removal in removals
            .iter()
            .filter(|removal| removal.fleet == access.fleet.0)
        {
            taken.push(removal.id);
            // Only in its own name: the agent checks no signature, so a
            // removal another member signed is the log's to bring here.
            if removal.signer != sender.0 {
                continue;
            }
            let targets = removal
                .targets
                .iter()
                .map(|target| Id(*target))
                .filter(|target| *target != sender)
                .filter(|target| {
                    access.members.contains_key(target)
                        && (sender_allowed || !reaching.contains(target))
                });
            let cause = Cause {
                signer: sender,
                seq: removal.seq,
            };
            for target in targets {
                let suspension = held.suspended.entry(target).or_default();
                if let Some(member) = access.members.get(&target) {
                    let left = suspension.keep_pairs(member.pairs());
                    cut_off_desktops(&mut held.revoked, &desktops, left);
                }
                let id = Id(removal.id);
                if suspension.removals.contains_key(&id) {
                    continue;
                }
                if suspension.removals.len() < MAX_CAUSES {
                    suspension.removals.insert(id, cause);
                } else {
                    suspension.overflow = true;
                }
                changed = true;
            }
        }
        if changed {
            held.save(state_dir, &held.crowded)?;
        }
        // Even when nothing changed: a suspension saved by an earlier try
        // whose `authorized_keys` write failed is finished before the
        // removal is acknowledged. Rewriting is a no-op when it is current.
        if !taken.is_empty() {
            held.write_keys(state_dir, keys)?;
        }
        Ok(taken)
    }

    /// `authorize` enrolled the desktop with `dial` and `tunnel` again: a
    /// person vouches for it, so a removed member's having had its keys no
    /// longer cuts it off. A suspended member's still does. Blocking.
    pub fn release(state_dir: &Path, dial: Key, tunnel: [u8; 32]) -> Result<()> {
        let _lock = files::Lock::acquire(state_dir)?;
        let mut held = Self::load(state_dir)?;
        if held.revoked.remove(&Pair { dial, tunnel }) {
            held.save(state_dir, &held.crowded)?;
        }
        Ok(())
    }

    /// Whether `member` is admitted at all: a member here, not suspended.
    #[must_use]
    pub fn admits(&self, member: &Id) -> bool {
        !self.refused
            && self
                .access
                .as_ref()
                .is_some_and(|access| access.members.contains_key(member))
            && !self.suspended.contains_key(member)
    }

    /// Whether member `id` may reach SSH here now: [`Held::grants`], the
    /// file not paused, and no file refused. Suspension is apart. What
    /// `Admitted` and `report` ask.
    #[must_use]
    pub fn reaches_ssh(&self, id: &Id) -> bool {
        !self.refused
            && self.grants(id)
            && self.access.as_ref().is_some_and(|access| !access.paused)
    }

    /// Whether the fleet grants member `id` SSH here, a pause aside:
    /// [`Access::granted`], and not waiting for room for its keys. What
    /// `suspend` asks, so a pause never widens who may suspend whom.
    #[must_use]
    pub fn grants(&self, id: &Id) -> bool {
        !self.crowded.contains(id)
            && self
                .access
                .as_ref()
                .is_some_and(|access| access.granted().any(|(member, _)| member == id))
    }

    /// The members that may reach SSH here now: [`Held::reaches_ssh`],
    /// and not suspended.
    pub fn ssh_members(&self) -> impl Iterator<Item = (&Id, &Member)> {
        self.access
            .iter()
            .flat_map(|access| &access.members)
            .filter(|(id, _)| self.reaches_ssh(id) && !self.suspended.contains_key(id))
    }

    fn write_keys(&self, state_dir: &Path, keys: Option<&Path>) -> Result<()> {
        let (Some(path), Some(access)) = (keys, &self.access) else {
            return Ok(());
        };
        let lines: Vec<(String, String)> = self
            .ssh_members()
            .filter_map(|(id, member)| Some((id.short(), member.ssh.clone()?)))
            .collect();
        let fleet = access.fleet.short();
        // Which fleet's lines these are, kept before any is written.
        if !lines.is_empty() && written_by(state_dir)?.as_deref() != Some(fleet.as_str()) {
            files::write_private(&state_dir.join(LINES), fleet.as_bytes())?;
        }
        authorized_keys::set_fleet(path, &fleet, &lines)
    }

    /// What `status` reports of the fleet, with `desktops`' entries a
    /// removed member's keys cut off; none before the first file.
    pub fn report(&self, desktops: &Authorized) -> Result<Option<Report>> {
        let Some(access) = &self.access else {
            return Ok(None);
        };
        let allowed = access
            .members
            .keys()
            .filter(|id| self.reaches_ssh(id))
            .count();
        let revoked = desktops
            .entries()
            .iter()
            .filter(|entry| {
                let pair = Pair {
                    dial: entry.dial,
                    tunnel: entry.tunnel,
                };
                self.revoked.contains(&pair)
            })
            .map(|entry| Revoked {
                label: entry.label.clone(),
                tunnel: Id(entry.tunnel),
            })
            .collect();
        Ok(Some(Report {
            allowed,
            denied: access.members.len() - allowed,
            digest: access.digest()?,
            fleet: access.fleet,
            removed: access.removed.len(),
            revoked,
            crowded: self.crowded.clone(),
            refused: self.refused,
            // The pairs each suspension keeps are the agent's own memory:
            // Sheprd needs only the removals to settle, and a report must
            // stay small enough for it to read.
            suspended: self
                .suspended
                .iter()
                .map(|(id, suspension)| {
                    let reported = Suspension {
                        pairs: BTreeSet::new(),
                        ..suspension.clone()
                    };
                    (*id, reported)
                })
                .collect(),
        }))
    }
}

/// When `merged` would need more dial keys than a relay takes, holds back
/// what does not fit, slot by slot: enrolled desktops and members whose
/// key is unchanged and not waiting keep theirs; then, in order, members
/// that rotated while working, then members already waiting (`crowded`),
/// take what room is left, and members new here come in only if all of
/// them fit. A member held back that the agent knew waits in `crowded`,
/// its old pair gone as a rotation always removes it, until there is
/// room; those new here are left out of the file until a later one, and
/// the error says so. Removals and denials always apply. A rotation that needs no
/// extra slot, its old one freed, is never held back for another's wait.
fn hold_back_additions(
    held: Option<&Access>,
    merged: &mut Access,
    desktops: &Authorized,
    crowded: &mut BTreeSet<Id>,
) -> Option<Error> {
    crowded.retain(|id| merged.members.contains_key(id));
    let Err(error) = fits(desktops, Some(merged)) else {
        crowded.clear();
        return None;
    };
    let before = |id: &Id| held.and_then(|held| held.members.get(id));
    let waiting = |id: &Id, member: &Member| {
        crowded.contains(id) || before(id).is_none_or(|old| old.dial != member.dial)
    };
    // The slots held already: desktops, and members neither rotating
    // nor waiting.
    let mut keys: BTreeSet<Key> = desktops.entries().iter().map(|entry| entry.dial).collect();
    keys.extend(
        merged
            .members
            .iter()
            .filter(|(id, member)| !waiting(id, member))
            .map(|(_, member)| member.dial),
    );
    // Then those that need one, working members first.
    let mut wanting: Vec<(u8, Id)> = merged
        .members
        .iter()
        .filter(|(id, member)| waiting(id, member))
        .map(|(id, _)| {
            let rank = match (before(id).is_some(), crowded.contains(id)) {
                (true, false) => 0,
                (true, true) => 1,
                (false, _) => 2,
            };
            (rank, *id)
        })
        .collect();
    wanting.sort_unstable();
    let mut still = BTreeSet::new();
    let mut new = Vec::new();
    for (rank, id) in wanting {
        let Some(dial) = merged.members.get(&id).map(|member| member.dial) else {
            continue;
        };
        if rank == 2 {
            new.push((id, dial));
        } else if keys.contains(&dial) || keys.len() < MAX_DIAL_KEYS {
            keys.insert(dial);
        } else {
            still.insert(id);
        }
    }
    // Members new here come in together or wait for a later file
    // together, so what a file grants never depends on id order.
    let new_keys: BTreeSet<Key> = new.iter().map(|(_, dial)| *dial).collect();
    let left_out = keys.union(&new_keys).count() > MAX_DIAL_KEYS;
    if left_out {
        for (id, _) in &new {
            merged.members.remove(id);
        }
    }
    let refused = left_out || !still.is_empty();
    *crowded = still;
    refused.then_some(error)
}

/// Whether every enrolled desktop and every member of `access` fits in
/// the dial keys a relay takes for one route ([`MAX_DIAL_KEYS`]). An
/// update that would crowd one out is refused rather than leave someone
/// enrolled who can no longer dial.
pub fn fits(desktops: &Authorized, access: Option<&Access>) -> Result<()> {
    let mut keys: BTreeSet<Key> = desktops.entries().iter().map(|entry| entry.dial).collect();
    keys.extend(
        access
            .iter()
            .flat_map(|access| access.members.values())
            .map(|member| member.dial),
    );
    if keys.len() > MAX_DIAL_KEYS {
        return Err(Error::DialKeys { needed: keys.len() });
    }
    Ok(())
}

/// The fleet in `status --json` and `fleet apply`'s answer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Report {
    /// Members that may reach SSH here, suspended ones included.
    pub allowed: usize,
    /// Members that may only pass removals.
    pub denied: usize,
    /// [`Access::digest`] of the access held: what Sheprd compares with
    /// the file it would write, to see whether this host is behind.
    pub digest: String,
    /// The fleet this account belongs to.
    pub fleet: Id,
    /// Members removed for good.
    pub removed: usize,
    /// Members whose new keys wait for room on the relays: Sheprd applies
    /// the access file again while any do, as room may have freed with
    /// the digest unchanged.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub crowded: BTreeSet<Id>,
    /// The last file handed over could not be read or checked, so the
    /// fleet reaches nothing here: Sheprd applies a good one again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub refused: bool,
    /// Enrolled desktops cut off because a member removed for good named
    /// their keys: `authorize` enrolls one again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revoked: Vec<Revoked>,
    /// Members suspended, with the removals that named them: what Sheprd
    /// asks its log about in the next file's `settled`.
    pub suspended: BTreeMap<Id, Suspension>,
}

/// An enrolled desktop a removed member's keys cut off.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Revoked {
    pub label: String,
    pub tunnel: Id,
}

fn read_json<T: serde::de::DeserializeOwned + Default>(state_dir: &Path, name: &str) -> Result<T> {
    let path = state_dir.join(name);
    match files::read_bounded(&path, MAX_SUSPENDED_BYTES)? {
        Some(bytes) => {
            serde_json::from_slice(&bytes).map_err(|source| Error::FleetFormat { path, source })
        }
        None => Ok(T::default()),
    }
}

/// The fleet, its short id, whose lines [`LINES`] says the agent may have
/// written; none when it wrote none. Blocking.
fn written_by(state_dir: &Path) -> Result<Option<String>> {
    Ok(files::read_bounded(&state_dir.join(LINES), 64)?
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned()))
}

/// Removes every `authorized_keys` line the agent may have written in
/// `keys`, needing nothing else here to read: the fleet the held access
/// file names, and the one [`LINES`] recorded before any line was
/// written. A record that does not read, or an access file whose fleet
/// does not, takes every fleet line: failing closed, since the account
/// holds one fleet's lines at most. With neither file, none was written.
/// Blocking.
fn remove_lines(state_dir: &Path, keys: Option<&Path>) -> Result<()> {
    let Some(path) = keys else {
        return Ok(());
    };
    let stored = files::read_bounded(
        &state_dir.join(access::FILE),
        access::MAX_STORED_BYTES as u64,
    )?;
    let recorded = written_by(state_dir)?;
    let valid =
        |fleet: &str| fleet.len() == 16 && fleet.bytes().all(|byte| byte.is_ascii_hexdigit());
    let mut fleets = BTreeSet::new();
    let mut unknown = false;
    if let Some(stored) = &stored {
        match access::fleet_of(stored) {
            Some(fleet) => {
                fleets.insert(fleet.short());
            }
            None => unknown = true,
        }
    }
    match recorded {
        Some(fleet) if valid(&fleet) => {
            fleets.insert(fleet);
        }
        Some(_) => unknown = true,
        None => {}
    }
    if unknown {
        return authorized_keys::clear_fleets(path);
    }
    for fleet in fleets {
        authorized_keys::set_fleet(path, &fleet, &[])?;
    }
    Ok(())
}

/// `value` as its file keeps it, refused past what [`read_json`] reads
/// back, so a save never leaves a file the next load rejects.
fn json(name: &str, value: &impl Serialize) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value).map_err(|source| Error::FleetFormat {
        path: name.into(),
        source,
    })?;
    if bytes.len() as u64 > MAX_SUSPENDED_BYTES {
        return Err(Error::TooLarge {
            limit: MAX_SUSPENDED_BYTES,
            path: name.into(),
        });
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests;
