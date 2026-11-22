//! An `Access` `rev` follows the operations its own history holds, so a
//! deny stays a deny when an allow it saw is voided later.

use super::*;

#[test]
fn a_deny_survives_the_removal_of_the_allow_it_saw() {
    let (a, b, c, d) = (computer(1), computer(2), computer(3), computer(4));
    let mut log = found(&a);
    for joiner in [&b, &c, &d] {
        admit(&mut log, &a, joiner);
    }
    act(&mut log, &b, Body::HostAdd(plain_host(40)));
    let host = plain_host(40).id().unwrap();
    let before = log.heads().to_vec();
    let access = |allow, rev| {
        Body::Access(Access {
            host,
            member: d.id(),
            allow,
            rev,
        })
    };
    act(&mut log, &a, access(true, 1));
    act(&mut log, &b, access(false, 2));
    // c removes a without having seen a's allow: the allow is void, the
    // deny that saw it is not.
    let seq = log.state().members[&c.id()].seq + 1;
    let removed = sign_on(&log, &c, &before, seq, remove_body(&a));
    apply(&mut log, removed);
    assert!(!log.state().members.contains_key(&a.id()));
    assert!(!log.state().allowed(&host, &d.id()));
    assert_eq!(
        log.state().access[&(host, d.id())],
        AccessRegister {
            allow: false,
            rev: 2
        }
    );
    // And the next one climbs from there.
    act(&mut log, &b, access(true, 3));
    assert!(log.state().allowed(&host, &d.id()));
}

#[test]
fn a_draft_takes_the_rev_its_parents_call_for() {
    let (a, b, c) = (computer(1), computer(2), computer(3));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &c);
    act(&mut log, &a, Body::HostAdd(plain_host(40)));
    let host = plain_host(40).id().unwrap();
    let access = |allow, rev| {
        Body::Access(Access {
            host,
            member: c.id(),
            allow,
            rev,
        })
    };
    act(&mut log, &a, access(false, 1));
    // Drafted with a stale or wild rev, it still follows its parents.
    for wild in [0, 1, u64::MAX] {
        let op = sign(&log, &b, access(true, wild));
        let Body::Access(signed) = op.body() else {
            unreachable!()
        };
        let expected = log.state().access[&(host, c.id())].rev + 1;
        assert_eq!(signed.rev, expected);
        apply(&mut log, op);
    }
    assert!(log.state().allowed(&host, &c.id()));
}
