//! The dialog's Copy link: asked once per click, sent between polls, and
//! copied once; a refusal stays the link's and leaves the QR showing.

use super::*;

fn linked(link: &str) -> Result<Response, control::Error> {
    Ok(Response::PairLink {
        link: link.into(),
        expires_in_ms: 120_000,
    })
}

#[test]
fn a_click_sends_one_link_request_and_the_link_is_copied_once() {
    let now = Instant::now();
    let mut pairing = showing(now);
    assert_eq!(pairing.link(), &Link::None);
    pairing.ask_link();
    pairing.ask_link();
    assert_eq!(
        step(&mut pairing, now, linked("sheprd-pair:abc")),
        Request::PairLink {
            id: PairingId("p1".into())
        }
    );
    assert!(matches!(pairing.phase(), Phase::Showing { .. }));
    assert_eq!(pairing.take_link(now).as_deref(), Some("sheprd-pair:abc"));
    assert_eq!(pairing.take_link(now), None);
    assert_eq!(
        pairing.link(),
        &Link::Copied {
            until: now + Duration::from_secs(120)
        }
    );
    // Polling goes on as before.
    assert!(matches!(
        pairing.next(now + POLL).unwrap().request,
        Request::Pairing { .. }
    ));
}

#[test]
fn a_computer_without_relays_says_so_and_keeps_its_qr() {
    let now = Instant::now();
    let mut pairing = showing(now);
    pairing.ask_link();
    step(
        &mut pairing,
        now,
        Err(control::Error::Daemon {
            code: ErrorCode::NoRelays,
            message: "this computer listens on no relay".into(),
        }),
    );
    assert_eq!(pairing.link(), &Link::NoRelays);
    assert!(matches!(pairing.phase(), Phase::Showing { .. }));
}

#[test]
fn another_refusal_is_the_links_alone() {
    let now = Instant::now();
    let mut pairing = showing(now);
    pairing.ask_link();
    step(
        &mut pairing,
        now,
        Err(control::Error::Daemon {
            code: ErrorCode::UnknownPairing,
            message: "no such pairing".into(),
        }),
    );
    assert!(matches!(pairing.link(), Link::Failed { .. }));
    assert!(matches!(pairing.phase(), Phase::Showing { .. }));
}

#[test]
fn no_link_is_asked_without_a_qr_and_a_new_pairing_forgets_it() {
    let now = Instant::now();
    let mut pairing = Pairing::new(true);
    pairing.ask_link();
    assert_eq!(pairing.link(), &Link::None);
    let mut pairing = showing(now);
    pairing.ask_link();
    step(&mut pairing, now, linked("sheprd-pair:abc"));
    pairing.restart();
    assert_eq!(pairing.link(), &Link::None);
}

#[test]
fn debug_never_shows_the_link() {
    let link = Link::Ready {
        link: LinkText("sheprd-pair:secret".into()),
        lifetime: Duration::from_secs(1),
    };
    assert!(!format!("{link:?}").contains("secret"));
}
