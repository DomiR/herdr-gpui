//! Hosts' self-records, and who owns each relay route.
//!
//! One pass over what touched hosts, in graph order, from the state the
//! base kept: additions (a `HostAdd`, or a computer's `Found`, `Admit`, or
//! `Rotate` naming its host), removals, and carried records. A route has
//! one owner: the host that claimed it first. An addition naming a route
//! another live host owns is held; a record naming one does not count and
//! flags its host, as a second record with one `rev` does. A removal frees
//! the host's routes and starts it over.
//!
//! Every step depends only on what came before it in graph order, never
//! on what came after. Everything before a checkpoint comes before it in
//! any log that holds it, and the state keeps where the pass stood there
//! (`State::routes`, and each host's record and flags), so a log pruned
//! to it decides as the whole log would.

use super::{
    Body, Bundle, HostId, HostIntro, HostKind, HostSeen, MemberId, Op, SelfRecord, State,
    body::RawSignature, graph::Graph, hosts::named_first, membership::Members,
};
use std::collections::{BTreeMap, BTreeSet};

/// A host as the pass has it so far.
struct Tracked<'a> {
    intro: &'a HostIntro,
    record: SelfRecord,
    signature: Option<RawSignature>,
    flagged: bool,
    /// Its intro named a route another host owned.
    held: bool,
    /// The computer whose own host it is, whose bundle gave the intro.
    by: Option<MemberId>,
}

impl<'a> Tracked<'a> {
    fn new(intro: &'a HostIntro) -> Self {
        Self {
            intro,
            record: intro.record.clone(),
            signature: intro.record_signature,
            flagged: false,
            held: false,
            by: None,
        }
    }
}

/// What the pass has seen so far.
struct Pass<'a> {
    hosts: BTreeMap<HostId, Tracked<'a>>,
    routes: BTreeMap<String, HostId>,
    /// The removals of each host so far, by place in the graph.
    removed: BTreeMap<HostId, Vec<usize>>,
    /// The host each computer names, as membership has it.
    own: BTreeMap<MemberId, Named<'a>>,
    /// The intro the base kept for each host.
    kept: BTreeMap<HostId, &'a HostIntro>,
    /// The additions of each host that count, since it was last removed;
    /// none for a host live since the base. A record counts only after
    /// one of them, so a host removed and added again starts its records
    /// over.
    lifetime: BTreeMap<HostId, Vec<usize>>,
}

/// A computer's claim on the host its bundle names.
struct Named<'a> {
    host: HostId,
    intro: &'a HostIntro,
    /// Its first operation naming this host since it last named another;
    /// none from the base (`Members::origins`).
    origin: Option<usize>,
    /// One of its claims saw every removal of the host so far.
    live: bool,
}

impl<'a> Pass<'a> {
    fn new(base: &'a State) -> Self {
        let hosts = base
            .hosts
            .iter()
            .map(|(id, host)| {
                let tracked = Tracked {
                    intro: &host.intro,
                    record: host.record.clone(),
                    signature: host.record_signature,
                    flagged: host.flagged,
                    held: false,
                    by: (host.intro.kind == HostKind::Member).then_some(host.added_by),
                };
                (*id, tracked)
            })
            .collect();
        let own = base
            .members
            .iter()
            .filter_map(|(member, entry)| {
                let intro = entry.bundle.host.as_ref()?;
                let host = intro.id().ok()?;
                let named = Named {
                    host,
                    intro,
                    origin: None,
                    live: base.hosts.contains_key(&host),
                };
                Some((*member, named))
            })
            .collect();
        Self {
            hosts,
            routes: base.routes.clone(),
            removed: BTreeMap::new(),
            own,
            kept: base
                .hosts
                .iter()
                .map(|(id, host)| (*id, &host.intro))
                .collect(),
            lifetime: BTreeMap::new(),
        }
    }

    /// Whether an addition of `id` at `position` saw every removal of it
    /// so far: one concurrent with a removal before it is void, as `hosts`
    /// has it.
    fn survives(&self, graph: &Graph<'_>, position: usize, id: HostId) -> bool {
        self.removed
            .get(&id)
            .is_none_or(|removals| removals.iter().all(|&at| graph.sees(position, at)))
    }

    /// A computer's bundle naming `id` at `position`.
    fn claim(
        &mut self,
        graph: &Graph<'_>,
        position: usize,
        member: MemberId,
        id: HostId,
        intro: &'a HostIntro,
    ) {
        let survives = self.survives(graph, position, id);
        if survives {
            self.added(id, position);
        }
        let named = match self.own.get(&member) {
            Some(named) if named.host == id => Named {
                host: id,
                intro,
                origin: named.origin,
                live: named.live || survives,
            },
            _ => Named {
                host: id,
                intro,
                origin: Some(position),
                live: survives,
            },
        };
        self.own.insert(member, named);
        self.settle(id);
    }

    /// An addition of `id` at `position` that counts, in its lifetime. A
    /// host live since the base keeps none: every record after the base
    /// counts for it, so a repeated addition must not narrow that. One
    /// added since gets each addition, concurrent ones too, so a record
    /// after any of them counts.
    fn added(&mut self, id: HostId, position: usize) {
        if self.hosts.contains_key(&id) && !self.lifetime.contains_key(&id) {
            return;
        }
        self.lifetime.entry(id).or_default().push(position);
    }

    /// A removal of `id` at `position`.
    fn removal(&mut self, position: usize, id: HostId) {
        self.removed.entry(id).or_default().push(position);
        self.remove(id);
    }

    /// Gives a computer's own host the intro of the computer that named it
    /// first ([`named_first`]), or drops it when none does now.
    fn settle(&mut self, id: HostId) {
        let kept = self.kept.get(&id).copied();
        let chosen = self
            .own
            .iter()
            .filter(|(_, named)| named.host == id && named.live)
            .min_by_key(|(member, named)| {
                named_first(named.origin, kept == Some(named.intro), **member)
            })
            .map(|(member, named)| (*member, named.intro));
        let Some((member, intro)) = chosen else {
            if self
                .hosts
                .get(&id)
                .is_some_and(|tracked| tracked.by.is_some())
            {
                self.remove(id);
            }
            return;
        };
        let tracked = self.hosts.entry(id).or_insert(Tracked::new(intro));
        if tracked.intro != intro {
            *tracked = Tracked {
                held: tracked.held,
                ..Tracked::new(intro)
            };
        }
        tracked.by = Some(member);
        self.claim_route(id);
    }

    /// The host gives up the routes its intro and record no longer name,
    /// and claims its intro's: held when another host owns it.
    fn claim_route(&mut self, id: HostId) {
        let Some(tracked) = self.hosts.get_mut(&id) else {
            return;
        };
        let route = &tracked.intro.record.route;
        let record = &tracked.record.route;
        self.routes
            .retain(|kept, owner| *owner != id || kept == route || kept == record);
        match self.routes.get(route) {
            Some(owner) if *owner != id => tracked.held = true,
            Some(_) => {}
            None => {
                self.routes.insert(route.clone(), id);
            }
        }
    }

    /// A `HostAdd` of `id` at `position`: it makes the host live when it
    /// was not; another addition of a live host changes nothing, its
    /// lifetime included. A host live since the base has no addition to
    /// see, so a record concurrent with a repeated one still counts.
    fn add(&mut self, graph: &Graph<'_>, position: usize, id: HostId, intro: &'a HostIntro) {
        if !self.survives(graph, position, id) {
            return;
        }
        self.added(id, position);
        if self.hosts.contains_key(&id) {
            return;
        }
        self.hosts.insert(id, Tracked::new(intro));
        self.claim_route(id);
    }

    /// A removal of `id`: it frees the host's routes, and the host starts
    /// over when added again.
    fn remove(&mut self, id: HostId) {
        self.hosts.remove(&id);
        self.lifetime.insert(id, Vec::new());
        self.routes.retain(|_, owner| *owner != id);
        for named in self.own.values_mut() {
            if named.host == id {
                named.live = false;
            }
        }
    }

    /// A carried record, if it is its host's word and its route is the
    /// host's own or free.
    fn seen(&mut self, graph: &Graph<'_>, position: usize, seen: &HostSeen) {
        let id = seen.record.host;
        // A record that had not seen the host's addition speaks of an
        // earlier lifetime of it.
        let after = self.lifetime.get(&id).is_none_or(|added| {
            added
                .iter()
                .any(|&at| at == position || graph.sees(position, at))
        });
        let Some(host) = self.hosts.get_mut(&id).filter(|_| after) else {
            return;
        };
        // A plain host's keys only change through a member that checked
        // them strictly, and a changed key cannot be checked that way. A
        // record that is not the host's word changes nothing, not even a
        // flag.
        let keys_kept =
            host.intro.ca.is_some() || seen.record.ssh_host_keys == host.intro.record.ssh_host_keys;
        if !keys_kept || !host.intro.vouches(&seen.record, seen.signature.as_ref()) {
            return;
        }
        if self
            .routes
            .get(&seen.record.route)
            .is_some_and(|owner| *owner != id)
        {
            host.flagged = true;
            return;
        }
        if seen.record.rev > host.record.rev {
            // The route it moves off is free again, unless its intro
            // names it.
            let old = std::mem::replace(&mut host.record, seen.record.clone());
            host.signature = seen.signature;
            if old.route != seen.record.route && old.route != host.intro.record.route {
                self.routes.remove(&old.route);
            }
            self.routes.entry(seen.record.route.clone()).or_insert(id);
        } else if seen.record.rev == host.record.rev && seen.record != host.record {
            host.flagged = true;
        }
    }
}

/// Applies the records `effective` carries to `state`'s hosts, holds those
/// whose intro names a route another host owns, and settles who owns each
/// route. `excluded` are the additions the [`super::MAX_HOSTS`] bound
/// left out, which claim nothing; earlier lifetimes of those hosts still
/// claimed what they did.
pub(super) fn records(
    graph: &Graph<'_>,
    members: &Members,
    effective: &[(usize, &Op)],
    held: &BTreeSet<usize>,
    excluded: &BTreeSet<usize>,
    base: &State,
    state: &mut State,
) {
    let mut pass = Pass::new(base);
    // Where a computer's own host went with it (`Members::dropped`).
    let mut dropped: BTreeMap<usize, Vec<HostId>> = BTreeMap::new();
    for &(position, host) in &members.dropped {
        dropped.entry(position).or_default().push(host);
    }
    for &(position, op) in effective {
        for &host in dropped.get(&position).into_iter().flatten() {
            pass.removal(position, host);
        }
        match op.body() {
            Body::HostAdd(intro) if intro.kind != HostKind::Member && !held.contains(&position) => {
                // An addition the bound left out (`hosts`) claims nothing.
                if let Ok(id) = intro.id()
                    && !excluded.contains(&position)
                {
                    pass.add(graph, position, id, intro);
                }
            }
            Body::Found(bundle) | Body::Rotate(bundle) if members.bundles.contains(&position) => {
                own(&mut pass, graph, position, bundle, bundle.id());
            }
            Body::Admit(admit) if members.bundles.contains(&position) => {
                own(&mut pass, graph, position, &admit.bundle, admit.bundle.id());
            }
            Body::HostRemove(id) => pass.removal(position, *id),
            Body::Remove(remove) => {
                let Some(reach) = members.removals.get(&position) else {
                    continue;
                };
                for member in reach {
                    pass.own.remove(member);
                }
                for id in &remove.also_hosts {
                    pass.removal(position, *id);
                }
            }
            Body::HostSeen(seen) => pass.seen(graph, position, seen),
            _ => {}
        }
    }
    for (id, host) in &mut state.hosts {
        let Some(tracked) = pass.hosts.get(id) else {
            continue;
        };
        if *tracked.intro != host.intro {
            continue;
        }
        host.record = tracked.record.clone();
        host.record_signature = tracked.signature;
        host.flagged = tracked.flagged;
        host.held |= tracked.held;
    }
    // Only the routes a live host names stay owned, so the state stays
    // bounded and a route a host moved off is free.
    pass.routes.retain(|route, owner| {
        state
            .hosts
            .get(owner)
            .is_some_and(|host| host.intro.record.route == *route || host.record.route == *route)
    });
    state.routes = pass.routes;
}

/// A computer's bundle naming its own host.
fn own<'a>(
    pass: &mut Pass<'a>,
    graph: &Graph<'_>,
    position: usize,
    bundle: &'a Bundle,
    member: MemberId,
) {
    match bundle.host.as_ref().map(|intro| (intro, intro.id())) {
        Some((intro, Ok(id))) => pass.claim(graph, position, member, id, intro),
        _ => {
            pass.own.remove(&member);
        }
    }
}
