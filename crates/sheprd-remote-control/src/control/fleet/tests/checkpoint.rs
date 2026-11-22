//! Checkpoints: naming the state of a history, pruning to one, and
//! starting a joining member's log from one.

use super::*;

fn with_history() -> (Log, Device, Device) {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    act(&mut log, &b, Body::HostAdd(plain_host(40)));
    (log, a, b)
}

#[test]
fn a_checkpoint_must_name_its_historys_state() {
    let (mut log, a, b) = with_history();
    let op = sign(&log, &a, Body::Checkpoint([7; 32]));
    assert_eq!(log.insert(op, NOW), Err(Error::Checkpoint));
    // Phones do not sign checkpoints.
    let p = phone(3);
    admit(&mut log, &a, &p);
    let hash = log.state().hash().unwrap();
    let op = sign(&log, &p, Body::Checkpoint(hash));
    assert_eq!(log.insert(op, NOW), Err(Error::Permission));
    let hash = log.state().hash().unwrap();
    act(&mut log, &b, Body::Checkpoint(hash));
}

#[test]
fn pruning_keeps_the_state_and_drops_the_history() {
    let (mut log, a, b) = with_history();
    let hash = log.state().hash().unwrap();
    let checkpoint = act(&mut log, &a, Body::Checkpoint(hash));
    act(&mut log, &b, Body::HostAdd(plain_host(41)));
    let before = log.state().clone();
    log.prune(&checkpoint.id()).unwrap();
    assert_eq!(log.state(), &before);
    assert_eq!(log.ops().count(), 1);
    assert_eq!(log.root().map(Op::id), Some(checkpoint.id()));
    // Work goes on on top of it, sequence numbers included.
    act(&mut log, &a, Body::HostAdd(plain_host(42)));
    let op = log.draft(b.id(), NOW, Body::HostRemove(plain_host(41).id().unwrap()));
    let mut stale = op.clone();
    stale.seq = 1;
    assert_eq!(
        log.insert(stale.sign(&b.key).unwrap(), NOW),
        Err(Error::Sequence)
    );
    apply(&mut log, op.sign(&b.key).unwrap());
    assert_eq!(log.state().hosts.len(), 4);
}

#[test]
fn pruning_waits_for_concurrent_operations() {
    let (mut log, a, b) = with_history();
    let before = log.heads().to_vec();
    let hash = log.state().hash().unwrap();
    let checkpoint = act(&mut log, &a, Body::Checkpoint(hash));
    let seq = log.state().members[&b.id()].seq + 1;
    let concurrent = sign_on(&log, &b, &before, seq, Body::HostAdd(plain_host(41)));
    apply(&mut log, concurrent);
    assert_eq!(log.prune(&checkpoint.id()), Err(Error::Checkpoint));
}

#[test]
fn a_joiner_starts_from_a_checkpoint_and_checks_what_follows() {
    let (mut log, a, b) = with_history();
    let hash = log.state().hash().unwrap();
    let checkpoint = act(&mut log, &a, Body::Checkpoint(hash));
    let handed = log.state_at(&checkpoint.id()).unwrap().encode().unwrap();
    let mut joined = Log::from_checkpoint(checkpoint.clone(), &handed).unwrap();
    assert_eq!(joined.state(), log.state());
    act(&mut log, &b, Body::HostAdd(plain_host(41)));
    let after: Vec<Op> = log
        .ops()
        .filter(|op| {
            !matches!(
                op.body(),
                Body::Found(_) | Body::Admit(_) | Body::Checkpoint(_)
            ) && op.parents().contains(&checkpoint.id())
        })
        .cloned()
        .collect();
    for (_, result) in joined.insert_all(after, NOW) {
        assert_eq!(result, Ok(Inserted::Applied));
    }
    assert_eq!(joined.state(), log.state());
    // A forged operation after it is refused there too.
    let outsider = computer(9);
    let op = sign(&joined, &outsider, Body::HostAdd(plain_host(42)));
    assert_eq!(joined.insert(op, NOW), Err(Error::Signer));
}

#[test]
fn a_checkpoint_handed_with_another_state_is_refused() {
    let (mut log, a, _) = with_history();
    let hash = log.state().hash().unwrap();
    let checkpoint = act(&mut log, &a, Body::Checkpoint(hash));
    let mut lie = log.state().clone();
    lie.tombstones.insert(MemberId([5; 32]));
    let lie = lie.encode().unwrap();
    assert_eq!(
        Log::from_checkpoint(checkpoint, &lie).unwrap_err(),
        Error::Checkpoint
    );
}
