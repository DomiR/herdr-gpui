//! A member's copy of the fleet log: the operations it holds, those
//! waiting for their parents, and the state they compute.

use super::{
    Admit, Body, Bundle, Error, FleetId, MAX_CHECKPOINTS_PER_SIGNER, MAX_MEMBERSHIP_OPS, MAX_OPS,
    MAX_PARENTS, MAX_PENDING, MAX_REMOVALS, MAX_REMOVALS_PER_SIGNER, MAX_SKEW_MS, MemberId, Op,
    OpId, PENDING_TTL_MS, Result, State, Unsigned,
    body::RawSignature,
    fold::fold,
    graph::{Bits, Graph},
    membership::Folder,
};
use std::collections::{BTreeMap, HashMap, HashSet};

/// What inserting an operation came to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Inserted {
    /// It passed its checks and is part of the log.
    Applied,
    /// The log already held it.
    Known,
    /// Its parents have not arrived; it waits for them.
    Pending,
}

/// The fleet log.
#[derive(Clone, Debug)]
pub struct Log {
    fleet: FleetId,
    /// The checkpoint the log was pruned to, which every kept operation
    /// descends from, and the state it names.
    root: Option<Op>,
    base: State,
    /// The checkpoint signer's sequence number in the state `root` names:
    /// `base` counts the checkpoint itself, the state it names does not.
    root_seq: u64,
    ops: BTreeMap<OpId, Op>,
    /// Operations waiting for parents, with when each arrived.
    /// Keyed by signature as well as id, which does not cover it: a copy
    /// with a damaged signature cannot stand in for the genuine one.
    pending: BTreeMap<(OpId, RawSignature), (Op, u64)>,
    state: State,
    heads: Vec<OpId>,
    /// Operations whose check passed, which never changes.
    checked: HashSet<OpId>,
    /// Operations just added whose signature `forged` checked, while they
    /// settle.
    signed: HashSet<OpId>,
}

impl Log {
    /// A new fleet, founded by `found`.
    pub fn found(found: Op) -> Result<Self> {
        if !matches!(found.body(), Body::Found(_)) {
            return Err(Error::Invalid);
        }
        let mut log = Self {
            fleet: FleetId(found.id().0),
            root: None,
            base: State::default(),
            root_seq: 0,
            ops: BTreeMap::from([(found.id(), found)]),
            pending: BTreeMap::new(),
            state: State::default(),
            heads: Vec::new(),
            checked: HashSet::new(),
            signed: HashSet::new(),
        };
        let graph = Graph::new(&log.ops, None)?;
        let folder = Folder::new(&graph, &log.base, log.fleet, &HashSet::new(), &log.signed)?;
        folder.check(0).clone()?;
        log.state = fold(&folder, &Bits::full(graph.len()), &log.base);
        log.heads = graph.heads();
        drop(folder);
        drop(graph);
        log.checked = log.ops.keys().copied().collect();
        Ok(log)
    }

    /// A log that starts at `checkpoint`, whose history computed `state`,
    /// as a joining member is handed it. The joiner trusts the member that
    /// approved it for that history; everything after is checked.
    pub fn from_checkpoint(checkpoint: Op, state: &[u8]) -> Result<Self> {
        let Body::Checkpoint(hash) = checkpoint.body() else {
            return Err(Error::Invalid);
        };
        let base = State::decode(state)?;
        if base.hash()? != *hash {
            return Err(Error::Checkpoint);
        }
        let signer = base
            .members
            .get(&checkpoint.signer())
            .ok_or(Error::Signer)?;
        if signer.bundle.kind != super::Kind::Computer {
            return Err(Error::Permission);
        }
        checkpoint.verify(&signer.bundle.verifying_key()?)?;
        let mut base = base;
        let root_seq = counted(&mut base, &checkpoint);
        Ok(Self {
            fleet: checkpoint.unsigned().fleet,
            root_seq,
            heads: vec![checkpoint.id()],
            root: Some(checkpoint),
            state: base.clone(),
            base,
            ops: BTreeMap::new(),
            pending: BTreeMap::new(),
            checked: HashSet::new(),
            signed: HashSet::new(),
        })
    }

    #[must_use]
    pub fn fleet(&self) -> FleetId {
        self.fleet
    }

    #[must_use]
    pub fn state(&self) -> &State {
        &self.state
    }

    /// The operations nothing in the log names as a parent.
    #[must_use]
    pub fn heads(&self) -> &[OpId] {
        &self.heads
    }

    /// The checkpoint the log was pruned to.
    #[must_use]
    pub fn root(&self) -> Option<&Op> {
        self.root.as_ref()
    }

    #[must_use]
    pub fn get(&self, id: &OpId) -> Option<&Op> {
        self.ops
            .get(id)
            .or_else(|| self.root.as_ref().filter(|root| root.id() == *id))
    }

    /// Every operation held, in id order.
    pub fn ops(&self) -> impl Iterator<Item = &Op> {
        self.ops.values()
    }

    /// Operations waiting for their parents.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// The checkpoint the log was pruned to and the state it names, as
    /// the log is saved.
    #[must_use]
    pub fn base(&self) -> Option<(&Op, &State)> {
        self.root.as_ref().map(|root| (root, &self.base))
    }

    /// What a peer whose heads are `heads` lacks: every operation held
    /// that is neither one of them nor an ancestor of one, in graph order,
    /// so the peer can apply them as they come. Heads this log does not
    /// know are passed over, so the peer may get what it already has.
    #[must_use]
    pub fn missing_for(&self, heads: &[OpId]) -> Vec<&Op> {
        let Ok(graph) = self.graph() else {
            return self.ops.values().collect();
        };
        let mut known = Bits::new(graph.len());
        for head in heads {
            if let Some(&position) = graph.index.get(head) {
                known.union(&graph.ancestors[position]);
                known.insert(position);
            }
        }
        (0..graph.len())
            .filter(|position| !known.contains(*position))
            .map(|position| graph.order[position])
            .collect()
    }

    /// Removals waiting for their parents whose signature already checks
    /// out against a live member's key: what a daemon suspends at once,
    /// before their history arrives (`docs/relay-plan.md`, Removals
    /// between daemons). Each, with its targets.
    #[must_use]
    pub fn pending_removals(&self) -> Vec<(&Op, Vec<MemberId>)> {
        self.pending
            .values()
            .filter_map(|(op, _)| {
                let Body::Remove(remove) = op.body() else {
                    return None;
                };
                let signer = self.state.members.get(&op.signer())?;
                op.verify(&signer.bundle.verifying_key().ok()?).ok()?;
                let targets = remove
                    .targets()
                    .filter(|target| self.state.members.contains_key(target))
                    .copied()
                    .collect::<Vec<_>>();
                (!targets.is_empty()).then_some((op, targets))
            })
            .collect()
    }

    /// The parents operations waiting in the pending pool need, that
    /// neither the log nor the pool holds: what to ask a peer for.
    #[must_use]
    pub fn wanted(&self) -> Vec<OpId> {
        let mut wanted: Vec<OpId> = self
            .pending
            .values()
            .flat_map(|(op, _)| op.parents().iter().copied())
            .filter(|parent| !self.present(parent) && !self.waiting(parent))
            .collect();
        wanted.sort_unstable();
        wanted.dedup();
        wanted
    }

    /// Who signed a removal of `member` among the operations held, whether
    /// or not it took effect.
    #[must_use]
    pub fn removers(&self, member: &MemberId) -> Vec<MemberId> {
        self.ops
            .values()
            .filter(|op| match op.body() {
                Body::Remove(remove) => remove.targets().any(|target| target == member),
                _ => false,
            })
            .map(Op::signer)
            .collect()
    }

    /// Whether `member` is live in this log.
    #[must_use]
    pub fn is_member(&self, member: &MemberId) -> bool {
        self.state.members.contains_key(member)
    }

    /// An operation for `signer` to sign next: on the heads, with the
    /// next sequence number.
    ///
    /// An operation names at most [`MAX_PARENTS`] heads. The first is one
    /// that descends from the signer's own latest operation, so its chain
    /// never looks forked. The rest are those of members not flagged for
    /// signing in two places first: a member that forks its key cannot
    /// crowd out the others, and what it signed that a removal did not
    /// name is void anyway. The heads left out are named by the next
    /// operations.
    ///
    /// An `Access` gets the `rev` its parents call for, one above the
    /// highest for its host and member they hold, whatever it was drafted
    /// with: the register can be ahead of what the parents name.
    #[must_use]
    pub fn draft(&self, signer: MemberId, issued_ms: u64, mut body: Body) -> Unsigned {
        let own = self.own_head(signer);
        let mut heads: Vec<(bool, OpId)> = self
            .heads
            .iter()
            .filter(|id| Some(**id) != own)
            .map(|id| {
                let forked = self
                    .get(id)
                    .is_some_and(|op| self.state.equivocating.contains(&op.signer()));
                (forked, *id)
            })
            .collect();
        heads.sort_unstable();
        let mut parents: Vec<OpId> = own
            .into_iter()
            .chain(heads.into_iter().map(|(_, id)| id))
            .take(MAX_PARENTS)
            .collect();
        parents.sort_unstable();
        if let Body::Access(access) = &mut body {
            access.rev = self.access_floor(&parents).saturating_add(1);
        }
        let seq = self
            .ops
            .values()
            .chain(self.root.as_ref())
            .filter(|op| op.signer() == signer)
            .map(|op| op.unsigned().seq)
            .chain(self.base.members.get(&signer).map(|member| member.seq))
            .max()
            .unwrap_or(0)
            + 1;
        Unsigned {
            fleet: self.fleet,
            signer,
            seq,
            issued_ms,
            parents,
            body,
        }
    }

    /// Drafts one operation per body for `signer`, each on the one before,
    /// and checks that each would apply once `signer` signs it: every rule
    /// but the signature, on a copy of this log that is then dropped. A
    /// member that keeps its own log asks this before its owner signs, so
    /// it never signs what its own replay refuses.
    pub fn drafts(
        &self,
        signer: MemberId,
        issued_ms: u64,
        bodies: Vec<Body>,
        now_ms: u64,
    ) -> Result<Vec<Unsigned>> {
        let mut copy = self.clone();
        let mut drafted = Vec::with_capacity(bodies.len());
        for body in bodies {
            let unsigned = copy.draft(signer, issued_ms, body);
            // A placeholder signature, never checked: `settle` presumes it.
            let op = Op::decode(&[unsigned.encode()?.as_slice(), &[0; 64]].concat())?;
            let id = op.id();
            if copy.admissible(&op, now_ms)? != Inserted::Pending {
                return Err(Error::Invalid);
            }
            copy.ops.insert(id, op);
            if let Some(error) = copy.settle(&[id], true)?.remove(&id) {
                return Err(error);
            }
            drafted.push(unsigned);
        }
        Ok(drafted)
    }

    /// The highest `rev` of any `Access` that `parents` and their
    /// ancestors hold, or the base's.
    fn access_floor(&self, parents: &[OpId]) -> u64 {
        let mut floor = self.base.access_rev;
        let mut seen: HashSet<OpId> = parents.iter().copied().collect();
        let mut queue = parents.to_vec();
        while let Some(id) = queue.pop() {
            let Some(op) = self.ops.get(&id) else {
                continue;
            };
            if let Body::Access(access) = op.body() {
                floor = floor.max(access.rev);
            }
            queue.extend(op.parents().iter().filter(|parent| seen.insert(**parent)));
        }
        floor
    }

    /// The first head, by id, that is or descends from `signer`'s latest
    /// operation held; none when it signed nothing since the root.
    fn own_head(&self, signer: MemberId) -> Option<OpId> {
        let latest = self
            .ops
            .values()
            .filter(|op| op.signer() == signer)
            .max_by_key(|op| (op.unsigned().seq, op.id()))?
            .id();
        let mut children: HashMap<OpId, Vec<OpId>> = HashMap::new();
        for op in self.ops.values() {
            for parent in op.parents() {
                children.entry(*parent).or_default().push(op.id());
            }
        }
        let mut seen = HashSet::from([latest]);
        let mut queue = vec![latest];
        while let Some(id) = queue.pop() {
            for child in children.get(&id).into_iter().flatten() {
                if seen.insert(*child) {
                    queue.push(*child);
                }
            }
        }
        self.heads
            .iter()
            .filter(|id| seen.contains(id))
            .min()
            .copied()
    }

    /// Inserts one operation that arrived at `now_ms`.
    pub fn insert(&mut self, op: Op, now_ms: u64) -> Result<Inserted> {
        let id = op.id();
        self.insert_all(vec![op], now_ms)
            .into_iter()
            .find(|(inserted, _)| *inserted == id)
            .map_or(Ok(Inserted::Known), |(_, result)| result)
    }

    /// Inserts operations that arrived together at `now_ms`, in any order,
    /// folding once. Says what each came to; one refused never stops the
    /// rest, though its descendants are refused with it.
    pub fn insert_all(&mut self, ops: Vec<Op>, now_ms: u64) -> Vec<(OpId, Result<Inserted>)> {
        let mut results = Vec::with_capacity(ops.len());
        let mut fresh = Vec::new();
        for op in ops {
            let id = op.id();
            let result = self.admissible(&op, now_ms);
            match result {
                Ok(Inserted::Pending) => {
                    self.pending
                        .entry((id, *op.signature()))
                        .or_insert((op, now_ms));
                    fresh.push(id);
                }
                other => results.push((id, other)),
            }
        }
        // Expired operations go first; the bound waits until those whose
        // parents are present have left the pool.
        self.expire(now_ms);
        let mut added = Vec::new();
        loop {
            let ready = self.ready();
            if ready.is_empty() {
                break;
            }
            for (id, key) in ready {
                if let Some((op, _)) = self.pending.remove(&key) {
                    self.pending.retain(|(other, _), _| *other != id);
                    self.ops.insert(id, op);
                    added.push(id);
                }
            }
        }
        self.bound();
        let refused = match self.settle(&added, false) {
            Ok(refused) => refused,
            Err(error) => {
                // The log cannot take these at all (full): put it back as
                // it was.
                for id in &added {
                    self.ops.remove(id);
                }
                added.iter().map(|id| (*id, error.clone())).collect()
            }
        };
        for id in fresh {
            let result = match refused.get(&id) {
                Some(error) => Err(error.clone()),
                None if self.ops.contains_key(&id) => Ok(Inserted::Applied),
                None if self.waiting(&id) => Ok(Inserted::Pending),
                // Dropped to keep the pool for missing parents bounded.
                None => Err(Error::Full),
            };
            results.push((id, result));
        }
        results
    }

    /// Every removal in effect among the operations held, with every
    /// member it removes under the log's rules: those it names, and those
    /// it takes with them, such as a member one it removes admitted at the
    /// same time. A fold starts at the last checkpoint every operation has
    /// seen or was seen by, so the removals before each such checkpoint
    /// are read from the history up to it, which settled them: none is
    /// lost behind a later checkpoint. A log pruned to a checkpoint holds
    /// no removal from before it to pass on, and every removal after it
    /// has seen every admission the checkpoint settled.
    pub fn removed_by(&self) -> Result<BTreeMap<OpId, std::collections::BTreeSet<MemberId>>> {
        let graph = self.graph()?;
        let folder = self.folder(&graph)?;
        let mut removed_by = BTreeMap::new();
        let mut set = Bits::full(graph.len());
        loop {
            let members = folder.members(&set);
            for (&position, reach) in &members.removals {
                removed_by
                    .entry(graph.order[position].id())
                    .or_insert_with(|| reach.clone());
            }
            let Some(at) = folder.barrier(&set) else {
                break;
            };
            // The history the checkpoint settled: its strict ancestors.
            let before = graph.ancestors[at].clone();
            if before.iter().next().is_none() {
                break;
            }
            set = before;
        }
        Ok(removed_by)
    }

    /// The state of the history of `id`, which a checkpoint at `id` names
    /// and a joining member is handed with it.
    pub fn state_at(&self, id: &OpId) -> Result<State> {
        if let Some(root) = self.root.as_ref().filter(|root| root.id() == *id) {
            let mut state = self.base.clone();
            if let Some(member) = state.members.get_mut(&root.signer()) {
                member.seq = self.root_seq;
            }
            return Ok(state);
        }
        let graph = self.graph()?;
        let folder = self.folder(&graph)?;
        let position = *graph.index.get(id).ok_or(Error::Unknown)?;
        Ok(fold(&folder, &graph.ancestors[position], &self.base))
    }

    /// Prunes the log to the checkpoint `id`: drops its history and keeps
    /// its state. Refused while any operation held is concurrent with it,
    /// since that one's history could not be recomputed after.
    pub fn prune(&mut self, id: &OpId) -> Result<()> {
        let graph = self.graph()?;
        let position = *graph.index.get(id).ok_or(Error::Unknown)?;
        if !matches!(graph.order[position].body(), Body::Checkpoint(_))
            || (0..graph.len()).any(|other| graph.concurrent(position, other))
        {
            return Err(Error::Checkpoint);
        }
        let folder = self.folder(&graph)?;
        let mut base = fold(&folder, &graph.ancestors[position], &self.base);
        let root_seq = counted(&mut base, graph.order[position]);
        let dropped: Vec<OpId> = graph.ancestors[position]
            .iter()
            .map(|ancestor| graph.order[ancestor].id())
            .collect();
        drop(folder);
        drop(graph);
        for ancestor in dropped {
            self.ops.remove(&ancestor);
        }
        self.root = self.ops.remove(id);
        self.base = base;
        self.root_seq = root_seq;
        self.recompute()
    }

    fn admissible(&self, op: &Op, now_ms: u64) -> Result<Inserted> {
        if self.present(&op.id()) || self.pending.contains_key(&(op.id(), *op.signature())) {
            return Ok(Inserted::Known);
        }
        if matches!(op.body(), Body::Found(_)) || op.unsigned().fleet != self.fleet {
            return Err(Error::Fleet);
        }
        if op.unsigned().issued_ms > now_ms.saturating_add(MAX_SKEW_MS) {
            return Err(Error::Future);
        }
        Ok(Inserted::Pending)
    }

    /// Whether a copy of `id` waits for its parents.
    fn waiting(&self, id: &OpId) -> bool {
        self.pending
            .range((*id, [0; 64])..=(*id, [0xff; 64]))
            .next()
            .is_some()
    }

    /// The operations waiting whose parents are all present, one copy of
    /// each: the first whose signature checks out with its signer's key,
    /// or the first when none does (it is refused then, as it should be).
    fn ready(&self) -> Vec<(OpId, (OpId, RawSignature))> {
        let mut ready: BTreeMap<OpId, Vec<(OpId, RawSignature)>> = BTreeMap::new();
        for (key, (op, _)) in &self.pending {
            if op.parents().iter().all(|parent| self.present(parent)) {
                ready.entry(key.0).or_default().push(*key);
            }
        }
        // Signatures are checked here only to pick between copies; every
        // operation's is checked when it settles.
        let keys = self.keys();
        let signed = |key: &(OpId, RawSignature)| {
            self.pending.get(key).is_some_and(|(op, _)| {
                keys.get(&op.signer())
                    .and_then(|bundle| bundle.verifying_key().ok())
                    .is_some_and(|verifying| op.verify(&verifying).is_ok())
            })
        };
        ready
            .into_iter()
            .filter_map(|(id, copies)| {
                let pick = match copies.as_slice() {
                    [only] => *only,
                    _ => copies
                        .iter()
                        .find(|key| signed(key))
                        .or(copies.first())
                        .copied()?,
                };
                Some((id, pick))
            })
            .collect()
    }

    /// The bundle held for each member, by id. A member's id is the hash of
    /// its key, so any bundle held for it gives the key, whatever
    /// operation carries it.
    fn keys(&self) -> HashMap<MemberId, &Bundle> {
        let mut keys: HashMap<MemberId, &Bundle> = self
            .base
            .members
            .iter()
            .map(|(id, member)| (*id, &member.bundle))
            .collect();
        for op in self.ops.values() {
            if let Body::Found(bundle) | Body::Rotate(bundle) | Body::Admit(Admit { bundle, .. }) =
                op.body()
            {
                keys.entry(bundle.id()).or_insert(bundle);
            }
        }
        keys
    }

    fn present(&self, id: &OpId) -> bool {
        self.ops.contains_key(id) || self.root.as_ref().is_some_and(|root| root.id() == *id)
    }

    fn expire(&mut self, now_ms: u64) {
        self.pending
            .retain(|_, (_, since)| now_ms.saturating_sub(*since) <= PENDING_TTL_MS);
    }

    /// Keeps at most [`MAX_PENDING`] operations waiting for parents.
    fn bound(&mut self) {
        while self.pending.len() > MAX_PENDING {
            // Removals are dropped last.
            let oldest = self
                .pending
                .iter()
                .min_by_key(|(key, (op, since))| {
                    (matches!(op.body(), Body::Remove(_)), *since, **key)
                })
                .map(|(key, _)| *key);
            if let Some(oldest) = oldest {
                self.pending.remove(&oldest);
            }
        }
    }

    /// Checks the operations just `added`, removes those refused and their
    /// descendants, and folds. Says why each was refused.
    /// With `presumed`, their signatures are taken as checked: only for
    /// [`Log::drafts`], on a copy that is dropped.
    fn settle(&mut self, added: &[OpId], presumed: bool) -> Result<BTreeMap<OpId, Error>> {
        let mut refused = if presumed {
            BTreeMap::new()
        } else {
            self.forged(added)
        };
        self.signed = added
            .iter()
            .filter(|id| !refused.contains_key(*id))
            .copied()
            .collect();
        self.orphans(added, &mut refused);
        let over = self.over_caps(added, &refused);
        refused.extend(over);
        self.orphans(added, &mut refused);
        for id in refused.keys() {
            self.ops.remove(id);
        }
        {
            let graph = self.graph()?;
            let folder = self.folder(&graph)?;
            let fresh: HashSet<OpId> = added.iter().copied().collect();
            for position in 0..graph.len() {
                let op = graph.order[position];
                if !fresh.contains(&op.id()) {
                    continue;
                }
                let parent_refused = op
                    .parents()
                    .iter()
                    .any(|parent| refused.contains_key(parent));
                let result = if parent_refused {
                    Err(Error::ParentMissing)
                } else {
                    folder
                        .check(position)
                        .clone()
                        .and_then(|()| checkpoint_matches(&folder, position, &self.base))
                };
                if let Err(error) = result {
                    refused.insert(op.id(), error);
                }
            }
        }
        for id in refused.keys() {
            self.ops.remove(id);
        }
        self.checked = self.ops.keys().copied().collect();
        self.signed.clear();
        self.recompute()?;
        Ok(refused)
    }

    /// The operations just `added` that no key of the member they name
    /// signed. Checked before any bound is charged to that member, so
    /// forgeries in its name cannot use up its share. A member's id is the
    /// hash of its key, so any bundle held for it gives the key, whatever
    /// operation carries it ([`Log::keys`]).
    fn forged(&self, added: &[OpId]) -> BTreeMap<OpId, Error> {
        let keys = self.keys();
        added
            .iter()
            .filter_map(|id| self.ops.get(id))
            .filter_map(|op| {
                let checked = keys
                    .get(&op.signer())
                    .ok_or(Error::Signer)
                    .and_then(|bundle| bundle.verifying_key())
                    .and_then(|key| op.verify(&key));
                checked.err().map(|error| (op.id(), error))
            })
            .collect()
    }

    /// The operations just `added` that would take the log past a bound.
    /// A removal never counts against the others' bounds, so filling the
    /// log cannot stop its signer's removal. Removals have their own: per
    /// signer, so no member can use up a live member's share, and one
    /// shared by members no longer live. A checkpoint is the way out of a
    /// full log, so it counts against no other bound either, only a few
    /// of its own per signer.
    /// Operations already `refused` (forgeries and what builds on them)
    /// are not counted, so they take no share.
    fn over_caps(&self, added: &[OpId], refused: &BTreeMap<OpId, Error>) -> BTreeMap<OpId, Error> {
        let removal = |op: &Op| matches!(op.body(), Body::Remove(_));
        let checkpoint = |op: &Op| matches!(op.body(), Body::Checkpoint(_));
        let costly = |op: &Op| op.body().membership() && !removal(op);
        let gone = |op: &Op| removal(op) && !self.state.members.contains_key(&op.signer());
        let held = || {
            self.ops
                .values()
                .filter(|op| !refused.contains_key(&op.id()))
        };
        let mut ops = held().filter(|op| !removal(op) && !checkpoint(op)).count();
        let mut membership = held().filter(|op| costly(op)).count();
        let mut removals = held().filter(|op| gone(op)).count();
        let mut by_signer: BTreeMap<MemberId, usize> = BTreeMap::new();
        let mut checkpoints: BTreeMap<MemberId, usize> = BTreeMap::new();
        let fresh: HashSet<&OpId> = added.iter().collect();
        for op in held().filter(|op| !fresh.contains(&op.id())) {
            if removal(op) {
                *by_signer.entry(op.signer()).or_default() += 1;
            } else if checkpoint(op) {
                *checkpoints.entry(op.signer()).or_default() += 1;
            }
        }
        let mut over = BTreeMap::new();
        // Newest last: the batch is in id order, which every member shares.
        let mut batch: Vec<&Op> = added
            .iter()
            .filter(|id| !refused.contains_key(*id))
            .filter_map(|id| self.ops.get(id))
            .collect();
        batch.sort_by_key(|op| op.id());
        for op in batch {
            if removal(op) {
                let mine = by_signer.entry(op.signer()).or_default();
                *mine += 1;
                if *mine > MAX_REMOVALS_PER_SIGNER || (gone(op) && removals > MAX_REMOVALS) {
                    over.insert(op.id(), Error::Full);
                    *mine -= 1;
                    if gone(op) {
                        removals -= 1;
                    }
                }
            } else if checkpoint(op) {
                let mine = checkpoints.entry(op.signer()).or_default();
                *mine += 1;
                if *mine > MAX_CHECKPOINTS_PER_SIGNER {
                    over.insert(op.id(), Error::Full);
                    *mine -= 1;
                }
            } else if ops > MAX_OPS || (costly(op) && membership > MAX_MEMBERSHIP_OPS) {
                over.insert(op.id(), Error::Full);
                ops -= 1;
                if costly(op) {
                    membership -= 1;
                }
            }
        }
        over
    }

    /// Adds to `refused` every operation just `added` that builds on one
    /// refused: it is refused with it.
    fn orphans(&self, added: &[OpId], refused: &mut BTreeMap<OpId, Error>) {
        loop {
            let orphans: Vec<OpId> = added
                .iter()
                .filter(|id| !refused.contains_key(*id))
                .filter(|id| {
                    self.ops.get(*id).is_some_and(|op| {
                        op.parents()
                            .iter()
                            .any(|parent| refused.contains_key(parent))
                    })
                })
                .copied()
                .collect();
            if orphans.is_empty() {
                return;
            }
            refused.extend(orphans.into_iter().map(|id| (id, Error::ParentMissing)));
        }
    }

    fn recompute(&mut self) -> Result<()> {
        let graph = self.graph()?;
        let folder = self.folder(&graph)?;
        let state = fold(&folder, &Bits::full(graph.len()), &self.base);
        let mut heads = graph.heads();
        if heads.is_empty()
            && let Some(root) = &self.root
        {
            heads.push(root.id());
        }
        drop(folder);
        drop(graph);
        self.state = state;
        self.heads = heads;
        Ok(())
    }

    fn graph(&self) -> Result<Graph<'_>> {
        Graph::new(&self.ops, self.root.as_ref().map(Op::id))
    }

    fn folder<'g, 'a>(&'g self, graph: &'g Graph<'a>) -> Result<Folder<'g, 'a>> {
        Folder::new(graph, &self.base, self.fleet, &self.checked, &self.signed)
    }
}

/// Counts the checkpoint `root` itself in a state computed from its
/// history, so its signer's next sequence number follows it.
/// Says the signer's sequence number before, as the state `root` names it.
fn counted(base: &mut State, root: &Op) -> u64 {
    let Some(member) = base.members.get_mut(&root.signer()) else {
        return 0;
    };
    let before = member.seq;
    member.seq = before.max(root.unsigned().seq);
    before
}

/// A checkpoint must name the state of its own history.
fn checkpoint_matches(folder: &Folder<'_, '_>, position: usize, base: &State) -> Result<()> {
    let graph = folder.graph();
    let Body::Checkpoint(hash) = graph.order[position].body() else {
        return Ok(());
    };
    let state = fold(folder, &graph.ancestors[position], base);
    if state.hash()? == *hash {
        Ok(())
    } else {
        Err(Error::Checkpoint)
    }
}
