//! Removing members: fast, final, and safe against a member that races
//! or backdates.

use super::*;

/// a founds, admits b, c, and d; every member holds the same log.
fn four() -> (Log, Device, Device, Device, Device) {
    let (a, b, c, d) = (computer(1), computer(2), computer(3), computer(4));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &c);
    admit(&mut log, &a, &d);
    (log, a, b, c, d)
}

fn live(log: &Log) -> Vec<MemberId> {
    log.state().members.keys().copied().collect()
}

#[test]
fn a_removed_member_loses_every_grant_and_never_returns() {
    let (mut log, a, b, c, _) = four();
    let host_a = a.bundle.host.as_ref().unwrap().id().unwrap();
    assert!(log.state().allowed(&host_a, &c.id()));
    act(&mut log, &b, remove_body(&c));
    assert!(!log.state().members.contains_key(&c.id()));
    assert!(log.state().tombstones.contains(&c.id()));
    assert!(!log.state().allowed(&host_a, &c.id()));
    // Its own host goes with it.
    let host_c = c.bundle.host.as_ref().unwrap().id().unwrap();
    assert!(!log.state().hosts.contains_key(&host_c));
    // The same key is never admitted again.
    let op = sign(&log, &a, admit_body(&log, &c));
    assert_eq!(log.insert(op, NOW), Err(Error::Invalid));
    // And it signs nothing more.
    let op = sign(&log, &c, Body::HostAdd(plain_host(40)));
    assert_eq!(log.insert(op, NOW), Err(Error::Signer));
}

#[test]
fn a_member_may_leave() {
    let (mut log, _, b, _, _) = four();
    act(
        &mut log,
        &b,
        Body::Remove(Remove {
            member: b.id(),
            also_members: Vec::new(),
            also_hosts: Vec::new(),
            reason: Reason::Left,
        }),
    );
    assert!(!log.state().members.contains_key(&b.id()));
}

#[test]
fn an_addition_concurrent_with_its_signers_removal_is_void() {
    let (mut log, a, b, c, _) = four();
    let before = log.heads().to_vec();
    // c adds a host, not having seen its removal; a removes c, not having
    // seen the host.
    let seq = log.state().members[&c.id()].seq + 1;
    let added = sign_on(&log, &c, &before, seq, Body::HostAdd(plain_host(40)));
    let removed = sign(&log, &a, remove_body(&c));
    apply(&mut log, removed);
    // Applied, so others can build on it, but void.
    apply(&mut log, added);
    let host = plain_host(40).id().unwrap();
    assert!(!log.state().hosts.contains_key(&host));
    // b is unaffected.
    assert!(log.state().members.contains_key(&b.id()));
}

#[test]
fn a_backdated_operation_from_a_removed_member_is_void() {
    let (mut log, a, b, c, d) = four();
    let early = log.heads().to_vec();
    act(&mut log, &a, Body::HostAdd(plain_host(40)));
    act(&mut log, &a, remove_body(&c));
    // Long after, c signs on the old history, as if it had never been
    // removed: an admission, a host, and a removal of a member that did
    // not remove it.
    let seq = log
        .state()
        .members
        .get(&c.id())
        .map_or(10, |member| member.seq)
        + 10;
    let joiner = computer(9);
    let backdated = [
        sign_on(&log, &c, &early, seq, admit_body(&log, &joiner)),
        sign_on(&log, &c, &early, seq + 1, Body::HostAdd(plain_host(41))),
        sign_on(&log, &c, &early, seq + 2, remove_body(&d)),
    ];
    for op in backdated {
        let _ = log.insert(op, NOW);
    }
    let state = log.state();
    assert!(!state.members.contains_key(&joiner.id()));
    assert!(!state.hosts.contains_key(&plain_host(41).id().unwrap()));
    assert!(state.members.contains_key(&d.id()));
    assert!(state.members.contains_key(&b.id()));
}

#[test]
fn a_counter_removal_takes_both_and_nothing_more() {
    let (mut log, a, b, c, d) = four();
    let before = log.heads().to_vec();
    // a (the owner) removes c; c, not having seen it, removes a, and
    // also tries to remove b.
    let seq = log.state().members[&c.id()].seq + 1;
    let by_c = sign_on(&log, &c, &before, seq, remove_body(&a));
    let by_c_too = sign_on(&log, &c, &before, seq + 1, remove_body(&b));
    act(&mut log, &a, remove_body(&c));
    apply(&mut log, by_c);
    let _ = log.insert(by_c_too, NOW);
    assert_eq!(live(&log), {
        let mut expected = vec![b.id(), d.id()];
        expected.sort();
        expected
    });
}

#[test]
fn a_removed_attacker_cannot_reach_members_admitted_after() {
    let (mut log, a, _, c, _) = four();
    let before = log.heads().to_vec();
    act(&mut log, &a, remove_body(&c));
    let late = computer(9);
    admit(&mut log, &a, &late);
    // c cannot name a member its own history never had.
    let seq = 50;
    let op = sign_on(&log, &c, &before, seq, remove_body(&late));
    assert_eq!(log.insert(op, NOW), Err(Error::Unknown));
    assert!(log.state().members.contains_key(&late.id()));
}

#[test]
fn removals_that_void_each_other_in_a_cycle_all_take_effect() {
    let (mut log, a, b, c, d) = four();
    let before = log.heads().to_vec();
    // a removes b, b removes c, c removes a, none having seen the others:
    // no rule says who wins, so all three go.
    let next = |member: &Device| log.state().members[&member.id()].seq + 1;
    let ops = [
        sign_on(&log, &a, &before, next(&a), remove_body(&b)),
        sign_on(&log, &b, &before, next(&b), remove_body(&c)),
        sign_on(&log, &c, &before, next(&c), remove_body(&a)),
    ];
    for op in ops {
        apply(&mut log, op);
    }
    assert_eq!(live(&log), [d.id()]);
}

#[test]
fn concurrent_removals_of_others_both_take_effect() {
    let (mut log, a, b, c, d) = four();
    let before = log.heads().to_vec();
    let next = |member: &Device| log.state().members[&member.id()].seq + 1;
    let ops = [
        sign_on(&log, &a, &before, next(&a), remove_body(&c)),
        sign_on(&log, &b, &before, next(&b), remove_body(&d)),
    ];
    for op in ops {
        apply(&mut log, op);
    }
    let mut expected = vec![a.id(), b.id()];
    expected.sort();
    assert_eq!(live(&log), expected);
}

#[test]
fn a_removal_takes_what_the_remover_ticked() {
    let (mut log, a, b, c, d) = four();
    act(&mut log, &c, Body::HostAdd(plain_host(40)));
    let host = plain_host(40).id().unwrap();
    assert!(log.state().hosts.contains_key(&host));
    act(
        &mut log,
        &a,
        Body::Remove(Remove {
            member: c.id(),
            also_members: vec![d.id()],
            also_hosts: vec![host],
            reason: Reason::Compromised,
        }),
    );
    assert_eq!(live(&log), {
        let mut expected = vec![a.id(), b.id()];
        expected.sort();
        expected
    });
    assert!(!log.state().hosts.contains_key(&host));
}

#[test]
fn hosts_a_removed_member_added_stay_unless_ticked() {
    let (mut log, a, _, c, _) = four();
    act(&mut log, &c, Body::HostAdd(plain_host(40)));
    act(&mut log, &a, remove_body(&c));
    let host = plain_host(40).id().unwrap();
    assert_eq!(log.state().hosts[&host].added_by, c.id());
}

#[test]
fn a_phone_removes_a_lost_laptop() {
    let (mut log, a, b, _, _) = four();
    let p = phone(5);
    admit(&mut log, &a, &p);
    act(&mut log, &p, remove_body(&b));
    assert!(!log.state().members.contains_key(&b.id()));
}

#[test]
fn removing_one_member_twice_is_refused() {
    let (mut log, a, b, c, _) = four();
    let op = sign(
        &log,
        &a,
        Body::Remove(Remove {
            member: c.id(),
            also_members: vec![c.id()],
            also_hosts: Vec::new(),
            reason: Reason::Lost,
        }),
    );
    assert_eq!(log.insert(op, NOW), Err(Error::Unknown));
    act(&mut log, &a, remove_body(&c));
    let op = sign(&log, &b, remove_body(&c));
    assert_eq!(log.insert(op, NOW), Err(Error::Unknown));
}
