//! A pairing link (`Request::PairLink`): a second token for the open
//! pairing, claimable through the relays for two minutes, whose claim
//! carries a code derived as a QR claim's is.

use super::nearby::{FP, open};
use super::*;
use crate::{
    auth::pairing::{LINK_TTL_MS, TOKEN_TTL_MS},
    control::pairing_link::PairingLink,
    relay::Info,
};

fn info(relays: usize) -> Info {
    Info {
        identity: "aWRlbnRpdHk".into(),
        labels: Default::default(),
        relays: (0..relays)
            .map(|index| format!("https://relay{index}.example.org"))
            .collect(),
        route: "cm91dGU".into(),
        rtt_ms: (0..relays)
            .map(|index| (format!("https://relay{index}.example.org"), 10))
            .collect(),
    }
}

fn linked(auth: &mut Auth, offer: &Offer, now_ms: u64) -> PairingLink {
    let (link, expires_in_ms) = auth
        .pair_link(&offer.id, "Studio", FP, None, now_ms)
        .unwrap();
    assert_eq!(expires_in_ms, LINK_TTL_MS);
    link.parse().unwrap()
}

fn claim(auth: &mut Auth, link: &PairingLink, now_ms: u64) -> Result<Claim> {
    auth.claim(
        "10.0.0.9".parse().unwrap(),
        &encode(&link.token.0),
        "Laptop",
        &public_key(&signing_key(9)),
        now_ms,
    )
}

#[test]
fn a_link_names_this_computer_its_authority_and_at_most_three_relays() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, offer) = open(dir.path());
    auth.set_relay_offer(Some(info(5)));
    let link = linked(&mut auth, &offer, T0);
    assert_eq!(link.computer, "Studio");
    assert_eq!(
        link.fingerprint,
        crate::auth::nearby::fingerprint_bytes(FP).unwrap()
    );
    assert_eq!(link.relay.relays.len(), 3);
    assert_eq!(link.relay.rtt_ms.len(), 3);
    assert_eq!(link.relay.route, "cm91dGU");
}

#[test]
fn a_link_opens_one_pairing_dial_for_its_token() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, offer) = open(dir.path());
    auth.set_relay_offer(Some(info(1)));
    let link = linked(&mut auth, &offer, T0);
    assert_eq!(auth.take_relay_pairing(), Some(link.token.0));
    assert!(auth.take_relay_pairing().is_none());
}

#[test]
fn a_computer_without_relays_makes_no_link() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, offer) = open(dir.path());
    let error = auth
        .pair_link(&offer.id, "Studio", FP, None, T0)
        .unwrap_err();
    assert!(matches!(error, Error::NoRelays));
    assert_eq!(
        control::ErrorCode::from(&error),
        control::ErrorCode::NoRelays
    );
    assert!(auth.take_relay_pairing().is_none());
}

#[test]
fn a_link_is_only_for_the_open_unclaimed_pairing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, offer) = open(dir.path());
    auth.set_relay_offer(Some(info(1)));
    let other = PairingId("other".into());
    assert!(matches!(
        auth.pair_link(&other, "Studio", FP, None, T0),
        Err(Error::UnknownPairing)
    ));
    let link = linked(&mut auth, &offer, T0);
    claim(&mut auth, &link, T0 + 1).unwrap();
    assert!(matches!(
        auth.pair_link(&offer.id, "Studio", FP, None, T0 + 2),
        Err(Error::NoPairing)
    ));
}

#[test]
fn a_link_outlasts_the_qr_and_survives_its_rotation() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, offer) = open(dir.path());
    auth.set_relay_offer(Some(info(1)));
    let link = linked(&mut auth, &offer, T0);
    let rotated = auth.mint(T0 + 50_000, &origins(), FP).unwrap();
    assert_eq!(rotated.id, offer.id);
    let later = T0 + TOKEN_TTL_MS + 30_000;
    assert!(auth.pairings().holds(&link.token.0, later));
    let claimed = claim(&mut auth, &link, later).unwrap();
    assert_eq!(claimed.code.len(), 6);
    assert!(matches!(
        auth.pairing(&offer.id, later).unwrap(),
        PairingState::Claimed { code, .. } if code == claimed.code
    ));
}

#[test]
fn a_link_expires_after_two_minutes() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, offer) = open(dir.path());
    auth.set_relay_offer(Some(info(1)));
    let link = linked(&mut auth, &offer, T0);
    let expired = T0 + LINK_TTL_MS;
    assert!(!auth.pairings().holds(&link.token.0, expired));
    assert!(claim(&mut auth, &link, expired).is_err());
}

#[test]
fn a_new_link_voids_the_previous_one() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, offer) = open(dir.path());
    auth.set_relay_offer(Some(info(1)));
    let first = linked(&mut auth, &offer, T0);
    let second = linked(&mut auth, &offer, T0 + 1);
    assert!(matches!(
        claim(&mut auth, &first, T0 + 2),
        Err(Error::InvalidToken)
    ));
    claim(&mut auth, &second, T0 + 2).unwrap();
}

#[test]
fn a_link_is_claimed_once() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, offer) = open(dir.path());
    auth.set_relay_offer(Some(info(1)));
    let link = linked(&mut auth, &offer, T0);
    claim(&mut auth, &link, T0 + 1).unwrap();
    assert!(matches!(
        claim(&mut auth, &link, T0 + 2),
        Err(Error::AlreadyClaimed)
    ));
}
