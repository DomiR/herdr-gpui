//! Counters for Prometheus and the dashboard. Every label is one of a fixed
//! set: no series names an address, a token, a handle, a device, or
//! anything from a payload. Devices are only ever counted ([`devices`]).
//!
//! [`devices`]: crate::devices

use crate::{
    credential::DeviceId,
    devices::{Counts, Devices},
    relay::Reply,
};
use serde::Serialize;
use sheprd_apns::PushType;
pub use sheprd_dashboard::metrics::Login;
use sheprd_dashboard::{
    labels,
    metrics::{family, labelled, series, single},
};
use std::{
    fmt::Write as _,
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicU64, Ordering::Relaxed},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

labels! {
    /// How an attestation went.
    Attestation {
        Invalid => "invalid",
        Issued => "issued",
        Limited => "limited",
        Refused => "refused",
        Unavailable => "unavailable",
    }
}

impl From<Reply> for Attestation {
    /// An attestation is answered with no other reply: the rest count as
    /// refused.
    fn from(reply: Reply) -> Self {
        match reply {
            Reply::Invalid => Self::Invalid,
            Reply::Limited => Self::Limited,
            Reply::Unavailable => Self::Unavailable,
            _ => Self::Refused,
        }
    }
}

labels! {
    /// How a request for a challenge went.
    Challenge {
        Issued => "issued",
        Limited => "limited",
        Unavailable => "unavailable",
    }
}

impl From<Reply> for Challenge {
    fn from(reply: Reply) -> Self {
        match reply {
            Reply::Limited => Self::Limited,
            _ => Self::Unavailable,
        }
    }
}

labels! {
    /// Which rate limit refused a request; `apns` is Apple's own 429.
    Limit {
        Apns => "apns",
        Attest => "attest",
        Device => "device",
        Handle => "handle",
        Revoke => "revoke",
        Source => "source",
    }
}

labels! {
    /// A push's `apns-push-type`.
    Kind {
        Alert => "alert",
        Background => "background",
        LiveActivity => "liveactivity",
    }
}

impl From<PushType> for Kind {
    fn from(push_type: PushType) -> Self {
        match push_type {
            PushType::Alert => Self::Alert,
            PushType::Background => Self::Background,
            PushType::LiveActivity => Self::LiveActivity,
        }
    }
}

/// Every counter the relay keeps.
#[derive(Debug)]
pub struct Metrics {
    attestations: [AtomicU64; Attestation::ALL.len()],
    challenges: [AtomicU64; Challenge::ALL.len()],
    devices: Mutex<Devices>,
    kinds: [AtomicU64; Kind::ALL.len()],
    limited: [AtomicU64; Limit::ALL.len()],
    logins: [AtomicU64; Login::ALL.len()],
    pushes: [AtomicU64; Reply::ALL.len()],
    revocations: [AtomicU64; Reply::ALL.len()],
    started: Instant,
    started_unix: u64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            attestations: Default::default(),
            challenges: Default::default(),
            devices: Mutex::default(),
            kinds: Default::default(),
            limited: Default::default(),
            logins: Default::default(),
            pushes: Default::default(),
            revocations: Default::default(),
            started: Instant::now(),
            started_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
        }
    }
}

fn add(counter: &AtomicU64) {
    counter.fetch_add(1, Relaxed);
}

impl Metrics {
    fn devices(&self) -> std::sync::MutexGuard<'_, Devices> {
        self.devices.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn attestation(&self, outcome: Attestation) {
        add(&self.attestations[outcome.index()]);
    }

    pub fn challenge(&self, outcome: Challenge) {
        add(&self.challenges[outcome.index()]);
    }

    pub fn limited(&self, limit: Limit) {
        add(&self.limited[limit.index()]);
    }

    pub fn login(&self, outcome: Login) {
        add(&self.logins[outcome.index()]);
    }

    /// A push asked for, before any check.
    pub fn kind(&self, push_type: PushType) {
        add(&self.kinds[Kind::from(push_type).index()]);
    }

    /// What a push was answered.
    pub fn push(&self, reply: Reply) {
        add(&self.pushes[reply.index()]);
    }

    /// What a revocation was answered.
    pub fn revocation(&self, reply: Reply) {
        add(&self.revocations[reply.index()]);
    }

    /// A device that attested.
    pub fn attested(&self, device: &DeviceId) {
        self.devices().attested(device);
    }

    /// A device a push went to APNs for, at `now_s`.
    pub fn pushed(&self, device: &DeviceId, now_s: u64) {
        self.devices().pushed(device, now_s);
    }

    /// Totals for the dashboard at `now_s`.
    #[must_use]
    pub fn totals(&self, now_s: u64) -> Totals {
        let load = |counters: &[AtomicU64], index: usize| counters[index].load(Relaxed);
        Totals {
            attestations: labelled(
                Attestation::ALL,
                |outcome| load(&self.attestations, outcome.index()),
                Attestation::label,
            ),
            challenges: labelled(
                Challenge::ALL,
                |outcome| load(&self.challenges, outcome.index()),
                Challenge::label,
            ),
            devices: self.devices().counts(now_s),
            kinds: labelled(
                Kind::ALL,
                |kind| load(&self.kinds, kind.index()),
                Kind::label,
            ),
            limited: labelled(
                Limit::ALL,
                |limit| load(&self.limited, limit.index()),
                Limit::label,
            ),
            logins_failed: load(&self.logins, Login::Failed.index()),
            pushes: labelled(
                Reply::ALL,
                |reply| load(&self.pushes, reply.index()),
                Reply::label,
            ),
            revocations: labelled(
                Reply::ALL,
                |reply| load(&self.revocations, reply.index()),
                Reply::label,
            ),
            uptime_secs: self.started.elapsed().as_secs(),
        }
    }

    /// The Prometheus text exposition at `now_s`, format 0.0.4.
    #[must_use]
    pub fn render(&self, now_s: u64) -> String {
        let mut out = String::with_capacity(4096);
        let load = |counters: &[AtomicU64], index: usize| counters[index].load(Relaxed);
        family(
            &mut out,
            "sheprd_push_relay_build_info",
            "gauge",
            "The relay's version.",
        );
        let _ = writeln!(
            out,
            "sheprd_push_relay_build_info{{version=\"{}\"}} 1",
            env!("CARGO_PKG_VERSION")
        );
        single(
            &mut out,
            "sheprd_push_relay_start_time_seconds",
            "gauge",
            "When the relay started, in Unix seconds.",
            self.started_unix,
        );
        series(
            &mut out,
            "sheprd_push_relay_pushes_total",
            "counter",
            "Pushes by the reply the caller got.",
            "outcome",
            Reply::ALL,
            Reply::label,
            |reply| load(&self.pushes, reply.index()),
        );
        series(
            &mut out,
            "sheprd_push_relay_push_types_total",
            "counter",
            "Pushes asked for, by APNs push type.",
            "type",
            Kind::ALL,
            Kind::label,
            |kind| load(&self.kinds, kind.index()),
        );
        series(
            &mut out,
            "sheprd_push_relay_attestations_total",
            "counter",
            "Attestations by outcome.",
            "outcome",
            Attestation::ALL,
            Attestation::label,
            |outcome| load(&self.attestations, outcome.index()),
        );
        series(
            &mut out,
            "sheprd_push_relay_challenges_total",
            "counter",
            "Challenge requests by outcome.",
            "outcome",
            Challenge::ALL,
            Challenge::label,
            |outcome| load(&self.challenges, outcome.index()),
        );
        series(
            &mut out,
            "sheprd_push_relay_revocations_total",
            "counter",
            "Revocations by the reply the caller got.",
            "outcome",
            Reply::ALL,
            Reply::label,
            |reply| load(&self.revocations, reply.index()),
        );
        series(
            &mut out,
            "sheprd_push_relay_limited_total",
            "counter",
            "Requests refused by a rate limit, by limit.",
            "limit",
            Limit::ALL,
            Limit::label,
            |limit| load(&self.limited, limit.index()),
        );
        series(
            &mut out,
            "sheprd_push_relay_dashboard_logins_total",
            "counter",
            "Dashboard logins by outcome.",
            "outcome",
            Login::ALL,
            Login::label,
            |outcome| load(&self.logins, outcome.index()),
        );
        let devices = self.devices().counts(now_s);
        single(
            &mut out,
            "sheprd_push_relay_attested_devices",
            "gauge",
            "Distinct devices attested since the relay started.",
            devices.attested,
        );
        family(
            &mut out,
            "sheprd_push_relay_active_devices",
            "gauge",
            "Distinct devices pushed to within the window, since the relay started.",
        );
        for (window, count) in [("24h", devices.active_day), ("7d", devices.active_week)] {
            let _ = writeln!(
                out,
                "sheprd_push_relay_active_devices{{window=\"{window}\"}} {count}"
            );
        }
        family(
            &mut out,
            "sheprd_push_relay_device_events_skipped_total",
            "counter",
            "Attestations and pushes left out of the device counts because they were full; \
             one device can be skipped many times.",
        );
        for (count, skipped) in [
            ("attested", devices.skipped_attestations),
            ("active", devices.skipped_pushes),
        ] {
            let _ = writeln!(
                out,
                "sheprd_push_relay_device_events_skipped_total{{count=\"{count}\"}} {skipped}"
            );
        }
        out
    }
}

/// What the dashboard shows.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Totals {
    pub attestations: Vec<(&'static str, u64)>,
    pub challenges: Vec<(&'static str, u64)>,
    pub devices: Counts,
    pub kinds: Vec<(&'static str, u64)>,
    pub limited: Vec<(&'static str, u64)>,
    pub logins_failed: u64,
    pub pushes: Vec<(&'static str, u64)>,
    pub revocations: Vec<(&'static str, u64)>,
    pub uptime_secs: u64,
}
