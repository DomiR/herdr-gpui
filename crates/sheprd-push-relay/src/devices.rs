//! How many devices the relay serves, counted in memory by the device id
//! credentials carry (an HMAC of an App Attest key id, so a device here is
//! one attested key). Nothing is written down, a restart starts over, and
//! the ids themselves are never kept: each is reduced to a hash under a key
//! drawn when the relay starts, which is all [`Devices`] holds, and only its
//! counts ever leave it.

use crate::credential::DeviceId;
use std::{
    collections::{HashMap, HashSet},
    fmt,
    hash::{BuildHasher as _, RandomState},
};

/// The most devices each count tracks, which bounds memory to a few MiB.
/// Past it, what a new device does is counted as skipped instead.
pub const MAX_DEVICES: usize = 131_072;
pub const DAY_S: u64 = 24 * 60 * 60;
pub const WEEK_S: u64 = 7 * DAY_S;

/// What [`Devices`] counts, for the dashboard and `/metrics`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct Counts {
    /// Distinct devices that attested since the relay started.
    pub attested: u64,
    /// Distinct devices a push went to APNs for in the last 24 hours.
    pub active_day: u64,
    /// The same, over the last seven days.
    pub active_week: u64,
    /// Attestations by devices left out of `attested` because it was full.
    /// Each attestation counts, so one device may count more than once.
    pub skipped_attestations: u64,
    /// Pushes for devices left out of the active counts because they were
    /// full. Each push counts, so one device may count many times.
    pub skipped_pushes: u64,
}

/// Attested devices, and when each last pushed.
pub struct Devices {
    attested: HashSet<u64>,
    /// Keys the hashes, so a hash says nothing without this process.
    hasher: RandomState,
    /// Each device's last push, in Unix seconds.
    pushed: HashMap<u64, u64>,
    skipped_attestations: u64,
    skipped_pushes: u64,
    /// When the oldest push in a full `pushed` turns a week old, so the
    /// next sweep can free a slot: until then a full count skips new
    /// devices without scanning.
    sweep_at_s: u64,
}

impl Default for Devices {
    fn default() -> Self {
        Self {
            attested: HashSet::new(),
            hasher: RandomState::new(),
            pushed: HashMap::new(),
            skipped_attestations: 0,
            skipped_pushes: 0,
            sweep_at_s: 0,
        }
    }
}

/// Counts only: the hashes stay inside.
impl fmt::Debug for Devices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Devices")
            .field("attested", &self.attested.len())
            .field("pushed", &self.pushed.len())
            .field("skipped_attestations", &self.skipped_attestations)
            .field("skipped_pushes", &self.skipped_pushes)
            .finish()
    }
}

impl Devices {
    fn key(&self, device: &DeviceId) -> u64 {
        self.hasher.hash_one(device)
    }

    /// Counts `device` as attested.
    pub fn attested(&mut self, device: &DeviceId) {
        let key = self.key(device);
        if self.attested.len() >= MAX_DEVICES && !self.attested.contains(&key) {
            self.skipped_attestations = self.skipped_attestations.saturating_add(1);
            return;
        }
        self.attested.insert(key);
    }

    /// Records a push for `device` at `now_s`. A push that arrives after a
    /// later one for the same device keeps the later time. When full,
    /// devices quiet for a week are forgotten first, since no count needs
    /// them, but only once one can be: a full count of recent devices
    /// skips newcomers without scanning.
    pub fn pushed(&mut self, device: &DeviceId, now_s: u64) {
        let key = self.key(device);
        if let Some(last_s) = self.pushed.get_mut(&key) {
            *last_s = (*last_s).max(now_s);
            return;
        }
        if self.pushed.len() >= MAX_DEVICES {
            if now_s >= self.sweep_at_s {
                self.pushed
                    .retain(|_, last_s| now_s.saturating_sub(*last_s) < WEEK_S);
                self.sweep_at_s = self
                    .pushed
                    .values()
                    .min()
                    .map_or(0, |oldest_s| oldest_s.saturating_add(WEEK_S));
            }
            if self.pushed.len() >= MAX_DEVICES {
                self.skipped_pushes = self.skipped_pushes.saturating_add(1);
                return;
            }
        }
        // A delayed push can be older than every device swept past: its
        // own week ending is then the earliest a slot frees.
        self.sweep_at_s = self.sweep_at_s.min(now_s.saturating_add(WEEK_S));
        self.pushed.insert(key, now_s);
    }

    /// The counts at `now_s`.
    #[must_use]
    pub fn counts(&self, now_s: u64) -> Counts {
        let within = |window: u64| {
            self.pushed
                .values()
                .filter(|last_s| now_s.saturating_sub(**last_s) < window)
                .count() as u64
        };
        Counts {
            attested: self.attested.len() as u64,
            active_day: within(DAY_S),
            active_week: within(WEEK_S),
            skipped_attestations: self.skipped_attestations,
            skipped_pushes: self.skipped_pushes,
        }
    }
}
