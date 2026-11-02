#![allow(clippy::unwrap_used)]
use super::*;
use std::io;

fn rejected(code: ErrorCode) -> crate::Error {
    crate::control::Error::Rejected {
        code,
        message: "busy".into(),
    }
    .into()
}

#[test]
fn backoff_doubles_to_its_cap_and_resets() {
    let mut backoff = Backoff::new();
    let delays: Vec<_> = (0..8).map(|_| backoff.next()).collect();
    assert_eq!(
        delays,
        vec![
            Duration::from_millis(100),
            Duration::from_millis(200),
            Duration::from_millis(400),
            Duration::from_millis(800),
            Duration::from_millis(1600),
            Duration::from_millis(2000),
            Duration::from_millis(2000),
            Duration::from_millis(2000),
        ]
    );
    backoff.reset();
    assert_eq!(backoff.next(), Duration::from_millis(100));
}

#[test]
fn is_busy_only_matches_control_busy() {
    assert!(is_busy(&rejected(ErrorCode::Busy)));
    for error in [
        rejected(ErrorCode::WindowBusy),
        rejected(ErrorCode::Timeout),
        rejected(ErrorCode::NotFound),
        crate::control::Error::Io(io::Error::from(io::ErrorKind::BrokenPipe)).into(),
    ] {
        assert!(!is_busy(&error));
    }
}
