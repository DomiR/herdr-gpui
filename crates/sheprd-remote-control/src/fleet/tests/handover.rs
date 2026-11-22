//! Handing the log to a computer with no fleet: in parts, as a member's
//! `fleet apply` does when the log is larger than one answer, and to a
//! Sheprd host the fleet shares, which is no member.

use super::*;
use crate::control::fleet::sync::Apply;

fn apply(node: &Node, ops: &[Op]) -> Answer {
    node.ask(Ask::Apply(Apply {
        ops: sync::encode_ops(ops),
        ..Apply::default()
    }))
}

/// `node`'s log in graph order, as a member hands it over.
fn in_order(node: &Node) -> Vec<Op> {
    let inner = node.fleet.inner();
    let log = inner.log.as_ref().unwrap();
    log.missing_for(&[]).into_iter().cloned().collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shared_sheprd_host_takes_the_log_and_lets_allowed_members_in() {
    let (http, _, _) = relay().await;
    let (a, host) = (Node::new(http, 1).await, Node::new(http, 5).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    // Before it is shared, the host refuses the log.
    assert!(matches!(
        host.fleet.receive(a.ops(), now_ms()),
        Err(Error::NotAdmitted)
    ));
    let Answer::Intro { intro } = host.ask(Ask::Intro) else {
        panic!("no intro");
    };
    assert_eq!(
        a.ask(Ask::AddHost {
            intro,
            deny: Vec::new(),
        }),
        Answer::Done
    );
    let Answer::Applied(applied) = host.ask(Ask::Apply(Apply {
        heads: a.view().heads,
        ops: sync::encode_ops(&a.ops()),
        ..Apply::default()
    })) else {
        panic!("not applied");
    };
    assert_eq!(applied.fleet, a.view().fleet);
    // It holds the log as a host, not as a member, and lets a in.
    let view = host.view();
    assert!(!view.members.iter().any(|member| member.id == host.me()));
    assert!(
        view.hosts
            .iter()
            .any(|shared| Some(shared.id.clone()) == host.fleet.host().map(|id| id.to_string()))
    );
    assert!(grants::still_allowed(&host.fleet, &a.fleet.me()));
    grants::apply(&host.state, &host.fleet).unwrap();
    assert_eq!(host.lines().len(), 1, "{:?}", host.lines());
    // It stays a host after a restart: how it joined is kept.
    let again = Fleet::load(host.dir.path(), now_ms()).unwrap();
    assert_eq!(again.host(), host.fleet.host());
    assert!(grants::still_allowed(&again, &a.fleet.me()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_removed_member_never_acts_as_a_shared_host() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    // b joined as a member: it has a certificate authority, but is no
    // shared host.
    assert!(b.fleet.host.is_some());
    assert_eq!(b.fleet.host(), None);
    a.ask(Ask::Remove {
        member: b.me(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: crate::control::fleet::Reason::Lost,
    });
    b.fleet.receive(a.ops(), now_ms()).unwrap();
    let a_host = {
        let inner = a.fleet.inner();
        grants::own_host(inner.log.as_ref().unwrap().state(), &a.fleet.me(), None).unwrap()
    };
    let inner = b.fleet.inner();
    let state = inner.log.as_ref().unwrap().state();
    assert!(state.tombstones.contains(&b.fleet.me()));
    // Even handed a host the fleet holds, a removed member gets none.
    assert_eq!(grants::own_host(state, &b.fleet.me(), Some(a_host)), None);
    drop(inner);
    assert!(!grants::still_allowed(&b.fleet, &a.fleet.me()));
    grants::apply(&b.state, &b.fleet).unwrap();
    assert!(b.lines().is_empty(), "{:?}", b.lines());
    let again = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert_eq!(again.host(), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_fleets_handing_over_at_once_keep_their_own_history() {
    let (http, _, _) = relay().await;
    let (a, c, b) = (
        Node::new(http, 1).await,
        Node::new(http, 3).await,
        Node::new(http, 2).await,
    );
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    assert_eq!(c.ask(Ask::Found), Answer::Done);
    let (first, second) = (a.view().fleet, c.view().fleet);
    for (by, fleet) in [(&a, &first), (&c, &second)] {
        let Answer::Bundle { bundle, signature } = b.ask(Ask::Bundle {
            fleet: fleet.clone(),
        }) else {
            panic!("no bundle");
        };
        by.ask(Ask::Admit {
            bundle,
            signature,
            method: crate::control::fleet::Method::Ssh,
        });
    }
    let named = |fleet: &str, ops: &[Op], heads: Vec<String>| {
        b.fleet
            .apply(
                &Apply {
                    fleet: Some(fleet.to_owned()),
                    heads,
                    ops: sync::encode_ops(ops),
                    ..Apply::default()
                },
                now_ms(),
            )
            .unwrap()
    };
    let (a_ops, c_ops) = (in_order(&a), in_order(&c));
    // Both founding operations arrive, each answered from its own fleet's
    // history; then the rest of a's: c's history never mixes in.
    let from_a = named(&first, &a_ops[..1], Vec::new());
    let from_c = named(&second, &c_ops[..1], Vec::new());
    assert_eq!(from_a.fleet, first);
    assert_eq!(from_c.fleet, second);
    assert_eq!(b.fleet.inner().joining.len(), 2);
    let joined = named(&first, &a_ops[1..], from_a.heads);
    assert_eq!(joined.fleet, first);
    assert_eq!(joined.refused, 0);
    assert!(b.fleet.inner().joining.is_empty());
    assert_eq!(b.view().fleet, first);
}

#[tokio::test(flavor = "multi_thread")]
async fn histories_of_too_many_fleets_drop_the_oldest() {
    let (http, _, _) = relay().await;
    let b = Node::new(http, 2).await;
    let mut fleets = Vec::new();
    for seed in 10..10 + crate::fleet::joining::MAX_FLEETS as u8 + 1 {
        let founder = Node::new(http, seed).await;
        assert_eq!(founder.ask(Ask::Found), Answer::Done);
        let fleet = founder.fleet.inner().log.as_ref().unwrap().fleet();
        // Each arrives later than the one before.
        b.fleet
            .receive(founder.ops(), now_ms() + u64::from(seed))
            .unwrap_err();
        fleets.push(fleet);
    }
    let inner = b.fleet.inner();
    assert_eq!(inner.joining.len(), crate::fleet::joining::MAX_FLEETS);
    assert!(!inner.joining.contains_key(&fleets[0]));
    assert!(
        fleets[1..]
            .iter()
            .all(|fleet| inner.joining.contains_key(fleet))
    );
    drop(inner);
    // Kept apart on disk, each in its own file, across a restart.
    let again = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert_eq!(
        again.inner().joining.len(),
        crate::fleet::joining::MAX_FLEETS
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_invitation_cut_off_after_admission_resumes() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let Answer::Bundle { bundle, signature } = b.ask(Ask::Bundle {
        fleet: a.view().fleet,
    }) else {
        panic!("no bundle");
    };
    let admit = || Ask::Admit {
        bundle: bundle.clone(),
        signature: signature.clone(),
        method: crate::control::fleet::Method::Ssh,
    };
    assert_eq!(a.ask(admit()), Answer::Done);
    // The log never reached b. Invite again: admitting the same bundle
    // signs nothing more, and the handover goes on.
    let heads = a.view().heads;
    assert_eq!(a.ask(admit()), Answer::Done);
    assert_eq!(a.view().heads, heads);
    let Answer::Applied(applied) = apply(&b, &in_order(&a)) else {
        panic!("not applied");
    };
    assert_eq!(applied.fleet, a.view().fleet);
    assert!(b.view().members.iter().any(|member| member.id == b.me()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shared_host_waits_for_the_denials_signed_with_its_share() {
    let (http, _, _) = relay().await;
    let (a, b, host) = (
        Node::new(http, 1).await,
        Node::new(http, 2).await,
        Node::new(http, 5).await,
    );
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    let Answer::Intro { intro } = host.ask(Ask::Intro) else {
        panic!("no intro");
    };
    // Shared with b left out.
    a.ask(Ask::AddHost {
        intro,
        deny: vec![b.me()],
    });
    let ops = in_order(&a);
    let heads = a.view().heads;
    // A part that ends with the share's addition, before its denial: the
    // host takes nothing yet, so b is never let in.
    let (first, rest) = ops.split_at(ops.len() - 1);
    assert!(matches!(first.last().unwrap().body(), Body::HostAdd(_)));
    let part = |ops: &[Op]| {
        host.fleet.apply(
            &Apply {
                fleet: Some(a.view().fleet),
                heads: heads.clone(),
                ops: sync::encode_ops(ops),
                ..Apply::default()
            },
            now_ms(),
        )
    };
    part(first).unwrap();
    assert!(host.fleet.inner().log.is_none());
    assert!(!grants::still_allowed(&host.fleet, &b.fleet.me()));
    // With the rest, the caller's whole log, it joins: a is let in, b not.
    part(rest).unwrap();
    assert!(host.fleet.inner().log.is_some());
    assert!(grants::still_allowed(&host.fleet, &a.fleet.me()));
    assert!(!grants::still_allowed(&host.fleet, &b.fleet.me()));
}
