//! Pruning to a checkpoint changes no decision: a held or flagged host
//! stays so, and the checkpoint still starts a joiner's log.

use super::*;

fn three() -> (Log, Device, Device, Device) {
    let (a, b, c) = (computer(1), computer(2), computer(3));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &c);
    (log, a, b, c)
}

/// Signs a checkpoint of everything `log` holds as `by`, and prunes to it.
fn prune(log: &mut Log, by: &Device) -> Op {
    let hash = log.state().hash().unwrap();
    let checkpoint = act(log, by, Body::Checkpoint(hash));
    let before = log.state().clone();
    log.prune(&checkpoint.id()).unwrap();
    assert_eq!(log.state(), &before);
    checkpoint
}

#[test]
fn pruning_keeps_a_disputed_host_held() {
    let (mut log, a, b, c) = three();
    let intro = plain_host(40);
    let host = intro.id().unwrap();
    let mut other = intro.clone();
    other.record.ssh_host_keys.push(ssh_line(251));
    let before = log.heads().to_vec();
    let next = |member: &Device| log.state().members[&member.id()].seq + 1;
    let ops = [
        sign_on(&log, &a, &before, next(&a), Body::HostAdd(intro.clone())),
        sign_on(&log, &b, &before, next(&b), Body::HostAdd(other)),
    ];
    for op in ops {
        apply(&mut log, op);
    }
    prune(&mut log, &a);
    act(&mut log, &b, Body::HostRemove(HostId([9; 32])));
    assert!(log.state().hosts[&host].held);
    assert!(!log.state().allowed(&host, &c.id()));
    // A removal settles it, and an addition after it is not held.
    act(&mut log, &a, Body::HostRemove(host));
    act(&mut log, &a, Body::HostAdd(intro));
    assert!(!log.state().hosts[&host].held);
}

#[test]
fn pruning_keeps_a_member_host_an_addition_disputes_held() {
    let (mut log, a, b, c) = three();
    let (intro, _) = ca_host(60, HostKind::Sheprd);
    let host = intro.id().unwrap();
    let mut copy = intro.clone();
    copy.kind = HostKind::Member;
    copy.user = "root".into();
    let mut joiner = computer(9);
    joiner.bundle.host = Some(copy);
    let before = log.heads().to_vec();
    let next = |member: &Device| log.state().members[&member.id()].seq + 1;
    let ops = [
        sign_on(&log, &a, &before, next(&a), Body::HostAdd(intro)),
        sign_on(&log, &b, &before, next(&b), admit_body(&log, &joiner)),
    ];
    for op in ops {
        apply(&mut log, op);
    }
    assert!(log.state().hosts[&host].held);
    prune(&mut log, &a);
    act(&mut log, &b, Body::HostRemove(HostId([9; 32])));
    assert!(log.state().hosts[&host].held);
    assert!(!log.state().allowed(&host, &c.id()));
}

#[test]
fn pruning_keeps_a_flag_raised_at_the_intros_rev() {
    let (mut log, a, b, _) = three();
    let (intro, ca) = ca_host(40, HostKind::Sheprd);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro));
    // Another record with the intro's own rev.
    let other = record(host, 61, 1);
    let signature = Some(raw_signature(&ca, &other.message().unwrap()));
    act(
        &mut log,
        &b,
        Body::HostSeen(HostSeen {
            record: other,
            signature,
        }),
    );
    assert!(log.state().hosts[&host].flagged);
    prune(&mut log, &a);
    act(&mut log, &b, Body::HostRemove(HostId([9; 32])));
    assert!(log.state().hosts[&host].flagged);
}

#[test]
fn a_pruned_checkpoint_still_starts_a_joiner() {
    let (mut log, a, b, _) = three();
    act(&mut log, &b, Body::HostAdd(plain_host(40)));
    let checkpoint = prune(&mut log, &a);
    let handed = log.state_at(&checkpoint.id()).unwrap().encode().unwrap();
    let mut joined = Log::from_checkpoint(checkpoint, &handed).unwrap();
    assert_eq!(joined.state(), log.state());
    let op = act(&mut log, &a, Body::HostAdd(plain_host(41)));
    assert_eq!(joined.insert(op, NOW), Ok(Inserted::Applied));
    assert_eq!(joined.state(), log.state());
}
