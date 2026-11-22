//! What a member that turns hostile can try against the rules: a
//! counter-removal that reaches further, a copied host, an access `rev`
//! out of reach, and filling the log so its removal cannot get in.

use super::*;

fn four() -> (Log, Device, Device, Device, Device) {
    let (a, b, c, d) = (computer(1), computer(2), computer(3), computer(4));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &c);
    admit(&mut log, &a, &d);
    (log, a, b, c, d)
}

fn live(log: &Log) -> Vec<MemberId> {
    log.state().members.keys().copied().collect()
}

fn sorted(mut ids: Vec<MemberId>) -> Vec<MemberId> {
    ids.sort();
    ids
}

fn removal(member: &Device, also_members: Vec<MemberId>, also_hosts: Vec<HostId>) -> Body {
    Body::Remove(Remove {
        member: member.id(),
        also_members,
        also_hosts,
        reason: Reason::Compromised,
    })
}

#[test]
fn a_counter_removal_reaches_no_further_than_its_removers() {
    let (mut log, a, b, c, d) = four();
    let before = log.heads().to_vec();
    let seq = log.state().members[&c.id()].seq + 1;
    // c, removed by a, counter-removes a and slips b in beside it.
    let reaching = sign_on(
        &log,
        &c,
        &before,
        seq,
        removal(&a, vec![b.id()], Vec::new()),
    );
    act(&mut log, &a, remove_body(&c));
    apply(&mut log, reaching);
    // Not a counter-removal, so void like anything else c signed: a, b,
    // and d stay.
    assert_eq!(live(&log), sorted(vec![a.id(), b.id(), d.id()]));
}

#[test]
fn a_counter_removal_takes_no_host() {
    let (mut log, a, b, c, d) = four();
    act(&mut log, &a, Body::HostAdd(plain_host(40)));
    let host = plain_host(40).id().unwrap();
    let before = log.heads().to_vec();
    let seq = log.state().members[&c.id()].seq + 1;
    let reaching = sign_on(&log, &c, &before, seq, removal(&a, Vec::new(), vec![host]));
    act(&mut log, &a, remove_body(&c));
    apply(&mut log, reaching);
    assert_eq!(live(&log), sorted(vec![a.id(), b.id(), d.id()]));
    assert!(log.state().hosts.contains_key(&host));
}

#[test]
fn a_counter_removal_takes_every_member_that_removed_it() {
    let (mut log, a, b, c, d) = four();
    let before = log.heads().to_vec();
    let next = |member: &Device| log.state().members[&member.id()].seq + 1;
    let ops = [
        sign_on(&log, &a, &before, next(&a), remove_body(&c)),
        sign_on(&log, &b, &before, next(&b), remove_body(&c)),
        sign_on(
            &log,
            &c,
            &before,
            next(&c),
            removal(&a, vec![b.id()], Vec::new()),
        ),
    ];
    for op in ops {
        apply(&mut log, op);
    }
    assert_eq!(live(&log), [d.id()]);
}

/// `victim`'s host intro, copied with another account to log in as.
fn copied_host(victim: &Device) -> HostIntro {
    let mut intro = victim.bundle.host.clone().unwrap();
    intro.user = "root".into();
    intro
}

/// A joiner whose bundle carries `host`.
fn joiner_with(seed: u8, host: HostIntro) -> Device {
    let mut joiner = computer(seed);
    joiner.bundle.host = Some(host);
    joiner
}

#[test]
fn a_member_cannot_take_another_members_host() {
    let (mut log, a, b, _, _) = four();
    let impostor = joiner_with(9, copied_host(&a));
    let op = sign(&log, &b, admit_body(&log, &impostor));
    assert_eq!(log.insert(op, NOW), Err(Error::Invalid));
    let mut rotated = b.bundle.clone();
    rotated.host = Some(copied_host(&a));
    let op = sign(&log, &b, Body::Rotate(rotated));
    assert_eq!(log.insert(op, NOW), Err(Error::Invalid));
    let host = a.bundle.host.as_ref().unwrap().id().unwrap();
    assert_eq!(log.state().hosts[&host].intro.user, "me");
}

#[test]
fn concurrent_claims_on_one_host_hold_it() {
    let (mut log, a, b, c, _) = four();
    // a and b each admit a computer, concurrently, both naming one new
    // host: neither saw the other, so the check in history passes both.
    let (intro, _) = ca_host(60, HostKind::Member);
    let host = intro.id().unwrap();
    let mut copy = intro.clone();
    copy.user = "root".into();
    let before = log.heads().to_vec();
    let next = |member: &Device| log.state().members[&member.id()].seq + 1;
    let ops = [
        sign_on(
            &log,
            &a,
            &before,
            next(&a),
            admit_body(&log, &joiner_with(9, intro)),
        ),
        sign_on(
            &log,
            &b,
            &before,
            next(&b),
            admit_body(&log, &joiner_with(10, copy)),
        ),
    ];
    for op in ops {
        apply(&mut log, op);
    }
    assert!(log.state().hosts[&host].held);
    assert!(!log.state().allowed(&host, &c.id()));
}

#[test]
fn a_member_naming_an_added_host_with_another_account_holds_it() {
    let (mut log, a, b, c, _) = four();
    let (intro, _) = ca_host(60, HostKind::Sheprd);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro.clone()));
    // The same machine joining as a member, as it is: no conflict.
    let mut same = intro.clone();
    same.kind = HostKind::Member;
    admit(&mut log, &b, &joiner_with(9, same));
    assert!(!log.state().hosts[&host].held);
    assert!(log.state().allowed(&host, &c.id()));
    // Another account on it: a copy, held.
    let (mut log, a, b, c, _) = four();
    act(&mut log, &a, Body::HostAdd(intro.clone()));
    let mut other = intro;
    other.kind = HostKind::Member;
    other.user = "root".into();
    admit(&mut log, &b, &joiner_with(9, other));
    assert!(log.state().hosts[&host].held);
    assert!(!log.state().allowed(&host, &c.id()));
}

#[test]
fn no_access_rev_puts_the_register_out_of_reach() {
    let (mut log, a, b, c, _) = four();
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
    // A huge rev, or a skipped one, is refused and sets no floor.
    for rev in [u64::MAX, 7] {
        let op = as_drafted(&log, &b, access(true, rev));
        assert_eq!(log.insert(op, NOW), Err(Error::Rev));
    }
    act(&mut log, &a, access(false, 1));
    assert!(!log.state().allowed(&host, &c.id()));
    assert_eq!(log.state().access[&(host, c.id())].rev, 1);
}

#[test]
fn a_full_log_still_takes_a_removal() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    // b fills the membership bound with rotations.
    let mut bundle = b.bundle.clone();
    loop {
        bundle.tunnel[0] = bundle.tunnel[0].wrapping_add(1);
        let op = sign(&log, &b, Body::Rotate(bundle.clone()));
        if log.insert(op, NOW) == Err(Error::Full) {
            break;
        }
    }
    act(&mut log, &a, remove_body(&b));
    assert!(!log.state().members.contains_key(&b.id()));
}

#[test]
fn one_member_cannot_use_up_the_removals() {
    let (mut log, a, b, c, _) = four();
    let before = log.heads().to_vec();
    let seq = log.state().members[&c.id()].seq;
    // c signs concurrent removals of b, one after another, past its share.
    let mut results = Vec::new();
    for offset in 1..=u64::try_from(MAX_REMOVALS_PER_SIGNER).unwrap() + 1 {
        let op = sign_on(&log, &c, &before, seq + offset, remove_body(&b));
        results.push(log.insert(op, NOW));
    }
    assert!(results[..MAX_REMOVALS_PER_SIGNER].iter().all(Result::is_ok));
    assert_eq!(results[MAX_REMOVALS_PER_SIGNER], Err(Error::Full));
    // a's own removal still gets in, though c's forks leave more heads
    // than one operation may name.
    assert!(log.heads().len() > MAX_PARENTS);
    act(&mut log, &a, remove_body(&c));
    assert!(!log.state().members.contains_key(&c.id()));
}

#[test]
fn adding_a_copy_of_a_members_host_cannot_hold_it() {
    let (mut log, a, b, c, _) = four();
    let host = a.bundle.host.as_ref().unwrap().id().unwrap();
    let mut copy = copied_host(&a);
    copy.kind = HostKind::Sheprd;
    act(&mut log, &b, Body::HostAdd(copy));
    assert!(!log.state().hosts[&host].held);
    assert_eq!(log.state().hosts[&host].intro.user, "me");
    assert!(log.state().allowed(&host, &c.id()));
}

#[test]
fn removed_members_cannot_use_up_a_live_members_removals() {
    let (mut log, a, b, c, d) = four();
    let before = log.heads().to_vec();
    let seq = log.state().members[&c.id()].seq;
    act(&mut log, &a, remove_body(&c));
    // c, removed, signs its whole share of backdated removals.
    for offset in 1..=u64::try_from(MAX_REMOVALS_PER_SIGNER).unwrap() {
        let op = sign_on(&log, &c, &before, seq + offset, remove_body(&d));
        assert!(log.insert(op, NOW).is_ok());
    }
    // b's share is untouched, and d is still there.
    act(&mut log, &b, remove_body(&d));
    assert!(!log.state().members.contains_key(&d.id()));
    assert!(log.state().members.contains_key(&b.id()));
}

#[test]
fn a_bundle_concurrent_with_an_added_host_cannot_take_it_over() {
    let (mut log, a, b, c, _) = four();
    let (intro, _) = ca_host(60, HostKind::Sheprd);
    let host = intro.id().unwrap();
    let mut copy = intro.clone();
    copy.kind = HostKind::Member;
    copy.user = "root".into();
    let before = log.heads().to_vec();
    let next = |member: &Device| log.state().members[&member.id()].seq + 1;
    let ops = [
        sign_on(&log, &a, &before, next(&a), Body::HostAdd(intro)),
        sign_on(
            &log,
            &b,
            &before,
            next(&b),
            admit_body(&log, &joiner_with(9, copy)),
        ),
    ];
    for op in ops {
        apply(&mut log, op);
    }
    assert!(log.state().hosts[&host].held);
    assert!(!log.state().allowed(&host, &c.id()));
}

#[test]
fn every_addition_weighed_must_match_the_members_intro() {
    let (mut log, a, b, c, _) = four();
    let (intro, _) = ca_host(60, HostKind::Sheprd);
    let host = intro.id().unwrap();
    // Two additions before the member: one as it is, one with another
    // account. The member matches the first; the second still holds it.
    let mut other = intro.clone();
    other.user = "root".into();
    let before = log.heads().to_vec();
    let next = |member: &Device| log.state().members[&member.id()].seq + 1;
    let ops = [
        sign_on(&log, &a, &before, next(&a), Body::HostAdd(intro.clone())),
        sign_on(&log, &b, &before, next(&b), Body::HostAdd(other)),
    ];
    for op in ops {
        apply(&mut log, op);
    }
    let mut same = intro;
    same.kind = HostKind::Member;
    admit(&mut log, &a, &joiner_with(9, same));
    assert!(log.state().hosts[&host].held);
    assert!(!log.state().allowed(&host, &c.id()));
}

#[test]
fn a_bundle_cannot_name_another_members_keys() {
    let (mut log, a, b, _, _) = four();
    // A joiner whose tunnel key is b's, then one whose dial key is.
    let mut tunnel = computer(9);
    tunnel.bundle.tunnel = b.bundle.tunnel;
    let op = sign(&log, &a, admit_body(&log, &tunnel));
    assert_eq!(log.insert(op, NOW), Err(Error::Invalid));
    let mut dial = computer(10);
    dial.bundle.dial = b.bundle.dial;
    let op = sign(&log, &a, admit_body(&log, &dial));
    assert_eq!(log.insert(op, NOW), Err(Error::Invalid));
    // Nor can a member rotate onto another's keys.
    let mut rotated = a.bundle.clone();
    rotated.tunnel = b.bundle.tunnel;
    let op = sign(&log, &a, Body::Rotate(rotated));
    assert_eq!(log.insert(op, NOW), Err(Error::Invalid));
}
