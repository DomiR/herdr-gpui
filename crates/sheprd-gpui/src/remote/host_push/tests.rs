use super::*;
use sheprd_remote_control::control::push::{Mode, Outcome};

#[test]
fn a_host_that_printed_nothing_is_unknown() {
    assert!(parse(b"").unwrap().is_none());
    assert!(parse(b" \n").unwrap().is_none());
}

#[test]
fn a_host_answers_its_mode_and_last_pushes() {
    let status = parse(br#"{"mode":"push_relay","alert":{"at_ms":20,"outcome":"delivered"}}"#)
        .unwrap()
        .unwrap();
    assert_eq!(status.mode, Mode::PushRelay);
    assert_eq!(status.alert.unwrap().outcome, Outcome::Delivered);
}

#[test]
fn an_answer_it_cannot_read_is_an_error_not_a_mode() {
    let error = parse(b"not json").unwrap_err();
    assert!(matches!(error, Error::Answer(_)));
    let line = answer("box".into(), Err(error));
    assert_eq!(line.name, "box");
    assert_eq!(line.status, None);
    assert_eq!(
        line.error.as_deref(),
        Some("The host's push status is not one this app can read")
    );
}

#[test]
fn the_script_asks_without_installing() {
    assert!(SCRIPT.contains("push-status"));
    assert!(!SCRIPT.contains("install"));
}
