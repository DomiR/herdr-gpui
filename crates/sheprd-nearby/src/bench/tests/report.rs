use super::*;
use crate::bench::report::percentile;

fn ms(values: &[u64]) -> Vec<Duration> {
    values.iter().copied().map(Duration::from_millis).collect()
}

fn fast() -> Throughput {
    Throughput::of(100_000, 100_000, Duration::from_secs(1))
}

#[test]
fn percentiles_are_nearest_rank() {
    let sorted: Vec<f64> = (1..=100).map(f64::from).collect();
    assert_eq!(percentile(&sorted, 50), Some(50.0));
    assert_eq!(percentile(&sorted, 95), Some(95.0));
    assert_eq!(percentile(&[7.0], 95), Some(7.0));
    assert_eq!(percentile(&[1.0, 2.0], 50), Some(1.0));
    assert_eq!(percentile(&[], 50), None);
}

#[test]
fn latency_sorts_its_samples_and_counts_the_lost() {
    let latency = Latency::of(&ms(&[30, 10, 20]), 2);
    assert_eq!(latency.answered, 3);
    assert_eq!(latency.lost, 2);
    assert_eq!(latency.p50_ms, Some(20.0));
    assert_eq!(latency.min_ms, Some(10.0));
    assert_eq!(latency.max_ms, Some(30.0));
    assert_eq!(Latency::of(&[], 0).p95_ms, None);
}

#[test]
fn throughput_is_kilobytes_of_a_thousand_bytes() {
    let throughput = Throughput::of(64_000, 64_000, Duration::from_millis(500));
    assert!((throughput.kb_per_s - 128.0).abs() < 1e-9);
    assert!((throughput.seconds - 0.5).abs() < 1e-9);
    assert_eq!(Throughput::of(1, 1, Duration::ZERO).kb_per_s, 0.0);
}

#[test]
fn the_bar_needs_both_directions_every_byte_and_a_quick_p95() {
    let quick = Latency::of(&ms(&[40; 20]), 0);
    let report = |echo: Latency, upload, download, stability| {
        Report::new(
            String::new(),
            "desk".into(),
            echo,
            upload,
            download,
            stability,
        )
    };
    assert!(report(quick.clone(), fast(), fast(), None).bar.met);

    let slow = Throughput::of(100_000, 100_000, Duration::from_secs(2));
    assert!(!report(quick.clone(), slow.clone(), fast(), None).bar.met);
    assert!(!report(quick.clone(), fast(), slow, None).bar.met);

    let short = Throughput::of(100_000, 90_000, Duration::from_millis(100));
    assert!(!report(quick.clone(), fast(), short, None).bar.met);

    let laggy = Latency::of(&ms(&[40, 40, 40, 200]), 0);
    assert!(!report(laggy, fast(), fast(), None).bar.met);
    let lossy = Latency::of(&ms(&[40; 20]), 1);
    assert!(!report(lossy, fast(), fast(), None).bar.met);

    let dropped = Stability {
        minutes: 10,
        disconnects: 1,
        latency: quick.clone(),
    };
    assert!(!report(quick.clone(), fast(), fast(), Some(dropped)).bar.met);
}

#[test]
fn the_report_prints_as_versioned_json() {
    let report = Report::new(
        "1 m".into(),
        "desk".into(),
        Latency::of(&ms(&[10]), 0),
        fast(),
        fast(),
        None,
    );
    let json: serde_json::Value = serde_json::to_value(&report).unwrap();
    assert_eq!(json["version"], 1);
    assert_eq!(json["label"], "1 m");
    assert_eq!(json["bar"]["kb_per_s"], 64.0);
    assert_eq!(json["bar"]["p95_ms"], 150.0);
    assert_eq!(json["stability"], serde_json::Value::Null);
    assert_eq!(json["upload"]["bytes"], 100_000);
}
