//! The fleet's part of `status --json` stays small whatever the agent
//! keeps: a suspension's pairs are the agent's own, and the largest report
//! the bounds allow takes at most half of what Sheprd reads
//! ([`MAX_REPORT_BYTES`]), the rest left for the status around it.

use super::*;
#[cfg(feature = "runtime")]
use crate::status::MAX_REPORT_BYTES;

/// The largest fleet report the bounds allow: every member suspended by
/// as many removals as are remembered, every desktop cut off with the
/// longest label (each character escaped in JSON), every member waiting.
#[cfg_attr(not(feature = "runtime"), allow(dead_code))]
pub(crate) fn largest() -> Report {
    let count = |n: usize| u8::try_from(n).unwrap();
    let member = |n: u8| Id([n; 32]);
    let suspension = |n: u8| Suspension {
        removals: (0..count(MAX_CAUSES))
            .map(|cause| {
                (
                    Id([cause.wrapping_add(n); 32]),
                    Cause {
                        signer: member(n),
                        seq: u64::MAX,
                    },
                )
            })
            .collect(),
        overflow: true,
        pairs: BTreeSet::new(),
    };
    Report {
        allowed: access::MAX_MEMBERS,
        denied: access::MAX_MEMBERS,
        digest: "f".repeat(64),
        fleet: member(0xf1),
        removed: access::MAX_REMOVED,
        crowded: (0..count(access::MAX_MEMBERS)).map(member).collect(),
        refused: true,
        revoked: (0..count(crate::authorized::MAX_ENTRIES))
            .map(|n| Revoked {
                label: "\"".repeat(crate::authorized::MAX_LABEL),
                tunnel: member(n),
            })
            .collect(),
        suspended: (0..count(access::MAX_MEMBERS))
            .map(|n| (member(n), suspension(n)))
            .collect(),
    }
}

// The limit is `status`'s, built with the agent's runtime.
#[cfg(feature = "runtime")]
#[test]
fn the_largest_fleet_report_fits_its_budget_with_room_to_spare() {
    let printed = serde_json::to_vec_pretty(&largest()).unwrap();
    assert!(
        printed.len() * 2 <= MAX_REPORT_BYTES,
        "{} bytes of {MAX_REPORT_BYTES}",
        printed.len()
    );
}

#[test]
fn a_report_leaves_out_the_pairs_a_suspension_keeps() {
    let host = Host::new();
    host.enroll(2);
    host.apply(file(&[(1, true, 0), (2, true, 0)], &[], &[]))
        .unwrap();
    host.suspend(1, &[removal(0x51, &[2])]);
    let held = Held::load(host.state()).unwrap();
    assert!(!held.suspended[&id(2)].pairs.is_empty());
    let report = held.report(&Authorized::default()).unwrap().unwrap();
    assert!(report.suspended[&id(2)].pairs.is_empty());
    let printed = serde_json::to_string(&report).unwrap();
    assert!(!printed.contains("\"pairs\""), "{printed}");
    assert!(report.suspended[&id(2)].removals.contains_key(&id(0x51)));
}
