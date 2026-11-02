use super::*;
use crate::service::retry_wait;
use std::time::Duration;

const MINUTE: Duration = Duration::from_secs(60);
const FIVE_MINUTES: Duration = Duration::from_secs(300);
const HALF_HOUR: Duration = Duration::from_secs(1800);
const HOUR: Duration = Duration::from_secs(3600);

fn github(error: herdr_github::Error) -> Error {
    Error::Github(Box::new(error))
}

fn offline() -> ureq::Error {
    ureq::Error::Io(std::io::Error::from(std::io::ErrorKind::ConnectionRefused))
}

/// The waits after `error` fails `times` times in a row.
fn waits(error: &Error, times: u32) -> Vec<Option<Duration>> {
    (1..=times)
        .map(|failures| retry_wait(error, None, failures))
        .collect()
}

#[test]
fn a_beads_failure_ignores_a_pause_github_left_behind() {
    assert_eq!(
        retry_wait(&Error::BeadsMissing, Some(HOUR), 1),
        Some(FIVE_MINUTES)
    );
    assert_eq!(
        retry_wait(&Error::BeadsMissing, None, 3),
        Some(FIVE_MINUTES)
    );
}

#[test]
fn a_github_failure_waits_as_long_as_github_asked() {
    let limited = github(herdr_github::Error::RateLimit { reset: None });
    assert_eq!(retry_wait(&limited, Some(HOUR), 1), Some(HOUR));
    assert_eq!(retry_wait(&limited, None, 1), Some(FIVE_MINUTES));
    let hidden = github(herdr_github::Error::RepositoryHidden {
        repository: "acme/hidden".into(),
        app_token: true,
    });
    assert_eq!(retry_wait(&hidden, None, 1), Some(HALF_HOUR));
    assert_eq!(retry_wait(&hidden, Some(FIVE_MINUTES), 1), Some(HALF_HOUR));
    assert_eq!(retry_wait(&hidden, Some(HOUR), 1), Some(HOUR));
}

#[test]
fn an_unreachable_provider_is_tried_again_sooner_each_time_less_often() {
    let seconds = |list: &[u64]| -> Vec<Option<Duration>> {
        list.iter()
            .map(|secs| Some(Duration::from_secs(*secs)))
            .collect()
    };
    let backoff = seconds(&[30, 60, 120, 240, 300, 300]);
    for error in [
        Error::LinearNetwork(Box::new(offline())),
        Error::LinearRead(std::io::Error::from(std::io::ErrorKind::TimedOut)),
        Error::LinearStatus {
            status: 503,
            message: None,
        },
        github(herdr_github::Error::Network(offline())),
        github(herdr_github::Error::Status(502)),
        Error::from(crate::gitlab::Error::Network {
            origin: "https://gitlab.com".into(),
            source: Box::new(offline()),
        }),
        Error::from(crate::gitlab::Error::Status(500)),
    ] {
        assert_eq!(waits(&error, 6), backoff, "{error:?}");
    }
    assert_eq!(
        retry_wait(&Error::LinearNetwork(Box::new(offline())), None, u32::MAX),
        Some(FIVE_MINUTES)
    );
    // A pause GitHub asked for still holds.
    let unreachable = github(herdr_github::Error::Network(offline()));
    assert_eq!(retry_wait(&unreachable, Some(HOUR), 1), Some(HOUR));
}

#[test]
fn a_refusal_that_is_not_the_network_keeps_the_long_wait() {
    let refused = Error::LinearStatus {
        status: 400,
        message: Some("Bad filter".into()),
    };
    assert_eq!(waits(&refused, 2), [Some(FIVE_MINUTES); 2]);
    let forbidden = github(herdr_github::Error::Status(403));
    assert_eq!(waits(&forbidden, 2), [Some(FIVE_MINUTES); 2]);
}

#[test]
fn a_rate_limited_linear_waits_as_long_as_linear_asked() {
    let limited = |retry_after| Error::LinearRateLimited { retry_after };
    assert_eq!(waits(&limited(None), 2), [Some(FIVE_MINUTES); 2]);
    let asked = Duration::from_secs(90);
    assert_eq!(retry_wait(&limited(Some(asked)), None, 1), Some(asked));
    assert_eq!(
        retry_wait(&limited(Some(Duration::from_secs(5))), None, 1),
        Some(MINUTE),
        "never sooner than a sync period"
    );
    assert_eq!(
        retry_wait(&limited(Some(Duration::from_secs(86_400))), None, 1),
        Some(HOUR),
        "never longer than Linear's quota window"
    );
}

#[test]
fn a_rejected_linear_key_waits_for_the_user() {
    assert_eq!(waits(&Error::LinearKeyRejected, 3), [None; 3]);
}

#[test]
fn linear_says_how_long_to_wait() {
    let mut headers = ureq::http::HeaderMap::new();
    assert_eq!(crate::linear::retry_after(&headers), None);
    headers.insert("retry-after", " 42 ".parse().unwrap());
    assert_eq!(
        crate::linear::retry_after(&headers),
        Some(Duration::from_secs(42))
    );
}
