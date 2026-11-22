//! Counting devices in memory: bounded, and never naming one.

use crate::devices::{DAY_S, Devices, MAX_DEVICES, WEEK_S};

fn device(index: usize) -> [u8; 16] {
    let mut id = [0xee; 16];
    id[..8].copy_from_slice(&(index as u64).to_be_bytes());
    id
}

#[test]
fn a_device_counts_once_however_often_it_appears() {
    let mut devices = Devices::default();
    for _ in 0..3 {
        devices.attested(&device(1));
        devices.pushed(&device(1), 1_000);
    }
    let counts = devices.counts(1_000);
    assert_eq!(
        (counts.attested, counts.active_day, counts.active_week),
        (1, 1, 1)
    );
}

#[test]
fn full_counts_forget_quiet_devices_then_skip_new_ones() {
    let mut devices = Devices::default();
    for index in 0..MAX_DEVICES {
        devices.pushed(&device(index), 0);
        devices.attested(&device(index));
    }
    // Each skipped event counts, a repeat included, and the two apart.
    devices.attested(&device(MAX_DEVICES));
    devices.attested(&device(MAX_DEVICES));
    devices.pushed(&device(MAX_DEVICES), DAY_S);
    let counts = devices.counts(DAY_S);
    assert_eq!((counts.skipped_attestations, counts.skipped_pushes), (2, 1));
    // A week on, every device is quiet, so a new one makes room.
    devices.pushed(&device(MAX_DEVICES), WEEK_S);
    let counts = devices.counts(WEEK_S);
    assert_eq!((counts.active_week, counts.skipped_pushes), (1, 1));
}

#[test]
fn a_full_count_of_recent_devices_waits_for_the_oldest_to_age_out() {
    let mut devices = Devices::default();
    devices.pushed(&device(0), 0);
    for index in 1..MAX_DEVICES {
        devices.pushed(&device(index), DAY_S);
    }
    // The first sweep frees nothing: every device pushed this week.
    devices.pushed(&device(MAX_DEVICES), DAY_S);
    devices.pushed(&device(MAX_DEVICES + 1), DAY_S + 1);
    assert_eq!(devices.counts(DAY_S + 1).skipped_pushes, 2);
    // Once the oldest turns a week old, it makes room.
    devices.pushed(&device(MAX_DEVICES + 2), WEEK_S);
    let counts = devices.counts(WEEK_S);
    assert_eq!(
        (counts.active_week, counts.skipped_pushes),
        (MAX_DEVICES as u64, 2)
    );
}

#[test]
fn a_delayed_newcomer_brings_the_next_sweep_forward() {
    let mut devices = Devices::default();
    devices.pushed(&device(0), 0);
    devices.pushed(&device(1), 0);
    for index in 2..MAX_DEVICES {
        devices.pushed(&device(index), 2 * DAY_S);
    }
    // A week on, a newcomer sweeps out the two quiet devices; the next
    // sweep would wait for the oldest left, a week after 2 * DAY_S.
    devices.pushed(&device(MAX_DEVICES), WEEK_S);
    // A delayed push from DAY_S takes the last slot, and ages out first.
    devices.pushed(&device(MAX_DEVICES + 1), DAY_S);
    devices.pushed(&device(MAX_DEVICES + 2), DAY_S + WEEK_S);
    let counts = devices.counts(DAY_S + WEEK_S);
    assert_eq!(counts.skipped_pushes, 0);
    assert_eq!(counts.active_week, MAX_DEVICES as u64);
}

#[test]
fn a_late_push_never_moves_a_device_back() {
    let mut devices = Devices::default();
    devices.pushed(&device(1), 2 * DAY_S);
    devices.pushed(&device(1), 10);
    assert_eq!(devices.counts(3 * DAY_S - 1).active_day, 1);
}

#[test]
fn debug_shows_counts_alone() {
    let mut devices = Devices::default();
    devices.attested(&device(1));
    let text = format!("{devices:?}");
    assert_eq!(
        text,
        "Devices { attested: 1, pushed: 0, skipped_attestations: 0, skipped_pushes: 0 }"
    );
}
