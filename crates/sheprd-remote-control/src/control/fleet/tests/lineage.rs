//! A removal reaches the members its target admitted that the remover had
//! not seen: no member keeps trust that came only from a voided admission.

use super::*;

fn four() -> (Log, Device, Device, Device, Device) {
    let (a, b, c, d) = (computer(1), computer(2), computer(3), computer(4));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &c);
    admit(&mut log, &a, &d);
    (log, a, b, c, d)
}

fn removal(member: &Device, also_hosts: Vec<HostId>) -> Body {
    Body::Remove(Remove {
        member: member.id(),
        also_members: Vec::new(),
        also_hosts,
        reason: Reason::Compromised,
    })
}

/// c, about to be removed by a, admits x without a having seen it.
fn admitted_on_the_side() -> (Log, Device, Device, Device, Device, Device, Op) {
    let (mut log, a, b, c, d) = four();
    let before = log.heads().to_vec();
    let seq = log.state().members[&c.id()].seq + 1;
    let x = computer(9);
    let admit_x = sign_on(&log, &c, &before, seq, admit_body(&log, &x));
    act(&mut log, &a, remove_body(&c));
    apply(&mut log, admit_x.clone());
    (log, a, b, c, d, x, admit_x)
}

#[test]
fn members_admitted_through_a_voided_admission_are_not_members() {
    let (mut log, _, b, c, d, x, admit_x) = admitted_on_the_side();
    // x, trusted only through c's voided admission, admits y and adds a
    // host.
    let y = computer(10);
    let admit_y = sign_on(&log, &x, &[admit_x.id()], 1, admit_body(&log, &y));
    apply(&mut log, admit_y.clone());
    let host = sign_on(&log, &x, &[admit_y.id()], 2, Body::HostAdd(plain_host(40)));
    apply(&mut log, host);
    let state = log.state();
    for gone in [&c, &x, &y] {
        assert!(!state.members.contains_key(&gone.id()));
        assert!(state.tombstones.contains(&gone.id()));
    }
    assert!(state.members.contains_key(&b.id()));
    assert!(state.members.contains_key(&d.id()));
    assert!(!state.hosts.contains_key(&plain_host(40).id().unwrap()));
}

#[test]
fn a_member_admitted_on_the_side_counters_only_as_its_admitter_could() {
    let (mut log, a, b, c, d, x, admit_x) = admitted_on_the_side();
    // x removes a, who removed c: a counter-removal by proxy, which takes
    // a with it as c's own would, and keeps no one.
    let counter = sign_on(&log, &x, &[admit_x.id()], 1, removal(&a, Vec::new()));
    apply(&mut log, counter);
    let live: Vec<MemberId> = log.state().members.keys().copied().collect();
    let mut expected = vec![b.id(), d.id()];
    expected.sort();
    assert_eq!(live, expected);
    assert!(log.state().tombstones.contains(&c.id()));
    assert!(log.state().tombstones.contains(&x.id()));
}

#[test]
fn a_member_admitted_on_the_side_cannot_reach_further() {
    let (log, a, b, c, d, x, admit_x) = admitted_on_the_side();
    // Removing a bystander, or a's removal with a host, is void.
    let host = a.bundle.host.as_ref().unwrap().id().unwrap();
    for body in [removal(&b, Vec::new()), removal(&a, vec![host])] {
        let mut log = copy(&log);
        let op = sign_on(&log, &x, &[admit_x.id()], 1, body);
        apply(&mut log, op);
        let state = log.state();
        for kept in [&a, &b, &d] {
            assert!(state.members.contains_key(&kept.id()));
        }
        assert!(!state.members.contains_key(&c.id()));
        assert!(!state.members.contains_key(&x.id()));
        assert!(state.hosts.contains_key(&host));
    }
}

#[test]
fn a_replayed_admission_does_not_take_the_member_it_copies() {
    let (mut log, a, b, c, d) = four();
    let before = log.heads().to_vec();
    let x = computer(9);
    // b admits x; c, about to be removed, replays x's join signature in an
    // admission of its own, backdated so x is not yet live there.
    let genuine = admit(&mut log, &b, &x);
    let seq = log.state().members[&c.id()].seq + 1;
    let Body::Admit(body) = genuine.body().clone() else {
        unreachable!()
    };
    let replay = sign_on(&log, &c, &before, seq, Body::Admit(body));
    act(&mut log, &a, remove_body(&c));
    apply(&mut log, replay);
    let state = log.state();
    assert!(state.members.contains_key(&x.id()));
    assert!(!state.tombstones.contains(&x.id()));
    assert!(!state.members.contains_key(&c.id()));
    for kept in [&a, &b, &d] {
        assert!(state.members.contains_key(&kept.id()));
    }
}

/// c and d both admit x without a having seen either admission; a then
/// removes c and d, one after the other or concurrently, or together.
fn admitted_twice(how: &str) -> (Log, Device, Device) {
    let (mut log, a, b, c, d) = four();
    let before = log.heads().to_vec();
    let x = computer(9);
    let body = admit_body(&log, &x);
    let next = |member: &Device| log.state().members[&member.id()].seq + 1;
    let by_c = sign_on(&log, &c, &before, next(&c), body.clone());
    let by_d = sign_on(&log, &d, &before, next(&d), body);
    let seq = next(&a);
    match how {
        "in turn" => {
            act(&mut log, &a, remove_body(&c));
            act(&mut log, &a, remove_body(&d));
        }
        "reversed" => {
            act(&mut log, &a, remove_body(&d));
            act(&mut log, &a, remove_body(&c));
        }
        "concurrently" => {
            let remove_c = sign_on(&log, &a, &before, seq, remove_body(&c));
            let remove_d = sign_on(&log, &b, &before, next(&b), remove_body(&d));
            apply(&mut log, remove_c);
            apply(&mut log, remove_d);
        }
        _ => unreachable!(),
    }
    apply(&mut log, by_c);
    apply(&mut log, by_d);
    (log, b, x)
}

#[test]
fn a_member_every_admitter_of_which_is_removed_goes_with_them() {
    for how in ["in turn", "reversed", "concurrently"] {
        let (mut log, b, x) = admitted_twice(how);
        // What x signed on either admission is void too.
        let heads: Vec<OpId> = log
            .ops()
            .filter(|op| matches!(op.body(), Body::Admit(admit) if admit.bundle.id() == x.id()))
            .map(Op::id)
            .collect();
        let added = sign_on(&log, &x, &heads, 1, Body::HostAdd(plain_host(40)));
        apply(&mut log, added);
        let state = log.state();
        assert!(!state.members.contains_key(&x.id()), "{how}");
        assert!(state.tombstones.contains(&x.id()), "{how}");
        assert!(
            !state.hosts.contains_key(&plain_host(40).id().unwrap()),
            "{how}"
        );
        assert!(state.members.contains_key(&b.id()), "{how}");
    }
}
