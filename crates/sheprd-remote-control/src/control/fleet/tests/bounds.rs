//! The bounds on computers and hosts leave room for what they should: a
//! seat a removal freed, and a route only a host within the bound holds.

use super::*;

/// Adds as many plain hosts as the bound allows, in one batch, as `by`.
fn fill(log: &mut Log, by: &Device) {
    let mut parents = log.heads().to_vec();
    let first = log.state().members[&by.id()].seq + 1;
    let mut ops = Vec::new();
    for (seq, seed) in (first..).zip(0..=u8::MAX) {
        let op = Unsigned {
            fleet: log.fleet(),
            signer: by.id(),
            seq,
            issued_ms: NOW,
            parents: parents.clone(),
            body: Body::HostAdd(plain_host(seed)),
        }
        .sign(&by.key)
        .unwrap();
        parents = vec![op.id()];
        ops.push(op);
    }
    for (_, result) in log.insert_all(ops, NOW) {
        assert_eq!(result, Ok(Inserted::Applied));
    }
}

/// A Sheprd host on `route`.
fn on(seed: u8, route: &str) -> HostIntro {
    let (mut intro, ca) = ca_host(seed, HostKind::Sheprd);
    intro.record.route = route.into();
    intro.record_signature = Some(raw_signature(&ca, &intro.record.message().unwrap()));
    intro
}

/// `c`'s own host, moved onto `route` with a newer record.
fn moved_onto(c: &Device, route: &str) -> Body {
    let (mut own, ca) = ca_host(3, HostKind::Member);
    let host = own.id().unwrap();
    own.record = record(host, 3, 2);
    own.record.route = route.into();
    own.record_signature = Some(raw_signature(&ca, &own.record.message().unwrap()));
    let mut moved = c.bundle.clone();
    moved.host = Some(own);
    Body::Rotate(moved)
}

#[test]
fn a_replacement_takes_the_seat_a_removal_freed_pruned_or_not() {
    let a = computer(1);
    let mut log = found(&a);
    for seed in 2..=u8::try_from(MAX_COMPUTERS).unwrap() {
        admit(&mut log, &a, &computer(seed));
    }
    let hash = log.state().hash().unwrap();
    let checkpoint = act(&mut log, &a, Body::Checkpoint(hash));
    let mut whole = copy(&log);
    let mut pruned = copy(&log);
    pruned.prune(&checkpoint.id()).unwrap();
    let replacement = computer(200);
    for log in [&mut pruned, &mut whole] {
        act(log, &a, remove_body(&computer(2)));
        admit(log, &a, &replacement);
        assert!(log.state().members.contains_key(&replacement.id()));
        // And it signs.
        act(log, &replacement, Body::HostRemove(HostId([9; 32])));
    }
    assert_eq!(pruned.state(), whole.state());
}

#[test]
fn a_host_the_bound_left_out_holds_no_route() {
    let (a, c) = (computer(1), computer(3));
    let mut log = found(&a);
    admit(&mut log, &a, &c);
    fill(&mut log, &a);
    assert_eq!(log.state().hosts.len(), MAX_HOSTS + 2);
    // One more host, on a free route: left out by the bound.
    let extra = on(60, "free-route");
    act(&mut log, &a, Body::HostAdd(extra.clone()));
    assert!(!log.state().hosts.contains_key(&extra.id().unwrap()));
    // c moves its own host onto that route: it is c's.
    let host = c.bundle.host.as_ref().unwrap().id().unwrap();
    act(&mut log, &c, moved_onto(&c, "free-route"));
    let state = log.state();
    assert!(!state.hosts[&host].held);
    assert_eq!(state.routes["free-route"], host);
    assert!(state.allowed(&host, &a.id()));
}

#[test]
fn a_left_out_addition_does_not_clear_an_earlier_lifetimes_claim() {
    let (a, c) = (computer(1), computer(3));
    let mut log = found(&a);
    admit(&mut log, &a, &c);
    // h holds a route; c moves onto it after, and is held.
    let h = on(60, "taken-route");
    act(&mut log, &a, Body::HostAdd(h.clone()));
    let host = c.bundle.host.as_ref().unwrap().id().unwrap();
    act(&mut log, &c, moved_onto(&c, "taken-route"));
    assert!(log.state().hosts[&host].held);
    // h is removed, the hosts fill up, and h is added again past the bound.
    act(&mut log, &a, Body::HostRemove(h.id().unwrap()));
    fill(&mut log, &a);
    act(&mut log, &a, Body::HostAdd(h.clone()));
    assert!(!log.state().hosts.contains_key(&h.id().unwrap()));
    // The claim h made before its removal still held c's host.
    assert!(log.state().hosts[&host].held);
}

#[test]
fn removing_a_host_the_bound_kept_out_holds_nothing() {
    let (a, c) = (computer(1), computer(3));
    let mut log = found(&a);
    admit(&mut log, &a, &c);
    fill(&mut log, &a);
    // h, past the bound, names a free route; c moves onto it, unheld.
    let h = on(60, "free-route");
    act(&mut log, &a, Body::HostAdd(h.clone()));
    let host = c.bundle.host.as_ref().unwrap().id().unwrap();
    act(&mut log, &c, moved_onto(&c, "free-route"));
    assert!(!log.state().hosts[&host].held);
    // Removing h, which never counted, changes nothing for c.
    act(&mut log, &a, Body::HostRemove(h.id().unwrap()));
    let state = log.state();
    assert!(!state.hosts[&host].held);
    assert_eq!(state.routes["free-route"], host);
    // Nor does a host added once there is room again: that one counts.
    act(&mut log, &a, Body::HostRemove(plain_host(0).id().unwrap()));
    let next = on(61, "next-route");
    act(&mut log, &a, Body::HostAdd(next.clone()));
    assert!(log.state().hosts.contains_key(&next.id().unwrap()));
}
