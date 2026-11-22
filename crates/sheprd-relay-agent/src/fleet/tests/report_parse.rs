//! The largest `status --json` the agent's bounds allow, its fleet part
//! included, reads back within what Sheprd reads, with room to spare.

use super::report_size::largest;
use crate::status::{MAX_RELAY_ERROR, MAX_REPORT_BYTES, RelayState, Report};

/// The longest host key file read, spelled out: every one OpenSSH makes.
const HOST_KEY: usize = 16 * 1024;
/// The live status file's bound, which holds the relays' states.
const LIVE: usize = 64 * 1024;

#[test]
fn the_largest_status_reads_back_within_the_limit_with_room_to_spare() {
    // As many relays as the live status file holds, each with the longest
    // endpoint Sheprd hands out and the longest failure.
    let failed = RelayState::Failed {
        error: "\"".repeat(MAX_RELAY_ERROR),
    };
    let per_relay = 512 + MAX_RELAY_ERROR * 2 + 64;
    let relays = (0..LIVE / per_relay + 1)
        .map(|n| (format!("https://{n:04}{}", "r".repeat(500)), failed.clone()))
        .collect();
    let report = Report {
        authorized: crate::authorized::MAX_ENTRIES,
        fleet: Some(largest()),
        identity: "i".repeat(43),
        relays,
        route: "r".repeat(43),
        running: true,
        ssh_host_keys: (0..3)
            .map(|n| format!("ssh-rsa {n}{}", "A".repeat(HOST_KEY)))
            .collect(),
        version: "0.0.0-largest".to_owned(),
    };
    let printed = serde_json::to_vec_pretty(&report).unwrap();
    assert!(
        printed.len() * 4 <= MAX_REPORT_BYTES * 3,
        "{} bytes of {MAX_REPORT_BYTES}",
        printed.len()
    );
    assert_eq!(Report::parse(&printed).unwrap(), report);
}

#[test]
fn a_status_past_the_limit_is_refused() {
    let bytes = vec![b' '; MAX_REPORT_BYTES + 1];
    assert!(matches!(
        Report::parse(&bytes),
        Err(crate::Error::TooLarge { .. })
    ));
}
