//! Joining over Bluetooth: a joiner's bundle bound to one pairing.

use super::*;
use crate::control::fleet::bluetooth::{Hint, Joining, joining, pairing_digest};

fn pairing(seed: u8) -> [u8; 32] {
    pairing_digest(&[seed; 32])
}

fn fleet_of(founder: &Device) -> FleetId {
    found(founder).fleet()
}

#[test]
fn a_joining_checks_for_its_fleet_and_pairing() {
    let fleet = fleet_of(&computer(1));
    let joiner = computer(2);
    let joining = joining(&joiner.key, &fleet, &pairing(7), &joiner.bundle).unwrap();
    assert_eq!(joining.check(&fleet, &pairing(7)).unwrap(), joiner.bundle);
}

#[test]
fn a_joining_from_another_pairing_is_refused() {
    let fleet = fleet_of(&computer(1));
    let joiner = computer(2);
    let joining = joining(&joiner.key, &fleet, &pairing(7), &joiner.bundle).unwrap();
    // Replayed into a pairing whose token differs.
    assert_eq!(joining.check(&fleet, &pairing(8)), Err(Error::Signature));
}

#[test]
fn a_joining_for_another_fleet_is_refused() {
    let fleet = fleet_of(&computer(1));
    let other = fleet_of(&computer(3));
    let joiner = computer(2);
    let joining = joining(&joiner.key, &other, &pairing(7), &joiner.bundle).unwrap();
    assert_eq!(
        joining.check(&fleet, &pairing(7)),
        Err(Error::JoinSignature)
    );
}

#[test]
fn a_bundle_signed_by_another_key_is_refused() {
    let fleet = fleet_of(&computer(1));
    let joiner = computer(2);
    let thief = computer(4);
    // The thief signs the joiner's bundle: neither signature checks against
    // the key the bundle names.
    let forged = joining(&thief.key, &fleet, &pairing(7), &joiner.bundle).unwrap();
    assert_eq!(forged.check(&fleet, &pairing(7)), Err(Error::JoinSignature));
}

#[test]
fn a_join_signature_lifted_from_a_log_cannot_be_bound_again() {
    // A former admission's bundle and join signature are in every member's
    // log; without the joiner's key nobody binds them to a new pairing.
    let fleet = fleet_of(&computer(1));
    let joiner = computer(2);
    let thief = computer(4);
    let honest = joining(&joiner.key, &fleet, &pairing(7), &joiner.bundle).unwrap();
    let rebound = Joining {
        binding: crate::control::fleet::bluetooth::bind(
            &thief.key,
            &fleet,
            &pairing(9),
            &joiner.bundle,
        )
        .unwrap(),
        ..honest
    };
    assert_eq!(rebound.check(&fleet, &pairing(9)), Err(Error::Signature));
}

#[test]
fn a_phone_bundle_is_refused() {
    let fleet = fleet_of(&computer(1));
    let joiner = phone(2);
    let joining = joining(&joiner.key, &fleet, &pairing(7), &joiner.bundle).unwrap();
    assert_eq!(joining.check(&fleet, &pairing(7)), Err(Error::Invalid));
}

#[test]
fn an_oversized_or_malformed_joining_is_refused() {
    let fleet = fleet_of(&computer(1));
    let joiner = computer(2);
    let honest = joining(&joiner.key, &fleet, &pairing(7), &joiner.bundle).unwrap();
    let long = Joining {
        bundle: "A".repeat(MAX_OP * 2),
        ..honest.clone()
    };
    assert_eq!(long.check(&fleet, &pairing(7)), Err(Error::Bounds));
    let long_signature = Joining {
        binding: "A".repeat(200),
        ..honest.clone()
    };
    assert_eq!(
        long_signature.check(&fleet, &pairing(7)),
        Err(Error::Bounds)
    );
    let garbage = Joining {
        bundle: "not base64!".into(),
        ..honest
    };
    assert_eq!(garbage.check(&fleet, &pairing(7)), Err(Error::Encoding));
}

#[test]
fn a_checked_joining_is_admitted_by_the_log() {
    let founder = computer(1);
    let mut log = found(&founder);
    let joiner = computer(2);
    let fleet = log.fleet();
    let joining = joining(&joiner.key, &fleet, &pairing(7), &joiner.bundle).unwrap();
    let bundle = joining.check(&fleet, &pairing(7)).unwrap();
    let signature = sync::bytes(&joining.signature).unwrap().try_into().unwrap();
    let admit = sign(
        &log,
        &founder,
        Body::Admit(Admit {
            bundle,
            method: Method::Bluetooth,
            joiner_signature: signature,
        }),
    );
    log.insert(admit, NOW).unwrap();
    assert!(log.is_member(&joiner.id()));
}

#[test]
fn a_hint_names_a_fleet() {
    let fleet = fleet_of(&computer(1));
    let hint = Hint {
        fleet: fleet.to_string(),
        computers: 1,
        hosts: 0,
    };
    assert!(hint.valid());
    assert_eq!(hint.id().unwrap(), fleet);
    let bad = Hint {
        fleet: "zz".into(),
        ..hint
    };
    assert!(!bad.valid());
}
