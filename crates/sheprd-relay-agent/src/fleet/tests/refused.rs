//! A file handed over that does not read or check out takes the fleet's
//! access away until a good one arrives: the file held before never goes
//! on granting.

use super::*;

fn allowed_first(host: &Host) -> Access {
    let good = file(&[(1, true, 0), (2, true, 0)], &[], &[]);
    host.apply(good.clone()).unwrap();
    assert_eq!(host.lines().len(), 2);
    good
}

fn shut_out(host: &Host) {
    assert!(host.lines().is_empty(), "{:?}", host.lines());
    let held = Held::load(host.state()).unwrap();
    assert!(held.refused);
    assert!(!held.admits(&id(1)));
    assert!(!held.reaches_ssh(&id(1)));
    let admitted = host.admitted();
    assert_eq!(admitted.peer(&dial(1), &[1; 32]), None);
    assert!(!admitted.admits_member(&id(1)));
    let report = held.report(&Authorized::default()).unwrap().unwrap();
    assert!(report.refused);
    assert_eq!(report.allowed, 0);
}

#[test]
fn an_unparsable_access_file_removes_fleet_ssh_access() {
    let host = Host::new();
    let good = allowed_first(&host);
    let error = Held::apply_bytes(host.state(), Some(&host.keys()), b"{\"fleet\": 7").unwrap_err();
    assert!(matches!(error, Error::AccessFormat(_)), "{error:?}");
    shut_out(&host);
    // A removal passed meanwhile is not acted on.
    assert!(host.suspend(1, &[removal(9, &[2])]).is_empty());
    // A good file lets the fleet in again.
    let bytes = good.to_bytes().unwrap();
    let held = Held::apply_bytes(host.state(), Some(&host.keys()), &bytes).unwrap();
    assert!(!held.refused);
    assert!(!host.state().join(REFUSED).exists());
    assert_eq!(host.lines().len(), 2);
    assert!(host.admitted().admits_member(&id(1)));
}

#[test]
fn an_access_file_that_does_not_check_out_removes_fleet_ssh_access() {
    let host = Host::new();
    let mut bad = allowed_first(&host);
    bad.members.get_mut(&id(1)).unwrap().dial = [0; 33];
    assert!(matches!(host.apply(bad), Err(Error::Access(_))));
    shut_out(&host);
}

#[test]
fn bytes_too_large_for_an_access_file_remove_fleet_ssh_access() {
    let host = Host::new();
    allowed_first(&host);
    let bytes = vec![b' '; access::MAX_BYTES + 1];
    assert!(Held::apply_bytes(host.state(), Some(&host.keys()), &bytes).is_err());
    shut_out(&host);
}

#[test]
fn scrubbing_forgets_a_refusal_too() {
    let host = Host::new();
    allowed_first(&host);
    assert!(Held::apply_bytes(host.state(), Some(&host.keys()), b"nope").is_err());
    Held::scrub(host.state(), Some(&host.keys())).unwrap();
    assert!(!host.state().join(REFUSED).exists());
}

#[test]
fn a_suspended_members_desktop_stays_cut_off_when_the_held_file_is_set_aside() {
    let host = Host::new();
    host.enroll(2);
    host.apply(file(&[(1, true, 0), (2, true, 0)], &[], &[]))
        .unwrap();
    host.suspend(1, &[removal(0x51, &[2])]);
    assert_eq!(host.admitted().peer(&dial(2), &[2; 32]), None);
    // The held file is damaged, then a bad file is handed over: the file
    // is set aside, and the desktop with member 2's keys stays cut off.
    let stored = host.state().join(access::FILE);
    std::fs::write(&stored, b"{\"fleet\": ").unwrap();
    assert!(Held::apply_bytes(host.state(), Some(&host.keys()), b"nope").is_err());
    let held = Held::load(host.state()).unwrap();
    assert!(held.refused && held.access.is_none());
    let admitted = host.admitted();
    assert_eq!(admitted.peer(&dial(2), &[2; 32]), None);
    assert!(!admitted.keeps_ssh(&Peer::Desktop, &dial(2), &[2; 32]));
    // A good file removing member 2 revokes the desktop for good, from
    // the pairs the suspension kept.
    host.apply(file(&[(1, true, 0)], &[2], &[])).unwrap();
    let held = Held::load(host.state()).unwrap();
    assert!(held.revoked.contains(&Pair {
        dial: dial(2),
        tunnel: [2; 32],
    }));
    assert_eq!(host.admitted().peer(&dial(2), &[2; 32]), None);
}
