//! Rate limits as token buckets in memory, and the keys addresses are
//! limited by. A restart resets them, and their keys never reach the disk
//! or the logs.

use std::{
    collections::HashMap,
    hash::Hash,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};
/// The most keys one limiter tracks, which bounds its memory.
const MAX_KEYS: usize = 65_536;
const MINUTE_MS: u64 = 60_000;

/// A bucket's size and how fast it refills.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rate {
    pub burst: u32,
    pub per_minute: u32,
}

impl Rate {
    /// `per_minute`, with a quarter of it (at least five) allowed at once.
    #[must_use]
    pub fn per_minute(per_minute: u32) -> Self {
        Self {
            burst: (per_minute / 4).max(5),
            per_minute,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Bucket {
    /// Thousandths of a token, so slow refills are not rounded away.
    milli: u64,
    updated_ms: u64,
}

/// What a limiter says to one request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Allowed,
    /// The key spent its bucket: the caller is going too fast.
    Limited,
    /// The limiter tracks as many keys as it may and this one is new: the
    /// caller is refused, but did nothing wrong.
    Full,
}

/// One token bucket per key, up to a bound.
#[derive(Debug)]
pub struct Limiter<K> {
    buckets: HashMap<K, Bucket>,
    max_keys: usize,
    rate: Rate,
}

impl<K: Eq + Hash> Limiter<K> {
    #[must_use]
    pub fn new(rate: Rate) -> Self {
        Self::bounded(rate, MAX_KEYS)
    }

    #[must_use]
    pub fn bounded(rate: Rate, max_keys: usize) -> Self {
        Self {
            buckets: HashMap::new(),
            max_keys,
            rate,
        }
    }

    /// Whether `key` may go ahead at `now_ms`, spending one if so.
    pub fn allow(&mut self, key: K, now_ms: u64) -> bool {
        self.check(key, now_ms) == Outcome::Allowed
    }

    /// What [`Limiter::check`] would say for `key` at `now_ms`, without
    /// spending or tracking anything: a new key with room is allowed.
    #[must_use]
    pub fn peek(&self, key: &K, now_ms: u64) -> Outcome {
        match self.buckets.get(key) {
            Some(bucket) if refilled(*bucket, self.rate, now_ms) < 1000 => Outcome::Limited,
            Some(_) => Outcome::Allowed,
            // Full only if no slot would come free: `check` would drop the
            // buckets that refilled first.
            None if self.buckets.len() >= self.max_keys
                && self.buckets.values().all(|bucket| {
                    refilled(*bucket, self.rate, now_ms) < u64::from(self.rate.burst) * 1000
                }) =>
            {
                Outcome::Full
            }
            None => Outcome::Allowed,
        }
    }

    /// [`Limiter::allow`], saying why not. When every slot is held by a
    /// key that has spent something, a new key is refused as
    /// [`Outcome::Full`] rather than pushing out one that is being limited.
    /// `now_ms` must come from a monotonic clock: a clock stepping back
    /// would stop refills.
    pub fn check(&mut self, key: K, now_ms: u64) -> Outcome {
        let rate = self.rate;
        let full = u64::from(rate.burst) * 1000;
        if self.buckets.len() >= self.max_keys && !self.buckets.contains_key(&key) {
            self.buckets
                .retain(|_, bucket| refilled(*bucket, rate, now_ms) < full);
            if self.buckets.len() >= self.max_keys {
                return Outcome::Full;
            }
        }
        let bucket = self.buckets.entry(key).or_insert(Bucket {
            milli: full,
            updated_ms: now_ms,
        });
        bucket.milli = refilled(*bucket, rate, now_ms);
        bucket.updated_ms = now_ms;
        if bucket.milli < 1000 {
            return Outcome::Limited;
        }
        bucket.milli -= 1000;
        Outcome::Allowed
    }
}

/// `bucket`'s level at `now_ms`. A clock that went back adds nothing.
fn refilled(bucket: Bucket, rate: Rate, now_ms: u64) -> u64 {
    let elapsed = now_ms.saturating_sub(bucket.updated_ms);
    let gained = elapsed.saturating_mul(u64::from(rate.per_minute)) * 1000 / MINUTE_MS;
    bucket
        .milli
        .saturating_add(gained)
        .min(u64::from(rate.burst) * 1000)
}

/// What a source is limited by: its IPv4 address, or its IPv6 /64, since one
/// customer usually holds a whole /64.
#[must_use]
pub fn source_key(address: IpAddr) -> IpAddr {
    match address.to_canonical() {
        IpAddr::V6(address) => {
            let bits = u128::from(address) & !u128::from(u64::MAX);
            IpAddr::V6(Ipv6Addr::from(bits))
        }
        address => address,
    }
}

/// The wide block `address` falls in: its IPv6 /48 or IPv4 /24, where an
/// attacker's many addresses usually sit together.
#[must_use]
pub fn wide_key(address: IpAddr) -> IpAddr {
    match address.to_canonical() {
        IpAddr::V4(address) => IpAddr::V4(Ipv4Addr::from(u32::from(address) & 0xffff_ff00)),
        IpAddr::V6(address) => {
            IpAddr::V6(Ipv6Addr::from(u128::from(address) & !((1u128 << 80) - 1)))
        }
    }
}
