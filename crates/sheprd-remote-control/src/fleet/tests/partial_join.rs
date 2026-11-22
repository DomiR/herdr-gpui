//! A history too large for one `fleet apply` reaches a joining computer
//! over several, across a restart, granting nothing until it admits it.

use super::*;
use crate::control::fleet::sync::Apply;

#[tokio::test(flavor = "multi_thread")]
async fn a_history_in_parts_joins_once_it_admits() {
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
    // In graph order, the founding first and the admission after it.
    let ops: Vec<Op> = {
        let inner = a.fleet.inner();
        let log = inner.log.as_ref().unwrap();
        log.missing_for(&[]).into_iter().cloned().collect()
    };
    let (first, rest) = ops.split_at(1);
    let part = |ops: &[Op], heads: Vec<String>| Apply {
        fleet: Some(fleet.clone()),
        heads,
        ops: sync::encode_ops(ops),
        ..Apply::default()
    };
    let applied = b.fleet.apply(&part(first, Vec::new()), now_ms()).unwrap();
    assert_eq!(applied.fleet, fleet);
    assert_eq!(applied.heads, vec![first[0].id().to_string()]);
    // Not admitted yet: no fleet here, and nothing granted.
    assert!(b.fleet.inner().log.is_none());
    assert!(ask::view(&b.fleet).is_none());
    assert!(grants::dial_keys(&b.fleet).is_empty());
    // The rest arrives after a restart, as `fleet apply` with no daemon.
    let again = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert_eq!(again.inner().joining.len(), 1);
    let applied = again.apply(&part(rest, applied.heads), now_ms()).unwrap();
    assert_eq!(applied.refused, 0);
    let inner = again.inner();
    assert!(inner.joining.is_empty());
    assert_eq!(
        inner.log.as_ref().unwrap().heads(),
        a.fleet.inner().log.as_ref().unwrap().heads()
    );
    assert!(!b.dir.path().join("fleet/joining").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_history_that_never_admits_grants_nothing() {
    let (http, _, _) = relay().await;
    let (a, stranger) = (Node::new(http, 1).await, Node::new(http, 4).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let apply = Apply {
        fleet: Some(a.view().fleet),
        heads: Vec::new(),
        ops: sync::encode_ops(&a.ops()),
        ..Apply::default()
    };
    // Answered from the history so far, so the rest can come; still no
    // fleet here.
    stranger.fleet.apply(&apply, now_ms()).unwrap();
    assert!(stranger.fleet.inner().log.is_none());
    assert!(matches!(
        stranger.fleet.receive(Vec::new(), now_ms()),
        Err(Error::NotAdmitted)
    ));
}
