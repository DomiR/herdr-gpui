//! Computing the state of a set of operations: members first
//! (`membership`), then hosts, access, and links, each resolved toward the
//! safer side when concurrent operations disagree.

use super::{
    Body, HostId, Kind, MemberId, Op,
    graph::Bits,
    hosts::{self, Lifetime},
    membership::Folder,
    routes,
    state::{AccessRegister, Member, State},
};
use std::collections::{BTreeMap, BTreeSet};

/// The state of the down-closed set `set`, on top of `base`.
pub(super) fn fold(folder: &Folder<'_, '_>, set: &Bits, base: &State) -> State {
    let graph = folder.graph();
    let members = folder.members(set);
    // What came before the set's barrier is settled in the state there.
    let (set, settled) = folder.split(set);
    let base = settled.as_ref().map_or(base, |(_, state)| state.as_ref());
    let set = &set;
    let positions: Vec<usize> = set
        .iter()
        .filter(|&position| folder.check(position).is_ok())
        .collect();
    let equivocating = equivocating(folder, &positions, base);
    // Operations that count: checked, and not voided by a removal of their
    // signer that had not seen them. Removals were settled with members.
    let effective: Vec<(usize, &Op)> = positions
        .iter()
        .map(|&position| (position, graph.order[position]))
        .filter(|(position, op)| op.body().membership() || !folder.voided(&members, *position))
        .collect();
    let mut state = State {
        members: members.live.clone(),
        tombstones: members.tombstones.clone(),
        equivocating,
        ..State::default()
    };
    for (_, op) in &effective {
        if let Some(member) = state.members.get_mut(&op.signer()) {
            member.seq = member.seq.max(op.unsigned().seq);
        }
    }
    let held = held(folder, &positions, &state.equivocating);
    let (anchors, excluded) = hosts::hosts(graph, &members, &effective, &held, base, &mut state);
    routes::records(
        graph, &members, &effective, &held, &excluded, base, &mut state,
    );
    access(
        folder, &positions, &anchors, &effective, &held, base, &mut state,
    );
    links(folder, &anchors, &effective, base, &mut state);
    state
}

/// The operations of members found `equivocating`, whose `HostAdd` and
/// `Access` are held. Only those after the set's barrier are folded at
/// all: what came before is settled in the state there.
fn held(
    folder: &Folder<'_, '_>,
    positions: &[usize],
    equivocating: &BTreeSet<MemberId>,
) -> BTreeSet<usize> {
    let graph = folder.graph();
    positions
        .iter()
        .copied()
        .filter(|&position| equivocating.contains(&graph.order[position].signer()))
        .collect()
}

/// Members with two operations neither had seen.
fn equivocating(folder: &Folder<'_, '_>, positions: &[usize], base: &State) -> BTreeSet<MemberId> {
    let graph = folder.graph();
    let mut by_signer: BTreeMap<MemberId, Vec<usize>> = BTreeMap::new();
    for &position in positions {
        by_signer
            .entry(graph.order[position].signer())
            .or_default()
            .push(position);
    }
    let mut found = base.equivocating.clone();
    for (signer, mut ops) in by_signer {
        // A signer's sequence numbers climb along its own chain, so its
        // operations in sequence order each see the one before, unless
        // the chain forked: then two neighbours are concurrent.
        ops.sort_by_key(|&position| (graph.order[position].unsigned().seq, position));
        if ops.windows(2).any(|pair| !graph.sees(pair[1], pair[0])) {
            found.insert(signer);
        }
    }
    found
}

/// One counted `Access`: its place, whether it allows, and its `rev`.
type Grant = (usize, bool, u64);

/// Sets the access registers. An `Access` counts when it is not void, its
/// signer is not equivocating (unless a checkpoint saw it, [`held`]), and it came after the host was added
/// (any addition in its [`Lifetime`]): a host added again starts without the exclusions
/// it had, as a pruned log, which dropped them with the host, does.
/// Concurrent allow and deny: deny. `state.access_rev` is the highest
/// `rev` any `Access` reached, voided or not: the floor the next signs on.
fn access(
    folder: &Folder<'_, '_>,
    positions: &[usize],
    anchors: &BTreeMap<HostId, Lifetime>,
    effective: &[(usize, &Op)],
    held: &BTreeSet<usize>,
    base: &State,
    state: &mut State,
) {
    let graph = folder.graph();
    state.access_rev = positions
        .iter()
        .filter_map(|&position| match graph.order[position].body() {
            Body::Access(access) => Some(access.rev),
            _ => None,
        })
        .fold(base.access_rev, u64::max);
    let live = |host: &HostId, member: &MemberId| {
        state
            .members
            .get(member)
            .is_some_and(|person| person.bundle.kind == Kind::Computer)
            && anchors.contains_key(host)
    };
    let mut by_pair: BTreeMap<(HostId, MemberId), Vec<Grant>> = BTreeMap::new();
    for &(position, op) in effective {
        let Body::Access(access) = op.body() else {
            continue;
        };
        let after = anchors
            .get(&access.host)
            .is_some_and(|lifetime| lifetime.covers(graph, position));
        if after && !held.contains(&position) {
            by_pair
                .entry((access.host, access.member))
                .or_default()
                .push((position, access.allow, access.rev));
        }
    }
    let mut registers: BTreeMap<(HostId, MemberId), AccessRegister> = base
        .access
        .iter()
        .filter(|((host, member), _)| {
            live(host, member) && anchors.get(host).is_some_and(Lifetime::in_base)
        })
        .map(|(pair, register)| (*pair, *register))
        .collect();
    for (pair, entries) in by_pair {
        if !live(&pair.0, &pair.1) {
            continue;
        }
        let latest: Vec<&Grant> = entries
            .iter()
            .filter(|(position, ..)| {
                !entries
                    .iter()
                    .any(|(other, ..)| graph.sees(*other, *position))
            })
            .collect();
        registers.insert(
            pair,
            AccessRegister {
                allow: latest.iter().all(|(_, allow, _)| *allow),
                rev: latest
                    .iter()
                    .map(|(.., rev)| *rev)
                    .max()
                    .unwrap_or_default(),
            },
        );
    }
    state.access = registers;
}

/// Sets each phone's device key per host: the latest it signed after the
/// host was added (its [`Lifetime`]).
fn links(
    folder: &Folder<'_, '_>,
    anchors: &BTreeMap<HostId, Lifetime>,
    effective: &[(usize, &Op)],
    base: &State,
    state: &mut State,
) {
    let graph = folder.graph();
    let phone = |member: &MemberId| {
        state
            .members
            .get(member)
            .is_some_and(|member: &Member| member.bundle.kind == Kind::Phone)
    };
    let mut links: BTreeMap<(MemberId, HostId), Vec<u8>> = base
        .links
        .iter()
        .filter(|((member, host), _)| {
            phone(member) && anchors.get(host).is_some_and(Lifetime::in_base)
        })
        .map(|(key, device)| (*key, device.clone()))
        .collect();
    let mut latest: BTreeMap<(MemberId, HostId), usize> = BTreeMap::new();
    for &(position, op) in effective {
        let Body::Link(link) = op.body() else {
            continue;
        };
        let after = anchors
            .get(&link.host)
            .is_some_and(|lifetime| lifetime.covers(graph, position));
        if !after || !phone(&op.signer()) {
            continue;
        }
        let key = (op.signer(), link.host);
        // One phone signs every link it has; the later one wins, and of
        // two it signed concurrently, the higher sequence number.
        let newer = latest.get(&key).is_none_or(|&kept| {
            graph.sees(position, kept)
                || (!graph.sees(kept, position)
                    && op.unsigned().seq > graph.order[kept].unsigned().seq)
        });
        if newer {
            latest.insert(key, position);
            links.insert(key, link.device_key.clone());
        }
    }
    state.links = links;
}
