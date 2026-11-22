//! Alerts from several computers to one phone: the same pane on two of
//! them neither replaces nor groups with the other's, while a newer alert
//! for one chat replaces its older one, and nothing in the clear names the
//! computer.

use super::alert::{COMPUTER, device, fixed_seal, json};
use crate::apns::{
    alert::{Sealed, clear, collapse_id, push, scope, thread_id},
    seal,
};
use crate::push::{EventId, Kind, Notification, Urgency};
use p256::{PublicKey, SecretKey};
use sheprd_apns::SealedPush;

/// The notify key the phone gave another computer: one per computer.
fn other_key() -> PublicKey {
    SecretKey::from_slice(&[9; 32]).unwrap().public_key()
}

fn asking(pane: &str) -> Notification {
    Notification {
        body: "Run tests?".to_owned(),
        clear: false,
        event: EventId::new(),
        kind: Kind::Question,
        pane: pane.to_owned(),
        permission: None,
        tag: format!("chat-{pane}"),
        title: "Claude needs an answer".to_owned(),
        urgency: Urgency::High,
        wizz: false,
    }
}

/// The collapse id and thread of `notification` sent to `key`.
fn sent(key: &PublicKey, notification: &Notification) -> (Option<String>, String) {
    let sealed = Sealed::new(notification, COMPUTER.to_owned(), 1);
    let push = push(sealed, &scope(key), &notification.tag, fixed_seal).unwrap();
    let thread = json(&push.body)["aps"]["thread-id"]
        .as_str()
        .unwrap()
        .to_owned();
    (push.collapse_id, thread)
}

#[test]
fn the_same_pane_on_two_computers_never_collapses_or_groups() {
    let here = device().public_key();
    let question = asking("w1:p2");
    let (id, thread) = sent(&here, &question);
    let (other_id, other_thread) = sent(&other_key(), &question);
    assert_ne!(id, other_id);
    assert_ne!(thread, other_thread);
}

#[test]
fn a_newer_alert_for_one_chat_still_replaces_the_older() {
    let here = device().public_key();
    let first = asking("w1:p2");
    let finished = Notification {
        kind: Kind::TurnEnded,
        title: "Claude finished".to_owned(),
        urgency: Urgency::Normal,
        ..first.clone()
    };
    assert_eq!(sent(&here, &first), sent(&here, &finished));
    assert_ne!(sent(&here, &first).0, sent(&here, &asking("w1:p3")).0);
}

#[test]
fn nothing_in_the_clear_names_the_computer() {
    let here = device().public_key();
    let scope = scope(&here);
    assert_eq!(scope.len(), 16);
    assert!(scope.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let (id, thread) = sent(&here, &asking("w1:p2"));
    for text in [id.unwrap(), thread] {
        assert!(!text.to_uppercase().contains(&COMPUTER[..8]), "{text}");
    }
}

#[test]
fn every_collapse_id_fits_apns() {
    let scope = scope(&device().public_key());
    for tag in ["chat-w1:p2", &format!("chat-{}", "x".repeat(200))] {
        assert!(collapse_id(&scope, tag).len() <= 64);
    }
}

#[test]
fn the_push_relay_accepts_the_scoped_thread_and_collapse_id() {
    let key = device().public_key();
    let scope = scope(&key);
    let question = asking("w1:p2");
    let sealed = Sealed::new(&question, COMPUTER.to_owned(), 1);
    let allowed = clear(&scope, "w1:p2", &question.tag);
    assert!(allowed.contains(&thread_id(&scope, "w1:p2")));
    assert!(allowed.contains(&collapse_id(&scope, &question.tag)));
    SealedPush::build(&key, seal::INFO, &allowed, |sealer| {
        push(sealed, &scope, &question.tag, |plaintext| {
            Ok(sealer.seal(plaintext)?)
        })
    })
    .unwrap();
}
