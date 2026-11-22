//! A suspended member's key pairs are kept with its suspension and only
//! ever added to: a rotation never lets a desktop with an older pair back
//! in, and past the bound the agent refuses rather than forget one.

use super::*;

/// Members 1 and 2, member 2 on its `round`th rotated keys.
fn rotated(round: u8) -> Access {
    let mut access = file(&[(1, true, 0), (2, true, 0)], &[], &[]);
    if round > 0 {
        let member = access.members.get_mut(&id(2)).unwrap();
        member.dial = dial(100 + round);
        member.tunnel = [100 + round; 32];
        member.keys = u64::from(round);
    }
    access
}

/// A host where member 2, with a desktop enrolled on its first keys, is
/// suspended by member 1.
fn suspended() -> Host {
    let host = Host::new();
    host.enroll(2);
    host.apply(rotated(0)).unwrap();
    host.suspend(1, &[removal(0x51, &[2])]);
    assert_eq!(host.admitted().peer(&dial(2), &[2; 32]), None);
    host
}

#[test]
fn a_suspended_member_that_rotates_leaves_its_old_desktop_cut_off() {
    let host = suspended();
    // More rotations than the agent's own key history remembers.
    let rounds = u8::try_from(access::MAX_FORMER + 1).unwrap();
    for round in 1..=rounds {
        host.apply(rotated(round)).unwrap();
    }
    let held = Held::load(host.state()).unwrap();
    let kept = &held.suspended[&id(2)].pairs;
    let first = Pair {
        dial: dial(2),
        tunnel: [2; 32],
    };
    assert!(kept.contains(&first));
    assert!(
        !held.access.unwrap().members[&id(2)]
            .pairs()
            .any(|pair| pair == first)
    );
    let admitted = host.admitted();
    assert_eq!(admitted.peer(&dial(2), &[2; 32]), None);
    assert!(!admitted.keeps_ssh(&Peer::Desktop, &dial(2), &[2; 32]));
}

/// Enrolls a desktop one by one with member 2's keys of `round`.
fn enroll_round(host: &Host, round: u8) -> Pair {
    let pair = Pair {
        dial: dial(100 + round),
        tunnel: [100 + round; 32],
    };
    let mut desktops = Authorized::load(host.state()).unwrap();
    desktops
        .insert(Entry {
            dial: pair.dial,
            tunnel: pair.tunnel,
            label: format!("round{round}"),
        })
        .unwrap();
    desktops.save(host.state()).unwrap();
    pair
}

#[test]
fn past_the_bound_every_pair_a_desktop_has_stays_cut_off() {
    let host = suspended();
    let rounds = u8::try_from(MAX_SUSPENDED_PAIRS + 3).unwrap();
    let last = rounds;
    // A desktop on keys the member will rotate away from, and long past
    // what the agent's key history and the bound remember.
    let early = enroll_round(&host, 3);
    // A desktop on the keys of the very rotation past the bound, enrolled
    // before that file arrives.
    let incoming = enroll_round(&host, last);
    for round in 1..=rounds {
        host.apply(rotated(round)).unwrap();
    }
    let held = Held::load(host.state()).unwrap();
    assert!(!held.refused);
    let suspension = &held.suspended[&id(2)];
    assert!(suspension.overflow, "only its removal ends it now");
    assert!(suspension.pairs.len() <= MAX_SUSPENDED_PAIRS);
    assert!(suspension.pairs.contains(&early));
    // The pair past the bound is kept with the cut-off desktops.
    assert!(held.revoked.contains(&incoming));
    // The rotation itself was taken: the file names the member's keys now.
    let member = held.access.as_ref().unwrap().members[&id(2)].clone();
    assert_eq!(member.pair(), incoming);
    let admitted = host.admitted();
    let first = Pair {
        dial: dial(2),
        tunnel: [2; 32],
    };
    for pair in [first, early, incoming] {
        assert_eq!(admitted.peer(&pair.dial, &pair.tunnel), None, "{pair:?}");
        assert!(!admitted.keeps_ssh(&Peer::Desktop, &pair.dial, &pair.tunnel));
    }
    assert!(!admitted.admits_member(&id(2)));
    // The same holds once the access file is set aside.
    std::fs::write(host.state().join(access::FILE), b"{").unwrap();
    assert!(Held::apply_bytes(host.state(), Some(&host.keys()), b"nope").is_err());
    let admitted = host.admitted();
    for pair in [first, early, incoming] {
        assert_eq!(admitted.peer(&pair.dial, &pair.tunnel), None, "{pair:?}");
    }
}

#[test]
fn a_member_past_the_bound_leaves_the_others_alone() {
    let host = suspended();
    let rounds = u8::try_from(MAX_SUSPENDED_PAIRS + 2).unwrap();
    for round in 1..=rounds {
        host.apply(rotated(round)).unwrap();
    }
    // A settling file does not lift it.
    let mut settling = rotated(rounds);
    settling.settled.insert(id(0x51));
    let held = host.apply(settling).unwrap();
    assert!(held.suspended.contains_key(&id(2)));
    // Member 1 is untouched.
    assert!(held.reaches_ssh(&id(1)));
    assert_eq!(host.lines(), [id(1).short()]);
    assert!(host.admitted().admits_member(&id(1)));
}
