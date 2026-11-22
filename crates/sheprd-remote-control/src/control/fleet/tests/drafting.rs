//! A draft names its signer's own latest operation, whatever the other
//! heads, so an honest member never looks like it signed in two places.

use super::*;

#[test]
fn a_draft_on_more_heads_than_it_may_name_keeps_its_own_chain() {
    let a = computer(1);
    let mut log = found(&a);
    let mut members = vec![a.clone()];
    for seed in 2..=u8::try_from(MAX_COMPUTERS).unwrap() {
        let joiner = computer(seed);
        admit(&mut log, &a, &joiner);
        members.push(joiner);
    }
    let p = phone(40);
    admit(&mut log, &a, &p);
    members.push(p);
    assert!(members.len() > MAX_PARENTS);
    // Every member signs on the same history, leaving one head each.
    let before = log.heads().to_vec();
    let ops: Vec<Op> = members
        .iter()
        .zip(1u8..)
        .map(|(member, n)| {
            let seq = log.state().members[&member.id()].seq + 1;
            sign_on(
                &log,
                member,
                &before,
                seq,
                Body::HostRemove(HostId([n; 32])),
            )
        })
        .collect();
    for op in ops.clone() {
        apply(&mut log, op);
    }
    assert_eq!(log.heads().len(), members.len());
    // The member whose head sorts last is the one a plain cut leaves out.
    let last = ops.iter().max_by_key(|op| op.id()).unwrap();
    let signer = members.iter().find(|m| m.id() == last.signer()).unwrap();
    let draft = log.draft(signer.id(), NOW, Body::HostRemove(HostId([99; 32])));
    assert_eq!(draft.parents.len(), MAX_PARENTS);
    assert!(draft.parents.contains(&last.id()));
    apply(&mut log, draft.sign(&signer.key).unwrap());
    assert!(log.state().equivocating.is_empty());
}
