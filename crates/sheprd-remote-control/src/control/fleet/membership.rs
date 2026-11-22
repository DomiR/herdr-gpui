//! Who is a member, in the history of any operation.
//!
//! Each operation is checked once, against the members of its own history
//! ([`Folder::check`]): its signer must have been live there, its signature
//! must check out with that signer's key, its sequence number must follow
//! the signer's earlier ones, and the signer must be allowed to sign that
//! body. That check depends only on the operation's ancestors, so it never
//! changes: an operation that fails it is refused for good.
//!
//! Which removals take effect depends on the whole set being folded,
//! because a removal voids every concurrent operation by the member it
//! removes, removals included. A removal also reaches the members its
//! target admitted that the remover had not seen, and theirs in turn: an
//! admission it voids leaves no member behind whose trust came only from
//! it. The one exception is a counter-removal of whoever removed it. Removals that void each other in a cycle take
//! effect, every one: when the rules cannot say who wins, everyone involved
//! is removed.

use super::{
    Body, Error, FleetId, Kind, MAX_COMPUTERS, MAX_MEMBERSHIP_OPS, MAX_PHONES, MAX_TOMBSTONES,
    MemberId, Op, OpId, Result,
    graph::{Bits, Graph},
    state::{Member, State},
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    rc::Rc,
};

/// The members of one history.
#[derive(Debug, Default)]
pub(super) struct Members {
    pub(super) live: BTreeMap<MemberId, Member>,
    pub(super) tombstones: BTreeSet<MemberId>,
    /// The removals that took effect, by place in the graph, with every
    /// member each removes ([`Folder::reach`]).
    pub(super) removals: BTreeMap<usize, BTreeSet<MemberId>>,
    /// Where each member first named the host its bundle names: its
    /// `Found`, `Admit`, or the `Rotate` that moved it to that host. A
    /// rotation that keeps the host keeps the claim where it was. A member
    /// from the base has none.
    pub(super) origins: BTreeMap<MemberId, usize>,
    /// Every operation that named the host each member's bundle names, in
    /// graph order, since it first named it: its `Found` or `Admit`, and
    /// each `Rotate` after. The last set the bundle it has now. A member
    /// from the base has none until it rotates.
    pub(super) claims: BTreeMap<MemberId, Vec<usize>>,
    /// The `Found`, `Admit`, and `Rotate` operations that took effect: a
    /// bundle refused or voided names nothing.
    pub(super) bundles: BTreeSet<usize>,
    /// Where a computer's own host went with it, as if removed there: the
    /// computer removed, or naming another host instead.
    pub(super) dropped: Vec<(usize, super::HostId)>,
}

impl Members {
    fn kind_count(&self, kind: Kind) -> usize {
        self.live
            .values()
            .filter(|member| member.bundle.kind == kind)
            .count()
    }
}

/// A set's membership operations after its barrier, and that barrier.
type MembersKey = (Bits, Option<usize>);

/// Checks operations and folds membership over a graph.
pub(super) struct Folder<'g, 'a> {
    graph: &'g Graph<'a>,
    base: &'g State,
    fleet: FleetId,
    /// The membership operations.
    mask: Bits,
    checks: Vec<Result<()>>,
    /// Members of each set's membership operations after its barrier
    /// ([`Folder::barrier`]), by that barrier.
    cache: RefCell<HashMap<MembersKey, Rc<Members>>>,
    /// The state at each barrier checkpoint, counting it.
    states: RefCell<HashMap<usize, Rc<State>>>,
    /// Operations whose signature was checked against their signer's key
    /// already ([`Folder::new`]).
    signed: &'g HashSet<OpId>,
}

impl<'g, 'a> Folder<'g, 'a> {
    /// Checks every operation of `graph`, in order. `base` is the state of
    /// the checkpoint the log was pruned to, or empty.
    /// Operations in `known` passed their check before, and are not
    /// checked again; those in `signed` had their signature checked with
    /// their signer's key, the one a member's id hashes, and only the rest
    /// of the check is left.
    pub(super) fn new(
        graph: &'g Graph<'a>,
        base: &'g State,
        fleet: FleetId,
        known: &HashSet<OpId>,
        signed: &'g HashSet<OpId>,
    ) -> Result<Self> {
        let mut mask = Bits::new(graph.len());
        for (position, op) in graph.order.iter().enumerate() {
            if op.body().membership() {
                mask.insert(position);
            }
        }
        let costly = mask
            .iter()
            .filter(|&position| !matches!(graph.order[position].body(), Body::Remove(_)))
            .count();
        if costly > MAX_MEMBERSHIP_OPS {
            return Err(Error::Full);
        }
        let mut folder = Self {
            graph,
            base,
            fleet,
            mask,
            checks: Vec::with_capacity(graph.len()),
            cache: RefCell::default(),
            states: RefCell::default(),
            signed,
        };
        for position in 0..graph.len() {
            let check = if known.contains(&graph.order[position].id()) {
                Ok(())
            } else {
                folder.check_one(position)
            };
            folder.checks.push(check);
        }
        Ok(folder)
    }

    pub(super) fn graph(&self) -> &Graph<'a> {
        self.graph
    }

    /// Whether the operation at `position` passed its check, for good.
    pub(super) fn check(&self, position: usize) -> &Result<()> {
        &self.checks[position]
    }

    /// The members of the down-closed set `set`.
    pub(super) fn members(&self, set: &Bits) -> Rc<Members> {
        let (rest, base) = self.split(set);
        let key = (rest.masked(&self.mask), base.as_ref().map(|(at, _)| *at));
        if let Some(members) = self.cache.borrow().get(&key) {
            return Rc::clone(members);
        }
        let members = Rc::new(self.fold(
            &key.0,
            base.as_ref().map_or(self.base, |(_, state)| state.as_ref()),
        ));
        self.cache.borrow_mut().insert(key, Rc::clone(&members));
        members
    }

    /// The last checkpoint in the down-closed `set` that every other
    /// operation there has seen or was seen by: a barrier. What came before
    /// it is settled in the state it names, the one a log pruned to it
    /// keeps, so the fold of `set` starts there, in a log pruned to it and
    /// in the whole log alike.
    pub(super) fn barrier(&self, set: &Bits) -> Option<usize> {
        let positions: Vec<usize> = set.iter().collect();
        positions.iter().rev().copied().find(|&at| {
            matches!(self.graph.order[at].body(), Body::Checkpoint(_))
                && self.checks.get(at).is_some_and(Result::is_ok)
                && positions.iter().all(|&other| {
                    other == at || self.graph.sees(at, other) || self.graph.sees(other, at)
                })
        })
    }

    /// `set` after its barrier, and the state there; `set` itself and no
    /// state when it has none.
    pub(super) fn split(&self, set: &Bits) -> (Bits, Option<(usize, Rc<State>)>) {
        let Some(at) = self.barrier(set) else {
            return (set.clone(), None);
        };
        let mut rest = Bits::new(self.graph.len());
        for position in set.iter().filter(|&position| self.graph.sees(position, at)) {
            rest.insert(position);
        }
        (rest, Some((at, self.state_at(at))))
    }

    /// The state the checkpoint at `at` names, counting the checkpoint
    /// itself, as a log pruned to it keeps it.
    fn state_at(&self, at: usize) -> Rc<State> {
        if let Some(state) = self.states.borrow().get(&at) {
            return Rc::clone(state);
        }
        let mut state = super::fold::fold(self, &self.graph.ancestors[at], self.base);
        let checkpoint = self.graph.order[at];
        if let Some(member) = state.members.get_mut(&checkpoint.signer()) {
            member.seq = member.seq.max(checkpoint.unsigned().seq);
        }
        let state = Rc::new(state);
        self.states.borrow_mut().insert(at, Rc::clone(&state));
        state
    }

    /// Whether `position` is void because an effective removal of its
    /// signer, in `members`, had not seen it.
    pub(super) fn voided(&self, members: &Members, position: usize) -> bool {
        let signer = self.graph.order[position].signer();
        members.removals.iter().any(|(&removal, reach)| {
            removal != position && !self.graph.sees(removal, position) && reach.contains(&signer)
        })
    }

    fn check_one(&self, position: usize) -> Result<()> {
        let op = self.graph.order[position];
        let members = self.members(&self.graph.ancestors[position]);
        let (key, kind) = match op.body() {
            Body::Found(bundle) => {
                if !self.base.members.is_empty()
                    || !self.base.tombstones.is_empty()
                    || bundle.id() != op.signer()
                    || bundle.kind != Kind::Computer
                    || FleetId(op.id().0) != self.fleet
                {
                    return Err(Error::Invalid);
                }
                (bundle.verifying_key()?, bundle.kind)
            }
            _ => {
                if op.unsigned().fleet != self.fleet {
                    return Err(Error::Fleet);
                }
                let member = members.live.get(&op.signer()).ok_or(Error::Signer)?;
                (member.bundle.verifying_key()?, member.bundle.kind)
            }
        };
        if !self.signed.contains(&op.id()) {
            op.verify(&key)?;
        }
        if op.unsigned().seq <= self.last_seq(position, op.signer()) {
            return Err(Error::Sequence);
        }
        self.permitted(op, kind, &members)?;
        if let Body::Access(access) = op.body()
            && Some(access.rev) != self.access_floor(position).checked_add(1)
        {
            return Err(Error::Rev);
        }
        Ok(())
    }

    /// The highest `rev` of any `Access` in the history of `position`, or
    /// the base's: the one an `Access` there must be one above. Every
    /// `Access` held passed this check, so none can put a register out of
    /// reach of a later deny. One floor for every host and member keeps it
    /// a matter of the operation's own history, which a checkpoint keeps
    /// whole in one number, whatever hosts and members came and went.
    fn access_floor(&self, position: usize) -> u64 {
        self.graph.ancestors[position]
            .iter()
            .filter(|&ancestor| self.checks.get(ancestor).is_some_and(Result::is_ok))
            .filter_map(|ancestor| match self.graph.order[ancestor].body() {
                Body::Access(other) => Some(other.rev),
                _ => None,
            })
            .fold(self.base.access_rev, u64::max)
    }

    /// The highest sequence number the signer used before `position`.
    fn last_seq(&self, position: usize, signer: MemberId) -> u64 {
        let base = self
            .base
            .members
            .get(&signer)
            .map_or(0, |member| member.seq);
        self.graph.ancestors[position]
            .iter()
            .map(|ancestor| self.graph.order[ancestor])
            .filter(|op| op.signer() == signer)
            .map(|op| op.unsigned().seq)
            .fold(base, u64::max)
    }

    fn permitted(&self, op: &Op, kind: Kind, members: &Members) -> Result<()> {
        let computer = kind == Kind::Computer;
        match op.body() {
            Body::Found(_) | Body::HostRemove(_) => Ok(()),
            Body::Admit(admit) => {
                let id = admit.bundle.id();
                if members.live.contains_key(&id)
                    || members.tombstones.contains(&id)
                    || taken(members, &admit.bundle, id)
                {
                    return Err(Error::Invalid);
                }
                let limit = match admit.bundle.kind {
                    Kind::Computer => MAX_COMPUTERS,
                    Kind::Phone => MAX_PHONES,
                };
                if members.kind_count(admit.bundle.kind) >= limit
                    || members.tombstones.len() >= MAX_TOMBSTONES
                {
                    return Err(Error::Full);
                }
                let message = admit
                    .bundle
                    .join_message(&self.fleet)
                    .ok_or(Error::Invalid)?;
                let signature = p256::ecdsa::Signature::from_slice(&admit.joiner_signature)
                    .map_err(|_| Error::JoinSignature)?;
                p256::ecdsa::signature::Verifier::verify(
                    &admit.bundle.verifying_key()?,
                    &message,
                    &signature,
                )
                .map_err(|_| Error::JoinSignature)
            }
            Body::Remove(remove) => {
                let mut seen = BTreeSet::new();
                if remove
                    .targets()
                    .all(|target| members.live.contains_key(target) && seen.insert(*target))
                {
                    Ok(())
                } else {
                    Err(Error::Unknown)
                }
            }
            Body::Rotate(bundle) => {
                let current = members.live.get(&op.signer()).ok_or(Error::Signer)?;
                if bundle.key != current.bundle.key || bundle.kind != kind {
                    return Err(Error::Permission);
                }
                if taken(members, bundle, op.signer()) {
                    return Err(Error::Invalid);
                }
                Ok(())
            }
            Body::HostAdd(_) | Body::Access(_) | Body::Checkpoint(_) if computer => Ok(()),
            Body::HostSeen(seen) if computer || seen.signature.is_some() => Ok(()),
            Body::Link(_) if kind == Kind::Phone => Ok(()),
            Body::HostAdd(_)
            | Body::Access(_)
            | Body::Checkpoint(_)
            | Body::HostSeen(_)
            | Body::Link(_) => Err(Error::Permission),
        }
    }

    /// Membership of the membership operations in `set`.
    fn fold(&self, set: &Bits, base: &State) -> Members {
        let checked: Vec<usize> = set
            .iter()
            .filter(|&position| self.checks.get(position).is_some_and(Result::is_ok))
            .collect();
        let removals = self.effective_removals(&checked);
        let mut members = Members {
            live: base.members.clone(),
            tombstones: base.tombstones.clone(),
            removals: BTreeMap::new(),
            origins: BTreeMap::new(),
            claims: BTreeMap::new(),
            bundles: BTreeSet::new(),
            dropped: Vec::new(),
        };
        let removed: BTreeSet<MemberId> = removals.values().flatten().copied().collect();
        let partial = Members {
            removals: removals.clone(),
            ..Members::default()
        };
        let mut unseated: BTreeMap<MemberId, super::HostId> = BTreeMap::new();
        // The seats an admission at `position` finds taken: live members of
        // its kind, but for those a removal it had seen took out, as its
        // own history (`Folder::check`) has them.
        let seated = |members: &Members, kind: Kind, position: usize| {
            members
                .live
                .iter()
                .filter(|(id, member)| {
                    member.bundle.kind == kind
                        && !removals.iter().any(|(&removal, reach)| {
                            reach.contains(id) && self.graph.sees(position, removal)
                        })
                })
                .count()
        };
        for &position in &checked {
            let op = self.graph.order[position];
            match op.body() {
                Body::Found(bundle) => {
                    members.origins.insert(bundle.id(), position);
                    members.claims.insert(bundle.id(), vec![position]);
                    members.bundles.insert(position);
                    members.live.insert(
                        bundle.id(),
                        Member {
                            bundle: bundle.clone(),
                            admitted_by: None,
                            seq: 0,
                            rotated: 0,
                        },
                    );
                }
                Body::Admit(admit) if !self.voided(&partial, position) => {
                    let id = admit.bundle.id();
                    let limit = match admit.bundle.kind {
                        Kind::Computer => MAX_COMPUTERS,
                        Kind::Phone => MAX_PHONES,
                    };
                    // A member removed in this set never goes live here,
                    // but the host it named still goes with it.
                    if removed.contains(&id)
                        && let Some(host) = host_id(&admit.bundle)
                    {
                        unseated.entry(id).or_insert(host);
                    }
                    // Concurrent admissions can pass the bound each on
                    // its own; the first in order are kept.
                    if !removed.contains(&id)
                        && !members.tombstones.contains(&id)
                        && !members.live.contains_key(&id)
                        && seated(&members, admit.bundle.kind, position) < limit
                    {
                        members.origins.insert(id, position);
                        members.claims.insert(id, vec![position]);
                        members.bundles.insert(position);
                        members.live.entry(id).or_insert(Member {
                            bundle: admit.bundle.clone(),
                            admitted_by: Some(op.signer()),
                            seq: 0,
                            rotated: 0,
                        });
                    }
                }
                Body::Rotate(bundle) if !self.voided(&partial, position) => {
                    if let Some(member) = members.live.get_mut(&op.signer()) {
                        let claims = members.claims.entry(op.signer()).or_default();
                        if host_id(&member.bundle) != host_id(bundle) {
                            if let Some(old) = host_id(&member.bundle) {
                                members.dropped.push((position, old));
                            }
                            members.origins.insert(op.signer(), position);
                            claims.clear();
                        }
                        claims.push(position);
                        members.bundles.insert(position);
                        member.bundle = bundle.clone();
                        member.rotated = op.unsigned().seq;
                    } else if let Some(host) = unseated.get_mut(&op.signer())
                        && let Some(id) = host_id(bundle)
                        && *host != id
                    {
                        // Naming another host drops the old one here too.
                        members.dropped.push((position, *host));
                        *host = id;
                    }
                }
                _ => {}
            }
        }
        for (&position, reach) in &removals {
            for member in reach {
                let host = members
                    .live
                    .get(member)
                    .and_then(|live| host_id(&live.bundle))
                    .or_else(|| unseated.get(member).copied());
                if let Some(host) = host {
                    members.dropped.push((position, host));
                }
            }
        }
        for id in removed {
            members.live.remove(&id);
            members.tombstones.insert(id);
        }
        members.removals = removals;
        members
    }

    /// The removals among `checked` that take effect, each with every
    /// member it removes.
    fn effective_removals(&self, checked: &[usize]) -> BTreeMap<usize, BTreeSet<MemberId>> {
        let removals: Vec<usize> = checked
            .iter()
            .copied()
            .filter(|&position| matches!(self.graph.order[position].body(), Body::Remove(_)))
            .collect();
        let admissions: Vec<usize> = checked
            .iter()
            .copied()
            .filter(|&position| matches!(self.graph.order[position].body(), Body::Admit(_)))
            .collect();
        let mut reach: Vec<BTreeSet<MemberId>> = removals
            .iter()
            .map(|&removal| self.reach(removal, targets(self.graph.order[removal]), &admissions))
            .collect();
        // Settle the removals, then let the ones in effect reach together:
        // a member every admission of which some removal in effect voids
        // has no trust left, though no one removal voids them all. Reaches
        // only grow, so this ends; a removal it leaves out of effect keeps
        // what it reached, which can only remove more (fail closed).
        loop {
            let decided = self.decide(&removals, &reach, &admissions);
            if !self.join_reach(&removals, &decided, &mut reach, &admissions) {
                return removals
                    .into_iter()
                    .zip(reach)
                    .zip(decided)
                    .filter(|(_, effective)| *effective)
                    .map(|(entry, _)| entry)
                    .collect();
            }
        }
    }

    /// Which `removals` take effect, given what each reaches.
    fn decide(
        &self,
        removals: &[usize],
        reach: &[BTreeSet<MemberId>],
        admissions: &[usize],
    ) -> Vec<bool> {
        let could_counter: Vec<bool> = (0..removals.len())
            .map(|slot| self.could_counter(slot, removals, reach, admissions))
            .collect();
        // Who could void each removal.
        let voiders: Vec<Vec<usize>> = (0..removals.len())
            .map(|of| {
                (0..removals.len())
                    .filter(|&by| self.voids(removals, reach, by, of, could_counter[of]))
                    .collect()
            })
            .collect();
        let mut decided: Vec<Option<bool>> = vec![None; removals.len()];
        loop {
            let mut changed = false;
            for slot in 0..removals.len() {
                if decided[slot].is_some() {
                    continue;
                }
                let states = voiders[slot].iter().map(|&other| decided[other]);
                if states.clone().any(|state| state == Some(true)) {
                    decided[slot] = Some(false);
                    changed = true;
                } else if states.clone().all(|state| state == Some(false)) {
                    decided[slot] = Some(true);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // What is left voids itself in a cycle: fail closed.
        decided
            .into_iter()
            .map(|state| state != Some(false))
            .collect()
    }

    /// Adds to the reach of the removals in effect every member each of
    /// whose admissions one of them voids: one that reaches its signer and
    /// had not seen it. The member joins the reach of each removal that
    /// voided one, so whatever it signed on that admission is void too.
    /// Says whether any reach grew.
    fn join_reach(
        &self,
        removals: &[usize],
        effective: &[bool],
        reach: &mut [BTreeSet<MemberId>],
        admissions: &[usize],
    ) -> bool {
        let mut grew = false;
        loop {
            let voiding = |admission: usize, reach: &[BTreeSet<MemberId>]| -> Vec<usize> {
                let signer = self.graph.order[admission].signer();
                (0..removals.len())
                    .filter(|&slot| {
                        effective[slot]
                            && reach[slot].contains(&signer)
                            && !self.graph.sees(removals[slot], admission)
                    })
                    .collect()
            };
            let mut joined: Vec<(usize, MemberId)> = Vec::new();
            let mut by_member: BTreeMap<MemberId, Vec<usize>> = BTreeMap::new();
            for &admission in admissions {
                if let Body::Admit(admit) = self.graph.order[admission].body() {
                    by_member
                        .entry(admit.bundle.id())
                        .or_default()
                        .push(admission);
                }
            }
            for (member, admitted_by) in by_member {
                if self.base.members.contains_key(&member) || self.base.tombstones.contains(&member)
                {
                    continue;
                }
                let voided: Vec<Vec<usize>> = admitted_by
                    .iter()
                    .map(|&admission| voiding(admission, reach))
                    .collect();
                if voided.iter().all(|slots| !slots.is_empty()) {
                    joined.extend(
                        voided
                            .into_iter()
                            .flatten()
                            .filter(|&slot| !reach[slot].contains(&member))
                            .map(|slot| (slot, member)),
                    );
                }
            }
            if joined.is_empty() {
                return grew;
            }
            grew = true;
            for (slot, member) in joined {
                reach[slot].insert(member);
            }
        }
    }

    /// Every member removal `removal` removes when it starts from `seeds`:
    /// those, and every member one of them admitted that `removal` had
    /// not seen, and so on. Such an admission is void with its signer, so
    /// whoever it admitted has no trust left of its own.
    ///
    /// Only a member every admission of which is void that way: a join
    /// signature is not bound to one admission, so a removed member can
    /// replay it in an admission of its own, and that must not take a
    /// member another admission let in. A member of the base was let in
    /// before any of these.
    fn reach<'m>(
        &self,
        removal: usize,
        seeds: impl Iterator<Item = &'m MemberId>,
        admissions: &[usize],
    ) -> BTreeSet<MemberId> {
        let mut reach: BTreeSet<MemberId> = seeds.copied().collect();
        let admitted = |admission: usize| match self.graph.order[admission].body() {
            Body::Admit(admit) => Some(admit.bundle.id()),
            _ => None,
        };
        loop {
            let before = reach.len();
            let void = |admission: usize, reach: &BTreeSet<MemberId>| {
                reach.contains(&self.graph.order[admission].signer())
                    && !self.graph.sees(removal, admission)
            };
            let joined: Vec<MemberId> = admissions
                .iter()
                .filter(|&&admission| void(admission, &reach))
                .filter_map(|&admission| admitted(admission))
                .filter(|id| {
                    !reach.contains(id)
                        && !self.base.members.contains_key(id)
                        && !self.base.tombstones.contains(id)
                        && admissions
                            .iter()
                            .filter(|&&other| admitted(other) == Some(*id))
                            .all(|&other| void(other, &reach))
                })
                .collect();
            reach.extend(joined);
            if reach.len() == before {
                return reach;
            }
        }
    }

    /// Whether removal `by`, if it takes effect, voids removal `of`: it
    /// removes `of`'s signer without having seen `of`, and `of` is not a
    /// counter-removal of `by`'s signer. `could_counter` says, for `of`,
    /// whether it could be one ([`Folder::could_counter`]).
    fn voids(
        &self,
        removals: &[usize],
        reach: &[BTreeSet<MemberId>],
        by: usize,
        of: usize,
        could_counter: bool,
    ) -> bool {
        let (by_op, of_op) = (
            self.graph.order[removals[by]],
            self.graph.order[removals[of]],
        );
        by != of
            && !self.graph.sees(removals[by], removals[of])
            && reach[by].contains(&of_op.signer())
            && !(could_counter && reach[of].contains(&by_op.signer()))
    }

    /// Whether removal `of` removes only members that removed its signer
    /// without its having seen them, each itself or through a member it
    /// admitted that `of` had not seen, and no host: what a
    /// counter-removal may do. A removed member takes with it the members
    /// that removed it, and nothing more.
    fn could_counter(
        &self,
        of: usize,
        removals: &[usize],
        reach: &[BTreeSet<MemberId>],
        admissions: &[usize],
    ) -> bool {
        let of_op = self.graph.order[removals[of]];
        let Body::Remove(remove) = of_op.body() else {
            return false;
        };
        remove.also_hosts.is_empty()
            && remove.targets().all(|target| {
                let lineage = self.reach(removals[of], std::iter::once(target), admissions);
                removals.iter().zip(reach).any(|(&other, other_reach)| {
                    lineage.contains(&self.graph.order[other].signer())
                        && !self.graph.sees(removals[of], other)
                        && other_reach.contains(&of_op.signer())
                })
            })
    }
}

/// The host a bundle names as its own.
fn host_id(bundle: &super::Bundle) -> Option<super::HostId> {
    bundle.host.as_ref().and_then(|intro| intro.id().ok())
}

/// Whether another live member already holds a key or the host `bundle`
/// names: its tunnel key, its dial key, or its own host (a copied intro,
/// perhaps with another user to log in as). Keys are public, so a bundle
/// naming another member's would make that member's sessions look like
/// its own.
fn taken(members: &Members, bundle: &super::Bundle, own: MemberId) -> bool {
    let host = host_id(bundle);
    members.live.iter().any(|(member, other)| {
        *member != own
            && (other.bundle.tunnel == bundle.tunnel
                || other.bundle.dial == bundle.dial
                || host.is_some_and(|host| host_id(&other.bundle) == Some(host)))
    })
}

/// The members an operation removes; none unless it is a removal.
pub(super) fn targets(op: &Op) -> impl Iterator<Item = &MemberId> {
    let remove = match op.body() {
        Body::Remove(remove) => Some(remove),
        _ => None,
    };
    remove.into_iter().flat_map(super::Remove::targets)
}
