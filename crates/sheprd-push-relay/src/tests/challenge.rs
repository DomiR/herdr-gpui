use super::*;
use crate::credential::CHALLENGE_LIFETIME_S;

#[test]
fn a_challenge_is_good_until_it_expires() {
    let challenge = issuer().challenge(NOW_S).unwrap();
    assert!(issuer().check_challenge(&challenge, NOW_S));
    assert!(issuer().check_challenge(&challenge, NOW_S + CHALLENGE_LIFETIME_S - 1));
    assert!(!issuer().check_challenge(&challenge, NOW_S + CHALLENGE_LIFETIME_S));
}

#[test]
fn challenges_differ_and_only_this_relays_are_taken() {
    let first = issuer().challenge(NOW_S).unwrap();
    assert_ne!(first, issuer().challenge(NOW_S).unwrap());
    let other = Issuer::new(&[6; 32]).unwrap();
    assert!(!other.check_challenge(&first, NOW_S));

    let mut bytes = URL_SAFE_NO_PAD.decode(&first).unwrap();
    bytes[0] ^= 1;
    assert!(!issuer().check_challenge(&URL_SAFE_NO_PAD.encode(&bytes), NOW_S));
    for junk in ["", "!!", &first[..first.len() - 2]] {
        assert!(!issuer().check_challenge(junk, NOW_S), "{junk}");
    }
}

#[test]
fn a_credential_is_not_a_challenge_and_a_challenge_not_a_credential() {
    let credential = issuer().issue(PHONE, Host::Production, NOW_S);
    assert!(!issuer().check_challenge(&credential, NOW_S));
    let challenge = issuer().challenge(NOW_S).unwrap();
    assert_eq!(issuer().check(&challenge), None);
}

#[test]
fn a_key_always_gets_the_same_device_and_keys_differ() {
    let device = issuer().device(&[1; 32]);
    assert_eq!(device, issuer().device(&[1; 32]));
    assert_ne!(device, issuer().device(&[2; 32]));
    assert_ne!(device, Issuer::new(&[6; 32]).unwrap().device(&[1; 32]));
}

#[test]
fn a_challenge_is_spent_once_and_forgotten_after_it_expires() {
    use crate::credential::Spent;
    let mut spent = Spent::default();
    assert!(spent.spend("a", NOW_S));
    assert!(!spent.spend("a", NOW_S));
    assert!(spent.spend("b", NOW_S));
    // Full, it drops what expired before refusing.
    for n in 2..65_536 {
        assert!(spent.spend(&n.to_string(), NOW_S));
    }
    assert!(!spent.spend("late", NOW_S + 1));
    assert!(spent.spend("late", NOW_S + CHALLENGE_LIFETIME_S));
}
