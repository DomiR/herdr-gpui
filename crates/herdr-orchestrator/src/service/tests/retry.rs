//! The worker's record of sources waiting out a failure.

use super::*;
use crate::{Provider, SourceKey, service::retry::Retry};

fn source(provider: Provider) -> SourceKey {
    SourceKey {
        provider,
        host: "linear.app".into(),
        repository: "acme".into(),
    }
}

fn offline() -> Error {
    Error::LinearNetwork(Box::new(ureq::Error::Io(std::io::Error::from(
        std::io::ErrorKind::ConnectionRefused,
    ))))
}

#[test]
fn network_failures_in_a_row_wait_longer_until_one_succeeds() {
    let linear = source(Provider::Linear);
    let mut retry = Retry::default();
    let fail = |retry: &mut Retry| retry.failed(&linear, &offline(), None);
    assert_eq!(fail(&mut retry), Some(Duration::from_secs(30)));
    assert!(retry.waits(&linear, Instant::now()));
    assert_eq!(fail(&mut retry), Some(Duration::from_secs(60)));
    assert_eq!(fail(&mut retry), Some(Duration::from_secs(120)));
    retry.succeeded(&linear);
    assert_eq!(fail(&mut retry), Some(Duration::from_secs(30)));
    retry.clear();
    assert!(!retry.waits(&linear, Instant::now()));
    assert_eq!(fail(&mut retry), Some(Duration::from_secs(30)));
}

#[test]
fn a_wait_ends_on_time_or_when_the_user_acts() {
    let (linear, github) = (source(Provider::Linear), source(Provider::Github));
    let mut retry = Retry::default();
    retry.failed(&linear, &Error::LinearKeyRejected, None);
    retry.failed(&github, &offline(), None);
    let later = Instant::now() + Duration::from_secs(3600 * 24 * 365);
    assert!(
        retry.waits(&linear, later),
        "a refused key waits for the user"
    );
    assert!(!retry.waits(&github, later));
    retry.forget(Provider::Linear);
    assert!(!retry.waits(&linear, Instant::now()));
}
