//! A host shared with members left out lands whole or not at all.

use super::*;
use crate::control::fleet::{Access, HostKind, MAX_OPS, MAX_PENDING, Unsigned};

/// Fills `node`'s log to one operation short of full, with `Access`
/// operations on its own host, each on the one before.
fn fill(node: &Node) {
    let mut inner = node.fleet.inner();
    let log = inner.log.as_mut().unwrap();
    let me = node.fleet.me();
    let host = grants::own_host(log.state(), &me, None).unwrap();
    let first = log.draft(me, now_ms(), Body::Checkpoint(Default::default()));
    let mut parents = first.parents.clone();
    let mut ops = Vec::new();
    let room = MAX_OPS - 1 - log.ops().count();
    for (seq, turn) in (first.seq..).zip(0..room) {
        let op = Unsigned {
            fleet: first.fleet,
            signer: me,
            seq,
            issued_ms: now_ms(),
            parents,
            body: Body::Access(Access {
                host,
                member: me,
                allow: turn % 2 == 1,
                rev: u64::try_from(turn).unwrap() + 1,
            }),
        }
        .sign(node.fleet.key())
        .unwrap();
        parents = vec![op.id()];
        ops.push(op);
    }
    for batch in ops.chunks(MAX_PENDING) {
        for (_, result) in log.insert_all(batch.to_vec(), now_ms()) {
            assert_eq!(result.unwrap(), Inserted::Applied);
        }
    }
    assert_eq!(log.ops().count(), MAX_OPS - 1);
    node.fleet.save(&inner).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_whose_exclusion_is_refused_is_not_shared() {
    let (http, _, _) = relay().await;
    let a = Node::new(http, 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let other = Node::new("127.0.0.1:9".parse().unwrap(), 9).await;
    let mut intro = me::intro(&other.state, now_ms()).unwrap();
    intro.kind = HostKind::Sheprd;
    // Room for the host, but not for its exclusion.
    fill(&a);
    let heads = a.fleet.inner().log.as_ref().unwrap().heads().to_vec();
    let hosts = a.view().hosts;
    let result = ask::answer(
        &a.state,
        &a.fleet,
        Ask::AddHost {
            intro: sync::text(&intro.to_bytes().unwrap()),
            deny: vec![a.me()],
        },
        now_ms(),
    );
    assert!(result.is_err());
    assert_eq!(a.view().hosts, hosts);
    assert_eq!(
        a.fleet.inner().log.as_ref().unwrap().heads(),
        heads.as_slice()
    );
    // Nor did the file take it.
    let again = Fleet::load(a.dir.path(), now_ms()).unwrap();
    assert_eq!(
        again.inner().log.as_ref().unwrap().heads(),
        heads.as_slice()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_lands_with_its_exclusions() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    let other = Node::new("127.0.0.1:9".parse().unwrap(), 9).await;
    let mut intro = me::intro(&other.state, now_ms()).unwrap();
    intro.kind = HostKind::Sheprd;
    let host = intro.id().unwrap();
    let before = a.ops().len();
    assert_eq!(
        a.ask(Ask::AddHost {
            intro: sync::text(&intro.to_bytes().unwrap()),
            deny: vec![b.me(), b.me()],
        }),
        Answer::Done
    );
    assert_eq!(a.ops().len(), before + 2);
    let inner = a.fleet.inner();
    let state = inner.log.as_ref().unwrap().state();
    assert!(!state.allowed(&host, &b.fleet.me()));
}
