//! A log recovers from what fills or damages it: a full log still takes
//! the checkpoint that frees it, and a damaged copy of an operation does
//! not keep the genuine one out.

use super::*;

/// `count` operations by `by`, each on the one before, starting on
/// `log`'s heads.
fn chain(log: &Log, by: &Device, count: usize) -> Vec<Op> {
    let mut parents = log.heads().to_vec();
    let mut seq = log.state().members[&by.id()].seq;
    (0..count)
        .map(|n| {
            seq += 1;
            let op = Unsigned {
                fleet: log.fleet(),
                signer: by.id(),
                seq,
                issued_ms: NOW,
                parents: parents.clone(),
                body: Body::HostRemove(HostId(sha256(&[&n.to_be_bytes()]))),
            }
            .sign(&by.key)
            .unwrap();
            parents = vec![op.id()];
            op
        })
        .collect()
}

#[test]
fn a_full_log_still_checkpoints_and_prunes() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    // Exactly full: the founding, the admission, and the chain.
    let ops = chain(&log, &b, MAX_OPS - 2);
    for (_, result) in log.insert_all(ops, NOW) {
        assert_eq!(result, Ok(Inserted::Applied));
    }
    let op = sign(&log, &a, Body::HostRemove(HostId([7; 32])));
    assert_eq!(log.insert(op, NOW), Err(Error::Full));
    let hash = log.state().hash().unwrap();
    let checkpoint = act(&mut log, &a, Body::Checkpoint(hash));
    log.prune(&checkpoint.id()).unwrap();
    assert_eq!(log.ops().count(), 0);
    act(&mut log, &a, Body::HostRemove(HostId([7; 32])));
}

#[test]
fn a_damaged_copy_does_not_keep_out_the_genuine_one() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    let ops = chain(&log, &b, 2);
    let (parent, genuine) = (ops[0].clone(), ops[1].clone());
    let mut bytes = genuine.encode();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    let damaged = Op::decode(&bytes).unwrap();
    assert_eq!(damaged.id(), genuine.id());
    // Both wait for the parent; the damaged one arrived first.
    assert_eq!(log.insert(damaged, NOW), Ok(Inserted::Pending));
    assert_eq!(log.insert(genuine.clone(), NOW), Ok(Inserted::Pending));
    apply(&mut log, parent);
    assert_eq!(log.get(&genuine.id()), Some(&genuine));
    assert_eq!(log.pending(), 0);
}
