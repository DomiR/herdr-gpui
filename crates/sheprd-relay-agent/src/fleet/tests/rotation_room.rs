//! At capacity, a member waiting for room never costs another member the
//! slot its own rotation frees.

use super::*;

/// Member `seed` on its `round`th keys.
fn on_keys(seed: u8, round: u8) -> (Key, [u8; 32]) {
    if round == 0 {
        (dial(seed), [seed; 32])
    } else {
        let key = seed.wrapping_mul(16).wrapping_add(round).wrapping_add(160);
        (dial(key), [key; 32])
    }
}

/// Members 1 and 2, each on the keys of its round.
fn rounds(one: u8, two: u8) -> Access {
    let mut access = file(&[(1, true, 0), (2, true, 0)], &[], &[]);
    for (seed, round) in [(1, one), (2, two)] {
        let (dial, tunnel) = on_keys(seed, round);
        let member = access.members.get_mut(&id(seed)).unwrap();
        member.dial = dial;
        member.tunnel = tunnel;
        member.keys = u64::from(round);
    }
    access
}

#[test]
fn a_rotation_that_frees_its_own_slot_is_never_crowded_out_by_another_wait() {
    let host = Host::new();
    // 63 desktops, one of them on member 1's keys, so member 1 takes no
    // slot of its own and member 2 takes the 64th.
    host.enroll(1);
    let mut desktops = Authorized::load(host.state()).unwrap();
    for seed in 0..u8::try_from(MAX_DIAL_KEYS - 2).unwrap() {
        desktops
            .insert(Entry {
                dial: dial(seed.wrapping_add(20)),
                tunnel: [seed.wrapping_add(20); 32],
                label: format!("desk{seed}"),
            })
            .unwrap();
    }
    desktops.save(host.state()).unwrap();
    host.apply(rounds(0, 0)).unwrap();
    assert!(Held::load(host.state()).unwrap().reaches_ssh(&id(2)));
    // Member 1 rotates off the desktop's keys: a 65th key, so it waits.
    assert!(matches!(
        host.apply(rounds(1, 0)),
        Err(Error::DialKeys { .. })
    ));
    let held = Held::load(host.state()).unwrap();
    assert_eq!(held.crowded, [id(1)].into());
    assert!(held.reaches_ssh(&id(2)));
    // Member 2 rotates: its old slot is freed for its new key.
    let file = rounds(1, 1);
    let _ = host.apply(file.clone());
    let held = Held::load(host.state()).unwrap();
    assert!(held.reaches_ssh(&id(2)), "member 2 lost its slot");
    assert!(!held.crowded.contains(&id(2)));
    assert!(held.crowded.contains(&id(1)));
    let (dial, tunnel) = on_keys(2, 1);
    assert!(matches!(
        host.admitted().peer(&dial, &tunnel),
        Some(Peer::Member { ssh: true, .. })
    ));
    // The same file again changes nothing.
    let _ = host.apply(file);
    let held = Held::load(host.state()).unwrap();
    assert!(held.reaches_ssh(&id(2)));
    assert_eq!(held.crowded, [id(1)].into());
}
