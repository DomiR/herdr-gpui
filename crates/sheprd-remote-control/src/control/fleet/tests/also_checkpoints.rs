//! What a removal takes beyond those it names is still known behind a
//! later checkpoint the log has not pruned to.

use super::*;
use crate::control::fleet::access::also;

#[test]
fn a_checkpoint_after_a_removal_keeps_what_it_takes_beyond_its_names() {
    let (a, b, c) = (computer(1), computer(2), computer(3));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &c);
    let before = log.heads().to_vec();
    let x = computer(9);
    let seq = log.state().members[&c.id()].seq + 1;
    let admitted = sign_on(&log, &c, &before, seq, admit_body(&log, &x));
    apply(&mut log, admitted);
    let removal = sign_on(
        &log,
        &a,
        &before,
        log.state().members[&a.id()].seq + 1,
        remove_body(&c),
    );
    apply(&mut log, removal.clone());
    assert!(log.state().tombstones.contains(&x.id()));
    // Later checkpoints, each seen by everything after it.
    for (by, host) in [(&a, 40), (&b, 41)] {
        let hash = log.state().hash().unwrap();
        act(&mut log, by, Body::Checkpoint(hash));
        act(&mut log, by, Body::HostAdd(plain_host(host)));
    }
    let removed_by = log.removed_by().unwrap();
    assert_eq!(
        removed_by[&removal.id()],
        [c.id(), x.id()].into_iter().collect()
    );
    assert_eq!(
        also(&removed_by, std::slice::from_ref(&removal)),
        [(removal.id(), x.id())]
    );
}
