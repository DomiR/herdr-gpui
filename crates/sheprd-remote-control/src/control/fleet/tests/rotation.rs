//! A rotation that keeps a member's host keeps its claim on it, so a copy
//! that came after the claim stays ignored.

use super::*;

#[test]
fn a_rotation_does_not_revive_an_ignored_copy() {
    let (a, b, c) = (computer(1), computer(2), computer(3));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &c);
    let host = c.bundle.host.as_ref().unwrap().id().unwrap();
    // b adds a copy of c's host after seeing c's bundle: ignored.
    let mut copy = c.bundle.host.clone().unwrap();
    copy.kind = HostKind::Sheprd;
    copy.user = "root".into();
    act(&mut log, &b, Body::HostAdd(copy));
    assert!(!log.state().hosts[&host].held);
    // c renames itself and changes its tunnel key; its host is the same.
    let mut rotated = c.bundle.clone();
    rotated.name = "renamed".into();
    rotated.tunnel = [77; 32];
    act(&mut log, &c, Body::Rotate(rotated));
    assert!(!log.state().hosts[&host].held);
    assert!(log.state().allowed(&host, &a.id()));
}
