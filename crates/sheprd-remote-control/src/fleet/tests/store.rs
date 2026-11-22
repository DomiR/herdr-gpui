//! The fleet on disk: kept across restarts, and taken over SSH when no
//! daemon runs.

use super::*;
use crate::control::fleet::sync::Apply;

#[tokio::test(flavor = "multi_thread")]
async fn the_key_and_the_log_survive_a_restart() {
    let (http, _, _) = relay().await;
    let a = Node::new(http, 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let heads = a.fleet.inner().log.as_ref().unwrap().heads().to_vec();
    let again = Fleet::load(a.dir.path(), now_ms()).unwrap();
    assert_eq!(again.me(), a.fleet.me());
    assert_eq!(
        again.inner().log.as_ref().unwrap().heads(),
        heads.as_slice()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn fleet_apply_with_no_daemon_running_keeps_the_log() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let fleet = a.view().fleet;
    let Answer::Bundle { bundle, signature } = b.ask(Ask::Bundle {
        fleet: fleet.clone(),
    }) else {
        panic!("no bundle");
    };
    a.ask(Ask::Admit {
        bundle,
        signature,
        method: crate::control::fleet::Method::Ssh,
    });
    // No daemon answers on b's control socket: the command writes the log
    // itself, which b's daemon reads when it starts.
    let apply = Apply {
        fleet: Some(fleet.clone()),
        heads: Vec::new(),
        ops: sync::encode_ops(&a.ops()),
        nonce: None,
        peer: None,
    };
    let applied = run_apply(b.dir.path(), apply).unwrap();
    assert_eq!(applied.fleet, fleet);
    assert_eq!(applied.refused, 0);
    let reread = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert_eq!(
        reread.inner().log.as_ref().unwrap().fleet().to_string(),
        fleet
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_carries_what_the_caller_lacks() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    let caller = b.fleet.inner().log.as_ref().unwrap().heads().to_vec();
    // a moves on; b asks with its heads and gets only what is new.
    a.ask(Ask::Access {
        host: b
            .view()
            .hosts
            .iter()
            .find(|host| host.added_by == b.me())
            .unwrap()
            .id
            .clone(),
        member: a.me(),
        allow: false,
    });
    let Answer::Applied(applied) = a.ask(Ask::Apply(Apply {
        fleet: Some(a.view().fleet),
        heads: caller.iter().map(ToString::to_string).collect(),
        ops: Vec::new(),
        nonce: None,
        peer: None,
    })) else {
        panic!("no answer");
    };
    assert_eq!(applied.ops.len(), 1);
    b.fleet
        .receive(sync::decode_ops(&applied.ops).unwrap(), now_ms())
        .unwrap();
    // The same hosts and grants; each computer's own `target` differs.
    let shared = |node: &Node| {
        node.view()
            .hosts
            .into_iter()
            .map(|host| (host.id, host.grants))
            .collect::<Vec<_>>()
    };
    assert_eq!(shared(&b), shared(&a));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_peers_file_that_does_not_read_is_an_error() {
    let (http, _, _) = relay().await;
    let a = Node::new(http, 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    std::fs::write(
        a.dir.path().join("fleet/peers.json"),
        b"{\"unconfirmed\": [1]}",
    )
    .unwrap();
    assert!(matches!(
        Fleet::load(a.dir.path(), now_ms()),
        Err(Error::Peers)
    ));
}

#[test]
fn what_members_and_plain_hosts_hold_survives_a_restart_beside_the_folds() {
    let dir = tempfile::tempdir().unwrap();
    let member = Holder::Member(MemberId([1; 32]));
    let host = Holder::Host(HostId([1; 32]));
    let mut inner = Inner::default();
    inner.held.insert(member, [OpId([7; 32])].into());
    inner.held.insert(host, [OpId([8; 32])].into());
    // A removal an agent reported that the log has not heard of.
    inner.unknown.insert(
        OpId([9; 32]),
        plain::Unknown {
            first_ms: 5,
            syncs: 2,
        },
    );
    // A suspension folded per signer and target, beside them.
    let target = suspensions::FoldedTarget {
        ids: [(
            OpId([10; 32]),
            suspensions::FoldedRemoval {
                since_ms: 6,
                named: [MemberId([3; 32])].into(),
                pinned: false,
            },
        )]
        .into(),
        overflow: Some(suspensions::Overflow {
            since_ms: 6,
            named: [(MemberId([3; 32]), 6)].into(),
            pinned: None,
        }),
    };
    inner
        .folded
        .insert(MemberId([2; 32]), [(MemberId([3; 32]), target)].into());
    crate::fleet::store::save(dir.path(), &inner, &mut Suspensions::default()).unwrap();
    let again = crate::fleet::store::load(dir.path(), now_ms()).unwrap();
    assert_eq!(again.held, inner.held);
    assert_eq!(again.unknown, inner.unknown);
    assert_eq!(again.folded, inner.folded);
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_naming_another_fleet_learns_nothing() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    assert_eq!(b.ask(Ask::Found), Answer::Done);
    let apply = Apply {
        fleet: Some(b.view().fleet),
        heads: Vec::new(),
        ops: Vec::new(),
        nonce: None,
        peer: None,
    };
    assert!(matches!(
        ask::answer(&a.state, &a.fleet, Ask::Apply(apply), now_ms()),
        Err(Error::OtherFleet)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_proves_which_host_gave_it() {
    let (http, _, _) = relay().await;
    let a = Node::new(http, 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let nonce = "n0nce";
    let Answer::Applied(applied) = a.ask(Ask::Apply(Apply {
        fleet: None,
        heads: Vec::new(),
        ops: Vec::new(),
        nonce: Some(nonce.into()),
        peer: None,
    })) else {
        panic!("no answer");
    };
    let hosts: Vec<String> = a.view().hosts.into_iter().map(|host| host.id).collect();
    assert!(sync::proven(&applied, nonce, &hosts));
    // Another nonce, other heads, or a host the fleet does not hold: no.
    assert!(!sync::proven(&applied, "other", &hosts));
    let mut moved = applied.clone();
    moved.heads.push("00".repeat(32));
    assert!(!sync::proven(&moved, nonce, &hosts));
    assert!(!sync::proven(&applied, nonce, &["11".repeat(32)]));
    // A machine that only echoes the fleet id, with no proof.
    let echoed = sync::Applied {
        proof: None,
        ..applied
    };
    assert!(!sync::proven(&echoed, nonce, &hosts));
}

#[tokio::test(flavor = "multi_thread")]
async fn proven_keys_an_earlier_build_recorded_are_dropped() {
    let (http, _, _) = relay().await;
    let a = Node::new(http, 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let member = "11".repeat(32);
    let key = "5a".repeat(32);
    // Both shapes earlier builds wrote, recorded on a handshake alone:
    // they still read, and count for nothing.
    for proven in [format!("\"{key}\""), format!("[\"{key}\"]")] {
        std::fs::write(
            a.dir.path().join("fleet/peers.json"),
            format!("{{\"proven\": {{\"{member}\": {proven}}}}}"),
        )
        .unwrap();
        let again = Fleet::load(a.dir.path(), now_ms()).unwrap();
        assert!(again.inner().proven.is_empty());
    }
    // What this build records survives a restart.
    a.fleet.proved(MemberId([0x11; 32]), [0x5a; 32]).unwrap();
    let again = Fleet::load(a.dir.path(), now_ms()).unwrap();
    assert_eq!(again.inner().proven[&MemberId([0x11; 32])].len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_past_the_bound_is_not_kept() {
    let (http, _, _) = relay().await;
    let a = Node::new(http, 1).await;
    // Proofs are kept only in a fleet.
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let member = MemberId([1; 32]);
    for seed in 0..u8::try_from(MAX_PROVEN).unwrap() {
        assert!(a.fleet.proved(member, [seed; 32]).unwrap());
    }
    assert!(!a.fleet.proved(member, [0xee; 32]).unwrap());
    assert!(a.fleet.proved(member, [0; 32]).unwrap());
}
