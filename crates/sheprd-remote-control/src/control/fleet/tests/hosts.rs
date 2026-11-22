//! Hosts: adding, excluding, updating, and removing them.

use super::*;

fn access_body(host: HostId, member: &Device, allow: bool, rev: u64) -> Body {
    Body::Access(Access {
        host,
        member: member.id(),
        allow,
        rev,
    })
}

fn three() -> (Log, Device, Device, Device) {
    let (a, b, c) = (computer(1), computer(2), computer(3));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &c);
    (log, a, b, c)
}

#[test]
fn a_shared_host_reaches_every_computer_but_the_excluded() {
    let (mut log, a, b, c) = three();
    let intro = plain_host(40);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro));
    act(&mut log, &a, access_body(host, &c, false, 1));
    assert!(log.state().allowed(&host, &a.id()));
    assert!(log.state().allowed(&host, &b.id()));
    assert!(!log.state().allowed(&host, &c.id()));
    // A member that joins later gets it too.
    let d = computer(4);
    admit(&mut log, &b, &d);
    assert!(log.state().allowed(&host, &d.id()));
    // Allowed again later.
    act(&mut log, &b, access_body(host, &c, true, 2));
    assert!(log.state().allowed(&host, &c.id()));
}

#[test]
fn concurrent_allow_and_deny_resolve_to_deny() {
    let (mut log, a, b, c) = three();
    let host = plain_host(40).id().unwrap();
    act(&mut log, &a, Body::HostAdd(plain_host(40)));
    let before = log.heads().to_vec();
    let next = |member: &Device| log.state().members[&member.id()].seq + 1;
    let ops = [
        sign_on(&log, &a, &before, next(&a), access_body(host, &c, true, 1)),
        sign_on(&log, &b, &before, next(&b), access_body(host, &c, false, 1)),
    ];
    for op in ops {
        apply(&mut log, op);
    }
    assert!(!log.state().allowed(&host, &c.id()));
    assert_eq!(log.state().access[&(host, c.id())].rev, 1);
}

#[test]
fn an_access_rev_must_climb() {
    let (mut log, a, _, c) = three();
    let host = plain_host(40).id().unwrap();
    act(&mut log, &a, Body::HostAdd(plain_host(40)));
    act(&mut log, &a, access_body(host, &c, false, 1));
    // Refused: an agent merging by rev would order it with the deny.
    let op = as_drafted(&log, &a, access_body(host, &c, true, 1));
    assert_eq!(log.insert(op, NOW), Err(Error::Rev));
    assert!(!log.state().allowed(&host, &c.id()));
    act(&mut log, &a, access_body(host, &c, true, 2));
    assert!(log.state().allowed(&host, &c.id()));
}

#[test]
fn a_removal_wins_over_a_concurrent_update_and_a_later_add_restores() {
    let (mut log, a, b, _) = three();
    let (intro, ca) = ca_host(40, HostKind::Sheprd);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro.clone()));
    let before = log.heads().to_vec();
    let next = |member: &Device| log.state().members[&member.id()].seq + 1;
    let mut newer = record(host, 41, 2);
    newer.host = host;
    let signature = Some(raw_signature(&ca, &newer.message().unwrap()));
    let ops = [
        sign_on(&log, &a, &before, next(&a), Body::HostRemove(host)),
        sign_on(
            &log,
            &b,
            &before,
            next(&b),
            Body::HostSeen(HostSeen {
                record: newer,
                signature,
            }),
        ),
    ];
    for op in ops {
        apply(&mut log, op);
    }
    assert!(!log.state().hosts.contains_key(&host));
    // Adding it again after the removal brings it back.
    act(&mut log, &b, Body::HostAdd(intro));
    assert_eq!(log.state().hosts[&host].added_by, b.id());
}

#[test]
fn a_concurrent_add_does_not_survive_a_removal() {
    let (mut log, a, b, _) = three();
    let intro = plain_host(40);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro.clone()));
    let before = log.heads().to_vec();
    let next = |member: &Device| log.state().members[&member.id()].seq + 1;
    let ops = [
        sign_on(&log, &a, &before, next(&a), Body::HostRemove(host)),
        sign_on(&log, &b, &before, next(&b), Body::HostAdd(intro)),
    ];
    for op in ops {
        apply(&mut log, op);
    }
    assert!(!log.state().hosts.contains_key(&host));
}

#[test]
fn a_host_updates_itself_only_with_its_own_key() {
    let (mut log, a, b, _) = three();
    let (intro, ca) = ca_host(40, HostKind::Sheprd);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro));
    let mut newer = record(host, 42, 7);
    newer.host = host;
    // Signed by another key: kept in the log, changes nothing.
    let forged = raw_signature(&signing_key(99), &newer.message().unwrap());
    act(
        &mut log,
        &b,
        Body::HostSeen(HostSeen {
            record: newer.clone(),
            signature: Some(forged),
        }),
    );
    assert_eq!(log.state().hosts[&host].record.rev, 1);
    // Its own CA's signature: carried by any member, phones included.
    let p = phone(5);
    admit(&mut log, &a, &p);
    let signature = Some(raw_signature(&ca, &newer.message().unwrap()));
    act(
        &mut log,
        &p,
        Body::HostSeen(HostSeen {
            record: newer.clone(),
            signature,
        }),
    );
    assert_eq!(log.state().hosts[&host].record, newer);
    // An older record never replaces it.
    let mut older = record(host, 43, 3);
    older.host = host;
    let signature = Some(raw_signature(&ca, &older.message().unwrap()));
    act(
        &mut log,
        &b,
        Body::HostSeen(HostSeen {
            record: older,
            signature,
        }),
    );
    assert_eq!(log.state().hosts[&host].record, newer);
}

#[test]
fn a_plain_host_record_is_vouched_for_by_a_computer_and_keeps_its_keys() {
    let (mut log, a, b, _) = three();
    let intro = plain_host(40);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro.clone()));
    let mut moved = intro.record.clone();
    moved.rev = 2;
    moved.relays = vec!["https://fra1.example.org".into()];
    act(
        &mut log,
        &b,
        Body::HostSeen(HostSeen {
            record: moved.clone(),
            signature: None,
        }),
    );
    assert_eq!(log.state().hosts[&host].record, moved);
    // Its SSH host keys cannot change this way.
    let mut rekeyed = moved.clone();
    rekeyed.rev = 3;
    rekeyed.ssh_host_keys = vec![ssh_line(250)];
    act(
        &mut log,
        &b,
        Body::HostSeen(HostSeen {
            record: rekeyed,
            signature: None,
        }),
    );
    assert_eq!(log.state().hosts[&host].record, moved);
    // A phone cannot vouch for a plain host.
    let p = phone(5);
    admit(&mut log, &a, &p);
    let mut later = moved;
    later.rev = 4;
    let op = sign(
        &log,
        &p,
        Body::HostSeen(HostSeen {
            record: later,
            signature: None,
        }),
    );
    assert_eq!(log.insert(op, NOW), Err(Error::Permission));
}

#[test]
fn equal_revs_with_different_records_flag_the_host() {
    let (mut log, a, b, _) = three();
    let (intro, ca) = ca_host(40, HostKind::Sheprd);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro));
    for (by, seed) in [(&a, 60), (&b, 61)] {
        let mut newer = record(host, seed, 2);
        newer.host = host;
        let signature = Some(raw_signature(&ca, &newer.message().unwrap()));
        act(
            &mut log,
            by,
            Body::HostSeen(HostSeen {
                record: newer,
                signature,
            }),
        );
    }
    assert!(log.state().hosts[&host].flagged);
}

#[test]
fn two_members_adding_one_host_with_different_keys_hold_it() {
    let (mut log, a, b, c) = three();
    let intro = plain_host(40);
    let host = intro.id().unwrap();
    let mut other = intro.clone();
    other.record.ssh_host_keys.push(ssh_line(251));
    let before = log.heads().to_vec();
    let next = |member: &Device| log.state().members[&member.id()].seq + 1;
    let ops = [
        sign_on(&log, &a, &before, next(&a), Body::HostAdd(intro)),
        sign_on(&log, &b, &before, next(&b), Body::HostAdd(other)),
    ];
    for op in ops {
        apply(&mut log, op);
    }
    assert!(log.state().hosts[&host].held);
    assert!(!log.state().allowed(&host, &c.id()));
}

#[test]
fn a_host_add_naming_a_member_host_changes_nothing() {
    let (mut log, a, b, _) = three();
    let mut intro = a.bundle.host.clone().unwrap();
    intro.kind = HostKind::Sheprd;
    intro.name = "impostor".into();
    act(&mut log, &b, Body::HostAdd(intro));
    let host = a.bundle.host.as_ref().unwrap().id().unwrap();
    assert_eq!(log.state().hosts[&host].added_by, a.id());
    assert_eq!(log.state().hosts[&host].intro.name, "host1");
}

#[test]
fn phones_link_to_hosts_and_the_latest_link_wins() {
    let (mut log, a, _, _) = three();
    let p = phone(5);
    admit(&mut log, &a, &p);
    let host = a.bundle.host.as_ref().unwrap().id().unwrap();
    for seed in [70, 71] {
        act(
            &mut log,
            &p,
            Body::Link(Link {
                host,
                device_key: compressed(&signing_key(seed)).to_vec(),
            }),
        );
    }
    assert_eq!(
        log.state().links[&(p.id(), host)],
        compressed(&signing_key(71)).to_vec()
    );
}
