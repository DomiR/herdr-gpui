//! The dial keys named to a relay: the fleet's computers always fit beside
//! paired devices, which keep the rest in their order.

use super::*;
use crate::control::fleet::MAX_COMPUTERS;
use sheprd_relay::wire::MAX_DIAL_KEYS;

fn key(seed: u8, kind: u8) -> Key {
    let mut key = [kind; 33];
    key[0] = 2;
    key[1] = seed;
    key
}

#[test]
fn the_fleets_computers_keep_their_dial_keys_beside_many_paired_devices() {
    let max = MAX_DIAL_KEYS;
    let paired: Vec<_> = (0..max)
        .map(|seed| key(u8::try_from(seed).unwrap(), 1))
        .collect();
    let fleet: Vec<_> = (0..MAX_COMPUTERS - 1)
        .map(|seed| key(u8::try_from(seed).unwrap(), 2))
        .collect();
    let (keys, left_out) = listen::combined(paired.clone(), fleet.clone());
    assert_eq!(keys.len(), max);
    assert!(fleet.iter().all(|key| keys.contains(key)));
    // The paired devices keep the rest, first ones first.
    assert_eq!(left_out, fleet.len());
    assert!(
        paired[..max - fleet.len()]
            .iter()
            .all(|key| keys.contains(key))
    );
}

#[test]
fn a_key_both_paired_and_in_the_fleet_counts_once() {
    let shared = key(9, 3);
    let (keys, left_out) = listen::combined(vec![shared, key(1, 1)], vec![shared]);
    assert_eq!(keys, vec![shared, key(1, 1)]);
    assert_eq!(left_out, 0);
}
