//! A checkpoint every other operation has seen, or was seen by, settles
//! what came before it: the fold starts from the state it names, in a log
//! pruned to it and in the whole log alike, so nothing after can reach
//! back and change it.

use super::*;

fn three() -> (Log, Device, Device, Device) {
    let (a, b, c) = (computer(1), computer(2), computer(3));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &c);
    (log, a, b, c)
}

/// Checkpoints `log` as `by`, and returns a pruned copy and the whole one.
fn split(log: &mut Log, by: &Device) -> (Log, Log) {
    let hash = log.state().hash().unwrap();
    let checkpoint = act(log, by, Body::Checkpoint(hash));
    let whole = copy(log);
    let mut pruned = copy(log);
    pruned.prune(&checkpoint.id()).unwrap();
    assert_eq!(pruned.state(), whole.state());
    (pruned, whole)
}

/// Signs `body` as `by` on `pruned` and applies it to both.
fn both(pruned: &mut Log, whole: &mut Log, by: &Device, body: Body) {
    let op = act(pruned, by, body);
    assert_eq!(whole.insert(op, NOW), Ok(Inserted::Applied));
    assert_eq!(pruned.state(), whole.state());
}

#[test]
fn a_checkpoint_keeps_the_access_its_state_names() {
    let (mut log, a, b, c) = three();
    let host = plain_host(40).id().unwrap();
    act(&mut log, &a, Body::HostAdd(plain_host(40)));
    let access = |allow| {
        Body::Access(Access {
            host,
            member: b.id(),
            allow,
            rev: 0,
        })
    };
    act(&mut log, &a, access(false));
    // c allows, then signs in two places: its allow is held.
    act(&mut log, &c, access(true));
    let heads = log.heads().to_vec();
    let seq = log.state().members[&c.id()].seq + 1;
    for n in [1, 2] {
        let op = sign_on(&log, &c, &heads, seq, Body::HostRemove(HostId([n; 32])));
        apply(&mut log, op);
    }
    assert!(!log.state().allowed(&host, &b.id()));
    // a checkpoints that: the allow stays held, pruned or not.
    let (mut pruned, mut whole) = split(&mut log, &a);
    assert!(!whole.state().allowed(&host, &b.id()));
    both(
        &mut pruned,
        &mut whole,
        &a,
        Body::HostRemove(HostId([9; 32])),
    );
    assert!(!pruned.state().allowed(&host, &b.id()));
}

#[test]
fn a_removal_after_a_checkpoint_keeps_the_route_decisions_before_it() {
    let (mut log, a, b, _) = three();
    let x = computer(9);
    admit(&mut log, &a, &x);
    // A host on x's route, added after x: held.
    let route = x.bundle.host.as_ref().unwrap().record.route.clone();
    let (mut other, ca) = ca_host(41, HostKind::Sheprd);
    other.record.route = route;
    other.record_signature = Some(raw_signature(&ca, &other.record.message().unwrap()));
    act(&mut log, &b, Body::HostAdd(other.clone()));
    assert!(log.state().hosts[&other.id().unwrap()].held);
    let (mut pruned, mut whole) = split(&mut log, &a);
    both(&mut pruned, &mut whole, &a, remove_body(&x));
    assert!(pruned.state().hosts[&other.id().unwrap()].held);
}

#[test]
fn a_host_a_rotation_dropped_stays_dropped_when_its_computer_goes() {
    let (mut log, a, b, _) = three();
    let (intro, _) = ca_host(60, HostKind::Sheprd);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro.clone()));
    let mut same = intro;
    same.kind = HostKind::Member;
    let mut x = computer(9);
    x.bundle.host = Some(same);
    admit(&mut log, &a, &x);
    // x moves to a host of its own, which drops the one it joined as.
    let mut moved = x.bundle.clone();
    moved.host = computer(9).bundle.host;
    act(&mut log, &x, Body::Rotate(moved));
    assert!(!log.state().hosts.contains_key(&host));
    let (mut pruned, mut whole) = split(&mut log, &a);
    both(&mut pruned, &mut whole, &b, remove_body(&x));
    assert!(!pruned.state().hosts.contains_key(&host));
}

#[test]
fn a_record_from_before_a_removal_does_not_cross_into_the_next_lifetime() {
    for seed in 40..48u8 {
        let (mut log, a, b, _) = three();
        let (intro, ca) = ca_host(seed, HostKind::Sheprd);
        let host = intro.id().unwrap();
        act(&mut log, &a, Body::HostAdd(intro.clone()));
        let before = log.heads().to_vec();
        // b carries a newer record, not having seen the host removed and
        // added again.
        let mut newer = record(host, seed, 2);
        newer.route = "old-route".into();
        let signature = Some(raw_signature(&ca, &newer.message().unwrap()));
        let seq = log.state().members[&b.id()].seq + 1;
        let old = sign_on(
            &log,
            &b,
            &before,
            seq,
            Body::HostSeen(HostSeen {
                record: newer,
                signature,
            }),
        );
        act(&mut log, &a, Body::HostRemove(host));
        act(&mut log, &a, Body::HostAdd(intro.clone()));
        apply(&mut log, old);
        let state = log.state();
        assert_eq!(state.hosts[&host].record, intro.record, "seed {seed}");
        assert!(!state.routes.contains_key("old-route"), "seed {seed}");
    }
}

#[test]
fn a_record_after_either_concurrent_addition_counts() {
    for seed in 40..48u8 {
        let (log, a, b, _) = three();
        let (intro, ca) = ca_host(seed, HostKind::Sheprd);
        let host = intro.id().unwrap();
        let before = log.heads().to_vec();
        let next = |member: &Device| log.state().members[&member.id()].seq + 1;
        let by_a = sign_on(&log, &a, &before, next(&a), Body::HostAdd(intro.clone()));
        let by_b = sign_on(&log, &b, &before, next(&b), Body::HostAdd(intro.clone()));
        // b carries a newer record, having seen only its own addition.
        let newer = record(host, seed, 2);
        let signature = Some(raw_signature(&ca, &newer.message().unwrap()));
        let seen = sign_on(
            &log,
            &b,
            &[by_b.id()],
            next(&b) + 1,
            Body::HostSeen(HostSeen {
                record: newer.clone(),
                signature,
            }),
        );
        let mut log = log;
        for op in [by_a, by_b, seen] {
            apply(&mut log, op);
        }
        assert_eq!(log.state().hosts[&host].record, newer, "seed {seed}");
    }
}
