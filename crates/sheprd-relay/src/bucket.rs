//! Rate limits and bandwidth throttles, as token buckets in memory. A
//! restart resets them, and their keys never reach the disk or the logs.

pub use sheprd_dashboard::limit::{Limiter, Outcome, Rate, source_key};
use std::{sync::Mutex, time::Duration};
use tokio::time::Instant;

/// Holds a stream to a number of bytes per second.
///
/// Callers reserve bytes before sending them and sleep off any debt, so
/// concurrent senders queue fairly in reservation order and the long-run
/// rate never exceeds the limit. One second of traffic may pass at once.
#[derive(Debug)]
pub struct Throttle {
    per_second: u64,
    state: Mutex<Level>,
}

#[derive(Debug)]
struct Level {
    /// Bytes that may go now; negative while senders owe time.
    bytes: i128,
    updated: Instant,
}

impl Throttle {
    /// A throttle at `per_second` bytes, or `None` for unlimited (zero).
    #[must_use]
    pub fn new(per_second: u64) -> Option<Self> {
        (per_second > 0).then(|| Self {
            per_second,
            state: Mutex::new(Level {
                bytes: i128::from(per_second),
                updated: Instant::now(),
            }),
        })
    }

    /// Reserves `bytes`, returning how long the caller must wait before
    /// sending them.
    pub fn reserve(&self, bytes: usize) -> Duration {
        let rate = i128::from(self.per_second);
        let mut level = match self.state.lock() {
            Ok(level) => level,
            Err(poisoned) => poisoned.into_inner(),
        };
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(level.updated).as_nanos();
        let gained = i128::try_from(elapsed)
            .unwrap_or(i128::MAX)
            .saturating_mul(rate)
            / 1_000_000_000;
        level.bytes = level.bytes.saturating_add(gained).min(rate);
        level.updated = now;
        level.bytes -= i128::try_from(bytes).unwrap_or(i128::MAX);
        if level.bytes >= 0 {
            return Duration::ZERO;
        }
        let nanos = (-level.bytes).saturating_mul(1_000_000_000) / rate;
        Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
    }

    /// Waits until `bytes` may be sent; how long that took.
    pub async fn take(&self, bytes: usize) -> Duration {
        let wait = self.reserve(bytes);
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
        wait
    }
}
