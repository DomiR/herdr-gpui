//! A saved log reads back however many operations the log could take:
//! the file's bound covers the removals and checkpoints that count
//! against no other bound, not only the ordinary operations.

use super::*;

/// `log` saved, with its founding operation written again until the file
/// names `count` operations: one the log already holds, so reading it
/// back costs nothing more.
fn padded(log: &Log, count: usize) -> Vec<u8> {
    let bytes = saved::encode(log).unwrap();
    // The magic, the flag of a log never pruned, then the count.
    let at = b"sheprd-fleet-log-v1".len() + 1;
    assert_eq!(bytes[at - 1], 0);
    let held = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let first = log
        .ops()
        .find(|op| matches!(op.body(), Body::Found(_)))
        .unwrap()
        .encode();
    let mut padded = bytes[..at].to_vec();
    padded.extend_from_slice(&u32::try_from(count).unwrap().to_be_bytes());
    padded.extend_from_slice(&bytes[at + 4..]);
    for _ in held..count {
        padded.extend_from_slice(&u32::try_from(first.len()).unwrap().to_be_bytes());
        padded.extend_from_slice(&first);
    }
    padded
}

#[test]
fn the_saved_bound_covers_every_operation_a_log_holds() {
    // Ordinary operations, every live member's removals and checkpoints,
    // and the departed members' removals besides.
    let live = MAX_COMPUTERS + MAX_PHONES;
    assert!(
        saved::MAX_HELD
            >= MAX_OPS
                + MAX_REMOVALS
                + live * (MAX_REMOVALS_PER_SIGNER + MAX_CHECKPOINTS_PER_SIGNER)
    );
}

#[test]
fn a_file_past_the_old_bound_reads_back() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    let heads = log.heads().to_vec();
    // One past ordinary operations and departed members' removals: the
    // bound this file was once refused at.
    let loaded = saved::decode(&padded(&log, MAX_OPS + MAX_REMOVALS + 1), NOW).unwrap();
    assert_eq!(loaded.heads(), heads.as_slice());
    assert_eq!(loaded.state(), log.state());
    // Past what any log holds, it is still refused.
    assert_eq!(
        saved::decode(&padded(&log, saved::MAX_HELD + 1), NOW).err(),
        Some(Error::Bounds)
    );
}
