//! A paused file (the fleet holds the host) lets no member reach SSH, and
//! changes nothing of who may suspend whom.

use super::*;

fn paused(members: &[(u8, bool, u64)]) -> Access {
    let mut access = file(members, &[], &[]);
    access.paused = true;
    access
}

#[test]
fn a_paused_file_lets_no_one_reach_ssh_yet_admits_every_member() {
    let host = Host::new();
    let held = host.apply(paused(&[(1, true, 0), (2, false, 0)])).unwrap();
    assert!(!held.reaches_ssh(&id(1)));
    assert!(held.grants(&id(1)));
    assert!(held.admits(&id(1)) && held.admits(&id(2)));
    assert!(host.lines().is_empty());
}

#[test]
fn on_a_paused_host_a_denied_sender_cannot_suspend_an_otherwise_allowed_member() {
    let host = Host::new();
    // Member 2 is denied here, member 1 allowed, and member 3 denied.
    host.apply(paused(&[(1, true, 0), (2, false, 0), (3, false, 0)]))
        .unwrap();
    host.suspend(2, &[removal(0x51, &[1, 3])]);
    let held = Held::load(host.state()).unwrap();
    assert!(!held.suspended.contains_key(&id(1)));
    assert!(held.suspended.contains_key(&id(3)));
    // An allowed sender still suspends anyone.
    host.suspend(1, &[removal(0x52, &[2])]);
    assert!(
        Held::load(host.state())
            .unwrap()
            .suspended
            .contains_key(&id(2))
    );
}

#[test]
fn lifting_the_pause_lets_the_allowed_back_in() {
    let host = Host::new();
    host.apply(paused(&[(1, true, 0)])).unwrap();
    let held = host.apply(file(&[(1, true, 0)], &[], &[])).unwrap();
    assert!(held.reaches_ssh(&id(1)));
    assert_eq!(host.lines(), [id(1).short()]);
}
