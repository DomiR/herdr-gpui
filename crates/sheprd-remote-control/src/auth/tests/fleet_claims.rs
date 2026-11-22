//! What a pairing keeps for a desktop joining the fleet: the token checked
//! before anything a claim carries, and approvals held until the fleet has
//! acted on them.

use super::*;
use crate::{auth::pairing::TOKEN_TTL_MS, control::fleet::bluetooth::ApprovedFleet};

#[test]
fn only_the_live_unclaimed_token_is_held() {
    let mut pairings = Pairings::default();
    let offer = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    let token: [u8; 32] = decode_fixed(&token(&offer)).unwrap();
    assert!(pairings.holds(&token, T0));
    assert!(!pairings.holds(&[7; 32], T0));
    assert!(!pairings.holds(&token, T0 + TOKEN_TTL_MS));

    let offer = pairings.mint(T0, &origins(), FINGERPRINT).unwrap();
    let token: [u8; 32] = decode_fixed(&super::token(&offer)).unwrap();
    pairings
        .claim(
            &super::token(&offer),
            "Laptop",
            &public_key(&signing_key(1)),
            T0,
        )
        .unwrap();
    // Spent: a second claim with it gets nothing read either.
    assert!(!pairings.holds(&token, T0));
}

#[test]
fn an_approval_stays_held_until_every_answer_acting_on_it_is_done() {
    let mut pairings = Pairings::default();
    let device = DeviceId("laptop".into());
    // Two answers race: both hold it, and the one that finds nothing to do
    // finishes first.
    pairings.settling_fleet(device.clone());
    pairings.settling_fleet(device.clone());
    pairings.settled_fleet(device.clone(), None);
    assert!(pairings.fleet_settling(&device));
    let joined = ApprovedFleet::Joined {
        fleet: "f".repeat(64),
    };
    pairings.settled_fleet(device.clone(), Some(joined.clone()));
    assert!(!pairings.fleet_settling(&device));
    assert_eq!(pairings.fleet_for(&device), Some(joined));
}
