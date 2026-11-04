//! The pure push scheduler for one document agent link.

use herdr_doc::Version;
use std::time::{Duration, Instant};

/// Versions `from` (what the agent last saw) to `to` (what this push brings).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Range {
    pub(crate) from: Version,
    pub(crate) to: Version,
}

#[derive(Debug)]
struct Flight {
    last_eligible: Option<Instant>,
    to: Version,
}

#[derive(Debug)]
pub(crate) struct Push {
    acked: Version,
    debounce: Duration,
    flight: Option<Flight>,
    last_eligible: Option<Instant>,
    retry_at: Option<Instant>,
    seen: Version,
}

impl Push {
    /// The agent was started or resynced at `version`.
    pub(crate) fn new(version: Version, debounce: Duration) -> Self {
        Self {
            acked: version,
            debounce,
            flight: None,
            last_eligible: None,
            retry_at: None,
            seen: version,
        }
    }

    pub(crate) fn acked(&self) -> Version {
        self.acked
    }

    pub(crate) fn in_flight(&self) -> bool {
        self.flight.is_some()
    }

    /// Apply a new debounce interval to the next pending deadline.
    pub(crate) fn set_debounce(&mut self, debounce: Duration) {
        self.debounce = debounce;
    }

    /// Record a document change while retaining one coalesced pending deadline.
    pub(crate) fn changed(&mut self, version: Version, own: bool, now: Instant) {
        if version == self.seen {
            return;
        }
        self.seen = version;
        if own {
            if self.flight.is_none() && self.last_eligible.is_none() {
                self.acked = version;
            }
            return;
        }
        self.last_eligible = Some(now);
    }

    /// The next quiet or retry deadline, if eligible work is pending.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        let quiet = self.last_eligible.map(|last| last + self.debounce);
        match (quiet, self.retry_at) {
            (Some(quiet), Some(retry)) => Some(quiet.max(retry)),
            (Some(quiet), None) => Some(quiet),
            (None, Some(retry)) => Some(retry),
            (None, None) => None,
        }
    }

    /// Start one flight when the pending work is due.
    pub(crate) fn take(&mut self, now: Instant, current: Version, eligible: bool) -> Option<Range> {
        if self.flight.is_some() || self.deadline().is_none_or(|deadline| now < deadline) {
            return None;
        }
        self.retry_at = None;
        let last_eligible = self.last_eligible.take();
        if !eligible {
            self.acked = current;
            self.seen = current;
            return None;
        }
        self.flight = Some(Flight {
            last_eligible,
            to: current,
        });
        Some(Range {
            from: self.acked,
            to: current,
        })
    }

    /// A Herdr acknowledgement advances only the range that was in flight.
    pub(crate) fn sent(&mut self) {
        let Some(flight) = self.flight.take() else {
            return;
        };
        self.acked = flight.to;
        if self.last_eligible.is_none() {
            self.acked = self.seen;
        }
    }

    /// Keep the unacknowledged range and defer its retry by the caller's backoff.
    pub(crate) fn failed(&mut self, now: Instant, delay: Duration) {
        if let Some(flight) = self.flight.take() {
            if self.last_eligible.is_none() {
                // A failure must not turn an elapsed edit deadline into a new quiet interval.
                self.last_eligible = flight.last_eligible;
            }
            self.retry_at = Some(now + delay);
        }
    }

    /// Resume normal scheduling after a readiness transition.
    pub(crate) fn ready_again(&mut self) {
        self.retry_at = None;
    }
}
