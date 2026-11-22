//! A plain host's access file, as the log computes it, and the agent's
//! reading of operations and frames checked against the log's own.

use super::*;
use crate::control::fleet::access::{self as host_access, Asked, Id};
use sheprd_relay_agent::removal;

fn access_body(host: HostId, member: &Device, allow: bool, rev: u64) -> Body {
    Body::Access(Access {
        host,
        member: member.id(),
        allow,
        rev,
    })
}

/// a, b, and c, computers, a phone, and a plain host a added.
fn fleet() -> (Log, [Device; 3], HostId) {
    let (a, b, c) = (computer(1), computer(2), computer(3));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &c);
    admit(&mut log, &a, &phone(9));
    let intro = plain_host(40);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro));
    (log, [a, b, c], host)
}

fn id(member: &Device) -> Id {
    Id(member.id().0)
}

#[test]
fn every_computer_is_named_with_its_keys_and_access() {
    let (mut log, [a, b, c], host) = fleet();
    act(&mut log, &a, access_body(host, &c, false, 1));
    let file = host_access::compute(&log, &host, [], never).unwrap();
    assert_eq!(file.fleet, Id(log.fleet().0));
    assert_eq!(file.members.len(), 3, "phones are not named");
    let first = &file.members[&id(&a)];
    assert!(first.allow);
    assert_eq!(first.rev, 0);
    assert_eq!(first.dial, a.bundle.dial);
    assert_eq!(first.tunnel, a.bundle.tunnel);
    assert_eq!(first.ssh.as_deref(), a.bundle.ssh.as_deref());
    assert!(file.members[&id(&b)].allow);
    let excluded = &file.members[&id(&c)];
    assert!(!excluded.allow);
    assert_eq!(excluded.rev, 1);
    assert!(file.removed.is_empty());
}

#[test]
fn removed_members_are_tombstones_and_named_no_more() {
    let (mut log, [a, b, _], host) = fleet();
    act(&mut log, &a, remove_body(&b));
    let file = host_access::compute(&log, &host, [], never).unwrap();
    assert!(file.removed.contains(&id(&b)));
    assert!(!file.members.contains_key(&id(&b)));
}

/// No removal has stayed unknown long enough.
fn never(_: &OpId) -> bool {
    false
}

/// `removal`, signed by `signer` at `seq`, suspending `target`.
fn asked(removal: OpId, signer: &Device, seq: u64, target: &Device) -> Asked {
    Asked {
        removal,
        signer: signer.id(),
        seq,
        target: target.id(),
    }
}

fn settled(file: host_access::Access) -> Vec<Id> {
    file.settled.into_iter().collect()
}

#[test]
fn an_operation_the_log_holds_is_settled_whatever_it_is() {
    let (mut log, [a, b, c], host) = fleet();
    let removal = act(&mut log, &a, remove_body(&b));
    let other = act(&mut log, &a, access_body(host, &a, true, 1));
    let file = host_access::compute(
        &log,
        &host,
        [asked(removal.id(), &a, 1, &b), asked(other.id(), &a, 1, &c)],
        never,
    )
    .unwrap();
    let mut wanted = vec![Id(removal.id().0), Id(other.id().0)];
    wanted.sort();
    assert_eq!(settled(file), wanted);
}

#[test]
fn an_unknown_removal_is_settled_on_evidence_it_is_no_members() {
    let (log, [a, b, _], host) = fleet();
    let unknown = OpId([0x42; 32]);
    let seq = log.state().members[&a.id()].seq;
    let stranger = computer(77);
    for (signer, seq, target) in [
        // Its signer is no member.
        (&stranger, seq + 5, &b),
        // Its target is no member.
        (&a, seq + 5, &stranger),
        // The log holds a later operation of its signer, and not it.
        (&a, seq, &b),
    ] {
        let file = host_access::compute(&log, &host, [asked(unknown, signer, seq, target)], never)
            .unwrap();
        assert_eq!(settled(file), [Id(unknown.0)]);
    }
}

#[test]
fn an_unknown_removal_that_may_be_on_its_way_waits_for_the_grace() {
    let (log, [a, b, _], host) = fleet();
    let unknown = OpId([0x42; 32]);
    // A sequence number past what the log holds of its signer.
    let seq = log.state().members[&a.id()].seq + 1;
    let ask = || [asked(unknown, &a, seq, &b)];
    let waiting = host_access::compute(&log, &host, ask(), never).unwrap();
    assert!(waiting.settled.is_empty());
    let graced = host_access::compute(&log, &host, ask(), |id| *id == unknown).unwrap();
    assert_eq!(settled(graced), [Id(unknown.0)]);
}

#[test]
fn a_removal_waiting_for_its_parents_is_never_settled() {
    let (mut log, [a, b, c], host) = fleet();
    let mut other = copy(&log);
    admit(&mut other, &b, &computer(5));
    let removal = act(&mut other, &b, remove_body(&c));
    assert_eq!(log.insert(removal.clone(), NOW), Ok(Inserted::Pending));
    let seq = removal.unsigned().seq;
    let file =
        host_access::compute(&log, &host, [asked(removal.id(), &b, seq, &c)], |_| true).unwrap();
    assert!(file.settled.is_empty());
    drop(a);
}

#[test]
fn only_a_known_plain_host_has_an_access_file() {
    let (mut log, [a, ..], _) = fleet();
    assert_eq!(
        host_access::compute(&log, &HostId([7; 32]), [], never),
        Err(Error::Unknown)
    );
    let (intro, _) = ca_host(41, HostKind::Sheprd);
    let sheprd = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro));
    assert_eq!(
        host_access::compute(&log, &sheprd, [], never),
        Err(Error::Invalid)
    );
}

#[test]
fn a_host_two_members_added_with_different_keys_is_paused() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    let intro = plain_host(40);
    let host = intro.id().unwrap();
    let mut other = intro.clone();
    other.record.ssh_host_keys.push(ssh_line(251));
    let before = log.heads().to_vec();
    let next = |member: &Device| log.state().members[&member.id()].seq + 1;
    let ops = [
        sign_on(&log, &a, &before, next(&a), Body::HostAdd(intro)),
        sign_on(&log, &b, &before, next(&b), Body::HostAdd(other)),
    ];
    for op in ops {
        apply(&mut log, op);
    }
    let file = host_access::compute(&log, &host, [], never).unwrap();
    assert!(file.paused);
    // Everyone is still named, so removals and denials land; no one
    // reaches SSH while it is held.
    assert_eq!(file.members.len(), 2);
    assert_eq!(file.allowed().count(), 0);
    // A file from before the hold, merged under it, grants nothing.
    let mut before = file.clone();
    before.paused = false;
    let merged = host_access::Access::merge(Some(&before), file).unwrap();
    assert_eq!(merged.allowed().count(), 0);
}

#[test]
fn a_stale_members_file_cannot_undo_a_removal_on_the_agent() {
    let (mut log, [a, b, _], host) = fleet();
    let stale = copy(&log);
    act(&mut log, &a, remove_body(&b));
    let fresh = host_access::compute(&log, &host, [], never).unwrap();
    let held = host_access::Access::merge(None, fresh).unwrap();
    let old = host_access::compute(&stale, &host, [], never).unwrap();
    let merged = host_access::Access::merge(Some(&held), old).unwrap();
    assert!(!merged.members.contains_key(&id(&b)));
    assert_eq!(merged.digest().unwrap(), held.digest().unwrap());
}

#[test]
fn a_file_the_agent_merged_has_the_digest_the_log_computes() {
    let (log, _, host) = fleet();
    let file = host_access::compute(&log, &host, [], never).unwrap();
    let held = host_access::Access::merge(None, file.clone()).unwrap();
    assert_eq!(held.digest().unwrap(), file.digest().unwrap());
}

#[test]
fn the_agent_reads_a_removal_as_the_log_encodes_it() {
    let (mut log, [a, b, c], _) = fleet();
    let body = Body::Remove(Remove {
        member: b.id(),
        also_members: vec![c.id()],
        also_hosts: vec![HostId([5; 32])],
        reason: Reason::Lost,
    });
    let op = act(&mut log, &a, body);
    let read = removal::peek(&op.encode()).unwrap().unwrap();
    assert_eq!(read.id, op.id().0);
    assert_eq!(read.fleet, log.fleet().0);
    assert_eq!(read.signer, a.id().0);
    assert_eq!(read.targets, [b.id().0, c.id().0]);
}

#[test]
fn the_agent_reads_every_other_operation_as_no_removal() {
    let (log, ..) = fleet();
    for op in log.ops() {
        assert_eq!(removal::peek(&op.encode()), Ok(None), "{:?}", op.body());
    }
}

#[test]
fn a_frame_of_operation_ids_reads_back() {
    let message = removal::Message::Held(vec![OpId([3; 32]), OpId([4; 32])]);
    let bytes = message.encode().unwrap();
    assert_eq!(
        crate::control::fleet::removal::Message::decode(&bytes).unwrap(),
        message
    );
    assert_eq!(
        crate::control::fleet::removal::Message::decode(&bytes[..3]),
        Err(removal::Error::Encoding)
    );
}
