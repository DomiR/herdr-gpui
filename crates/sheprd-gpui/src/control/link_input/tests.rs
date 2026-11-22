#![allow(clippy::unwrap_used)]
use super::*;
use sheprd_remote_control::control::{
    host_relay::Info,
    pairing_link::{PairingLink, Token},
};

fn link() -> String {
    PairingLink {
        computer: "Studio".into(),
        fingerprint: [7; 32],
        fleet: None,
        relay: Info {
            identity: "aWRlbnRpdHk".into(),
            labels: Default::default(),
            relays: vec!["https://relay.example.org".into()],
            route: "cm91dGU".into(),
            rtt_ms: Default::default(),
        },
        token: Token([9; 32]),
    }
    .to_string()
}

#[test]
fn a_piped_link_is_read_without_its_white_space() {
    let link = link();
    let (head, tail) = link.split_at(20);
    let piped = format!("{head}\n{tail}\n");
    assert_eq!(read(piped.as_bytes()).unwrap(), LinkText(link));
}

#[test]
fn nothing_piped_is_refused() {
    assert!(matches!(read(&b" \n"[..]), Err(Error::Empty)));
}

#[test]
fn other_text_is_no_link() {
    assert!(matches!(
        read(&b"https://example.org"[..]),
        Err(Error::Link(_))
    ));
    assert!(matches!(read(&[0xff, 0xfe][..]), Err(Error::Text)));
}

#[test]
fn too_much_is_refused_before_it_is_read() {
    let big = vec![b'a'; MAX_INPUT + 1];
    assert!(matches!(read(&big[..]), Err(Error::TooLong)));
}

#[test]
fn a_bad_link_is_the_callers_to_fix_and_a_failed_read_is_a_failure() {
    use crate::control::client::status;
    let usage = status(&crate::Error::LinkInput(Error::Empty));
    let refused = status(&crate::Error::LinkInput(Error::Read(
        std::io::Error::other("gone"),
    )));
    assert_eq!(
        usage,
        status(&crate::Error::ListenToken(
            crate::control::listen_token::Error::Empty
        ))
    );
    assert_ne!(usage, refused);
}
