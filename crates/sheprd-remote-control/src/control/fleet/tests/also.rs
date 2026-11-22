//! A removal takes out more than it names: a member one it removes
//! admitted at the same time. The log says so, and a plain host's agent,
//! which cannot work it out, is told with the removal.

use super::*;
use crate::control::fleet::access::also;

/// a, b, and c, computers.
fn three() -> (Log, Device, Device) {
    let (a, b, c) = (computer(1), computer(2), computer(3));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &c);
    (log, a, c)
}

#[test]
fn a_member_admitted_alongside_its_admitters_removal_goes_with_it() {
    let (mut log, a, c) = three();
    let before = log.heads().to_vec();
    // c admits x, not having seen its own removal; a removes c, not
    // having seen x.
    let x = computer(9);
    let seq = log.state().members[&c.id()].seq + 1;
    let admitted = sign_on(&log, &c, &before, seq, admit_body(&log, &x));
    apply(&mut log, admitted);
    assert!(log.state().members.contains_key(&x.id()));
    let removal = sign_on(
        &log,
        &a,
        &before,
        log.state().members[&a.id()].seq + 1,
        remove_body(&c),
    );
    apply(&mut log, removal.clone());
    let state = log.state();
    assert!(state.tombstones.contains(&c.id()));
    assert!(state.tombstones.contains(&x.id()), "x goes with c");
    // The log names both; the removal names only c.
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

#[test]
fn a_removal_that_takes_only_what_it_names_has_nothing_else_to_say() {
    let (mut log, a, c) = three();
    let removal = act(&mut log, &a, remove_body(&c));
    let removed_by = log.removed_by().unwrap();
    assert!(also(&removed_by, &[removal]).is_empty());
}
