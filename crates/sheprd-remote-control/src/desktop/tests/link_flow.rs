//! Adding a Sheprd from a pairing link, on a real relay on loopback: no
//! Bluetooth, only the link its Pair device copied.

use super::flow::{allow, other, ssh_key};
use super::*;
use crate::control::{PairingState, pairing_link::PairingLink};
use std::time::Duration;
use tokio::io::AsyncReadExt as _;

/// What the control socket's `PairLink` does: a link for the open
/// pairing, and its pairing dial opened on the relays.
fn link(other: &flow::Other) -> String {
    let hint = None;
    let (link, _) = {
        let mut auth = other.state.auth();
        crate::relay::refresh_offer(other.state.relay.as_deref(), &mut auth);
        auth.pair_link(
            &other.pairing,
            &other.state.computer,
            &other.state.fingerprint,
            hint,
            now_ms(),
        )
        .unwrap()
    };
    crate::relay::open_pairing(&other.state);
    link
}

#[tokio::test(flavor = "multi_thread")]
async fn a_desktop_with_only_a_link_pairs_and_reaches_ssh_through_a_relay() {
    let other = other().await;
    let here = tempfile::tempdir().unwrap();
    let keys = Keys::load_or_create(here.path()).unwrap();
    let opened: Opened = link(&other).parse::<PairingLink>().unwrap().into();
    assert_eq!(opened.computer, "Studio");
    assert!(opened.code.is_none());

    let ssh_key = ssh_key();
    let claimant = claim::Claimant {
        keys: &keys,
        name: "Laptop",
        ssh_key: &ssh_key,
        fleet: None,
    };
    let mut shown = None;
    let (allowed, approved) = tokio::join!(
        claim::claim(&opened, &claimant, |code| shown = Some(code.to_owned())),
        allow(&other)
    );
    let allowed = allowed.unwrap();
    // Both screens show the daemon's code for this claim.
    assert_eq!(shown.as_deref(), Some(approved.as_str()));

    let id = allowed
        .record
        .save(here.path(), &allowed.login.host_keys)
        .unwrap();
    let (_connection, mut tunnel) =
        connect::open(here.path(), &sheprd_client::relay_host::host_name(&id))
            .await
            .unwrap();
    let mut greeting = [0; 14];
    tokio::time::timeout(Duration::from_secs(5), tunnel.io.read_exact(&mut greeting))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&greeting, b"SSH-2.0-test\r\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_spent_link_dials_no_more() {
    let other = other().await;
    let here = tempfile::tempdir().unwrap();
    let keys = Keys::load_or_create(here.path()).unwrap();
    let opened: Opened = link(&other).parse::<PairingLink>().unwrap().into();
    let ssh_key = ssh_key();
    let claimant = claim::Claimant {
        keys: &keys,
        name: "Laptop",
        ssh_key: &ssh_key,
        fleet: None,
    };
    let (first, _) = tokio::join!(claim::claim(&opened, &claimant, |_| {}), allow(&other));
    first.unwrap();
    // The relay spent the pairing dial, and the token is claimed.
    let again = claim::claim(&opened, &claimant, |_| {}).await;
    assert!(again.is_err(), "{again:?}");
    assert!(matches!(
        other
            .state
            .auth()
            .pairing(&other.pairing, now_ms())
            .unwrap(),
        PairingState::Approved { .. }
    ));
}
