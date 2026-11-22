//! What a run measured, and whether it meets the bar for proposing
//! Bluetooth as a transport (`docs/relay-plan.md`, Measuring Bluetooth as
//! a transport): 64 KB/s both ways and a p95 round trip under 150 ms.

use serde::Serialize;
use std::time::Duration;

/// The throughput a transport needs both ways, in KB/s (1 KB = 1000 bytes).
pub(crate) const BAR_KB_PER_S: f64 = 64.0;
/// The p95 echo round trip it must stay under, in milliseconds.
pub(crate) const BAR_P95_MS: f64 = 150.0;

/// Round trips, in milliseconds.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(crate) struct Latency {
    pub(crate) answered: usize,
    pub(crate) lost: usize,
    pub(crate) p50_ms: Option<f64>,
    pub(crate) p95_ms: Option<f64>,
    pub(crate) min_ms: Option<f64>,
    pub(crate) max_ms: Option<f64>,
}

impl Latency {
    pub(crate) fn of(samples: &[Duration], lost: usize) -> Self {
        let mut ms: Vec<f64> = samples
            .iter()
            .map(|sample| sample.as_secs_f64() * 1000.0)
            .collect();
        ms.sort_by(f64::total_cmp);
        Self {
            answered: ms.len(),
            lost,
            p50_ms: percentile(&ms, 50),
            p95_ms: percentile(&ms, 95),
            min_ms: ms.first().copied(),
            max_ms: ms.last().copied(),
        }
    }
}

/// The nearest-rank percentile of sorted samples.
pub(crate) fn percentile(sorted: &[f64], percent: usize) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (percent * sorted.len()).div_ceil(100).max(1);
    sorted.get(rank - 1).copied()
}

/// Bytes moved one way.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(crate) struct Throughput {
    /// What the run set out to move.
    pub(crate) asked: u64,
    /// What arrived.
    pub(crate) bytes: u64,
    pub(crate) seconds: f64,
    pub(crate) kb_per_s: f64,
}

impl Throughput {
    pub(crate) fn of(asked: u64, bytes: u64, elapsed: Duration) -> Self {
        let seconds = elapsed.as_secs_f64();
        let kb_per_s = if seconds > 0.0 {
            bytes as f64 / 1000.0 / seconds
        } else {
            0.0
        };
        Self {
            asked,
            bytes,
            seconds,
            kb_per_s,
        }
    }

    /// Whether everything asked for arrived at the bar's rate.
    fn meets_bar(&self) -> bool {
        self.bytes >= self.asked && self.kb_per_s >= BAR_KB_PER_S
    }
}

/// Echoes once a second for a while, through disconnections.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(crate) struct Stability {
    pub(crate) minutes: u32,
    pub(crate) disconnects: usize,
    pub(crate) latency: Latency,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct Bar {
    pub(crate) kb_per_s: f64,
    pub(crate) p95_ms: f64,
    pub(crate) met: bool,
}

/// One run, printed as JSON.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct Report {
    pub(crate) version: u8,
    /// Where it ran, as the person running it describes it ("5 m, one
    /// wall, macOS to Linux").
    pub(crate) label: String,
    pub(crate) peer: String,
    pub(crate) echo: Latency,
    pub(crate) upload: Throughput,
    pub(crate) download: Throughput,
    pub(crate) stability: Option<Stability>,
    pub(crate) bar: Bar,
}

impl Report {
    pub(crate) fn new(
        label: String,
        peer: String,
        echo: Latency,
        upload: Throughput,
        download: Throughput,
        stability: Option<Stability>,
    ) -> Self {
        let under = |latency: &Latency| {
            latency.lost == 0 && latency.p95_ms.is_some_and(|p95| p95 < BAR_P95_MS)
        };
        let met = upload.meets_bar()
            && download.meets_bar()
            && under(&echo)
            && stability
                .as_ref()
                .is_none_or(|stability| under(&stability.latency) && stability.disconnects == 0);
        Self {
            version: 1,
            label,
            peer,
            echo,
            upload,
            download,
            stability,
            bar: Bar {
                kb_per_s: BAR_KB_PER_S,
                p95_ms: BAR_P95_MS,
                met,
            },
        }
    }
}
