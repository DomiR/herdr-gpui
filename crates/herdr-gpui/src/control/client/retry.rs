use crate::control::ErrorCode;
use std::time::Duration;

/// How long to wait before asking a busy app again: doubling from `FIRST` to
/// `CAP`, from the start again once it answers.
pub(super) struct Backoff {
    next: Duration,
}

impl Backoff {
    pub(super) const FIRST: Duration = Duration::from_millis(100);
    pub(super) const CAP: Duration = Duration::from_secs(2);

    pub(super) fn new() -> Self {
        Self { next: Self::FIRST }
    }

    /// Returns the current delay, then doubles it up to [`Self::CAP`].
    pub(super) fn next(&mut self) -> Duration {
        let delay = self.next;
        self.next = self.next.saturating_add(self.next).min(Self::CAP);
        delay
    }

    pub(super) fn reset(&mut self) {
        self.next = Self::FIRST;
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

/// The error code the app answered with, if it answered with one.
pub(super) fn rejected(error: &crate::Error) -> Option<ErrorCode> {
    match error {
        crate::Error::Control(crate::control::Error::Rejected { code, .. }) => Some(*code),
        _ => None,
    }
}

/// Whether the app refused a request before acting on it.
pub(super) fn is_busy(error: &crate::Error) -> bool {
    rejected(error) == Some(ErrorCode::Busy)
}

#[cfg(test)]
mod tests;
