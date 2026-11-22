//! The pool for operations waiting on parents is bounded only after those
//! whose parents are present have left it, and says what it dropped.

use super::*;

/// `count` operations by a, each on the one before, after a's founding.
fn chain(count: usize) -> (Log, Vec<Op>) {
    let a = computer(1);
    let log = found(&a);
    let mut other = found(&a);
    let ops = (0..count)
        .map(|n| {
            let host = HostId(sha256(&[&n.to_be_bytes()]));
            act(&mut other, &a, Body::HostRemove(host))
        })
        .collect();
    (log, ops)
}

#[test]
fn a_whole_history_larger_than_the_pool_applies_in_one_batch() {
    let (mut log, ops) = chain(MAX_PENDING + 10);
    for (id, result) in log.insert_all(ops.clone(), NOW) {
        assert_eq!(result, Ok(Inserted::Applied), "{id:?}");
    }
    assert_eq!(log.pending(), 0);
    assert_eq!(log.ops().count(), ops.len() + 1);
}

#[test]
fn an_operation_dropped_from_the_pool_is_reported() {
    let (mut log, ops) = chain(MAX_PENDING + 2);
    // The first never arrives, so the rest wait, one too many.
    let results = log.insert_all(ops[1..].to_vec(), NOW);
    let pending = results
        .iter()
        .filter(|(_, result)| *result == Ok(Inserted::Pending))
        .count();
    let dropped = results
        .iter()
        .filter(|(_, result)| *result == Err(Error::Full))
        .count();
    assert_eq!((pending, dropped), (MAX_PENDING, 1));
    assert_eq!(log.pending(), MAX_PENDING);
}
