//! The fleet's files stay within what loading reads back: the largest
//! state the bounds allow saves and loads, desktops coming and going never
//! grow the cut-offs, and a file that would not read back is not written.

use super::*;

fn pair(n: u8, salt: u8) -> Pair {
    Pair {
        dial: dial(n.wrapping_add(salt)),
        tunnel: [n.wrapping_add(salt); 32],
    }
}

/// Every member suspended by as many removals as are remembered and
/// holding as many pairs as are kept, every desktop cut off, every member
/// waiting.
fn largest() -> Held {
    let count = |n: usize| u8::try_from(n).unwrap();
    let suspension = |n: u8| Suspension {
        removals: (0..count(MAX_CAUSES))
            .map(|cause| {
                (
                    Id([cause.wrapping_mul(31).wrapping_add(n); 32]),
                    Cause {
                        signer: id(n),
                        seq: u64::MAX,
                    },
                )
            })
            .collect(),
        overflow: true,
        pairs: (0..count(MAX_SUSPENDED_PAIRS))
            .map(|k| Pair {
                dial: dial(k.wrapping_add(n)),
                tunnel: {
                    let mut tunnel = [1; 32];
                    tunnel[0] = k;
                    tunnel[1] = n;
                    tunnel
                },
            })
            .collect(),
    };
    Held {
        access: None,
        suspended: (0..count(access::MAX_MEMBERS))
            .map(|n| (id(n), suspension(n)))
            .collect(),
        revoked: (0..count(crate::authorized::MAX_ENTRIES))
            .map(|n| pair(n, 0x80))
            .collect(),
        crowded: (0..count(access::MAX_MEMBERS)).map(id).collect(),
        refused: false,
    }
}

#[test]
fn the_largest_state_the_bounds_allow_saves_and_loads_with_room_to_spare() {
    let held = largest();
    for (name, bytes) in held.files(&held.crowded).unwrap() {
        assert!(
            bytes.len() as u64 * 2 <= MAX_SUSPENDED_BYTES,
            "{name}: {} bytes of {MAX_SUSPENDED_BYTES}",
            bytes.len()
        );
    }
    let host = Host::new();
    held.save(host.state(), &held.crowded).unwrap();
    let loaded = Held::load(host.state()).unwrap();
    assert_eq!(loaded.suspended, held.suspended);
    assert_eq!(loaded.revoked, held.revoked);
}

#[test]
fn a_file_that_would_not_read_back_is_not_written() {
    let host = Host::new();
    host.apply(file(&[(1, true, 0)], &[], &[])).unwrap();
    let before = std::fs::read(host.state().join(SUSPENDED)).unwrap();
    let mut held = Held::load(host.state()).unwrap();
    // Three times the largest suspensions, each copy under other ids.
    for copy in 1..=3 {
        held.suspended
            .extend(largest().suspended.into_iter().map(|(member, suspension)| {
                let mut raw = member.0;
                raw[1] = copy;
                (Id(raw), suspension)
            }));
    }
    assert!(matches!(
        held.save(host.state(), &held.crowded),
        Err(Error::TooLarge { .. })
    ));
    assert_eq!(std::fs::read(host.state().join(SUSPENDED)).unwrap(), before);
    assert!(Held::load(host.state()).is_ok());
}

#[test]
fn desktops_coming_and_going_never_grow_the_cut_offs() {
    let host = Host::new();
    host.enroll(2);
    host.apply(file(&[(1, true, 0), (2, true, 0)], &[], &[]))
        .unwrap();
    host.suspend(1, &[removal(0x51, &[2])]);
    // Member 2 rotates far past the bound while desktops on each of its
    // keys are enrolled and then revoked, one after another.
    let rounds = u8::try_from(MAX_SUSPENDED_PAIRS * 3).unwrap();
    for round in 1..=rounds {
        let keys = Pair {
            dial: dial(100 + round),
            tunnel: [100 + round; 32],
        };
        let mut desktops = Authorized::load(host.state()).unwrap();
        desktops
            .insert(Entry {
                dial: keys.dial,
                tunnel: keys.tunnel,
                label: format!("round{round}"),
            })
            .unwrap();
        desktops.save(host.state()).unwrap();
        let mut access = file(&[(1, true, 0), (2, true, 0)], &[], &[]);
        let member = access.members.get_mut(&id(2)).unwrap();
        member.dial = keys.dial;
        member.tunnel = keys.tunnel;
        member.keys = u64::from(round);
        host.apply(access).unwrap();
        assert_eq!(host.admitted().peer(&keys.dial, &keys.tunnel), None);
        // The desktop goes again.
        let mut desktops = Authorized::load(host.state()).unwrap();
        desktops.remove(&keys.dial);
        desktops.save(host.state()).unwrap();
    }
    let held = host
        .apply(file(&[(1, true, 0), (2, true, 0)], &[], &[]))
        .unwrap();
    let enrolled = Authorized::load(host.state()).unwrap().entries().len();
    assert!(held.revoked.len() <= enrolled, "{:?}", held.revoked);
    assert!(held.suspended[&id(2)].pairs.len() <= MAX_SUSPENDED_PAIRS);
}
