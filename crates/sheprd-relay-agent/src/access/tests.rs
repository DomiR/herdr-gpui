#![allow(clippy::unwrap_used)]

use super::*;

/// A valid dial key from `seed`.
pub(crate) fn dial(seed: u8) -> Key {
    let key = p256::ecdsa::SigningKey::from_slice(&[seed.max(1); 32]).unwrap();
    key.verifying_key()
        .to_sec1_point(true)
        .as_bytes()
        .try_into()
        .unwrap()
}

pub(crate) fn member(seed: u8, allow: bool, rev: u64) -> Member {
    Member {
        allow,
        dial: dial(seed),
        fleet_key: fleet_key(seed)
            .verifying_key()
            .to_sec1_point(true)
            .as_bytes()
            .try_into()
            .unwrap(),
        name: format!("computer{seed}"),
        rev,
        ssh: Some(crate::authorized_keys::tests::key(seed)),
        tunnel: [seed; 32],
        keys: 0,
        tiebreak: Id::default(),
        former: Vec::new(),
    }
}

/// A member's fleet key from `seed`.
pub(crate) fn fleet_key(seed: u8) -> p256::ecdsa::SigningKey {
    p256::ecdsa::SigningKey::from_slice(&[seed.wrapping_add(100).max(1); 32]).unwrap()
}

pub(crate) fn id(seed: u8) -> Id {
    Id([seed; 32])
}

fn access(members: &[(u8, bool, u64)], removed: &[u8]) -> Access {
    Access {
        fleet: id(0xf1),
        members: members
            .iter()
            .map(|&(seed, allow, rev)| (id(seed), member(seed, allow, rev)))
            .collect(),
        removed: removed.iter().map(|&seed| id(seed)).collect(),
        paused: false,
        settled: BTreeSet::new(),
    }
}

#[test]
fn a_file_reads_back_as_written_and_its_digest_is_stable() {
    let file = access(&[(1, true, 0), (2, false, 3)], &[9]);
    let bytes = file.to_bytes().unwrap();
    let read = Access::parse(&bytes).unwrap();
    assert_eq!(read, file);
    assert_eq!(read.digest().unwrap(), file.digest().unwrap());
    assert_eq!(read.to_bytes().unwrap(), bytes);
}

#[test]
fn the_digest_leaves_settled_out() {
    let file = access(&[(1, true, 0)], &[]);
    let mut settling = file.clone();
    settling.settled.insert(id(0x77));
    assert_eq!(file.digest().unwrap(), settling.digest().unwrap());
    let mut other = file.clone();
    other.removed.insert(id(5));
    assert_ne!(file.digest().unwrap(), other.digest().unwrap());
}

#[test]
fn a_removed_member_is_never_readmitted() {
    let held = Access::merge(None, access(&[(1, true, 0), (2, true, 0)], &[2])).unwrap();
    assert!(!held.members.contains_key(&id(2)));
    // A member that has not heard of the removal sends 2 back, allowed,
    // at a higher rev, and without the removal.
    let stale = access(&[(1, true, 0), (2, true, 9)], &[]);
    let merged = Access::merge(Some(&held), stale).unwrap();
    assert!(merged.removed.contains(&id(2)));
    assert!(!merged.members.contains_key(&id(2)));
    assert_eq!(merged.allowed().count(), 1);
}

#[test]
fn the_higher_rev_wins_either_way() {
    let held = Access::merge(None, access(&[(1, false, 4)], &[])).unwrap();
    let older = Access::merge(Some(&held), access(&[(1, true, 3)], &[])).unwrap();
    assert!(!older.members[&id(1)].allow);
    let newer = Access::merge(Some(&held), access(&[(1, true, 5)], &[])).unwrap();
    assert!(newer.members[&id(1)].allow);
    let deny = Access::merge(Some(&newer), access(&[(1, false, 6)], &[])).unwrap();
    assert!(!deny.members[&id(1)].allow);
}

#[test]
fn a_tie_between_allow_and_deny_is_a_deny() {
    let allowed = Access::merge(None, access(&[(1, true, 2)], &[])).unwrap();
    let denied = Access::merge(Some(&allowed), access(&[(1, false, 2)], &[])).unwrap();
    assert!(!denied.members[&id(1)].allow);
    let still = Access::merge(Some(&denied), access(&[(1, true, 2)], &[])).unwrap();
    assert!(!still.members[&id(1)].allow);
}

#[test]
fn keys_follow_their_own_version_never_a_staler_file() {
    let mut first = access(&[(1, true, 2)], &[]);
    first.members.get_mut(&id(1)).unwrap().keys = 5;
    let held = Access::merge(None, first).unwrap();
    // A file computed before the rotation, at the same access rev: its
    // older keys do not come back.
    let mut stale = access(&[(1, true, 2)], &[]);
    stale.members.get_mut(&id(1)).unwrap().tunnel = [0x42; 32];
    stale.members.get_mut(&id(1)).unwrap().keys = 4;
    let merged = Access::merge(Some(&held), stale).unwrap();
    assert_eq!(merged.members[&id(1)].tunnel, [1; 32]);
    // Nor do keys of the same version from another file.
    let mut same = access(&[(1, true, 2)], &[]);
    same.members.get_mut(&id(1)).unwrap().tunnel = [0x43; 32];
    same.members.get_mut(&id(1)).unwrap().keys = 5;
    let merged = Access::merge(Some(&merged), same).unwrap();
    assert_eq!(merged.members[&id(1)].tunnel, [1; 32]);
    // A rotation, newer: taken, whatever the access rev says.
    let mut rotated = access(&[(1, false, 1)], &[]);
    rotated.members.get_mut(&id(1)).unwrap().tunnel = [0x44; 32];
    rotated.members.get_mut(&id(1)).unwrap().keys = 6;
    let merged = Access::merge(Some(&merged), rotated).unwrap();
    assert_eq!(merged.members[&id(1)].tunnel, [0x44; 32]);
    // The access answer still follows the higher rev.
    assert!(merged.members[&id(1)].allow);
    assert_eq!(merged.members[&id(1)].rev, 2);
}

#[test]
fn members_sharing_a_key_are_let_in_again_once_the_clash_ends() {
    let held = Access::merge(
        None,
        access(&[(1, true, 0), (2, true, 0), (3, true, 0)], &[]),
    )
    .unwrap();
    // A file whose member 2 rotates onto member 1's tunnel key, and that
    // removes member 3: the removal lands, and neither 1 nor 2 is let in.
    let mut shared = access(&[(1, true, 0), (2, true, 0)], &[3]);
    let rotated = shared.members.get_mut(&id(2)).unwrap();
    rotated.tunnel = [1; 32];
    rotated.keys = 1;
    let merged = Access::merge(Some(&held), shared).unwrap();
    assert!(merged.removed.contains(&id(3)));
    assert_eq!(merged.allowed().count(), 0);
    // What each may do is kept as the log says: a later rotation away
    // from the clash lets both in again.
    assert!(merged.members[&id(1)].allow && merged.members[&id(2)].allow);
    let mut fixed = access(&[(1, true, 0), (2, true, 0)], &[]);
    let rotated = fixed.members.get_mut(&id(2)).unwrap();
    rotated.tunnel = [0x22; 32];
    rotated.keys = 2;
    let merged = Access::merge(Some(&merged), fixed).unwrap();
    assert_eq!(merged.allowed().count(), 2);
}

#[test]
fn a_rotation_remembers_the_keys_it_replaced_outside_the_digest() {
    let mut first = access(&[(1, true, 0)], &[]);
    first.members.get_mut(&id(1)).unwrap().keys = 1;
    let held = Access::merge(None, first.clone()).unwrap();
    let mut rotated = access(&[(1, true, 0)], &[]);
    rotated.members.get_mut(&id(1)).unwrap().tunnel = [0x44; 32];
    rotated.members.get_mut(&id(1)).unwrap().keys = 2;
    let merged = Access::merge(Some(&held), rotated.clone()).unwrap();
    let pairs: Vec<_> = merged.members[&id(1)].pairs().collect();
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[1].tunnel, [1; 32]);
    // Sheprd computes no former keys; the digests still agree.
    assert_eq!(
        merged.digest().unwrap(),
        Access::merge(None, rotated).unwrap().digest().unwrap()
    );
}

#[test]
fn a_member_a_file_leaves_out_is_kept() {
    let held = Access::merge(None, access(&[(1, true, 0), (2, true, 0)], &[])).unwrap();
    let merged = Access::merge(Some(&held), access(&[(1, true, 0)], &[])).unwrap();
    assert!(merged.members.contains_key(&id(2)));
}

#[test]
fn a_file_for_another_fleet_is_refused_naming_the_fleet_that_holds_it() {
    let held = Access::merge(None, access(&[(1, true, 0)], &[])).unwrap();
    let mut other = access(&[(1, true, 0)], &[]);
    other.fleet = id(0xf2);
    let error = Access::merge(Some(&held), other).unwrap_err();
    assert_eq!(
        error,
        Error::OtherFleet {
            held: id(0xf1),
            offered: id(0xf2)
        }
    );
    assert!(error.to_string().contains(&id(0xf1).short()));
}

#[test]
fn settled_is_not_kept() {
    let mut file = access(&[(1, true, 0)], &[]);
    file.settled.insert(id(0x77));
    assert!(Access::merge(None, file).unwrap().settled.is_empty());
}

#[test]
fn a_full_file_fits_its_byte_bound() {
    let members: Vec<_> = (1..=u8::try_from(MAX_MEMBERS).unwrap())
        .map(|seed| (seed, true, u64::MAX))
        .collect();
    let mut file = access(&members, &[]);
    for index in 0..MAX_REMOVED {
        let mut removed = [0xff; 32];
        removed[..8].copy_from_slice(&(index as u64).to_be_bytes());
        file.removed.insert(Id(removed));
    }
    for member in file.members.values_mut() {
        member.name = "n".repeat(MAX_NAME);
    }
    for seed in 0..u8::try_from(MAX_SETTLED).unwrap() {
        file.settled.insert(Id([seed; 32]));
    }
    let bytes = file.to_bytes().unwrap();
    assert!(bytes.len() <= MAX_BYTES, "{} bytes", bytes.len());
    // The agent's own key history has its own room beside it: every
    // member with all the pairs it may remember still saves and reads.
    for (index, member) in file.members.values_mut().enumerate() {
        member.former = (0..MAX_FORMER)
            .map(|slot| Pair {
                dial: dial(u8::try_from(index * MAX_FORMER + slot).unwrap() % 250 + 1),
                tunnel: [0xee; 32],
            })
            .collect();
    }
    let stored = file.to_stored_bytes().unwrap();
    assert!(stored.len() > MAX_BYTES, "{} bytes", stored.len());
    assert!(stored.len() <= MAX_STORED_BYTES, "{} bytes", stored.len());
    assert_eq!(Access::parse_stored(&stored).unwrap(), file);
    // A file Sheprd hands over carries no history.
    assert_eq!(file.to_bytes(), Err(Error::Bounds));
    assert!(Access::parse(&stored).is_err());
}

#[test]
fn counts_over_their_bounds_are_refused() {
    let members: Vec<_> = (1..=u8::try_from(MAX_MEMBERS + 1).unwrap())
        .map(|seed| (seed, true, 0))
        .collect();
    assert_eq!(access(&members, &[]).check(), Err(Error::Bounds));
    let mut file = access(&[], &[]);
    for index in 0..=MAX_REMOVED {
        let mut removed = [0; 32];
        removed[..8].copy_from_slice(&(index as u64).to_be_bytes());
        file.removed.insert(Id(removed));
    }
    assert_eq!(file.check(), Err(Error::Bounds));
    assert!(matches!(
        Access::parse(&vec![b' '; MAX_BYTES + 1]),
        Err(crate::Error::Access(Error::Bounds))
    ));
}

#[test]
fn a_member_with_bad_keys_or_name_is_refused() {
    let mut file = access(&[(1, true, 0)], &[]);
    file.members.get_mut(&id(1)).unwrap().dial = [0; 33];
    assert_eq!(file.check(), Err(Error::Member { member: id(1) }));
    let mut file = access(&[(1, true, 0)], &[]);
    file.members.get_mut(&id(1)).unwrap().ssh = Some("ssh-rsa AAAA".to_owned());
    assert_eq!(file.check(), Err(Error::Member { member: id(1) }));
    let mut file = access(&[(1, true, 0)], &[]);
    file.members.get_mut(&id(1)).unwrap().name = "line\nbreak".to_owned();
    assert_eq!(file.check(), Err(Error::Member { member: id(1) }));
}

#[test]
fn json_that_is_no_access_file_is_refused() {
    assert!(matches!(
        Access::parse(b"{\"fleet\": \"short\"}"),
        Err(crate::Error::AccessFormat(_))
    ));
    assert!(matches!(
        Access::parse(b"[]"),
        Err(crate::Error::AccessFormat(_))
    ));
}

#[test]
fn a_tie_on_key_versions_breaks_on_the_bundle_whichever_arrives_first() {
    let view = |tunnel: u8, tiebreak: u8| {
        let mut file = access(&[(1, true, 0)], &[]);
        let member = file.members.get_mut(&id(1)).unwrap();
        member.keys = 5;
        member.tunnel = [tunnel; 32];
        member.tiebreak = Id([tiebreak; 32]);
        file
    };
    let (low, high) = (view(0x51, 1), view(0x52, 2));
    for (first, second) in [(&low, &high), (&high, &low)] {
        let held = Access::merge(None, first.clone()).unwrap();
        let merged = Access::merge(Some(&held), second.clone()).unwrap();
        assert_eq!(merged.members[&id(1)].tunnel, [0x52; 32]);
    }
}
