//! Which hosts are live, and since when.
//!
//! Every host is live from an operation that added it: a `HostAdd`, or for
//! a computer's own host the `Found`, `Admit`, or `Rotate` that named it.
//! A removal of the host (`HostRemove`, or `Remove::also_hosts`) wins over
//! every addition it had not seen, a computer's own included, and only an
//! addition that saw every removal brings it back.
//!
//! The additions that survive are the host's lifetime ([`Lifetime`]):
//! what is said about the host (exclusions, links) counts when it came
//! after any of them, and what was said before them went with the
//! removal. The base stands for the additions a checkpoint replaced, so a
//! log pruned to it decides as the whole log.

use super::{
    Body, HostId, HostIntro, HostKind, MAX_HOSTS, MemberId, Op,
    graph::Graph,
    membership::Members,
    state::{Host, State},
};
use std::collections::{BTreeMap, BTreeSet};

/// The additions of a live host that survive: `None` for the base.
#[derive(Clone, Debug, Default)]
pub(super) struct Lifetime(Vec<Option<usize>>);

impl Lifetime {
    /// Whether the operation at `position` came after one of the
    /// additions, and so speaks of the host as it is now.
    pub(super) fn covers(&self, graph: &Graph<'_>, position: usize) -> bool {
        self.0
            .iter()
            .any(|added| added.is_none_or(|added| added == position || graph.sees(position, added)))
    }

    /// Whether the host was live in the base and nothing since removed it.
    pub(super) fn in_base(&self) -> bool {
        self.0.contains(&None)
    }
}

/// One way a host was added: from the base, which every operation
/// descends from, or by an operation.
struct Add<'a> {
    position: Option<usize>,
    intro: &'a HostIntro,
    added_by: MemberId,
    order: [u8; 32],
    /// The base held it: the additions that disagreed went with the
    /// history a checkpoint replaced, and still disagree.
    held: bool,
}

/// Sets `state.hosts` from the additions and removals in `effective`, on
/// top of `base`, and says where each live host's history starts, and
/// which additions the [`MAX_HOSTS`] bound left out: those of the current
/// lifetime of each host it left out. The `HostAdd`s in `held`
/// are an equivocating member's, and do not count.
pub(super) fn hosts(
    graph: &Graph<'_>,
    members: &Members,
    effective: &[(usize, &Op)],
    held: &BTreeSet<usize>,
    base: &State,
    state: &mut State,
) -> (BTreeMap<HostId, Lifetime>, BTreeSet<usize>) {
    let mut adds: BTreeMap<HostId, Vec<Add<'_>>> = BTreeMap::new();
    for (id, host) in &base.hosts {
        if host.intro.kind != HostKind::Member {
            adds.entry(*id).or_default().push(Add {
                position: None,
                intro: &host.intro,
                added_by: host.added_by,
                order: [0; 32],
                held: host.held,
            });
        }
    }
    let mut removals: BTreeMap<HostId, Vec<usize>> = BTreeMap::new();
    for &(position, op) in effective {
        match op.body() {
            Body::HostAdd(intro) if !held.contains(&position) => {
                if let Ok(id) = intro.id()
                    && intro.kind != HostKind::Member
                {
                    adds.entry(id).or_default().push(Add {
                        position: Some(position),
                        intro,
                        added_by: op.signer(),
                        order: op.id().0,
                        held: false,
                    });
                }
            }
            Body::HostRemove(host) => removals.entry(*host).or_default().push(position),
            Body::Remove(remove) if members.removals.contains_key(&position) => {
                for host in &remove.also_hosts {
                    removals.entry(*host).or_default().push(position);
                }
            }
            _ => {}
        }
    }
    // A computer's own host goes with it, as if removed there.
    for &(position, host) in &members.dropped {
        removals.entry(host).or_default().push(position);
    }
    // An addition survives only when it saw every removal: a removal wins
    // over every addition it is concurrent with.
    let survives = |id: &HostId, at: Option<usize>| {
        removals.get(id).is_none_or(|removed_by| {
            at.is_some_and(|at| removed_by.iter().all(|&removal| graph.sees(at, removal)))
        })
    };
    let void = bounded(graph, base, &adds, &removals);
    let mut anchors = BTreeMap::new();
    let mut live: Vec<(usize, HostId, Host)> = Vec::new();
    // Where each surviving addition was made; `None` for the base.
    let mut preceding: BTreeMap<HostId, Vec<(Option<usize>, &HostIntro)>> = BTreeMap::new();
    for (id, mut candidates) in adds {
        candidates.retain(|add| {
            survives(&id, add.position) && add.position.is_none_or(|at| !void.contains(&at))
        });
        // The first added gives the host its intro: the base's before any
        // other, so a pruned log picks the same one.
        candidates.sort_by_key(|add| (add.position.map_or(0, |at| at + 1), add.order));
        let Some(first) = candidates.first() else {
            continue;
        };
        preceding.insert(
            id,
            candidates
                .iter()
                .map(|add| (add.position, add.intro))
                .collect(),
        );
        let held = candidates.iter().any(|add| {
            add.held
                || add.intro.ca != first.intro.ca
                || add.intro.record.ssh_host_keys != first.intro.record.ssh_host_keys
        });
        anchors.insert(
            id,
            Lifetime(candidates.iter().map(|add| add.position).collect()),
        );
        live.push((
            first.position.map_or(0, |at| at + 1),
            id,
            Host {
                intro: first.intro.clone(),
                added_by: first.added_by,
                record: first.intro.record.clone(),
                record_signature: first.intro.record_signature,
                held,
                flagged: false,
            },
        ));
    }
    // Concurrent additions can each pass the bound with the other removed
    // in between: the first added are kept, and the rest's additions in
    // their current lifetime count for nothing either.
    live.sort_by_key(|(earliest, id, _)| (*earliest, *id));
    let mut excluded = void;
    excluded.extend(
        live.iter()
            .skip(MAX_HOSTS)
            .filter_map(|(_, id, _)| anchors.get(id))
            .flat_map(|lifetime: &Lifetime| lifetime.0.iter().flatten().copied()),
    );
    live.truncate(MAX_HOSTS);
    state.hosts = live.into_iter().map(|(_, id, host)| (id, host)).collect();
    anchors.retain(|id, _| state.hosts.contains_key(id));
    member_hosts(
        graph,
        members,
        base,
        &preceding,
        &survives,
        state,
        &mut anchors,
    );
    (anchors, excluded)
}

/// The `HostAdd`s that found [`MAX_HOSTS`] hosts live when they were made,
/// in graph order from the base: they never count, whatever is removed
/// later, so removing a host never brings back what its bound kept out.
/// The route pass skips them too.
fn bounded(
    graph: &Graph<'_>,
    base: &State,
    adds: &BTreeMap<HostId, Vec<Add<'_>>>,
    removals: &BTreeMap<HostId, Vec<usize>>,
) -> BTreeSet<usize> {
    // Additions and removals, in graph order; a removal at a place sorts
    // before an addition there (they are never one operation).
    let mut events: Vec<(usize, bool, HostId)> = Vec::new();
    for (id, candidates) in adds {
        events.extend(
            candidates
                .iter()
                .filter_map(|add| add.position.map(|at| (at, true, *id))),
        );
    }
    for (id, removed_by) in removals {
        events.extend(removed_by.iter().map(|&at| (at, false, *id)));
    }
    events.sort_unstable();
    let mut live: BTreeSet<HostId> = base
        .hosts
        .iter()
        .filter(|(_, host)| host.intro.kind != HostKind::Member)
        .map(|(id, _)| *id)
        .collect();
    let mut removed: BTreeMap<HostId, Vec<usize>> = BTreeMap::new();
    let mut void = BTreeSet::new();
    for (at, addition, id) in events {
        if !addition {
            live.remove(&id);
            removed.entry(id).or_default().push(at);
            continue;
        }
        let survives = removed
            .get(&id)
            .is_none_or(|removals| removals.iter().all(|&removal| graph.sees(at, removal)));
        if !survives || live.contains(&id) {
            continue;
        }
        if live.len() >= MAX_HOSTS {
            void.insert(at);
        } else {
            live.insert(id);
        }
    }
    void
}

/// A computer naming a host: its intro, its id, where it first named the
/// host (`Members::origins`), and its claims that survive.
type Claimant<'a> = (&'a HostIntro, MemberId, Option<usize>, Vec<Option<usize>>);

/// How early a computer named a host, for which one gives it its intro:
/// the base's intro first, then other computers from the base, then by
/// where each first named it, ties by member id. `origin` is the place of
/// the computer's first operation naming the host since it last named
/// another; `kept` says its intro is the one the base kept. The host fold
/// and the route pass both rank by this, so they pick the same intro.
pub(super) fn named_first(
    origin: Option<usize>,
    kept: bool,
    member: MemberId,
) -> (usize, MemberId) {
    let at = match origin {
        Some(at) => at + 2,
        None if kept => 0,
        None => 1,
    };
    (at, member)
}

/// A computer is its own host, from the operations that named it.
///
/// Two members naming one host is a copied intro: held until a person
/// picks. So is a member naming an added host with another account or
/// other SSH host keys, unless the addition came after the member first
/// named the host and saw it: only then is the addition the copy, and it
/// changes nothing, so no member can hold another's host by adding a copy
/// of it. An addition before or concurrent with it is weighed, so a bundle
/// cannot silently take over a host another member added.
fn member_hosts(
    graph: &Graph<'_>,
    members: &Members,
    base: &State,
    preceding: &BTreeMap<HostId, Vec<(Option<usize>, &HostIntro)>>,
    survives: &impl Fn(&HostId, Option<usize>) -> bool,
    state: &mut State,
    anchors: &mut BTreeMap<HostId, Lifetime>,
) {
    let mut named: BTreeMap<HostId, Vec<(&HostIntro, MemberId)>> = BTreeMap::new();
    for (id, member) in &members.live {
        if let Some(intro) = member.bundle.host.as_ref()
            && let Ok(host) = intro.id()
        {
            named.entry(host).or_default().push((intro, *id));
        }
    }
    for (id, claimants) in named {
        // Every live computer naming the host weighs on whether it is
        // held, whether or not its claim survives; only one whose claim
        // survives can give it its intro.
        let naming = claimants.len();
        let kept = base.hosts.get(&id).map(|host| &host.intro);
        // Where each named this host: the base, while the base had it
        // live, then each operation since. A computer's claim lives from
        // the first that saw every removal of the host.
        let mut claimants: Vec<Claimant<'_>> = claimants
            .into_iter()
            .map(|(intro, member)| {
                let origin = members.origins.get(&member).copied();
                let from_base = origin.is_none() && base.hosts.contains_key(&id);
                let surviving = from_base
                    .then_some(None)
                    .into_iter()
                    .chain(
                        members
                            .claims
                            .get(&member)
                            .into_iter()
                            .flatten()
                            .map(|&at| Some(at)),
                    )
                    .filter(|at| survives(&id, *at))
                    .collect();
                (intro, member, origin, surviving)
            })
            .filter(|(.., surviving): &Claimant<'_>| !surviving.is_empty())
            .collect();
        // The computer that named it first gives it its intro
        // ([`named_first`]); the others' intros are copies.
        claimants.sort_by_key(|(intro, member, origin, _)| {
            named_first(*origin, kept == Some(*intro), *member)
        });
        let Some((intro, member, origin, surviving)) = claimants.first().cloned() else {
            continue;
        };
        // Every addition weighed must name the same machine the same way:
        // one that differs, in account, CA, or SSH host keys, holds it.
        let weighed =
            preceding
                .get(&id)
                .into_iter()
                .flatten()
                .filter(|(at, _)| match (at, origin) {
                    (None, _) => true,
                    (Some(at), Some(origin)) => !graph.sees(*at, origin),
                    // A member from the base came before every addition here.
                    (Some(_), None) => false,
                });
        // A host the base held for an addition that disagreed stays held:
        // the addition went with the history a checkpoint replaced, and a
        // removal of the host is what would have dropped it. When two of
        // the base's members claimed it, that is why it was held, and the
        // claims are counted again from the members live.
        let carried = surviving.contains(&None)
            && base.hosts.get(&id).is_some_and(|kept| {
                kept.held
                    && kept.intro.kind == HostKind::Member
                    && base
                        .members
                        .values()
                        .filter(|other| {
                            other.bundle.host.as_ref().and_then(|intro| intro.id().ok()) == Some(id)
                        })
                        .count()
                        <= 1
            });
        let held = naming > 1
            || carried
            || weighed.into_iter().any(|(_, added)| {
                added.user != intro.user
                    || added.ca != intro.ca
                    || added.record.ssh_host_keys != intro.record.ssh_host_keys
            });
        anchors.insert(id, Lifetime(surviving));
        state.hosts.insert(
            id,
            Host {
                intro: intro.clone(),
                added_by: member,
                record: intro.record.clone(),
                record_signature: intro.record_signature,
                held,
                flagged: false,
            },
        );
    }
}
