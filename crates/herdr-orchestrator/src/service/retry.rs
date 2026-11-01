//! When a source that failed to sync is tried again. A network failure is
//! tried again soon and less often with each failure in a row; a refusal
//! waits as long as the provider asked, and a refused credential waits for
//! the user.

use super::{Error, Provider, SourceKey};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// After a failure, a source waits this long before trying again.
const BACKOFF: Duration = Duration::from_secs(300);
/// A repository GitHub would not show this sign-in waits for the user to
/// install the App or change token; Refresh asks sooner.
const HIDDEN_BACKOFF: Duration = Duration::from_secs(1800);
/// The first wait after a network failure; each failure in a row doubles
/// it, up to [`BACKOFF`].
const NETWORK_BACKOFF: Duration = Duration::from_secs(30);
/// The bounds on the wait Linear's `Retry-After` asks for: sooner than a
/// sync period gains nothing, and an hour renews its whole quota.
const LINEAR_LIMITED: (Duration, Duration) = (Duration::from_secs(60), Duration::from_secs(3600));

/// How long a failed source waits, `None` until the user acts: Refresh, or
/// a new Linear key. `failures` counts this one and those in a row before
/// it. A rate limit or refused token pauses as long as the provider asked;
/// a repository the sign-in cannot see waits for a user action. The
/// `cooldown` applies only to a provider's own failures, so a Beads failure
/// never inherits a pause a GitHub request left behind.
pub fn retry_wait(error: &Error, cooldown: Option<Duration>, failures: u32) -> Option<Duration> {
    let floor = match error {
        Error::LinearKeyRejected => return None,
        Error::LinearRateLimited { retry_after } => {
            let (least, most) = LINEAR_LIMITED;
            return Some(retry_after.map_or(BACKOFF, |wait| wait.clamp(least, most)));
        }
        error if unreachable(error) => {
            let doubled = 2u32.saturating_pow(failures.saturating_sub(1));
            NETWORK_BACKOFF.saturating_mul(doubled).min(BACKOFF)
        }
        Error::Github(source) => match **source {
            herdr_github::Error::RepositoryHidden { .. } => HIDDEN_BACKOFF,
            _ => BACKOFF,
        },
        Error::Gitlab(source) if source.needs_user() => HIDDEN_BACKOFF,
        Error::Gitlab(_) => BACKOFF,
        _ => return Some(BACKOFF),
    };
    Some(cooldown.map_or(floor, |wait| wait.max(floor)))
}

/// A provider that could not be reached or answered with a server error,
/// which the network coming back or the server recovering soon fixes.
fn unreachable(error: &Error) -> bool {
    let server = |status: u16| (500..=599).contains(&status);
    match error {
        Error::LinearNetwork(_) | Error::LinearRead(_) => true,
        Error::LinearStatus { status, .. } => server(*status),
        Error::Github(source) => match **source {
            herdr_github::Error::Network(_) | herdr_github::Error::Read(_) => true,
            herdr_github::Error::Status(status) => server(status),
            _ => false,
        },
        Error::Gitlab(source) => match source {
            super::gitlab::Error::Network { .. } | super::gitlab::Error::Read(_) => true,
            super::gitlab::Error::Status(status) => server(*status),
            _ => false,
        },
        _ => false,
    }
}

/// Sources waiting out a failure, and how many times each failed in a row.
#[derive(Default)]
pub(super) struct Retry {
    /// Until when each waits; `None` until the user acts.
    waiting: Vec<(SourceKey, Option<Instant>)>,
    failures: HashMap<SourceKey, u32>,
}

impl Retry {
    /// Refresh: every source syncs now, and its waits start over.
    pub(super) fn clear(&mut self) {
        self.waiting.clear();
        self.failures.clear();
    }

    /// A new credential for `provider`: its sources sync at the next chance.
    pub(super) fn forget(&mut self, provider: Provider) {
        self.waiting
            .retain(|(source, _)| source.provider != provider);
        self.failures
            .retain(|source, _| source.provider != provider);
    }

    /// Whether `key` still waits at `now`.
    pub(super) fn waits(&mut self, key: &SourceKey, now: Instant) -> bool {
        self.waiting
            .retain(|(_, until)| until.is_none_or(|until| until > now));
        self.waiting.iter().any(|(waiting, _)| waiting == key)
    }

    /// Records that `key` failed with `error`, returning how long it waits.
    pub(super) fn failed(
        &mut self,
        key: &SourceKey,
        error: &Error,
        cooldown: Option<Duration>,
    ) -> Option<Duration> {
        let failures = self.failures.entry(key.clone()).or_default();
        *failures = failures.saturating_add(1);
        let wait = retry_wait(error, cooldown, *failures);
        let until = wait.map(|wait| Instant::now().checked_add(wait));
        // A wait too long for the clock to hold lasts until the user acts.
        self.waiting.push((key.clone(), until.flatten()));
        wait
    }

    pub(super) fn succeeded(&mut self, key: &SourceKey) {
        self.failures.remove(key);
    }

    #[cfg(test)]
    pub(super) fn waiting(&self) -> impl Iterator<Item = (&SourceKey, Option<Instant>)> {
        self.waiting.iter().map(|(source, until)| (source, *until))
    }
}
