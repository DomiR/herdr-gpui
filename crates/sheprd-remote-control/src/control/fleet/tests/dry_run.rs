//! `Log::drafts`: what a member that keeps its own log checks before its
//! owner signs. It drafts on its own heads and refuses what the replay
//! would refuse, without touching the log it was asked on.

use super::*;

fn link_body(host: HostId, seed: u8) -> Body {
    Body::Link(Link {
        host,
        device_key: uncompressed(&signing_key(seed)).to_vec(),
    })
}

/// A fleet of two computers and a phone, with a Sheprd host added.
fn fleet() -> (Log, Device, Device, Device, HostId) {
    let a = computer(1);
    let b = computer(2);
    let p = phone(3);
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &p);
    let (intro, _) = ca_host(9, HostKind::Sheprd);
    let host = intro.record.host;
    act(&mut log, &a, Body::HostAdd(intro));
    (log, a, b, p, host)
}

#[test]
fn drafts_chain_on_the_log_and_apply_once_signed() {
    let (mut log, _, b, p, host) = fleet();
    let heads = log.heads().to_vec();
    let held = log.ops().count();
    let drafts = log
        .drafts(p.id(), NOW, vec![link_body(host, 60), remove_body(&b)], NOW)
        .unwrap();
    // The log it was asked on is as it was.
    assert_eq!(log.heads(), heads.as_slice());
    assert_eq!(log.ops().count(), held);
    let [first, second] = drafts.as_slice() else {
        panic!("two drafts");
    };
    assert_eq!(first.parents, heads);
    assert_eq!(second.seq, first.seq + 1);
    let first = first.clone().sign(&p.key).unwrap();
    assert_eq!(second.parents, vec![first.id()]);
    let second = second.clone().sign(&p.key).unwrap();
    let results = log.insert_all(vec![first, second], NOW);
    assert!(
        results
            .iter()
            .all(|(_, result)| *result == Ok(Inserted::Applied))
    );
    assert!(!log.is_member(&b.id()));
}

#[test]
fn a_removed_signer_drafts_nothing() {
    let (mut log, a, _, p, host) = fleet();
    act(&mut log, &a, remove_body(&p));
    assert_eq!(
        log.drafts(p.id(), NOW, vec![link_body(host, 60)], NOW),
        Err(Error::Signer)
    );
}

#[test]
fn a_phone_may_not_draft_what_only_computers_sign() {
    let (log, _, b, p, host) = fleet();
    let access = Body::Access(Access {
        host,
        member: b.id(),
        allow: false,
        rev: 1,
    });
    assert_eq!(
        log.drafts(p.id(), NOW, vec![access], NOW),
        Err(Error::Permission)
    );
}

#[test]
fn a_removal_of_no_live_member_is_refused() {
    let (log, _, _, p, _) = fleet();
    assert_eq!(
        log.drafts(p.id(), NOW, vec![remove_body(&computer(7))], NOW),
        Err(Error::Unknown)
    );
}

#[test]
fn a_removal_twice_in_one_batch_is_refused_at_the_second() {
    // The second is drafted on the first, which already removed `b`.
    let (log, _, b, p, _) = fleet();
    assert_eq!(
        log.drafts(p.id(), NOW, vec![remove_body(&b), remove_body(&b)], NOW),
        Err(Error::Unknown)
    );
}

#[test]
fn an_operation_dated_too_far_ahead_is_refused() {
    let (log, _, _, p, host) = fleet();
    assert_eq!(
        log.drafts(
            p.id(),
            NOW + MAX_SKEW_MS + 1,
            vec![link_body(host, 60)],
            NOW
        ),
        Err(Error::Future)
    );
}

#[test]
fn a_draft_signed_by_another_key_is_still_refused() {
    // The dry run presumes the signature; the log never does.
    let (mut log, a, _, p, host) = fleet();
    let draft = log
        .drafts(p.id(), NOW, vec![link_body(host, 60)], NOW)
        .unwrap()
        .remove(0);
    let forged = draft.sign(&a.key).unwrap();
    assert_eq!(log.insert(forged, NOW), Err(Error::Signature));
}
