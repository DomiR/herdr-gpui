//! A host kept in a checkpoint's state has no addition in the log: another
//! addition of it changes nothing, so a record concurrent with that
//! addition still counts.

use super::*;

#[test]
fn a_record_beside_a_repeated_addition_of_a_checkpointed_host_counts() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    let (intro, ca) = ca_host(40, HostKind::Sheprd);
    act(&mut log, &a, Body::HostAdd(intro.clone()));
    // Pruned: the host lives on in the checkpoint's state alone.
    let hash = log.state().hash().unwrap();
    let checkpoint = act(&mut log, &a, Body::Checkpoint(hash));
    log.prune(&checkpoint.id()).unwrap();
    let id = intro.id().unwrap();
    // Concurrently, b adds the host again and a carries its moved record;
    // across records, the two fall in either graph order.
    for seed in 41..61 {
        let mut log = log.clone();
        let mut moved = record(id, seed, 2);
        moved.route = intro.record.route.clone();
        let signature = Some(raw_signature(&ca, &moved.message().unwrap()));
        let again = sign(&log, &b, Body::HostAdd(intro.clone()));
        let seen = sign(
            &log,
            &a,
            Body::HostSeen(HostSeen {
                record: moved,
                signature,
            }),
        );
        apply(&mut log, again);
        apply(&mut log, seen);
        let host = &log.state().hosts[&id];
        assert_eq!(host.record.rev, 2, "seed {seed}");
        assert_eq!(host.record.identity, format!("identity{seed}"));
    }
}

#[test]
fn a_record_after_either_of_two_concurrent_additions_counts() {
    let (a, b) = (computer(1), computer(2));
    let mut base = found(&a);
    admit(&mut base, &a, &b);
    let (intro, ca) = ca_host(40, HostKind::Sheprd);
    let id = intro.id().unwrap();
    // Across records, the additions and the record fall in every order.
    for seed in 41..61 {
        let mut log = base.clone();
        let by_a = sign(&log, &a, Body::HostAdd(intro.clone()));
        let by_b = sign(&log, &b, Body::HostAdd(intro.clone()));
        // b carries the host's moved record, having seen only its own
        // addition.
        let mut seen_by_b = log.clone();
        apply(&mut seen_by_b, by_b.clone());
        let mut moved = record(id, seed, 2);
        moved.route = intro.record.route.clone();
        let signature = Some(raw_signature(&ca, &moved.message().unwrap()));
        let seen = sign(
            &seen_by_b,
            &b,
            Body::HostSeen(HostSeen {
                record: moved,
                signature,
            }),
        );
        for op in [by_a, by_b, seen] {
            apply(&mut log, op);
        }
        let host = &log.state().hosts[&id];
        assert_eq!(host.record.rev, 2, "seed {seed}");
        assert_eq!(host.record.identity, format!("identity{seed}"));
    }
}
