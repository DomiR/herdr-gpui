//! What plain hosts' agents are told: only removals that removed every
//! member they name, and members' keys versioned by their own rotations.

use super::*;
use crate::control::fleet::access::{self as host_access, Id};

fn access_body(host: HostId, member: &Device, allow: bool, rev: u64) -> Body {
    Body::Access(Access {
        host,
        member: member.id(),
        allow,
        rev,
    })
}

fn removing(targets: &[&Device]) -> Body {
    Body::Remove(Remove {
        member: targets[0].id(),
        also_members: targets[1..].iter().map(|target| target.id()).collect(),
        also_hosts: Vec::new(),
        reason: Reason::Compromised,
    })
}

#[test]
fn a_voided_removal_naming_a_member_another_removal_took_is_not_passed_to_agents() {
    let (a, b, c, d) = (computer(1), computer(2), computer(3), computer(4));
    let mut log = found(&a);
    for member in [&b, &c, &d] {
        admit(&mut log, &a, member);
    }
    // c removes a and b while a removes c: c's removal is void.
    let before = log.heads().to_vec();
    let seq = log.state().members[&c.id()].seq + 1;
    let voided = sign_on(&log, &c, &before, seq, removing(&[&a, &b]));
    act(&mut log, &a, removing(&[&c]));
    apply(&mut log, voided.clone());
    // d, having seen it all, removes a: a is gone, b stays.
    let taken = act(&mut log, &d, removing(&[&a]));
    let state = log.state();
    assert!(state.tombstones.contains(&a.id()));
    assert!(state.members.contains_key(&b.id()));
    assert!(!host_access::for_agents(state, &voided));
    assert!(host_access::for_agents(state, &taken));
}

#[test]
fn a_removal_suspending_several_members_counts_once_against_the_settled_bound() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    let intro = plain_host(40);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro));
    let stranger = computer(77);
    // Eight removals, each reported for eight members, then one more:
    // signed by no member, so every one is void.
    let asked = |removal: u8| host_access::Asked {
        removal: OpId([removal; 32]),
        signer: stranger.id(),
        seq: 1,
        target: b.id(),
    };
    let mut reported: Vec<_> = (0..8).flat_map(|_| (0x10..0x18).map(asked)).collect();
    reported.push(asked(0xf0));
    let file = host_access::compute(&log, &host, reported, |_| false).unwrap();
    assert_eq!(file.settled.len(), 9);
    assert!(file.settled.contains(&Id([0xf0; 32])));
}

#[test]
fn a_session_proof_is_the_fleet_key_signing_the_logs_domain_and_the_hash() {
    use p256::ecdsa::signature::Verifier as _;
    let (log, b) = {
        let (a, b) = (computer(1), computer(2));
        let mut log = found(&a);
        admit(&mut log, &a, &b);
        (log, b)
    };
    let hash = [9; 32];
    let proof = crate::control::fleet::removal::prove(&b.key, &hash);
    // The agent's wire signs the same bytes the log's domain names.
    let message = [DOMAIN, b"session".as_slice(), &hash].concat();
    let signature = p256::ecdsa::Signature::from_slice(&proof).unwrap();
    assert!(b.key.verifying_key().verify(&message, &signature).is_ok());
    assert!(crate::control::fleet::removal::proves(
        log.state(),
        &b.id(),
        &hash,
        &proof
    ));
    assert!(!crate::control::fleet::removal::proves(
        log.state(),
        &b.id(),
        &[8; 32],
        &proof
    ));
}

#[test]
fn an_agents_access_file_names_each_members_fleet_key() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    let intro = plain_host(40);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro));
    let file = host_access::compute(&log, &host, [], |_| false).unwrap();
    assert_eq!(file.members[&Id(b.id().0)].fleet_key, b.bundle.key);
}

/// a and b, and a plain host a added.
fn two() -> (Log, Device, Device, HostId) {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    let intro = plain_host(40);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro));
    (log, a, b, host)
}

fn rotated(member: &Device, tunnel: u8) -> Body {
    let mut bundle = member.bundle.clone();
    bundle.tunnel = [tunnel; 32];
    Body::Rotate(bundle)
}

fn keys_of(log: &Log, host: &HostId, member: &Device) -> host_access::Member {
    host_access::compute(log, host, [], |_| false)
        .unwrap()
        .members[&Id(member.id().0)]
        .clone()
}

#[test]
fn a_pruned_log_and_a_whole_log_give_the_same_key_version() {
    let (mut log, a, b, host) = two();
    act(&mut log, &b, rotated(&b, 0x62));
    act(&mut log, &b, access_body(host, &b, true, 1));
    let hash = log.state().hash().unwrap();
    let checkpoint = act(&mut log, &a, Body::Checkpoint(hash));
    let whole = copy(&log);
    log.prune(&checkpoint.id()).unwrap();
    let (pruned, whole) = (keys_of(&log, &host, &b), keys_of(&whole, &host, &b));
    assert_eq!((pruned.keys, pruned.tiebreak), (whole.keys, whole.tiebreak));
    assert_eq!(pruned.tunnel, [0x62; 32]);
}

#[test]
fn an_agent_keeps_a_pruned_logs_newer_keys_over_a_stale_whole_view() {
    let (mut log, a, b, host) = two();
    let stale = copy(&log);
    act(&mut log, &b, rotated(&b, 0x62));
    let hash = log.state().hash().unwrap();
    let checkpoint = act(&mut log, &a, Body::Checkpoint(hash));
    log.prune(&checkpoint.id()).unwrap();
    let held = host_access::Access::merge(
        None,
        host_access::compute(&log, &host, [], |_| false).unwrap(),
    )
    .unwrap();
    let after = host_access::Access::merge(
        Some(&held),
        host_access::compute(&stale, &host, [], |_| false).unwrap(),
    )
    .unwrap();
    assert_eq!(after.members[&Id(b.id().0)].tunnel, [0x62; 32]);
}

#[test]
fn an_operation_that_is_no_rotation_leaves_the_key_version_pruned_or_whole() {
    let (mut log, a, b, host) = two();
    let rotation = act(&mut log, &b, rotated(&b, 0x62));
    let version = |log: &Log| {
        let member = keys_of(log, &host, &b);
        (member.keys, member.tiebreak)
    };
    let after_rotation = version(&log);
    assert_eq!(after_rotation.0, rotation.unsigned().seq);
    act(&mut log, &b, access_body(host, &b, true, 1));
    act(&mut log, &b, access_body(host, &b, true, 2));
    assert_eq!(version(&log), after_rotation);
    let hash = log.state().hash().unwrap();
    let checkpoint = act(&mut log, &a, Body::Checkpoint(hash));
    let whole = copy(&log);
    log.prune(&checkpoint.id()).unwrap();
    act(&mut log, &b, access_body(host, &b, true, 3));
    assert_eq!(version(&log), after_rotation);
    assert_eq!(version(&whole), after_rotation);
}

#[test]
fn rotations_competing_at_one_number_are_ordered_by_their_bundle_in_every_view() {
    let (log, _, b, host) = two();
    let before = log.heads().to_vec();
    let seq = log.state().members[&b.id()].seq + 1;
    let first = sign_on(&log, &b, &before, seq, rotated(&b, 0x61));
    let second = sign_on(&log, &b, &before, seq, rotated(&b, 0x62));
    let view = |op: &Op| {
        let mut one = copy(&log);
        apply(&mut one, op.clone());
        keys_of(&one, &host, &b)
    };
    let (one, other) = (view(&first), view(&second));
    assert_eq!(one.keys, other.keys);
    assert_ne!(one.tiebreak, other.tiebreak);
    // An agent ends on the same keys whichever view it got first.
    let file = |member: &host_access::Member| {
        let mut file = host_access::compute(&log, &host, [], |_| false).unwrap();
        file.members.insert(Id(b.id().0), member.clone());
        file
    };
    let ends = |first: &host_access::Member, then: &host_access::Member| {
        let held = host_access::Access::merge(None, file(first)).unwrap();
        host_access::Access::merge(Some(&held), file(then))
            .unwrap()
            .members[&Id(b.id().0)]
            .tunnel
    };
    assert_eq!(ends(&one, &other), ends(&other, &one));
}
