//! Members that hold the same operations compute the same state, however
//! the operations reached them.

use super::*;

/// A busy history across three members' logs, with concurrent work.
fn busy() -> (Vec<Op>, State) {
    let (a, b, c, p) = (computer(1), computer(2), computer(3), phone(4));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &b, &p);
    let before = log.heads().to_vec();
    let next = |log: &Log, member: &Device| log.state().members[&member.id()].seq + 1;
    let ops = [
        sign_on(
            &log,
            &a,
            &before,
            next(&log, &a),
            Body::HostAdd(plain_host(40)),
        ),
        sign_on(&log, &b, &before, next(&log, &b), admit_body(&log, &c)),
        sign_on(&log, &p, &before, next(&log, &p), remove_body(&b)),
    ];
    for op in ops {
        apply(&mut log, op);
    }
    act(&mut log, &a, Body::HostAdd(plain_host(41)));
    let ops: Vec<Op> = log.ops().cloned().collect();
    (ops, log.state().clone())
}

#[test]
fn any_arrival_order_computes_the_same_state() {
    let (ops, expected) = busy();
    let founder = ops
        .iter()
        .find(|op| matches!(op.body(), Body::Found(_)))
        .unwrap()
        .clone();
    let rest: Vec<Op> = ops
        .iter()
        .filter(|op| op.id() != founder.id())
        .cloned()
        .collect();
    // Reversed, rotated, and one at a time: parents arrive after their
    // children and wait for them.
    for shift in 0..rest.len() {
        let mut order = rest.clone();
        order.reverse();
        order.rotate_left(shift);
        let mut log = Log::found(founder.clone()).unwrap();
        for op in order {
            let _ = log.insert(op, NOW).unwrap();
        }
        assert_eq!(log.pending(), 0);
        assert_eq!(log.state(), &expected, "shift {shift}");
    }
}

#[test]
fn spreading_to_n_members_converges() {
    let a = computer(1);
    let mut logs = vec![found(&a)];
    let devices: Vec<Device> = (2..=6).map(computer).collect();
    for device in &devices {
        admit(&mut logs[0], &a, device);
        // The joiner starts from everything the approver holds.
        let joined = copy(&logs[0]);
        logs.push(joined);
    }
    // Each member adds a host on its own copy.
    for (index, device) in devices.iter().enumerate() {
        let log = &mut logs[index + 1];
        act(
            log,
            device,
            Body::HostAdd(plain_host(40 + u8::try_from(index).unwrap())),
        );
    }
    // Two rounds of everyone syncing with everyone.
    for _ in 0..2 {
        for to in 0..logs.len() {
            for from in 0..logs.len() {
                if to != from {
                    let source = logs[from].ops().cloned().collect::<Vec<_>>();
                    for (_, result) in logs[to].insert_all(source, NOW) {
                        assert!(result.is_ok());
                    }
                }
            }
        }
    }
    let first = logs[0].state().clone();
    assert_eq!(first.hosts.len(), 6 + devices.len());
    for log in &logs {
        assert_eq!(log.state(), &first);
        assert_eq!(log.heads(), logs[0].heads());
    }
}

#[test]
fn a_member_away_during_an_update_catches_up() {
    let (a, b) = (computer(1), computer(2));
    let mut log_a = found(&a);
    admit(&mut log_a, &a, &b);
    let mut log_b = copy(&log_a);
    // b sleeps while a adds hosts, excludes b from one, and records moves.
    let host = plain_host(40).id().unwrap();
    act(&mut log_a, &a, Body::HostAdd(plain_host(40)));
    act(
        &mut log_a,
        &a,
        Body::Access(Access {
            host,
            member: b.id(),
            allow: false,
            rev: 1,
        }),
    );
    let mut moved = plain_host(40).record;
    moved.rev = 2;
    act(
        &mut log_a,
        &a,
        Body::HostSeen(HostSeen {
            record: moved,
            signature: None,
        }),
    );
    assert!(!log_b.state().hosts.contains_key(&host));
    sync(&mut log_b, &log_a);
    assert_eq!(log_b.state(), log_a.state());
    assert!(!log_b.state().allowed(&host, &b.id()));
}

#[test]
fn a_replayed_operation_changes_nothing() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    let admitted = admit(&mut log, &a, &b);
    let host = plain_host(40).id().unwrap();
    let deny = act(
        &mut log,
        &a,
        Body::Access(Access {
            host,
            member: b.id(),
            allow: false,
            rev: 1,
        }),
    );
    act(&mut log, &a, Body::HostAdd(plain_host(40)));
    let before = log.state().clone();
    for op in [admitted, deny] {
        assert_eq!(log.insert(op.clone(), NOW), Ok(Inserted::Known));
        // Re-encoded, it is the same operation.
        let again = Op::decode(&op.encode()).unwrap();
        assert_eq!(log.insert(again, NOW), Ok(Inserted::Known));
    }
    assert_eq!(log.state(), &before);
}

#[test]
fn a_child_of_a_refused_operation_is_refused_with_it() {
    let (a, outsider) = (computer(1), computer(9));
    let mut log = found(&a);
    let forged = sign(&log, &outsider, Body::HostAdd(plain_host(40)));
    let child = Unsigned {
        parents: vec![forged.id()],
        ..log.draft(a.id(), NOW, Body::HostAdd(plain_host(41)))
    }
    .sign(&a.key)
    .unwrap();
    let results = log.insert_all(vec![child.clone(), forged.clone()], NOW);
    let result = |id: OpId| results.iter().find(|(op, _)| *op == id).unwrap().1.clone();
    assert_eq!(result(forged.id()), Err(Error::Signer));
    assert_eq!(result(child.id()), Err(Error::ParentMissing));
    assert_eq!(log.state().hosts.len(), 1);
}

#[test]
fn operations_wait_for_parents_within_bounds() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    let mut other = found(&a);
    let admitted = admit(&mut other, &a, &b);
    let host = act(&mut other, &a, Body::HostAdd(plain_host(40)));
    assert_eq!(log.insert(host.clone(), NOW), Ok(Inserted::Pending));
    assert_eq!(log.pending(), 1);
    // Its parent arrives: both apply.
    assert_eq!(log.insert(admitted, NOW), Ok(Inserted::Applied));
    assert_eq!(log.pending(), 0);
    assert_eq!(log.state(), other.state());
    // One that waits too long is dropped.
    let mut late = found(&a);
    let admitted = admit(&mut other, &a, &computer(3));
    let child = act(&mut other, &a, Body::HostAdd(plain_host(41)));
    assert_eq!(late.insert(child, NOW), Ok(Inserted::Pending));
    let _ = late.insert(admitted.clone(), NOW + PENDING_TTL_MS + 1);
    assert_eq!(late.pending(), 1, "only the newcomer waits");
}

#[test]
fn the_pending_pool_is_bounded() {
    let a = computer(1);
    let mut log = found(&a);
    let mut other = found(&a);
    act(&mut other, &a, Body::HostAdd(plain_host(40)));
    let mut waiting = Vec::new();
    for rev in 2..=u64::try_from(MAX_PENDING).unwrap() + 2 {
        let mut moved = plain_host(40).record;
        moved.rev = rev;
        waiting.push(act(
            &mut other,
            &a,
            Body::HostSeen(HostSeen {
                record: moved,
                signature: None,
            }),
        ));
    }
    // Their first parent never arrives.
    for op in waiting {
        let _ = log.insert(op, NOW);
    }
    assert_eq!(log.pending(), MAX_PENDING);
}
